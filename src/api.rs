//! HTTP + SSE API — axum router, frontend'in tek arayüzü.
//!
//! Endpoint özeti:
//! - `POST /api/bots` — yeni bot (opsiyonel kimlik bilgileri), `auto_start=true` → spawn.
//! - `GET  /api/bots` — bot listesi.
//! - `GET  /api/bots/:id` — bot detayı.
//! - `DELETE /api/bots/:id` — durdur + sil.
//! - `POST /api/bots/:id/start` — başlat.
//! - `POST /api/bots/:id/stop` — durdur.
//! - `GET  /api/bots/:id/logs?limit=N` — son N log.
//! - `GET  /api/bots/:id/pnl` — son PnL snapshot.
//! - `GET  /api/bots/:id/session` — aktif session özeti (Gamma cache).
//! - `GET  /api/bots/:id/sessions` — bot'un tüm session'ları (özet liste).
//! - `GET  /api/bots/:id/sessions/:slug` — session detay (Gamma + position).
//! - `GET  /api/bots/:id/sessions/:slug/ticks?since_ms=N&limit=N` — BBA + signal history.
//! - `GET  /api/bots/:id/sessions/:slug/pnl?since_ms=N&limit=N` — PnL history.
//! - `GET  /api/bots/:id/sessions/:slug/trades?since_ms=N&limit=N` — trade history.
//! - `GET  /api/events` — SSE stream (`FrontendEvent`).
//!
//! Referans: [docs/bot-platform-mimari.md §2 §5](../../../docs/bot-platform-mimari.md).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::{Any, CorsLayer};

use crate::config::{BotConfig, Credentials, StrategyParams};
use crate::db;
use crate::error::AppError;
use crate::polymarket::GammaClient;
use crate::supervisor::{self, AppState};
use crate::types::{RunMode, Strategy};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/bots", get(list_bots).post(create_bot))
        .route("/api/bots/{id}", get(get_bot).delete(delete_bot))
        .route("/api/bots/{id}/start", post(start_bot))
        .route("/api/bots/{id}/stop", post(stop_bot))
        .route("/api/bots/{id}/logs", get(bot_logs))
        .route("/api/bots/{id}/pnl", get(bot_pnl))
        .route("/api/bots/{id}/session", get(bot_session))
        .route("/api/bots/{id}/sessions", get(bot_sessions))
        .route("/api/bots/{id}/sessions/{slug}", get(session_detail))
        .route("/api/bots/{id}/sessions/{slug}/ticks", get(session_ticks))
        .route("/api/bots/{id}/sessions/{slug}/pnl", get(session_pnl))
        .route(
            "/api/bots/{id}/sessions/{slug}/trades",
            get(session_trades),
        )
        .route("/api/events", get(events_sse))
        .layer(
            CorsLayer::new()
                .allow_methods(Any)
                .allow_headers(Any)
                .allow_origin(Any),
        )
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Debug, Deserialize)]
struct CreateBotReq {
    name: String,
    slug_pattern: String,
    strategy: Strategy,
    run_mode: RunMode,
    order_usdc: f64,
    signal_weight: f64,
    #[serde(default = "default_min_price")]
    min_price: f64,
    #[serde(default = "default_max_price")]
    max_price: f64,
    #[serde(default = "default_cooldown_threshold")]
    cooldown_threshold: u64,
    #[serde(default)]
    start_offset: u32,
    #[serde(default)]
    strategy_params: StrategyParams,
    #[serde(default)]
    credentials: Option<Credentials>,
    #[serde(default)]
    auto_start: bool,
}

fn default_min_price() -> f64 {
    0.05
}

fn default_max_price() -> f64 {
    0.95
}

fn default_cooldown_threshold() -> u64 {
    30_000
}

#[derive(Debug, Serialize)]
struct CreateBotResp {
    id: i64,
}

