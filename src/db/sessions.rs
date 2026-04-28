//! `market_sessions` CRUD. Pozisyon agregatları yalnız `pnl_snapshots`'ta
//! tutulur; list/detail sorguları `LATEST_PNL_JOIN` ile son satırı çeker.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::time::now_ms;

/// `api::bot_session` için minimal özet (Gamma cache + slug).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub slug: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub state: String,
}

/// `api::sessions_for_bot` listesi için: özet + pozisyon agregatları
/// + en son PnL snapshot'undan if-up / if-down (yoksa `None`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListItem {
    pub slug: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub state: String,
    pub cost_basis: f64,
    pub up_filled: f64,
    pub down_filled: f64,
    pub realized_pnl: Option<f64>,
    pub pnl_if_up: Option<f64>,
    pub pnl_if_down: Option<f64>,
    pub winning_outcome: Option<String>,
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
    pub up_filled: f64,
    pub down_filled: f64,
    pub realized_pnl: Option<f64>,
    pub session_id: i64,
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

/// İlk RTDS tick'inde pencere açılış snapshot'unu yazar (migration 0010).
/// Çağırıcı bir flag ile tek-yazım garantisi sağlamalıdır.
pub async fn set_rtds_window_open(
    pool: &SqlitePool,
    session_id: i64,
    price: f64,
    ts_ms: i64,
) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query(
        "UPDATE market_sessions SET rtds_window_open_price = ?, \
         rtds_window_open_ts_ms = ?, updated_at_ms = ? WHERE id = ?",
    )
    .bind(price)
    .bind(ts_ms)
    .bind(now)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_market_session_meta(
    pool: &SqlitePool,
    session_id: i64,
    condition_id: &str,
    asset_id_up: &str,
    asset_id_down: &str,
    tick_size: f64,
    min_order_size: f64,
) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query(
        "UPDATE market_sessions SET condition_id = ?, asset_id_up = ?, asset_id_down = ?, \
         tick_size = ?, min_order_size = ?, state = 'ACTIVE', updated_at_ms = ? WHERE id = ?",
    )
    .bind(condition_id)
    .bind(asset_id_up)
    .bind(asset_id_down)
    .bind(tick_size)
    .bind(min_order_size)
    .bind(now)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// `pnl_snapshots`'tan en son satırı LEFT JOIN eder. Korelasyonlu MAX
/// `idx_pnl_session_ts` (migration 0009) sayesinde O(log N).
const LATEST_PNL_JOIN: &str = "LEFT JOIN pnl_snapshots p \
       ON p.market_session_id = s.id \
      AND p.ts_ms = (SELECT MAX(ts_ms) FROM pnl_snapshots \
                     WHERE market_session_id = s.id)";

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
        slug: r.get("slug"),
        start_ts: r.get("start_ts"),
        end_ts: r.get("end_ts"),
        state: r.get("state"),
    }))
}

/// `/api/bots/:id/sessions` — sayfalanmış liste; toplam sayı
/// [`count_sessions_for_bot`] ile çekilir.
pub async fn list_sessions_for_bot(
    pool: &SqlitePool,
    bot_id: i64,
    limit: i64,
    offset: i64,
) -> Result<Vec<SessionListItem>, AppError> {
    // CTE: önce sayfa, sonra JOIN — büyük botlarda lineer patlama olmasın.
    let sql = format!(
        "WITH paged AS ( \
             SELECT id, slug, start_ts, end_ts, state, realized_pnl, condition_id \
             FROM market_sessions \
             WHERE bot_id = ? \
             ORDER BY start_ts DESC \
             LIMIT ? OFFSET ? \
         ) \
         SELECT s.slug, s.start_ts, s.end_ts, s.state, s.realized_pnl, \
                COALESCE(p.cost_basis,  0.0) AS cost_basis,  \
                COALESCE(p.up_filled,   0.0) AS up_filled,   \
                COALESCE(p.down_filled, 0.0) AS down_filled, \
                p.pnl_if_up, p.pnl_if_down, \
                CASE \
                    WHEN lt.up_best_bid   > 0.95 THEN 'Up' \
                    WHEN lt.down_best_bid > 0.95 THEN 'Down' \
                    ELSE mr.winning_outcome \
                END AS winning_outcome \
         FROM paged s \
         {LATEST_PNL_JOIN} \
         LEFT JOIN market_resolved mr ON mr.market = s.condition_id \
         LEFT JOIN market_ticks lt \
                ON lt.market_session_id = s.id \
               AND lt.ts_ms = (SELECT MAX(ts_ms) FROM market_ticks \
                                WHERE market_session_id = s.id) \
         ORDER BY s.start_ts DESC"
    );
    let rows = sqlx::query(&sql)
        .bind(bot_id)
        .bind(limit)
        .bind(offset)
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
            up_filled: r.get("up_filled"),
            down_filled: r.get("down_filled"),
            realized_pnl: r.get("realized_pnl"),
            pnl_if_up: r.get("pnl_if_up"),
            pnl_if_down: r.get("pnl_if_down"),
            winning_outcome: r.try_get("winning_outcome").ok().flatten(),
        })
        .collect())
}

/// `/api/bots/:id/sessions` toplam satır sayısı (sayfa kontrolleri için).
pub async fn count_sessions_for_bot(
    pool: &SqlitePool,
    bot_id: i64,
) -> Result<i64, AppError> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM market_sessions WHERE bot_id = ?")
        .bind(bot_id)
        .fetch_one(pool)
        .await?;
    Ok(row.get("n"))
}

/// `/api/bots/:id/sessions/:slug` için: detay + position agregatları.
pub async fn session_by_bot_slug(
    pool: &SqlitePool,
    bot_id: i64,
    slug: &str,
) -> Result<Option<SessionDetail>, AppError> {
    let sql = format!(
        "SELECT s.id, s.slug, s.start_ts, s.end_ts, s.state, s.realized_pnl, \
                COALESCE(p.cost_basis,  0.0) AS cost_basis,  \
                COALESCE(p.fee_total,   0.0) AS fee_total,   \
                COALESCE(p.up_filled,   0.0) AS up_filled,   \
                COALESCE(p.down_filled, 0.0) AS down_filled  \
         FROM market_sessions s \
         {LATEST_PNL_JOIN} \
         WHERE s.bot_id = ? AND s.slug = ?"
    );
    let row = sqlx::query(&sql)
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
        up_filled: r.get("up_filled"),
        down_filled: r.get("down_filled"),
        realized_pnl: r.get("realized_pnl"),
        session_id: r.get("id"),
    }))
}
