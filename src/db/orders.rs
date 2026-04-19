//! `orders` tablosu CRUD'u.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecord {
    pub order_id: String,
    pub bot_id: i64,
    pub market_session_id: Option<i64>,
    pub source: String,
    pub lifecycle_type: Option<String>,
    pub market: Option<String>,
    pub asset_id: Option<String>,
    pub side: Option<String>,
    pub price: Option<f64>,
    pub outcome: Option<String>,
    pub order_type: Option<String>,
    pub original_size: Option<f64>,
    pub size_matched: Option<f64>,
    pub expiration: Option<i64>,
    pub associate_trades: Option<String>,
    pub post_status: Option<String>,
    pub order_status: Option<String>,
    pub ts_ms: i64,
    pub raw_payload: Option<String>,
    /// CLOB DELETE /order başarılı yanıtı (JSON). Sadece `engine::Executor::cancel`
    /// veya benzer iptal yolu doldurur. Doc §8.
    pub delete_canceled: Option<String>,
    /// CLOB DELETE /order yanıtında iptal edilemeyen orderlar (JSON).
    pub delete_not_canceled: Option<String>,
}

pub async fn upsert_order(pool: &SqlitePool, r: &OrderRecord) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO orders (order_id, bot_id, market_session_id, source, lifecycle_type, market, \
         asset_id, side, price, outcome, order_type, original_size, size_matched, expiration, \
         associate_trades, post_status, order_status, ts_ms, raw_payload, \
         delete_canceled, delete_not_canceled) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(order_id) DO UPDATE SET \
         lifecycle_type = COALESCE(excluded.lifecycle_type, orders.lifecycle_type), \
         size_matched = COALESCE(excluded.size_matched, orders.size_matched), \
         associate_trades = COALESCE(excluded.associate_trades, orders.associate_trades), \
         post_status = COALESCE(excluded.post_status, orders.post_status), \
         order_status = COALESCE(excluded.order_status, orders.order_status), \
         ts_ms = excluded.ts_ms, \
         raw_payload = COALESCE(excluded.raw_payload, orders.raw_payload), \
         delete_canceled = COALESCE(excluded.delete_canceled, orders.delete_canceled), \
         delete_not_canceled = COALESCE(excluded.delete_not_canceled, orders.delete_not_canceled)",
    )
    .bind(&r.order_id)
    .bind(r.bot_id)
    .bind(r.market_session_id)
    .bind(&r.source)
    .bind(&r.lifecycle_type)
    .bind(&r.market)
    .bind(&r.asset_id)
    .bind(&r.side)
    .bind(r.price)
    .bind(&r.outcome)
    .bind(&r.order_type)
    .bind(r.original_size)
    .bind(r.size_matched)
    .bind(r.expiration)
    .bind(&r.associate_trades)
    .bind(&r.post_status)
    .bind(&r.order_status)
    .bind(r.ts_ms)
    .bind(&r.raw_payload)
    .bind(&r.delete_canceled)
    .bind(&r.delete_not_canceled)
    .execute(pool)
    .await?;
    Ok(())
}