async fn create_bot(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBotReq>,
) -> Result<Json<CreateBotResp>, AppError> {
    if !(req.min_price > 0.0 && req.min_price < req.max_price && req.max_price < 1.0) {
        return Err(AppError::Config(format!(
            "invalid price bounds: 0 < min_price ({}) < max_price ({}) < 1 olmalı",
            req.min_price, req.max_price
        )));
    }
    if req.cooldown_threshold == 0 {
        return Err(AppError::Config(
            "invalid cooldown_threshold: > 0 ms olmalı".to_string(),
        ));
    }
    if req.start_offset > 1 {
        return Err(AppError::Config(format!(
            "invalid start_offset ({}): 0 (aktif) veya 1 (sonraki) olmalı",
            req.start_offset
        )));
    }
    let cfg = BotConfig {
        id: 0,
        name: req.name,
        slug_pattern: req.slug_pattern,
        strategy: req.strategy,
        run_mode: req.run_mode,
        order_usdc: req.order_usdc,
        signal_weight: req.signal_weight,
        min_price: req.min_price,
        max_price: req.max_price,
        cooldown_threshold: req.cooldown_threshold,
        start_offset: req.start_offset,
        strategy_params: req.strategy_params,
    };
    let id = db::insert_bot(&state.pool, &cfg).await?;
    if let Some(creds) = req.credentials {
        db::upsert_credentials(&state.pool, id, &creds).await?;
    }
    if req.auto_start {
        supervisor::start_bot(state.clone(), id).await?;
    }
    Ok(Json(CreateBotResp { id }))
}

async fn list_bots(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Value>>, AppError> {
    let rows = db::list_bots(&state.pool).await?;
    rows.into_iter()
        .map(bot_row_to_json)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn get_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let row = db::get_bot(&state.pool, id)
        .await?
        .ok_or(AppError::BotNotFound { bot_id: id })?;
    bot_row_to_json(row).map(Json)
}

async fn delete_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    // Silme operasyonunun semantiği: koşan child varsa önce durdurmaya çalış,
    // ancak DB satırını her durumda sil ("force delete"). Stop hatası
    // operatörün görmesi için warn olarak loglanır.
    if let Err(e) = supervisor::stop_bot(state.clone(), id).await {
        tracing::warn!(bot_id = id, error = %e, "stop_bot failed during delete; proceeding with DB delete");
    }
    db::delete_bot(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    supervisor::start_bot(state.clone(), id).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn stop_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    supervisor::stop_bot(state.clone(), id).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
struct LogQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    200
}

async fn bot_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(q): Query<LogQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    let logs = db::recent_logs(&state.pool, Some(id), q.limit).await?;
    Ok(Json(
        logs.into_iter()
            .map(|l| {
                serde_json::json!({
                    "id": l.id,
                    "bot_id": l.bot_id,
                    "level": l.level,
                    "message": l.message,
                    "ts_ms": l.ts_ms,
                })
            })
            .collect(),
    ))
}

async fn bot_pnl(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let snap = db::pnl::latest_pnl_for_bot(&state.pool, id).await?;
    match snap {
        Some(s) => Ok(Json(serde_json::json!({
            "cost_basis": s.cost_basis,
            "fee_total": s.fee_total,
            "shares_yes": s.shares_yes,
            "shares_no": s.shares_no,
            "pnl_if_up": s.pnl_if_up,
            "pnl_if_down": s.pnl_if_down,
            "mtm_pnl": s.mtm_pnl,
            "pair_count": s.pair_count,
            "ts_ms": s.ts_ms,
        }))),
        None => Ok(Json(Value::Null)),
    }
}

async fn bot_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let summary = db::sessions::latest_session_for_bot(&state.pool, id).await?;
    match summary {
        Some(s) => {
            let gamma = GammaClient::new(reqwest::Client::new(), state.env.gamma_base_url.clone());
            let m = gamma.get_market_by_slug(&s.slug).await?;
            Ok(Json(serde_json::json!({
                "slug":     s.slug,
                "start_ts": s.start_ts,
                "end_ts":   s.end_ts,
                "state":    s.state,
                "title":    m.question,
                "image":    m.image,
            })))
        }
        None => Ok(Json(Value::Null)),
    }
}

async fn bot_sessions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Value>>, AppError> {
    let rows = db::sessions::list_sessions_for_bot(&state.pool, id).await?;
    let now = crate::time::now_secs() as i64;
    Ok(Json(
        rows.into_iter()
            .map(|s| {
                let is_live = s.end_ts > now && s.state != "RESOLVED" && s.state != "CLOSED";
                serde_json::json!({
                    "slug":          s.slug,
                    "start_ts":      s.start_ts,
                    "end_ts":        s.end_ts,
                    "state":         s.state,
                    "cost_basis":    s.cost_basis,
                    "shares_yes":    s.shares_yes,
                    "shares_no":     s.shares_no,
                    "realized_pnl":  s.realized_pnl,
                    "is_live":       is_live,
                })
            })
            .collect(),
    ))
}

