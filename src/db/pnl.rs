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
    pub shares_yes: f64,
    pub shares_no: f64,
    pub pnl_if_up: f64,
    pub pnl_if_down: f64,
    pub mtm_pnl: f64,
    pub pair_count: f64,
    pub ts_ms: i64,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_pnl_snapshot(
    pool: &SqlitePool,
    bot_id: i64,
    market_session_id: i64,
    cost_basis: f64,
    fee_total: f64,
    shares_yes: f64,
    shares_no: f64,
    pnl_if_up: f64,
    pnl_if_down: f64,
    mtm_pnl: f64,
    pair_count: f64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO pnl_snapshots (bot_id, market_session_id, cost_basis, fee_total, \
         shares_yes, shares_no, pnl_if_up, pnl_if_down, mtm_pnl, pair_count, ts_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bot_id)
    .bind(market_session_id)
    .bind(cost_basis)
    .bind(fee_total)
    .bind(shares_yes)
    .bind(shares_no)
    .bind(pnl_if_up)
    .bind(pnl_if_down)
    .bind(mtm_pnl)
    .bind(pair_count)
    .bind(now_ms() as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// `api::bot_pnl` için: bot'un en son PnL snapshot'ı.
pub async fn latest_pnl_for_bot(
    pool: &SqlitePool,
    bot_id: i64,
) -> Result<Option<PnlSnapshot>, AppError> {
    let row = sqlx::query(
        "SELECT cost_basis, fee_total, shares_yes, shares_no, pnl_if_up, pnl_if_down, \
         mtm_pnl, pair_count, ts_ms \
         FROM pnl_snapshots WHERE bot_id = ? ORDER BY ts_ms DESC LIMIT 1",
    )
    .bind(bot_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| PnlSnapshot {
        cost_basis: r.get::<f64, _>("cost_basis"),
        fee_total: r.get::<f64, _>("fee_total"),
        shares_yes: r.get::<f64, _>("shares_yes"),
        shares_no: r.get::<f64, _>("shares_no"),
        pnl_if_up: r.get::<f64, _>("pnl_if_up"),
        pnl_if_down: r.get::<f64, _>("pnl_if_down"),
        mtm_pnl: r.get::<f64, _>("mtm_pnl"),
        pair_count: r.get::<f64, _>("pair_count"),
        ts_ms: r.get::<i64, _>("ts_ms"),
    }))
}
