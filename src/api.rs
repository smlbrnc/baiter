//! HTTP + SSE API — axum router, frontend'in tek arayüzü.
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
use crate::db::{self, BotUpdate, GlobalCredentials};
use crate::error::AppError;
use crate::polymarket::auth as polymarket_auth;
use crate::supervisor::{self, AppState};
use crate::types::{RunMode, Strategy};

/// Frontend yalnız `(private_key, signature_type, funder?)` gönderir; backend
/// L1 EIP-712 ile `apiKey/secret/passphrase`'i türetir. `builder_code` per-bot
/// değil — `config::BUILDER_CODE_HEX` sabitinden okunur.
#[derive(Debug, Deserialize)]
struct CredentialsInput {
    private_key: String,
    signature_type: i32,
    #[serde(default)]
    funder: Option<String>,
    #[serde(default)]
    nonce: u64,
}

fn trim_opt(s: Option<String>) -> Option<String> {
    s.and_then(|v| {
        let t = v.trim();
        (!t.is_empty()).then(|| t.to_string())
    })
}

async fn resolve_credentials(
    state: &AppState,
    input: CredentialsInput,
) -> Result<Credentials, AppError> {
    input.into_credentials(state).await
}

impl CredentialsInput {
    async fn into_credentials(self, state: &AppState) -> Result<Credentials, AppError> {
        let pk = self.private_key.trim();
        if pk.is_empty() {
            return Err(AppError::Config("private_key gerekli".into()));
        }
        if !(0..=2).contains(&self.signature_type) {
            return Err(AppError::Config(format!(
                "signature_type {} desteklenmiyor (0|1|2)",
                self.signature_type
            )));
        }
        let funder = trim_opt(self.funder);
        if matches!(self.signature_type, 1 | 2) && funder.is_none() {
            return Err(AppError::Config(format!(
                "signature_type={} için funder zorunlu",
                self.signature_type
            )));
        }
        let derived = polymarket_auth::derive_api_key(
            &state.http,
            &state.env.clob_base_url,
            pk,
            self.nonce,
        )
        .await?;
        Ok(Credentials {
            poly_address: derived.signer_address,
            poly_api_key: derived.api_key,
            poly_passphrase: derived.passphrase,
            poly_secret: derived.secret,
            polygon_private_key: pk.to_string(),
            signature_type: self.signature_type,
            funder,
        })
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/bots", get(list_bots).post(create_bot))
        .route(
            "/api/bots/{id}",
            get(get_bot).patch(update_bot).delete(delete_bot),
        )
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
        .route(
            "/api/settings/credentials",
            get(get_settings_credentials).put(put_settings_credentials),
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
    credentials: Option<CredentialsInput>,
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

/// Tüm ihlalleri tek `AppError::Config` mesajında birleştirip 400 döner.
fn validate_bot_settings(
    min_price: f64,
    max_price: f64,
    start_offset: u32,
) -> Result<(), AppError> {
    let mut errors: Vec<String> = Vec::new();
    if !(min_price > 0.0 && min_price < max_price && max_price < 1.0) {
        errors.push(format!(
            "price bounds: 0 < min_price ({min_price}) < max_price ({max_price}) < 1 olmalı"
        ));
    }
    if start_offset > 1 {
        errors.push(format!(
            "start_offset ({start_offset}): 0 (aktif) veya 1 (sonraki) olmalı"
        ));
    }
    if errors.is_empty() {
        return Ok(());
    }
    Err(AppError::Config(format!(
        "invalid bot settings: {}",
        errors.join("; ")
    )))
}

#[derive(Debug, Serialize)]
struct CreateBotResp {
    id: i64,
}

async fn create_bot(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBotReq>,
) -> Result<Json<CreateBotResp>, AppError> {
    validate_bot_settings(req.min_price, req.max_price, req.start_offset)?;
    let cfg = BotConfig {
        id: 0,
        name: req.name,
        slug_pattern: req.slug_pattern,
        strategy: req.strategy,
        run_mode: req.run_mode,
        order_usdc: req.order_usdc,
        min_price: req.min_price,
        max_price: req.max_price,
        cooldown_threshold: req.cooldown_threshold,
        start_offset: req.start_offset,
        strategy_params: req.strategy_params,
    };
    let id = db::insert_bot(&state.pool, &cfg).await?;
    if let Some(input) = req.credentials {
        let creds = resolve_credentials(&state, input).await?;
        db::upsert_credentials(&state.pool, id, &creds).await?;
    }
    if req.auto_start {
        supervisor::start_bot(state.clone(), id).await?;
    }
    Ok(Json(CreateBotResp { id }))
}

/// `slug_pattern` ve `strategy` immutable — bot oluşturulurken belirlenir.
#[derive(Debug, Deserialize)]
struct UpdateBotReq {
    name: String,
    run_mode: RunMode,
    order_usdc: f64,
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
    credentials: Option<CredentialsInput>,
}

async fn update_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateBotReq>,
) -> Result<Json<Value>, AppError> {
    let row = db::get_bot(&state.pool, id)
        .await?
        .ok_or(AppError::BotNotFound { bot_id: id })?;
    // State ↔ config drift'i önlemek için yalnız STOPPED kabul.
    if row.state != "STOPPED" {
        return Err(AppError::Conflict(format!(
            "bot {id} state={s}; ayarları güncellemek için önce durdur",
            s = row.state
        )));
    }
    validate_bot_settings(req.min_price, req.max_price, req.start_offset)?;
    let upd = BotUpdate {
        name: req.name,
        run_mode: req.run_mode,
        order_usdc: req.order_usdc,
        min_price: req.min_price,
        max_price: req.max_price,
        cooldown_threshold: req.cooldown_threshold,
        start_offset: req.start_offset,
        strategy_params: req.strategy_params,
    };
    db::update_bot(&state.pool, id, &upd).await?;
    if let Some(input) = req.credentials {
        let creds = resolve_credentials(&state, input).await?;
        db::upsert_credentials(&state.pool, id, &creds).await?;
    }
    let updated = db::get_bot(&state.pool, id)
        .await?
        .ok_or(AppError::BotNotFound { bot_id: id })?;
    bot_row_to_json(updated).map(Json)
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

/// Force delete: koşan child varsa önce durdurmaya çalış, hata olsa bile sil.
async fn delete_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if let Err(e) = supervisor::stop_bot(state.clone(), id).await {
        tracing::warn!(bot_id = id, error = %e, "stop_bot failed during delete");
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
) -> Result<Json<Vec<db::LogRow>>, AppError> {
    let logs = db::recent_logs(&state.pool, Some(id), q.limit).await?;
    Ok(Json(logs))
}

async fn bot_pnl(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Option<db::PnlSnapshot>>, AppError> {
    let snap = db::pnl::latest_pnl_for_bot(&state.pool, id).await?;
    Ok(Json(snap))
}

async fn bot_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let Some(s) = db::sessions::latest_session_for_bot(&state.pool, id).await? else {
        return Ok(Json(Value::Null));
    };
    let m = state.gamma.get_market_by_slug(&s.slug).await?;
    Ok(Json(serde_json::json!({
        "slug":     s.slug,
        "start_ts": s.start_ts,
        "end_ts":   s.end_ts,
        "state":    s.state,
        "title":    m.question,
        "image":    m.image,
    })))
}

#[derive(Debug, Deserialize)]
struct SessionListQuery {
    #[serde(default = "default_sessions_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_sessions_limit() -> i64 {
    20
}

async fn bot_sessions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(q): Query<SessionListQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    let (rows, total) = tokio::try_join!(
        db::sessions::list_sessions_for_bot(&state.pool, id, limit, offset),
        db::sessions::count_sessions_for_bot(&state.pool, id),
    )?;
    let now = crate::time::now_secs() as i64;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|s| {
            let is_live = s.end_ts > now && s.state != "RESOLVED" && s.state != "CLOSED";
            serde_json::json!({
                "slug":          s.slug,
                "start_ts":      s.start_ts,
                "end_ts":        s.end_ts,
                "state":         s.state,
                "cost_basis":    s.cost_basis,
                "up_filled":     s.up_filled,
                "down_filled":   s.down_filled,
                "realized_pnl":    s.realized_pnl,
                "pnl_if_up":       s.pnl_if_up,
                "pnl_if_down":     s.pnl_if_down,
                "winning_outcome": s.winning_outcome,
                "is_live":         is_live,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "items":  items,
        "total":  total,
        "limit":  limit,
        "offset": offset,
    })))
}

async fn session_detail(
    State(state): State<Arc<AppState>>,
    Path((id, slug)): Path<(i64, String)>,
) -> Result<Json<Value>, AppError> {
    let detail = db::sessions::session_by_bot_slug(&state.pool, id, &slug).await?;
    let Some(d) = detail else {
        return Ok(Json(Value::Null));
    };
    let market = state.gamma.get_market_by_slug(&d.slug).await.ok();
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
        "up_filled":     d.up_filled,
        "down_filled":   d.down_filled,
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
    2_000
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

/// `GET /api/settings/credentials` cevabı. Hassas alanlar (PK, secret, apiKey,
/// passphrase) kasıtlı olarak yok — yalnız "kayıt var mı?" + display meta.
#[derive(Debug, Serialize)]
struct SettingsCredentialsResp {
    poly_address: String,
    signature_type: i32,
    funder: Option<String>,
    has_credentials: bool,
    updated_at_ms: i64,
}

async fn get_settings_credentials(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SettingsCredentialsResp>, AppError> {
    let resp = db::get_global_credentials(&state.pool).await?.map_or_else(
        || SettingsCredentialsResp {
            poly_address: String::new(),
            signature_type: 0,
            funder: None,
            has_credentials: false,
            updated_at_ms: 0,
        },
        |c| SettingsCredentialsResp {
            poly_address: c.poly_address,
            signature_type: c.signature_type,
            funder: c.funder,
            has_credentials: true,
            updated_at_ms: c.updated_at_ms,
        },
    );
    Ok(Json(resp))
}

async fn put_settings_credentials(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CredentialsInput>,
) -> Result<StatusCode, AppError> {
    let creds = resolve_credentials(&state, req).await?;
    db::upsert_global_credentials(
        &state.pool,
        &GlobalCredentials {
            poly_address: creds.poly_address,
            poly_api_key: creds.poly_api_key,
            poly_passphrase: creds.poly_passphrase,
            poly_secret: creds.poly_secret,
            polygon_private_key: creds.polygon_private_key,
            signature_type: creds.signature_type,
            funder: creds.funder,
            updated_at_ms: 0,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
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
    let cfg = r.to_config()?;
    let strategy_params = serde_json::to_value(&cfg.strategy_params).map_err(|e| {
        AppError::Config(format!(
            "bot {id} strategy_params serialize: {e}",
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
