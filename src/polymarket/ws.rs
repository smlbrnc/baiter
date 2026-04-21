//! Polymarket Market + User WebSocket istemcileri.
//!
//! - PING/PONG her 10 sn (resmi şart).
//! - Reconnect + exponential backoff.
//! - `mpsc::Sender<PolymarketEvent>` — §⚡ Kural 6 (WS okuyucu asla bloke olmaz).
//!
//! Referans: [docs/api/polymarket-clob.md §WebSocket](../../../docs/api/polymarket-clob.md).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::time::{interval, sleep};
use tokio_tungstenite::tungstenite::Message;

use crate::config::Credentials;
use crate::error::AppError;
use crate::types::{OrderType, Side};

/// Polymarket User Channel `trade` event status.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.
/// Lifecycle: `MATCHED → MINED → CONFIRMED` (CONFIRMED terminal);
/// `RETRYING` ara, `FAILED` terminal. Fill attribution **sadece ilk MATCHED**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeStatus {
    Matched,
    Mined,
    Confirmed,
    Retrying,
    Failed,
}

impl TradeStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "MATCHED" => Some(Self::Matched),
            "MINED" => Some(Self::Mined),
            "CONFIRMED" => Some(Self::Confirmed),
            "RETRYING" => Some(Self::Retrying),
            "FAILED" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "MATCHED",
            Self::Mined => "MINED",
            Self::Confirmed => "CONFIRMED",
            Self::Retrying => "RETRYING",
            Self::Failed => "FAILED",
        }
    }

    /// Sadece ilk `MATCHED`'de fill attribution yapılır; `MINED/CONFIRMED`
    /// upsert sadece status/ts/raw günceller.
    pub fn is_initial_match(self) -> bool {
        matches!(self, Self::Matched)
    }
}

/// Polymarket User Channel `order` event lifecycle (`type` field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderLifecycle {
    Placement,
    Update,
    Cancellation,
}

impl OrderLifecycle {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "PLACEMENT" => Some(Self::Placement),
            "UPDATE" => Some(Self::Update),
            "CANCELLATION" => Some(Self::Cancellation),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Placement => "PLACEMENT",
            Self::Update => "UPDATE",
            Self::Cancellation => "CANCELLATION",
        }
    }
}

/// User Channel `trade.maker_orders[]` entry.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.
/// Per-entry `fee_rate_bps`/`outcome`/`owner` payload alanları okunmaz —
/// fee politikası `MarketSession.fee_rate_bps` (CLOB `GET /fee-rate`) tek
/// otoritedir; outcome `outcome_from_asset_id` mapping'inden çıkarılır.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MakerOrder {
    pub order_id: String,
    pub asset_id: String,
    pub matched_amount: f64,
    pub price: f64,
    pub side: Side,
}

/// User Channel `trade` event tipli payload.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.
/// Per-trade `fee_rate_bps` payload alanı parse edilmez — `MarketSession`
/// CLOB `GET /fee-rate?token_id=` ile fetch edip tek otorite olarak tutar.
/// `outcome` payload string'i parse edilmez — `outcome_from_asset_id(sess,
/// asset_id)` mapping'i tek otoritedir.
#[derive(Debug, Clone)]
pub struct TradePayload {
    pub trade_id: String,
    pub market: String,
    pub asset_id: String,
    pub side: Side,
    pub size: f64,
    pub price: f64,
    pub status: TradeStatus,
    /// Sadece taker fill içeren event'lerde set edilir (User Channel spec).
    pub taker_order_id: Option<String>,
    /// "TAKER" | "MAKER" — User Channel spec'te bu trade'i tetikleyen tarafımızın
    /// rolü; UI sembol etiketinde gösterilir. Per-fill rol `extract_fills`'te
    /// `maker_orders` üyeliğinden türetilir.
    pub trader_side: Option<String>,
    pub maker_orders: Vec<MakerOrder>,
    pub timestamp_ms: u64,
}

