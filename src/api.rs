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
//! - `GET  /api/events` — SSE stream (`FrontendEvent`).
//!
//! Referans: [docs/bot-platform-mimari.md §2 §5](../../../docs/bot-platform-mimari.md).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::{Any, CorsLayer};

use crate::config::{BotConfig, Credentials, StrategyParams};
use crate::db;
use crate::polymarket::gamma::GammaClient;
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
) -> Result<Json<CreateBotResp>, ApiError> {
    if !(req.min_price > 0.0 && req.min_price < req.max_price && req.max_price < 1.0) {
        return Err(ApiError::Internal(format!(
            "invalid price bounds: 0 < min_price ({}) < max_price ({}) < 1 olmalı",
            req.min_price, req.max_price
        )));
    }
    if req.cooldown_threshold == 0 {
        return Err(ApiError::Internal(
            "invalid cooldown_threshold: > 0 ms olmalı".to_string(),
        ));
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
        strategy_params: req.strategy_params,
    };
    let id = db::insert_bot(&state.pool, &cfg).await?;
    if let Some(creds) = req.credentials {
        db::upsert_credentials(&state.pool, id, &creds).await?;
    }
    if req.auto_start {
        supervisor::start_bot(state.clone(), id)
            .await
            .map_err(ApiError::from)?;
    }
    Ok(Json(CreateBotResp { id }))
}

async fn list_bots(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Value>>, ApiError> {
    let rows = db::list_bots(&state.pool).await?;
    Ok(Json(rows.into_iter().map(bot_row_to_json).collect()))
}

async fn get_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = db::get_bot(&state.pool, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(bot_row_to_json(row)))
}

async fn delete_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let _ = supervisor::stop_bot(state.clone(), id).await;
    db::delete_bot(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    supervisor::start_bot(state.clone(), id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::ACCEPTED)
}

async fn stop_bot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    supervisor::stop_bot(state.clone(), id)
        .await
        .map_err(ApiError::from)?;
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
) -> Result<Json<Vec<Value>>, ApiError> {
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
) -> Result<Json<Value>, ApiError> {
    let row = sqlx::query(
        "SELECT cost_basis, fee_total, shares_yes, shares_no, pnl_if_up, pnl_if_down, mtm_pnl, ts_ms \
         FROM pnl_snapshots WHERE bot_id = ? ORDER BY ts_ms DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    match row {
        Some(r) => {
            use sqlx::Row as _;
            Ok(Json(serde_json::json!({
                "cost_basis": r.get::<f64, _>("cost_basis"),
                "fee_total": r.get::<f64, _>("fee_total"),
                "shares_yes": r.get::<f64, _>("shares_yes"),
                "shares_no": r.get::<f64, _>("shares_no"),
                "pnl_if_up": r.get::<f64, _>("pnl_if_up"),
                "pnl_if_down": r.get::<f64, _>("pnl_if_down"),
                "mtm_pnl": r.get::<f64, _>("mtm_pnl"),
                "ts_ms": r.get::<i64, _>("ts_ms"),
            })))
        }
        None => Ok(Json(Value::Null)),
    }
}

async fn bot_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = sqlx::query(
        "SELECT slug, start_ts, end_ts, state FROM market_sessions \
         WHERE bot_id = ? ORDER BY updated_at_ms DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    match row {
        Some(r) => {
            use sqlx::Row as _;
            let slug = r.get::<String, _>("slug");
            let gamma = GammaClient::new(reqwest::Client::new(), state.env.gamma_base_url.clone());
            let m = gamma
                .get_market_by_slug(&slug)
                .await
                .map_err(ApiError::from)?;
            Ok(Json(serde_json::json!({
                "slug": slug,
                "start_ts": r.get::<i64, _>("start_ts"),
                "end_ts":   r.get::<i64, _>("end_ts"),
                "state":    r.get::<String, _>("state"),
                "title":    m.question,
                "image":    m.image,
            })))
        }
        None => Ok(Json(Value::Null)),
    }
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

fn bot_row_to_json(r: db::BotRow) -> Value {
    serde_json::json!({
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
        "strategy_params": serde_json::from_str::<Value>(&r.strategy_params).unwrap_or(Value::Null),
        "state": r.state,
        "last_active_ms": r.last_active_ms,
        "created_at_ms": r.created_at_ms,
        "updated_at_ms": r.updated_at_ms,
    })
}

// ----- errors --------------------------------------------------------------

#[derive(Debug)]
enum ApiError {
    NotFound,
    Internal(String),
}

impl From<crate::error::AppError> for ApiError {
    fn from(e: crate::error::AppError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<String> for ApiError {
    fn from(e: String) -> Self {
        Self::Internal(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            Self::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        }
    }
}