async fn session_detail(
    State(state): State<Arc<AppState>>,
    Path((id, slug)): Path<(i64, String)>,
) -> Result<Json<Value>, AppError> {
    let detail = db::sessions::session_by_bot_slug(&state.pool, id, &slug).await?;
    let Some(d) = detail else {
        return Ok(Json(Value::Null));
    };
    let gamma = GammaClient::new(reqwest::Client::new(), state.env.gamma_base_url.clone());
    let market = gamma.get_market_by_slug(&d.slug).await.ok();
    let now = crate::time::now_secs() as i64;
    let is_live = d.end_ts > now && d.state != "RESOLVED" && d.state != "CLOSED";
    Ok(Json(serde_json::json!({
        "bot_id":        d.bot_id,
        "slug":          d.slug,
        "start_ts":      d.start_ts,
        "end_ts":        d.end_ts,
        "state":         d.state,
        "cost_basis":    d.cost_basis,
        "fee_total":     d.fee_total,
        "shares_yes":    d.shares_yes,
        "shares_no":     d.shares_no,
        "realized_pnl":  d.realized_pnl,
        "is_live":       is_live,
        "title":         market.as_ref().and_then(|m| m.question.clone()),
        "image":         market.and_then(|m| m.image),
    })))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    #[serde(default)]
    since_ms: Option<i64>,
    #[serde(default = "default_history_limit")]
    limit: i64,
}

fn default_history_limit() -> i64 {
    2000
}

async fn session_ticks(
    State(state): State<Arc<AppState>>,
    Path((id, slug)): Path<(i64, String)>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<db::MarketTick>>, AppError> {
    let session = db::sessions::session_by_bot_slug(&state.pool, id, &slug)
        .await?
        .ok_or(AppError::BotNotFound { bot_id: id })?;
    let ticks =
        db::ticks::ticks_for_session(&state.pool, session.session_id, q.since_ms, q.limit).await?;
    Ok(Json(ticks))
}

async fn session_pnl(
    State(state): State<Arc<AppState>>,
    Path((id, slug)): Path<(i64, String)>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<db::PnlSnapshot>>, AppError> {
    let session = db::sessions::session_by_bot_slug(&state.pool, id, &slug)
        .await?
        .ok_or(AppError::BotNotFound { bot_id: id })?;
    let history =
        db::pnl::pnl_history_for_session(&state.pool, session.session_id, q.since_ms, q.limit)
            .await?;
    Ok(Json(history))
}

async fn session_trades(
    State(state): State<Arc<AppState>>,
    Path((id, slug)): Path<(i64, String)>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<db::TradeRecord>>, AppError> {
    let session = db::sessions::session_by_bot_slug(&state.pool, id, &slug)
        .await?
        .ok_or(AppError::BotNotFound { bot_id: id })?;
    let trades =
        db::trades::trades_for_session(&state.pool, session.session_id, q.since_ms, q.limit)
            .await?;
    Ok(Json(trades))
}

async fn events_sse(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(ev) => serde_json::to_string(&ev)
                .ok()
                .map(|s| Ok(Event::default().data(s))),
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

fn bot_row_to_json(r: db::BotRow) -> Result<Value, AppError> {
    let strategy_params: Value = serde_json::from_str(&r.strategy_params).map_err(|e| {
        AppError::Config(format!(
            "bot {id} strategy_params JSON bozuk: {e}",
            id = r.id
        ))
    })?;
    Ok(serde_json::json!({
        "id": r.id,
        "name": r.name,
        "slug_pattern": r.slug_pattern,
        "strategy": r.strategy,
        "run_mode": r.run_mode,
        "order_usdc": r.order_usdc,
        "signal_weight": r.signal_weight,
        "min_price": r.min_price,
        "max_price": r.max_price,
        "cooldown_threshold": r.cooldown_threshold,
        "start_offset": r.start_offset,
        "strategy_params": strategy_params,
        "state": r.state,
        "last_active_ms": r.last_active_ms,
        "created_at_ms": r.created_at_ms,
        "updated_at_ms": r.updated_at_ms,
    }))
}