/// User Channel `order` event tipli payload.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.
/// `original_size/size_matched/price/order_type` `UPDATE` lifecycle'da bazıları
/// boş gelebilir; bu yüzden `Option`. `outcome` redundant (asset_id'den çıkarılır).
#[derive(Debug, Clone)]
pub struct OrderPayload {
    pub order_id: String,
    pub market: String,
    pub asset_id: String,
    pub side: Side,
    pub original_size: Option<f64>,
    pub size_matched: Option<f64>,
    pub price: Option<f64>,
    pub order_type: Option<OrderType>,
    pub status: String,
    pub lifecycle: OrderLifecycle,
    pub timestamp_ms: u64,
}

/// `market_resolved` event tipli payload.
#[derive(Debug, Clone)]
pub struct MarketResolvedPayload {
    pub market: String,
    pub winning_outcome: String,
    /// Pre-resolved event'te eksik olabilir.
    pub winning_asset_id: Option<String>,
    pub timestamp_ms: u64,
}

/// Engine/strateji tarafına iletilen tek tip Polymarket event'i.
/// Yalnız in-process (mpsc) — JSON serialize/deserialize edilmez.
#[derive(Debug, Clone)]
pub enum PolymarketEvent {
    Book {
        asset_id: String,
        /// Yalnız fiyat (size tüketici tarafında okunmuyordu); per-level
        /// String alloc'u sıfırlamak için `Vec<f64>` olarak taşınır.
        bids: Vec<f64>,
        asks: Vec<f64>,
    },
    PriceChange {
        changes: Vec<PriceChangeLevel>,
    },
    BestBidAsk {
        asset_id: String,
        best_bid: f64,
        best_ask: f64,
    },
    Trade(TradePayload),
    Order(OrderPayload),
    MarketResolved(MarketResolvedPayload),
}

/// `price_change` event'inde her seviye için yalnız best_bid/best_ask delta'sı
/// taşınır; `price/size/side/hash` field'ları tüketici tarafında okunmuyordu.
#[derive(Debug, Clone)]
pub struct PriceChangeLevel {
    pub asset_id: String,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
}

/// Market WS okuyucu task'ı — mpsc'ye event yayar.
/// Kopma halinde exponential backoff (1s → 60s) ile yeniden bağlanır.
pub async fn run_market_ws(
    base_ws: String,
    asset_ids: Vec<String>,
    tx: mpsc::Sender<PolymarketEvent>,
) {
    let url = format!("{}/market", base_ws);
    let sub = serde_json::json!({
        "assets_ids": asset_ids,
        "type": "market",
        "custom_feature_enabled": true,
    });
    run_ws_loop(&url, sub, tx).await;
}

/// User WS okuyucu task'ı.
pub async fn run_user_ws(
    base_ws: String,
    creds: Credentials,
    markets: Vec<String>,
    tx: mpsc::Sender<PolymarketEvent>,
) {
    let url = format!("{}/user", base_ws);
    let sub = serde_json::json!({
        "auth": {
            "apiKey": creds.poly_api_key,
            "secret": creds.poly_secret,
            "passphrase": creds.poly_passphrase,
        },
        "markets": markets,
        "type": "user",
    });
    run_ws_loop(&url, sub, tx).await;
}

async fn run_ws_loop(url: &str, subscription: Value, tx: mpsc::Sender<PolymarketEvent>) {
    let mut backoff_secs: u64 = 1;
    loop {
        match connect_and_stream(url, &subscription, &tx).await {
            Ok(()) => {
                tracing::warn!(url, "ws closed cleanly, reconnect in {backoff_secs}s");
                // Temiz kapanışta backoff sıfırlanır (uzun süreli sağlıklı sessionun
                // ardından bir sonraki bağlanma denemesi gecikmesin).
                backoff_secs = 1;
            }
            Err(e) => {
                tracing::error!(url, error=%e, "ws error, reconnect in {backoff_secs}s");
            }
        }
        sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(60);
    }
}

