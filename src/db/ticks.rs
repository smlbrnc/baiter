//! `market_ticks` tablosu — 1 sn cadence BBA + sinyal snapshot'ları.
//!
//! Yazım: `bot/persist.rs::snapshot_tick` fire-and-forget (`spawn_db`).
//! Okuma: `api.rs::session_ticks` history endpoint'i.
//!
//! DB sütunları (`bsi`, `ofi`, `cvd`) eski adlarıyla korunur;
//! Rust struct alanları yeni sinyal anlamlarını yansıtır.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;

use super::spawn_db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketTick {
    pub up_best_bid: f64,
    pub up_best_ask: f64,
    pub down_best_bid: f64,
    pub down_best_ask: f64,
    /// `skor × 5 + 5 ∈ [0, 10]`; 5.0 = nötr.
    pub signal_score: f64,
    /// Binance CVD imbalance ∈ [−1, +1] — DB sütun adı: `bsi`.
    pub imbalance: f64,
    /// OKX EMA momentum (bps, kırpılmamış) — DB sütun adı: `ofi`.
    pub momentum_bps: f64,
    /// Birleşik sinyal skoru ∈ [−1, +1] — DB sütun adı: `cvd`.
    pub skor: f64,
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
        "INSERT INTO market_ticks (bot_id, market_session_id, up_best_bid, up_best_ask, \
         down_best_bid, down_best_ask, signal_score, bsi, ofi, cvd, ts_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bot_id)
    .bind(market_session_id)
    .bind(tick.up_best_bid)
    .bind(tick.up_best_ask)
    .bind(tick.down_best_bid)
    .bind(tick.down_best_ask)
    .bind(tick.signal_score)
    .bind(tick.imbalance)
    .bind(tick.momentum_bps)
    .bind(tick.skor)
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
        "SELECT up_best_bid, up_best_ask, down_best_bid, down_best_ask, \
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
            up_best_bid: r.get("up_best_bid"),
            up_best_ask: r.get("up_best_ask"),
            down_best_bid: r.get("down_best_bid"),
            down_best_ask: r.get("down_best_ask"),
            signal_score: r.get("signal_score"),
            imbalance: r.get("bsi"),
            momentum_bps: r.get("ofi"),
            skor: r.get("cvd"),
            ts_ms: r.get("ts_ms"),
        })
        .collect())
}
