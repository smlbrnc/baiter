//! Polymarket Market + User WebSocket istemcileri.
//!
//! - PING/PONG her 10 sn (resmi şart).
//! - Reconnect + exponential backoff.
//! - `mpsc::Sender<PolymarketEvent>` — §⚡ Kural 6 (WS okuyucu asla bloke olmaz).
//!
//! Referans: [docs/api/polymarket-clob.md §WebSocket](../../../docs/api/polymarket-clob.md).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::time::{interval, sleep};
use tokio_tungstenite::tungstenite::Message;

use crate::config::Credentials;
use crate::error::AppError;

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
    MarketResolved {
        market: String,
        winning_outcome: String,
        winning_asset_id: Option<String>,
        timestamp_ms: u64,
    },
    Order {
        order_id: String,
        market: String,
        asset_id: String,
        side: String,
        outcome: Option<String>,
        original_size: Option<f64>,
        size_matched: Option<f64>,
        price: Option<f64>,
        order_type: Option<String>,
        status: String,
        lifecycle_type: String,
        timestamp_ms: u64,
        /// Ham JSON; per-event `Value::clone` (deep) yerine `Arc::clone` (atomic
        /// inc) ile paylaşılır. DB writer arka planda `raw.to_string()` üretir.
        raw: Arc<Value>,
    },
    Trade {
        trade_id: String,
        market: String,
        asset_id: String,
        side: Option<String>,
        outcome: Option<String>,
        size: f64,
        price: f64,
        status: String,
        fee_rate_bps: Option<f64>,
        timestamp_ms: u64,
        raw: Arc<Value>,
    },
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
        // Item bir kez Arc'a sarılır; Order/Trade kollarında `Arc::clone`
        // (atomic inc), diğer kollarda `&Value` deref coercion ile kullanılır.
        let item = Arc::new(item);
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
        PolymarketEvent::MarketResolved { .. } => "market_resolved",
        PolymarketEvent::Order { .. } => "order",
        PolymarketEvent::Trade { .. } => "trade",
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
/// `event_type`'a göre küçük yardımcılara dağıtır; her yardımcı kendi
/// alanlarını zorunlu/opsiyonel olarak çözer.
fn map_event(v: &Arc<Value>) -> Option<PolymarketEvent> {
    let etype = v.get("event_type")?.as_str()?;
    match etype {
        "book" => map_book(v),
        "price_change" => map_price_change(v),
        "best_bid_ask" => map_best_bid_ask(v),
        "market_resolved" => map_market_resolved(v, as_u64(v, "timestamp").unwrap_or(0)),
        "order" => Some(map_order(v, as_u64(v, "timestamp").unwrap_or(0))),
        "trade" => Some(map_trade(v, as_u64(v, "timestamp").unwrap_or(0))),
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
    Some(PolymarketEvent::MarketResolved {
        market: as_str(v, "market")?,
        winning_outcome: as_str(v, "winning_outcome").unwrap_or_default(),
        winning_asset_id: as_str(v, "winning_asset_id"),
        timestamp_ms,
    })
}

fn map_order(v: &Arc<Value>, timestamp_ms: u64) -> PolymarketEvent {
    PolymarketEvent::Order {
        order_id: as_str(v, "id").unwrap_or_default(),
        market: as_str(v, "market").unwrap_or_default(),
        asset_id: as_str(v, "asset_id").unwrap_or_default(),
        side: as_str(v, "side").unwrap_or_default(),
        outcome: as_str(v, "outcome"),
        original_size: as_f64(v, "original_size"),
        size_matched: as_f64(v, "size_matched"),
        price: as_f64(v, "price"),
        order_type: as_str(v, "order_type"),
        status: as_str(v, "status").unwrap_or_default(),
        lifecycle_type: as_str(v, "type").unwrap_or_default(),
        timestamp_ms,
        raw: Arc::clone(v),
    }
}

fn map_trade(v: &Arc<Value>, timestamp_ms: u64) -> PolymarketEvent {
    PolymarketEvent::Trade {
        trade_id: as_str(v, "id").unwrap_or_default(),
        market: as_str(v, "market").unwrap_or_default(),
        asset_id: as_str(v, "asset_id").unwrap_or_default(),
        side: as_str(v, "side"),
        outcome: as_str(v, "outcome"),
        size: as_f64(v, "size").unwrap_or(0.0),
        price: as_f64(v, "price").unwrap_or(0.0),
        status: as_str(v, "status").unwrap_or_default(),
        fee_rate_bps: as_f64(v, "fee_rate_bps"),
        timestamp_ms,
        raw: Arc::clone(v),
    }
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
}