async fn connect_and_stream(
    url: &str,
    subscription: &Value,
    tx: &mpsc::Sender<PolymarketEvent>,
) -> Result<(), AppError> {
    let (ws_stream, _resp) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| AppError::WebSocket(format!("connect: {e}")))?;
    let (mut write, mut read) = ws_stream.split();

    // Initial subscription
    write
        .send(Message::Text(subscription.to_string().into()))
        .await
        .map_err(|e| AppError::WebSocket(format!("send sub: {e}")))?;

    // PING every 10s
    let mut ping_tick = interval(Duration::from_secs(10));
    ping_tick.tick().await; // ilk tick'i hemen tüket

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                // Polymarket CLOB WS: text "PING" bekler, yanıtı "PONG".
                if write.send(Message::Text("PING".to_string().into())).await.is_err() {
                    return Err(AppError::WebSocket("ping send failed".to_string()));
                }
            }
            msg = read.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(AppError::WebSocket(format!("read: {e}"))),
                    None => return Err(AppError::WebSocket("stream closed".to_string())),
                };
                match msg {
                    Message::Text(t) => {
                        parse_and_dispatch(&t, tx);
                    }
                    Message::Binary(b) => {
                        if let Ok(s) = String::from_utf8(b.to_vec()) {
                            parse_and_dispatch(&s, tx);
                        }
                    }
                    Message::Ping(p) => {
                        // Pong gönderilemezse bağlantı zaten bozulmuş — döngüden
                        // çık ve dış reconnect'e yetki ver.
                        if write.send(Message::Pong(p)).await.is_err() {
                            return Err(AppError::WebSocket("pong send failed".to_string()));
                        }
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

fn parse_and_dispatch(text: &str, tx: &mpsc::Sender<PolymarketEvent>) {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed == "{}"
        || trimmed.eq_ignore_ascii_case("pong")
        || trimmed.eq_ignore_ascii_case("ping")
    {
        return;
    }
    // Server batch array veya tek obje dönebilir.
    let value: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error=%e, raw=%trimmed, "ws parse failed");
            return;
        }
    };
    let items: Vec<Value> = match value {
        Value::Array(arr) => arr,
        other => vec![other],
    };
    for item in items {
        if let Some(ev) = map_event(&item) {
            if !forward_event(tx, ev) {
                return; // receiver dropped
            }
        }
    }
}

/// Toplam drop edilen WS event sayısı (process-wide). Her 100 drop'ta bir
/// `tracing::warn!` ile özet log atılır; tek tek warn spam'i önlenir.
static DROP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// ⚡ Kural 6: WS okuyucu mpsc kanalda asla bloke olmaz. `try_send` kullanır;
/// kanal doluysa event drop edilir ve `DROP_COUNTER` artırılır. Receiver
/// dropped ise `false` döner ve caller döngüden çıkar.
fn forward_event(tx: &mpsc::Sender<PolymarketEvent>, ev: PolymarketEvent) -> bool {
    match tx.try_send(ev) {
        Ok(()) => true,
        Err(TrySendError::Full(dropped)) => {
            let total = DROP_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
            if total.is_multiple_of(100) {
                tracing::warn!(
                    drop_total = total,
                    last_kind = event_kind_label(&dropped),
                    "ws event channel full — drop summary (every 100 drops)"
                );
            }
            true
        }
        Err(TrySendError::Closed(_)) => false,
    }
}

fn event_kind_label(ev: &PolymarketEvent) -> &'static str {
    match ev {
        PolymarketEvent::Book { .. } => "book",
        PolymarketEvent::PriceChange { .. } => "price_change",
        PolymarketEvent::BestBidAsk { .. } => "best_bid_ask",
        PolymarketEvent::MarketResolved(_) => "market_resolved",
        PolymarketEvent::Order(_) => "order",
        PolymarketEvent::Trade(_) => "trade",
    }
}

