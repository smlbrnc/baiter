//! `market_ticks` tablosu — 1 sn cadence BBA + Binance signal snapshot'ları.
//!
//! Yazım: `bot/persist.rs::snapshot_tick` fire-and-forget (`spawn_db`).
//! Okuma: `api.rs::session_ticks` history endpoint'i.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;

use super::spawn_db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketTick {
    pub yes_best_bid: f64,
    pub yes_best_ask: f64,
    pub no_best_bid: f64,
    pub no_best_ask: f64,
    pub signal_score: f64,
    pub bsi: f64,
    pub ofi: f64,
    pub cvd: f64,
    pub ts_ms: i64,
}

/// Tek satır insert — `bot_id` + `market_session_id` ile birlikte.
pub async fn insert_market_tick(
    pool: &SqlitePool,
    bot_id: i64,
    market_session_id: i64,
    tick: &MarketTick,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO market_ticks (bot_id, market_session_id, yes_best_bid, yes_best_ask, \
         no_best_bid, no_best_ask, signal_score, bsi, ofi, cvd, ts_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bot_id)
    .bind(market_session_id)
    .bind(tick.yes_best_bid)
    .bind(tick.yes_best_ask)
    .bind(tick.no_best_bid)
    .bind(tick.no_best_ask)
    .bind(tick.signal_score)
    .bind(tick.bsi)
    .bind(tick.ofi)
    .bind(tick.cvd)
    .bind(tick.ts_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fire-and-forget kalıbı (§⚡ Kural 4).
pub fn persist_tick(
    pool: &SqlitePool,
    bot_id: i64,
    market_session_id: i64,
    tick: MarketTick,
    label: &'static str,
) {
    let pool = pool.clone();
    spawn_db(label, async move {
        insert_market_tick(&pool, bot_id, market_session_id, &tick).await
    });
}

/// History fetch — `after_ts_ms` opsiyonel filtre, `limit` üst sınır.
/// Sıralama: `ts_ms ASC` (chart için kronolojik).
pub async fn ticks_for_session(
    pool: &SqlitePool,
    market_session_id: i64,
    after_ts_ms: Option<i64>,
    limit: i64,
) -> Result<Vec<MarketTick>, AppError> {
    let after = after_ts_ms.unwrap_or(0);
    let rows = sqlx::query(
        "SELECT yes_best_bid, yes_best_ask, no_best_bid, no_best_ask, \
         signal_score, bsi, ofi, cvd, ts_ms \
         FROM market_ticks \
         WHERE market_session_id = ? AND ts_ms > ? \
         ORDER BY ts_ms ASC LIMIT ?",
    )
    .bind(market_session_id)
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MarketTick {
            yes_best_bid: r.get("yes_best_bid"),
            yes_best_ask: r.get("yes_best_ask"),
            no_best_bid: r.get("no_best_bid"),
            no_best_ask: r.get("no_best_ask"),
            signal_score: r.get("signal_score"),
            bsi: r.get("bsi"),
            ofi: r.get("ofi"),
            cvd: r.get("cvd"),
            ts_ms: r.get("ts_ms"),
        })
        .collect())
}
