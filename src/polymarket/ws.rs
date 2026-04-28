//! Polymarket CLOB V2 market & user WebSocket dispatch.
//!
//! Hot path tasarımı:
//! * Mesaj parse'ı tek geçişlik typed deserialize (`serde(tag="event_type")`).
//! * Book/PriceChange/BestBidAsk → `book_tx` (silent drop on full).
//! * Trade/Order/MarketResolved → `event_tx` (warn on full, drop continues).
//! * Reconnect: 1s → 2s → … 60s exponential backoff.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
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

/// Book/PriceChange/BestBidAsk → `book_tx` (silent drop on full);
/// Trade/Order/Resolved → `event_tx` (warn on full).
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
    run_ws_loop(&url, sub.to_string(), chans).await;
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
    run_ws_loop(&url, sub.to_string(), chans).await;
}

async fn run_ws_loop(url: &str, subscription: String, chans: WsChannels) {
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
    subscription: &str,
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
                        if !parse_and_dispatch(t.as_str(), chans) {
                            return Ok(());
                        }
                    }
                    Message::Binary(b) => {
                        let s = std::str::from_utf8(&b).map_err(|e| {
                            AppError::WebSocket(format!("binary frame not utf-8: {e}"))
                        })?;
                        if !parse_and_dispatch(s, chans) {
                            return Ok(());
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

/// Mesajı parse eder ve event'leri dispatch eder.
/// Dönüş: `false` = downstream channel kapalı → outer loop'tan çık.
fn parse_and_dispatch(text: &str, chans: &WsChannels) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed == "{}"
        || trimmed.eq_ignore_ascii_case("pong")
        || trimmed.eq_ignore_ascii_case("ping")
    {
        return true;
    }

    if trimmed.starts_with('[') {
        let items: Vec<RawEvent> = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, raw=%trimmed, "ws parse failed");
                return true;
            }
        };
        for raw in items {
            if let Some(ev) = raw.into_event() {
                if !forward_event(chans, ev) {
                    return false;
                }
            }
        }
        true
    } else {
        let raw: RawEvent = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, raw=%trimmed, "ws parse failed");
                return true;
            }
        };
        match raw.into_event() {
            Some(ev) => forward_event(chans, ev),
            None => true,
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

// ---------------------------------------------------------------------------
// Wire-format typed deserialization
// ---------------------------------------------------------------------------

mod wire {
    use serde::Deserialize;

    /// String-encoded f64 helper (Polymarket fiyat/size alanlarını string yollar).
    pub fn de_f64_str<'de, D>(d: D) -> Result<f64, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = StringOrNumber::deserialize(d)?;
        raw.into_f64().map_err(serde::de::Error::custom)
    }

    pub fn de_opt_f64_str<'de, D>(d: D) -> Result<Option<f64>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw: Option<StringOrNumber> = Option::deserialize(d)?;
        match raw {
            None => Ok(None),
            Some(v) => v.into_f64().map(Some).map_err(serde::de::Error::custom),
        }
    }

    pub fn de_u64_str<'de, D>(d: D) -> Result<u64, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = StringOrNumber::deserialize(d)?;
        raw.into_u64().map_err(serde::de::Error::custom)
    }

    pub fn de_opt_u64_str<'de, D>(d: D) -> Result<Option<u64>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw: Option<StringOrNumber> = Option::deserialize(d)?;
        match raw {
            None => Ok(None),
            Some(v) => v.into_u64().map(Some).map_err(serde::de::Error::custom),
        }
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        S(String),
        N(serde_json::Number),
    }

    impl StringOrNumber {
        fn into_f64(self) -> Result<f64, String> {
            match self {
                Self::S(s) => s.parse().map_err(|e| format!("f64 parse: {e}")),
                Self::N(n) => n.as_f64().ok_or_else(|| "number not f64".to_string()),
            }
        }
        fn into_u64(self) -> Result<u64, String> {
            match self {
                Self::S(s) => s.parse().map_err(|e| format!("u64 parse: {e}")),
                Self::N(n) => n.as_u64().ok_or_else(|| "number not u64".to_string()),
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "event_type")]
enum RawEvent {
    #[serde(rename = "book")]
    Book(BookFrame),
    #[serde(rename = "price_change")]
    PriceChange(PriceChangeFrame),
    #[serde(rename = "best_bid_ask")]
    BestBidAsk(BestBidAskFrame),
    #[serde(rename = "trade")]
    Trade(TradeFrame),
    #[serde(rename = "order")]
    Order(OrderFrame),
    #[serde(rename = "market_resolved")]
    MarketResolved(MarketResolvedFrame),
    #[serde(rename = "last_trade_price")]
    LastTradePrice,
    #[serde(rename = "new_market")]
    NewMarket,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct BookFrame {
    asset_id: String,
    #[serde(default)]
    bids: Vec<BookLevel>,
    #[serde(default)]
    asks: Vec<BookLevel>,
    #[serde(default, deserialize_with = "wire::de_opt_u64_str")]
    timestamp: Option<u64>,
}

#[derive(Deserialize)]
struct BookLevel {
    #[serde(deserialize_with = "wire::de_f64_str")]
    price: f64,
}

#[derive(Deserialize)]
struct PriceChangeFrame {
    #[serde(default)]
    price_changes: Vec<PriceChangeRow>,
    #[serde(default, deserialize_with = "wire::de_opt_u64_str")]
    timestamp: Option<u64>,
}

#[derive(Deserialize)]
struct PriceChangeRow {
    asset_id: String,
    #[serde(default, deserialize_with = "wire::de_opt_f64_str")]
    best_bid: Option<f64>,
    #[serde(default, deserialize_with = "wire::de_opt_f64_str")]
    best_ask: Option<f64>,
}

#[derive(Deserialize)]
struct BestBidAskFrame {
    asset_id: String,
    #[serde(deserialize_with = "wire::de_f64_str")]
    best_bid: f64,
    #[serde(deserialize_with = "wire::de_f64_str")]
    best_ask: f64,
    #[serde(default, deserialize_with = "wire::de_opt_u64_str")]
    timestamp: Option<u64>,
}

#[derive(Deserialize)]
struct TradeFrame {
    id: String,
    market: String,
    asset_id: String,
    side: String,
    #[serde(deserialize_with = "wire::de_f64_str")]
    size: f64,
    #[serde(deserialize_with = "wire::de_f64_str")]
    price: f64,
    status: String,
    #[serde(default)]
    taker_order_id: Option<String>,
    #[serde(default)]
    trader_side: Option<String>,
    #[serde(default)]
    maker_orders: Vec<MakerOrderFrame>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(deserialize_with = "wire::de_u64_str")]
    timestamp: u64,
}

#[derive(Deserialize)]
struct MakerOrderFrame {
    order_id: String,
    asset_id: String,
    #[serde(deserialize_with = "wire::de_f64_str")]
    matched_amount: f64,
    #[serde(deserialize_with = "wire::de_f64_str")]
    price: f64,
    side: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
}

#[derive(Deserialize)]
struct OrderFrame {
    id: String,
    #[serde(default, deserialize_with = "wire::de_opt_f64_str")]
    size_matched: Option<f64>,
    #[serde(default)]
    order_type: Option<String>,
    status: String,
    #[serde(rename = "type")]
    lifecycle: String,
}

#[derive(Deserialize)]
struct MarketResolvedFrame {
    market: String,
    winning_outcome: String,
    #[serde(default)]
    winning_asset_id: Option<String>,
    #[serde(deserialize_with = "wire::de_u64_str")]
    timestamp: u64,
}

impl RawEvent {
    fn into_event(self) -> Option<PolymarketEvent> {
        match self {
            Self::Book(b) => b.into_event(),
            Self::PriceChange(p) => Some(p.into_event()),
            Self::BestBidAsk(b) => Some(b.into_event()),
            Self::Trade(t) => t.into_event(),
            Self::Order(o) => o.into_event(),
            Self::MarketResolved(m) => Some(m.into_event()),
            Self::LastTradePrice | Self::NewMarket => None,
            Self::Other => {
                tracing::trace!("ws unknown event_type, skipped");
                None
            }
        }
    }
}

impl BookFrame {
    fn into_event(self) -> Option<PolymarketEvent> {
        let best_bid = self.bids.first()?.price;
        let best_ask = self.asks.first()?.price;
        Some(PolymarketEvent::Book {
            asset_id: self.asset_id,
            best_bid,
            best_ask,
            timestamp_ms: self.timestamp.unwrap_or_default(),
        })
    }
}

impl PriceChangeFrame {
    fn into_event(self) -> PolymarketEvent {
        let changes = self
            .price_changes
            .into_iter()
            .map(|r| PriceChangeLevel {
                asset_id: r.asset_id,
                best_bid: r.best_bid,
                best_ask: r.best_ask,
            })
            .collect();
        PolymarketEvent::PriceChange {
            changes,
            timestamp_ms: self.timestamp.unwrap_or_default(),
        }
    }
}

impl BestBidAskFrame {
    fn into_event(self) -> PolymarketEvent {
        PolymarketEvent::BestBidAsk {
            asset_id: self.asset_id,
            best_bid: self.best_bid,
            best_ask: self.best_ask,
            timestamp_ms: self.timestamp.unwrap_or_default(),
        }
    }
}

impl TradeFrame {
    fn into_event(self) -> Option<PolymarketEvent> {
        let side = Side::parse(&self.side)?;
        let status = TradeStatus::parse(&self.status)?;
        let maker_orders = self
            .maker_orders
            .into_iter()
            .filter_map(|m| {
                Some(MakerOrder {
                    order_id: m.order_id,
                    asset_id: m.asset_id,
                    matched_amount: m.matched_amount,
                    price: m.price,
                    side: Side::parse(&m.side)?,
                    owner: m.owner,
                    outcome: m.outcome,
                })
            })
            .collect();
        Some(PolymarketEvent::Trade(TradePayload {
            trade_id: self.id,
            market: self.market,
            asset_id: self.asset_id,
            side,
            size: self.size,
            price: self.price,
            status,
            taker_order_id: self.taker_order_id,
            trader_side: self.trader_side,
            maker_orders,
            owner: self.owner,
            outcome: self.outcome,
            timestamp_ms: self.timestamp,
        }))
    }
}

impl OrderFrame {
    fn into_event(self) -> Option<PolymarketEvent> {
        Some(PolymarketEvent::Order(OrderPayload {
            order_id: self.id,
            size_matched: self.size_matched,
            order_type: self.order_type.as_deref().and_then(OrderType::parse),
            status: self.status,
            lifecycle: OrderLifecycle::parse(&self.lifecycle)?,
        }))
    }
}

impl MarketResolvedFrame {
    fn into_event(self) -> PolymarketEvent {
        PolymarketEvent::MarketResolved(MarketResolvedPayload {
            market: self.market,
            winning_outcome: self.winning_outcome,
            winning_asset_id: self.winning_asset_id,
            timestamp_ms: self.timestamp,
        })
    }
}