fn as_f64(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| match x {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_f64(),
        _ => None,
    })
}

fn as_u64(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|x| match x {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    })
}

fn as_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// Tek bir JSON event objesini `PolymarketEvent`'e map'ler.
///
/// `event_type`'a göre küçük yardımcılara dağıtır; her yardımcı **parse-or-skip**:
/// zorunlu alan eksik/invalid → `None` ve event drop (defensive `unwrap_or`
/// fallback yok).
fn map_event(v: &Value) -> Option<PolymarketEvent> {
    let etype = v.get("event_type")?.as_str()?;
    let ts = as_u64(v, "timestamp").unwrap_or(0);
    match etype {
        "book" => map_book(v),
        "price_change" => map_price_change(v),
        "best_bid_ask" => map_best_bid_ask(v),
        "market_resolved" => map_market_resolved(v, ts),
        "order" => map_order(v, ts),
        "trade" => map_trade(v, ts),
        other => {
            tracing::debug!(event_type = other, "unknown event_type, skipped");
            None
        }
    }
}

fn map_book(v: &Value) -> Option<PolymarketEvent> {
    Some(PolymarketEvent::Book {
        asset_id: as_str(v, "asset_id")?,
        bids: extract_levels(v, "bids"),
        asks: extract_levels(v, "asks"),
    })
}

fn map_price_change(v: &Value) -> Option<PolymarketEvent> {
    let arr = v.get("price_changes")?.as_array()?;
    let changes = arr
        .iter()
        .filter_map(|c| {
            Some(PriceChangeLevel {
                asset_id: as_str(c, "asset_id")?,
                best_bid: as_f64(c, "best_bid"),
                best_ask: as_f64(c, "best_ask"),
            })
        })
        .collect();
    Some(PolymarketEvent::PriceChange { changes })
}

fn map_best_bid_ask(v: &Value) -> Option<PolymarketEvent> {
    Some(PolymarketEvent::BestBidAsk {
        asset_id: as_str(v, "asset_id")?,
        best_bid: as_f64(v, "best_bid")?,
        best_ask: as_f64(v, "best_ask")?,
    })
}

fn map_market_resolved(v: &Value, timestamp_ms: u64) -> Option<PolymarketEvent> {
    Some(PolymarketEvent::MarketResolved(MarketResolvedPayload {
        market: as_str(v, "market")?,
        winning_outcome: as_str(v, "winning_outcome")?,
        winning_asset_id: as_str(v, "winning_asset_id"),
        timestamp_ms,
    }))
}

/// Parse-or-skip: `id/market/asset_id/side/type` zorunlu — biri eksik/invalid
/// ise event drop edilir. `order_type` opsiyonel; invalid string `None` döner
/// (event drop edilmez).
fn map_order(v: &Value, timestamp_ms: u64) -> Option<PolymarketEvent> {
    let order_type = as_str(v, "order_type").and_then(|s| OrderType::parse(&s));
    Some(PolymarketEvent::Order(OrderPayload {
        order_id: as_str(v, "id")?,
        market: as_str(v, "market")?,
        asset_id: as_str(v, "asset_id")?,
        side: Side::parse(&as_str(v, "side")?)?,
        original_size: as_f64(v, "original_size"),
        size_matched: as_f64(v, "size_matched"),
        price: as_f64(v, "price"),
        order_type,
        status: as_str(v, "status")?,
        lifecycle: OrderLifecycle::parse(&as_str(v, "type")?)?,
        timestamp_ms,
    }))
}

