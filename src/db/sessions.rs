//! `market_sessions` tablosu CRUD'u.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::time::now_ms;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSessionRow {
    pub id: i64,
    pub bot_id: i64,
    pub slug: String,
    pub condition_id: Option<String>,
    pub asset_id_yes: Option<String>,
    pub asset_id_no: Option<String>,
    pub tick_size: Option<f64>,
    pub min_order_size: Option<f64>,
    pub start_ts: i64,
    pub end_ts: i64,
    pub state: String,
    pub cost_basis: f64,
    pub fee_total: f64,
    pub shares_yes: f64,
    pub shares_no: f64,
    pub realized_pnl: Option<f64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// `api::bot_session` için minimal özet (Gamma cache + slug).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub slug: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub state: String,
}

/// `api::sessions_for_bot` listesi için: özet + pozisyon agregatları.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListItem {
    pub slug: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub state: String,
    pub cost_basis: f64,
    pub shares_yes: f64,
    pub shares_no: f64,
    pub realized_pnl: Option<f64>,
}

/// `api::session_detail` için: pozisyon agregatları + window meta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub bot_id: i64,
    pub slug: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub state: String,
    pub cost_basis: f64,
    pub fee_total: f64,
    pub shares_yes: f64,
    pub shares_no: f64,
    pub realized_pnl: Option<f64>,
    pub session_id: i64,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for MarketSessionRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            bot_id: row.try_get("bot_id")?,
            slug: row.try_get("slug")?,
            condition_id: row.try_get("condition_id")?,
            asset_id_yes: row.try_get("asset_id_yes")?,
            asset_id_no: row.try_get("asset_id_no")?,
            tick_size: row.try_get("tick_size")?,
            min_order_size: row.try_get("min_order_size")?,
            start_ts: row.try_get("start_ts")?,
            end_ts: row.try_get("end_ts")?,
            state: row.try_get("state")?,
            cost_basis: row.try_get("cost_basis")?,
            fee_total: row.try_get("fee_total")?,
            shares_yes: row.try_get("shares_yes")?,
            shares_no: row.try_get("shares_no")?,
            realized_pnl: row.try_get("realized_pnl")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

pub async fn upsert_market_session(
    pool: &SqlitePool,
    bot_id: i64,
    slug: &str,
    start_ts: i64,
    end_ts: i64,
) -> Result<i64, AppError> {
    let now = now_ms() as i64;
    let row = sqlx::query(
        "INSERT INTO market_sessions (bot_id, slug, start_ts, end_ts, created_at_ms, updated_at_ms) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(bot_id, slug) DO UPDATE SET updated_at_ms = excluded.updated_at_ms \
         RETURNING id",
    )
    .bind(bot_id)
    .bind(slug)
    .bind(start_ts)
    .bind(end_ts)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("id"))
}

pub async fn update_market_session_meta(
    pool: &SqlitePool,
    session_id: i64,
    condition_id: &str,
    asset_id_yes: &str,
    asset_id_no: &str,
    tick_size: f64,
    min_order_size: f64,
) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query(
        "UPDATE market_sessions SET condition_id = ?, asset_id_yes = ?, asset_id_no = ?, \
         tick_size = ?, min_order_size = ?, state = 'ACTIVE', updated_at_ms = ? WHERE id = ?",
    )
    .bind(condition_id)
    .bind(asset_id_yes)
    .bind(asset_id_no)
    .bind(tick_size)
    .bind(min_order_size)
    .bind(now)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// `api::bot_session` için: bot'un en yeni `market_sessions` satırının özeti.
pub async fn latest_session_for_bot(
    pool: &SqlitePool,
    bot_id: i64,
) -> Result<Option<SessionSummary>, AppError> {
    let row = sqlx::query(
        "SELECT slug, start_ts, end_ts, state FROM market_sessions \
         WHERE bot_id = ? ORDER BY updated_at_ms DESC LIMIT 1",
    )
    .bind(bot_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| SessionSummary {
        slug: r.get::<String, _>("slug"),
        start_ts: r.get::<i64, _>("start_ts"),
        end_ts: r.get::<i64, _>("end_ts"),
        state: r.get::<String, _>("state"),
    }))
}

/// `/api/bots/:id/sessions` için: bot'un tüm session'ları, en yeniden eskiye.
pub async fn list_sessions_for_bot(
    pool: &SqlitePool,
    bot_id: i64,
) -> Result<Vec<SessionListItem>, AppError> {
    let rows = sqlx::query(
        "SELECT slug, start_ts, end_ts, state, cost_basis, shares_yes, shares_no, realized_pnl \
         FROM market_sessions WHERE bot_id = ? ORDER BY start_ts DESC",
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SessionListItem {
            slug: r.get("slug"),
            start_ts: r.get("start_ts"),
            end_ts: r.get("end_ts"),
            state: r.get("state"),
            cost_basis: r.get("cost_basis"),
            shares_yes: r.get("shares_yes"),
            shares_no: r.get("shares_no"),
            realized_pnl: r.get("realized_pnl"),
        })
        .collect())
}

/// `/api/bots/:id/sessions/:slug` için: detay + position agregatları.
pub async fn session_by_bot_slug(
    pool: &SqlitePool,
    bot_id: i64,
    slug: &str,
) -> Result<Option<SessionDetail>, AppError> {
    let row = sqlx::query(
        "SELECT id, slug, start_ts, end_ts, state, cost_basis, fee_total, \
         shares_yes, shares_no, realized_pnl \
         FROM market_sessions WHERE bot_id = ? AND slug = ?",
    )
    .bind(bot_id)
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| SessionDetail {
        bot_id,
        slug: r.get("slug"),
        start_ts: r.get("start_ts"),
        end_ts: r.get("end_ts"),
        state: r.get("state"),
        cost_basis: r.get("cost_basis"),
        fee_total: r.get("fee_total"),
        shares_yes: r.get("shares_yes"),
        shares_no: r.get("shares_no"),
        realized_pnl: r.get("realized_pnl"),
        session_id: r.get("id"),
    }))
}
