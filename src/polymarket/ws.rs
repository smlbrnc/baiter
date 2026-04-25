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

    pub fn is_initial_match(self) -> bool {
        matches!(self, Self::Matched)
    }
}

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
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MakerOrder {
    pub order_id: String,
    pub asset_id: String,
    pub matched_amount: f64,
    pub price: f64,
    pub side: Side,
    pub owner: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TradePayload {
    pub trade_id: String,
    pub market: String,
    pub asset_id: String,
    pub side: Side,
    pub size: f64,
    pub price: f64,
    pub status: TradeStatus,
    pub taker_order_id: Option<String>,
    pub trader_side: Option<String>,
    pub maker_orders: Vec<MakerOrder>,
    /// Taker'ın API key UUID'si.
    pub owner: Option<String>,
    /// Taker outcome; maker fill'leri için `maker_orders[].outcome`.
    pub outcome: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone)]
pub struct OrderPayload {
    pub order_id: String,
    pub size_matched: Option<f64>,
    pub order_type: Option<OrderType>,
    pub status: String,
    pub lifecycle: OrderLifecycle,
}

#[derive(Debug, Clone)]
pub struct MarketResolvedPayload {
    pub market: String,
    pub winning_outcome: String,
    pub winning_asset_id: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone)]
pub enum PolymarketEvent {
    Book {
        asset_id: String,
        best_bid: f64,
        best_ask: f64,
        /// WS server ts (ms).
        timestamp_ms: u64,
    },
    PriceChange {
        changes: Vec<PriceChangeLevel>,
        timestamp_ms: u64,
    },
    BestBidAsk {
        asset_id: String,
        best_bid: f64,
        best_ask: f64,
        timestamp_ms: u64,
    },
    Trade(TradePayload),
    Order(OrderPayload),
    MarketResolved(MarketResolvedPayload),
}

#[derive(Debug, Clone)]
pub struct PriceChangeLevel {
    pub asset_id: String,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
}

/// Book/PriceChange/BestBidAsk → `book_tx` (silent drop); Trade/Order/Resolved → `event_tx` (warn).
#[derive(Clone)]
pub struct WsChannels {
    pub book_tx: mpsc::Sender<PolymarketEvent>,
    pub event_tx: mpsc::Sender<PolymarketEvent>,
}

pub async fn run_market_ws(
    base_ws: String,
    asset_ids: Vec<String>,
    chans: WsChannels,
) {
    let url = format!("{}/market", base_ws);
    let sub = serde_json::json!({
        "assets_ids": asset_ids,
        "type": "market",
        "custom_feature_enabled": true,
    });
    run_ws_loop(&url, sub, chans).await;
}

pub async fn run_user_ws(
    base_ws: String,
    creds: Credentials,
    markets: Vec<String>,
    chans: WsChannels,
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
    run_ws_loop(&url, sub, chans).await;
}