/// Parse-or-skip: `id/market/asset_id/side/size/price/status` zorunlu —
/// biri eksik/invalid ise event drop edilir. Per-trade `fee_rate_bps` /
/// `outcome` payload alanları parse edilmez (sırasıyla `MarketSession.fee_rate_bps`
/// ve `outcome_from_asset_id` mapping'i tek otoritedir).
fn map_trade(v: &Value, timestamp_ms: u64) -> Option<PolymarketEvent> {
    Some(PolymarketEvent::Trade(TradePayload {
        trade_id: as_str(v, "id")?,
        market: as_str(v, "market")?,
        asset_id: as_str(v, "asset_id")?,
        side: Side::parse(&as_str(v, "side")?)?,
        size: as_f64(v, "size")?,
        price: as_f64(v, "price")?,
        status: TradeStatus::parse(&as_str(v, "status")?)?,
        taker_order_id: as_str(v, "taker_order_id"),
        trader_side: as_str(v, "trader_side"),
        maker_orders: parse_maker_orders(v.get("maker_orders")),
        timestamp_ms,
    }))
}

/// `maker_orders[]`'ı tipli `MakerOrder` listesine map'ler. Bir entry'de
/// zorunlu alan eksik/invalid ise sessiz drop (entry dışındakiler korunur).
fn parse_maker_orders(v: Option<&Value>) -> Vec<MakerOrder> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            Some(MakerOrder {
                order_id: as_str(m, "order_id")?,
                asset_id: as_str(m, "asset_id")?,
                matched_amount: as_f64(m, "matched_amount")?,
                price: as_f64(m, "price")?,
                side: Side::parse(&as_str(m, "side")?)?,
            })
        })
        .collect()
}

