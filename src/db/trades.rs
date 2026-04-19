//! `trades` tablosu CRUD'u.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub trade_id: String,
    pub bot_id: i64,
    pub market_session_id: Option<i64>,
    pub market: Option<String>,
    pub asset_id: Option<String>,
    pub taker_order_id: Option<String>,
    pub maker_orders: Option<String>,
    pub trader_side: Option<String>,
    pub side: Option<String>,
    pub outcome: Option<String>,
    pub size: f64,
    pub price: f64,
    pub status: String,
    pub fee: f64,
    pub ts_ms: i64,
    pub raw_payload: Option<String>,
}

pub async fn upsert_trade(pool: &SqlitePool, r: &TradeRecord) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO trades (trade_id, bot_id, market_session_id, market, asset_id, \
         taker_order_id, maker_orders, trader_side, side, outcome, size, price, status, fee, \
         ts_ms, raw_payload) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(trade_id) DO UPDATE SET \
         status = excluded.status, \
         ts_ms = excluded.ts_ms, \
         raw_payload = COALESCE(excluded.raw_payload, trades.raw_payload)",
    )
    .bind(&r.trade_id)
    .bind(r.bot_id)
    .bind(r.market_session_id)
    .bind(&r.market)
    .bind(&r.asset_id)
    .bind(&r.taker_order_id)
    .bind(&r.maker_orders)
    .bind(&r.trader_side)
    .bind(&r.side)
    .bind(&r.outcome)
    .bind(r.size)
    .bind(r.price)
    .bind(&r.status)
    .bind(r.fee)
    .bind(r.ts_ms)
    .bind(&r.raw_payload)
    .execute(pool)
    .await?;
    Ok(())
}
