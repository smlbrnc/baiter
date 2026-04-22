//! `pnl_snapshots` tablosu — INSERT + en yeni satır helper'ı.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::time::now_ms;

/// `pnl_snapshots` tablosundan tek satır (api JSON'una eşlenir).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlSnapshot {
    pub cost_basis: f64,
    pub fee_total: f64,
    pub up_filled: f64,
    pub down_filled: f64,
    pub pnl_if_up: f64,
    pub pnl_if_down: f64,
    pub mtm_pnl: f64,
    pub pair_count: f64,
    pub avg_up: f64,
    pub avg_down: f64,
    pub ts_ms: i64,
}

pub async fn insert_pnl_snapshot(
    pool: &SqlitePool,
    bot_id: i64,
    market_session_id: i64,
    snap: &PnlSnapshot,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO pnl_snapshots (bot_id, market_session_id, cost_basis, fee_total, \
         up_filled, down_filled, pnl_if_up, pnl_if_down, mtm_pnl, pair_count, avg_up, avg_down, ts_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bot_id)
    .bind(market_session_id)
    .bind(snap.cost_basis)
    .bind(snap.fee_total)
    .bind(snap.up_filled)
    .bind(snap.down_filled)
    .bind(snap.pnl_if_up)
    .bind(snap.pnl_if_down)
    .bind(snap.mtm_pnl)
    .bind(snap.pair_count)
    .bind(snap.avg_up)
    .bind(snap.avg_down)
    .bind(now_ms() as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// `/api/bots/:id/sessions/:slug/pnl` için: session bazlı PnL geçmişi.
/// Sıralama: `ts_ms ASC` (chart için kronolojik).
pub async fn pnl_history_for_session(
    pool: &SqlitePool,
    market_session_id: i64,
    after_ts_ms: Option<i64>,
    limit: i64,
) -> Result<Vec<PnlSnapshot>, AppError> {
    let after = after_ts_ms.unwrap_or(0);
    let rows = sqlx::query(
        "SELECT cost_basis, fee_total, up_filled, down_filled, pnl_if_up, pnl_if_down, \
         mtm_pnl, pair_count, avg_up, avg_down, ts_ms \
         FROM pnl_snapshots \
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
        .map(|r| PnlSnapshot {
            cost_basis: r.get("cost_basis"),
            fee_total: r.get("fee_total"),
            up_filled: r.get("up_filled"),
            down_filled: r.get("down_filled"),
            pnl_if_up: r.get("pnl_if_up"),
            pnl_if_down: r.get("pnl_if_down"),
            mtm_pnl: r.get("mtm_pnl"),
            pair_count: r.get("pair_count"),
            avg_up: r.get("avg_up"),
            avg_down: r.get("avg_down"),
            ts_ms: r.get("ts_ms"),
        })
        .collect())
}

/// `api::bot_pnl` için: bot'un en son PnL snapshot'ı.
pub async fn latest_pnl_for_bot(
    pool: &SqlitePool,
    bot_id: i64,
) -> Result<Option<PnlSnapshot>, AppError> {
    let row = sqlx::query(
        "SELECT cost_basis, fee_total, up_filled, down_filled, pnl_if_up, pnl_if_down, \
         mtm_pnl, pair_count, avg_up, avg_down, ts_ms \
         FROM pnl_snapshots WHERE bot_id = ? ORDER BY ts_ms DESC LIMIT 1",
    )
    .bind(bot_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| PnlSnapshot {
        cost_basis: r.get::<f64, _>("cost_basis"),
        fee_total: r.get::<f64, _>("fee_total"),
        up_filled: r.get::<f64, _>("up_filled"),
        down_filled: r.get::<f64, _>("down_filled"),
        pnl_if_up: r.get::<f64, _>("pnl_if_up"),
        pnl_if_down: r.get::<f64, _>("pnl_if_down"),
        mtm_pnl: r.get::<f64, _>("mtm_pnl"),
        pair_count: r.get::<f64, _>("pair_count"),
        avg_up: r.get::<f64, _>("avg_up"),
        avg_down: r.get::<f64, _>("avg_down"),
        ts_ms: r.get::<i64, _>("ts_ms"),
    }))
}
