//! `trades` tablosu CRUD'u.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;

use super::spawn_db;

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
}

/// Fire-and-forget kalıbı (§⚡ Kural 4).
pub fn persist_trade(pool: &SqlitePool, record: TradeRecord, label: &'static str) {
    let pool = pool.clone();
    spawn_db(label, async move { upsert_trade(&pool, &record).await });
}

pub async fn upsert_trade(pool: &SqlitePool, r: &TradeRecord) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO trades (trade_id, bot_id, market_session_id, market, asset_id, \
         taker_order_id, maker_orders, trader_side, side, outcome, size, price, status, fee, \
         ts_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(trade_id) DO UPDATE SET \
         status = excluded.status, \
         ts_ms = excluded.ts_ms",
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
    .execute(pool)
    .await?;
    Ok(())
}

/// `/api/bots/:id/sessions/:slug/trades` için: session bazlı trade listesi.
/// Sıralama: `ts_ms ASC` (chart marker'ları için kronolojik).
pub async fn trades_for_session(
    pool: &SqlitePool,
    market_session_id: i64,
    after_ts_ms: Option<i64>,
    limit: i64,
) -> Result<Vec<TradeRecord>, AppError> {
    let after = after_ts_ms.unwrap_or(0);
    let rows = sqlx::query(
        "SELECT trade_id, bot_id, market_session_id, market, asset_id, taker_order_id, \
         maker_orders, trader_side, side, outcome, size, price, status, fee, ts_ms \
         FROM trades \
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
        .map(|r| TradeRecord {
            trade_id: r.get("trade_id"),
            bot_id: r.get("bot_id"),
            market_session_id: r.get("market_session_id"),
            market: r.get("market"),
            asset_id: r.get("asset_id"),
            taker_order_id: r.get("taker_order_id"),
            maker_orders: r.get("maker_orders"),
            trader_side: r.get("trader_side"),
            side: r.get("side"),
            outcome: r.get("outcome"),
            size: r.get("size"),
            price: r.get("price"),
            status: r.get("status"),
            fee: r.get("fee"),
            ts_ms: r.get("ts_ms"),
        })
        .collect())
}

/// User WS `trade` event payload'ından satır üretmek için input bag.
/// Tipli alanlar — `event.rs::persist_trade` `TradePayload`'tan doğrudan
/// geçirir (raw JSON fishing yok).
pub struct WsTradeInput {
    pub bot_id: i64,
    pub market_session_id: i64,
    pub trade_id: String,
    pub market: String,
    pub asset_id: String,
    pub side: Option<String>,
    pub outcome: Option<String>,
    pub size: f64,
    pub price: f64,
    pub status: String,
    pub fee: f64,
    pub ts_ms: i64,
    pub taker_order_id: Option<String>,
    pub maker_orders_json: Option<String>,
    pub trader_side: Option<String>,
}

impl TradeRecord {
    pub fn from_user_ws(input: WsTradeInput) -> Self {
        Self {
            trade_id: input.trade_id,
            bot_id: input.bot_id,
            market_session_id: Some(input.market_session_id),
            market: Some(input.market),
            asset_id: Some(input.asset_id),
            taker_order_id: input.taker_order_id,
            maker_orders: input.maker_orders_json,
            trader_side: input.trader_side,
            side: input.side,
            outcome: input.outcome,
            size: input.size,
            price: input.price,
            status: input.status,
            fee: input.fee,
            ts_ms: input.ts_ms,
        }
    }
}