async fn run_ws_loop(url: &str, subscription: Value, chans: WsChannels) {
    let mut backoff_secs: u64 = 1;
    loop {
        match connect_and_stream(url, &subscription, &chans).await {
            Ok(()) => {
                tracing::warn!(url, "ws closed cleanly, reconnect in {backoff_secs}s");
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
    chans: &WsChannels,
) -> Result<(), AppError> {
    let (ws_stream, _resp) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| AppError::WebSocket(format!("connect: {e}")))?;
    let (mut write, mut read) = ws_stream.split();

    write
        .send(Message::Text(subscription.to_string().into()))
        .await
        .map_err(|e| AppError::WebSocket(format!("send sub: {e}")))?;

    let mut ping_tick = interval(Duration::from_secs(10));
    ping_tick.tick().await;

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
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
                        parse_and_dispatch(&t, chans);
                    }
                    Message::Binary(b) => {
                        if let Ok(s) = String::from_utf8(b.to_vec()) {
                            parse_and_dispatch(&s, chans);
                        }
                    }
                    Message::Ping(p) => {
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

fn parse_and_dispatch(text: &str, chans: &WsChannels) {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed == "{}"
        || trimmed.eq_ignore_ascii_case("pong")
        || trimmed.eq_ignore_ascii_case("ping")
    {
        return;
    }
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
            if !forward_event(chans, ev) {
                return;
            }
        }
    }
}

static EVENT_DROP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn forward_event(chans: &WsChannels, ev: PolymarketEvent) -> bool {
    let (tx, is_critical) = match &ev {
        PolymarketEvent::Trade(_)
        | PolymarketEvent::Order(_)
        | PolymarketEvent::MarketResolved(_) => (&chans.event_tx, true),
        PolymarketEvent::Book { .. }
        | PolymarketEvent::PriceChange { .. }
        | PolymarketEvent::BestBidAsk { .. } => (&chans.book_tx, false),
    };
    match tx.try_send(ev) {
        Ok(()) => true,
        Err(TrySendError::Full(dropped)) => {
            if is_critical {
                let total = EVENT_DROP_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(
                    drop_total = total,
                    kind = event_kind_label(&dropped),
                    "CRITICAL event channel full — drop will skew state"
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

fn map_event(v: &Value) -> Option<PolymarketEvent> {
    let etype = v.get("event_type")?.as_str()?;
    match etype {
        "book" => map_book(v),
        "price_change" => map_price_change(v),
        "best_bid_ask" => map_best_bid_ask(v),
        "market_resolved" => map_market_resolved(v),
        "order" => map_order(v),
        "trade" => map_trade(v, as_u64(v, "timestamp")?),
        "last_trade_price" | "new_market" => None,
        other => {
            tracing::debug!(event_type = other, "unknown event_type, skipped");
            None
        }
    }
}

fn map_book(v: &Value) -> Option<PolymarketEvent> {
    let asset_id = as_str(v, "asset_id")?;
    let best_bid = level_extreme(v, "bids", true)?;
    let best_ask = level_extreme(v, "asks", false)?;
    Some(PolymarketEvent::Book {
        asset_id,
        best_bid,
        best_ask,
        timestamp_ms: as_u64(v, "timestamp").unwrap_or(0),
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
    Some(PolymarketEvent::PriceChange {
        changes,
        timestamp_ms: as_u64(v, "timestamp").unwrap_or(0),
    })
}

fn map_best_bid_ask(v: &Value) -> Option<PolymarketEvent> {
    Some(PolymarketEvent::BestBidAsk {
        asset_id: as_str(v, "asset_id")?,
        best_bid: as_f64(v, "best_bid")?,
        best_ask: as_f64(v, "best_ask")?,
        timestamp_ms: as_u64(v, "timestamp").unwrap_or(0),
    })
}

fn map_market_resolved(v: &Value) -> Option<PolymarketEvent> {
    Some(PolymarketEvent::MarketResolved(MarketResolvedPayload {
        market: as_str(v, "market")?,
        winning_outcome: as_str(v, "winning_outcome")?,
        winning_asset_id: as_str(v, "winning_asset_id"),
        timestamp_ms: as_u64(v, "timestamp")?,
    }))
}

fn map_order(v: &Value) -> Option<PolymarketEvent> {
    Some(PolymarketEvent::Order(OrderPayload {
        order_id: as_str(v, "id")?,
        size_matched: as_f64(v, "size_matched"),
        order_type: as_str(v, "order_type").and_then(|s| OrderType::parse(&s)),
        status: as_str(v, "status")?,
        lifecycle: OrderLifecycle::parse(&as_str(v, "type")?)?,
    }))
}

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
        owner: as_str(v, "owner"),
        outcome: as_str(v, "outcome"),
        timestamp_ms,
    }))
}

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
                owner: as_str(m, "owner"),
                outcome: as_str(m, "outcome"),
            })
        })
        .collect()
}

fn level_extreme(v: &Value, key: &str, is_max: bool) -> Option<f64> {
    let arr = v.get(key)?.as_array()?;
    let mut best: Option<f64> = None;
    for lvl in arr {
        let Some(p) = lvl.get("price").and_then(|x| x.as_str()) else { continue };
        let Ok(px) = p.parse::<f64>() else { continue };
        best = Some(match best {
            None => px,
            Some(cur) if (is_max && px > cur) || (!is_max && px < cur) => px,
            Some(cur) => cur,
        });
    }
    best
}
