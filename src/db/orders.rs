//! `orders` tablosu CRUD'u.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppError;

use super::spawn_db;

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

/// Fire-and-forget kalıbı (§⚡ Kural 4): kritik yolu bloke etmeden
/// `OrderRecord`'u DB'ye yazmak isteyen tüm callerlar bu helper'ı kullanır.
/// Hata sadece `tracing::warn!` ile loglanır.
pub fn persist_order(pool: &SqlitePool, record: OrderRecord, label: &'static str) {
    let pool = pool.clone();
    spawn_db(label, async move { upsert_order(&pool, &record).await });
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

/// Caller'ın WS `order` event payload'ından üretebileceği veriler.
/// `OrderRecord::from_user_ws` argüman patlamasını engellemek için bu struct
/// kullanılır.
pub struct WsOrderInput<'a> {
    pub bot_id: i64,
    pub market_session_id: i64,
    pub order_id: String,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub outcome: Option<String>,
    pub original_size: Option<f64>,
    pub size_matched: Option<f64>,
    pub price: Option<f64>,
    pub order_type: Option<String>,
    pub status: String,
    pub lifecycle_type: String,
    pub ts_ms: i64,
    pub raw: &'a serde_json::Value,
}

/// `OrderRecord::rest_placement` için input bag — REST POST /order yanıtından
/// türetilir.
pub struct RestPlacementInput {
    pub bot_id: i64,
    pub market_session_id: i64,
    pub market: String,
    pub order_id: String,
    pub asset_id: String,
    pub side: &'static str,
    pub outcome: &'static str,
    pub order_type: &'static str,
    pub price: f64,
    pub original_size: f64,
    pub size_matched: Option<f64>,
    pub post_status: String,
    pub ts_ms: i64,
}

impl OrderRecord {
    /// REST `POST /order` başarılı yanıtından `placement` satırı üretir.
    pub fn rest_placement(input: RestPlacementInput) -> Self {
        Self {
            order_id: input.order_id,
            bot_id: input.bot_id,
            market_session_id: Some(input.market_session_id),
            source: "rest_post".into(),
            lifecycle_type: Some("PLACEMENT".into()),
            market: Some(input.market),
            asset_id: Some(input.asset_id),
            side: Some(input.side.into()),
            price: Some(input.price),
            outcome: Some(input.outcome.into()),
            order_type: Some(input.order_type.into()),
            original_size: Some(input.original_size),
            size_matched: input.size_matched,
            expiration: None,
            associate_trades: None,
            post_status: Some(input.post_status),
            order_status: None,
            ts_ms: input.ts_ms,
            raw_payload: None,
            delete_canceled: None,
            delete_not_canceled: None,
        }
    }

    /// REST `DELETE /order` yanıtından `cancellation` satırı üretir.
    pub fn rest_cancellation(
        bot_id: i64,
        market_session_id: i64,
        market: String,
        order_id: String,
        canceled: &[String],
        not_canceled: &serde_json::Value,
        ts_ms: i64,
    ) -> Self {
        Self {
            order_id,
            bot_id,
            market_session_id: Some(market_session_id),
            source: "rest_delete".into(),
            lifecycle_type: Some("CANCELLATION".into()),
            market: Some(market),
            asset_id: None,
            side: None,
            price: None,
            outcome: None,
            order_type: None,
            original_size: None,
            size_matched: None,
            expiration: None,
            associate_trades: None,
            post_status: None,
            order_status: Some("CANCELED".into()),
            ts_ms,
            raw_payload: None,
            // `Vec<String>` serializasyonu CLOB DELETE çıktısı için sabit;
            // yapısal serde hatası oluşturmaz, Failure case'i yapısal bug demektir.
            delete_canceled: Some(
                serde_json::to_string(canceled).expect("Vec<String> serialization is infallible"),
            ),
            delete_not_canceled: Some(not_canceled.to_string()),
        }
    }

    /// User WS `order` event payload'ından satır üretir.
    pub fn from_user_ws(input: WsOrderInput<'_>) -> Self {
        let associate_trades = input.raw.get("associate_trades").map(|v| v.to_string());
        let raw_payload = input.raw.to_string();
        Self {
            order_id: input.order_id,
            bot_id: input.bot_id,
            market_session_id: Some(input.market_session_id),
            source: "user_ws".into(),
            lifecycle_type: Some(input.lifecycle_type),
            market: Some(input.market),
            asset_id: Some(input.asset_id),
            side: Some(input.side),
            price: input.price,
            outcome: input.outcome,
            order_type: input.order_type,
            original_size: input.original_size,
            size_matched: input.size_matched,
            expiration: None,
            associate_trades,
            post_status: None,
            order_status: Some(input.status),
            ts_ms: input.ts_ms,
            raw_payload: Some(raw_payload),
            delete_canceled: None,
            delete_not_canceled: None,
        }
    }
}