fn extract_levels(v: &Value, key: &str) -> Vec<f64> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|lvl| lvl.get("price")?.as_str()?.parse::<f64>().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn maps_best_bid_ask() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "best_bid_ask",
            "asset_id": "abc",
            "best_bid": "0.73",
            "best_ask": "0.77",
        }));
        let ev = map_event(&raw).unwrap();
        match ev {
            PolymarketEvent::BestBidAsk {
                best_bid, best_ask, ..
            } => {
                assert!((best_bid - 0.73).abs() < 1e-9);
                assert!((best_ask - 0.77).abs() < 1e-9);
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn maps_book() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "book",
            "asset_id": "abc",
            "market": "0x1",
            "bids": [{"price":"0.48","size":"30"}],
            "asks": [{"price":"0.52","size":"25"}],
            "timestamp": "100"
        }));
        let ev = map_event(&raw).unwrap();
        match ev {
            PolymarketEvent::Book { bids, asks, .. } => {
                assert_eq!(bids.len(), 1);
                assert!((asks[0] - 0.52).abs() < 1e-9);
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn unknown_event_skipped() {
        let raw = Arc::new(serde_json::json!({"event_type": "banana"}));
        assert!(map_event(&raw).is_none());
    }

    #[test]
    fn maps_trade_with_maker_orders() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "trade",
            "id": "T1",
            "market": "0xMKT",
            "asset_id": "UP_TOKEN",
            "side": "BUY",
            "size": "10",
            "price": "0.42",
            "status": "MATCHED",
            "taker_order_id": "0xTAKER",
            "maker_orders": [
                {"order_id": "0xM1", "asset_id": "UP_TOKEN", "matched_amount": "6", "price": "0.42", "side": "SELL"},
                {"order_id": "0xM2", "asset_id": "UP_TOKEN", "matched_amount": "4", "price": "0.43", "side": "SELL"}
            ],
            "timestamp": "12345"
        }));
        match map_event(&raw).unwrap() {
            PolymarketEvent::Trade(t) => {
                assert_eq!(t.trade_id, "T1");
                assert_eq!(t.side, Side::Buy);
                assert!((t.size - 10.0).abs() < 1e-9);
                assert!((t.price - 0.42).abs() < 1e-9);
                assert_eq!(t.status, TradeStatus::Matched);
                assert_eq!(t.taker_order_id.as_deref(), Some("0xTAKER"));
                assert_eq!(t.maker_orders.len(), 2);
                assert_eq!(t.maker_orders[0].order_id, "0xM1");
                assert!((t.maker_orders[1].matched_amount - 4.0).abs() < 1e-9);
                assert_eq!(t.maker_orders[1].side, Side::Sell);
                assert_eq!(t.timestamp_ms, 12345);
            }
            _ => panic!("expected Trade"),
        }
    }

    #[test]
    fn maps_trade_status_variants() {
        let mk = |status: &str| {
            Arc::new(serde_json::json!({
                "event_type": "trade",
                "id": "T", "market": "M", "asset_id": "A",
                "side": "BUY", "size": "1", "price": "0.5",
                "status": status,
            }))
        };
        for (s, expected) in [
            ("MATCHED", TradeStatus::Matched),
            ("MINED", TradeStatus::Mined),
            ("CONFIRMED", TradeStatus::Confirmed),
            ("RETRYING", TradeStatus::Retrying),
            ("FAILED", TradeStatus::Failed),
        ] {
            match map_event(&mk(s)).unwrap() {
                PolymarketEvent::Trade(t) => assert_eq!(t.status, expected),
                _ => panic!("expected Trade"),
            }
        }
    }

    #[test]
    fn maps_order_lifecycle_variants() {
        let mk = |t: &str| {
            Arc::new(serde_json::json!({
                "event_type": "order",
                "id": "O", "market": "M", "asset_id": "A",
                "side": "BUY", "type": t, "status": "LIVE",
            }))
        };
        for (t, expected) in [
            ("PLACEMENT", OrderLifecycle::Placement),
            ("UPDATE", OrderLifecycle::Update),
            ("CANCELLATION", OrderLifecycle::Cancellation),
        ] {
            match map_event(&mk(t)).unwrap() {
                PolymarketEvent::Order(o) => assert_eq!(o.lifecycle, expected),
                _ => panic!("expected Order"),
            }
        }
    }

    #[test]
    fn drop_invalid_trade_payload_missing_side() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "trade",
            "id": "T", "market": "M", "asset_id": "A",
            "size": "1", "price": "0.5", "status": "MATCHED",
        }));
        assert!(map_event(&raw).is_none());
    }

    #[test]
    fn drop_invalid_trade_payload_unknown_status() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "trade",
            "id": "T", "market": "M", "asset_id": "A",
            "side": "BUY", "size": "1", "price": "0.5",
            "status": "GHOST",
        }));
        assert!(map_event(&raw).is_none());
    }

    #[test]
    fn drop_invalid_order_payload_missing_lifecycle() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "order",
            "id": "O", "market": "M", "asset_id": "A",
            "side": "BUY", "status": "LIVE",
        }));
        assert!(map_event(&raw).is_none());
    }

    #[test]
    fn drop_invalid_order_payload_missing_status() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "order",
            "id": "O", "market": "M", "asset_id": "A",
            "side": "BUY", "type": "PLACEMENT",
        }));
        assert!(map_event(&raw).is_none());
    }

    #[test]
    fn drop_invalid_maker_order_entry_skipped() {
        let raw = Arc::new(serde_json::json!({
            "event_type": "trade",
            "id": "T", "market": "M", "asset_id": "A",
            "side": "BUY", "size": "1", "price": "0.5", "status": "MATCHED",
            "maker_orders": [
                {"order_id": "good", "asset_id": "A", "matched_amount": "1", "price": "0.5", "side": "SELL"},
                {"order_id": "bad_no_side", "asset_id": "A", "matched_amount": "1", "price": "0.5"}
            ],
        }));
        match map_event(&raw).unwrap() {
            PolymarketEvent::Trade(t) => {
                assert_eq!(t.maker_orders.len(), 1);
                assert_eq!(t.maker_orders[0].order_id, "good");
            }
            _ => panic!("expected Trade"),
        }
    }
}
