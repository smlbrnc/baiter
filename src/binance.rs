//! Binance Spot aggTrade sinyal katmanı — 3 saniyelik kayan CVD penceresi.
//! `imbalance = (buy_vol − sell_vol) / (buy_vol + sell_vol)` ∈ [−1, +1].
//! Warmup tamamlanana kadar `imbalance = 0.0`, `warmup = true`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;

use crate::ipc;

/// Minimum işlem adedi; bu sayıya ulaşmadan `warmup = true`.
const WARMUP_TRADES: usize = 5;
/// CVD kayan pencere (saniye) — signal.md WINDOW_S.
const WINDOW_S: f64 = 3.0;
/// 60 sn frame yoksa WS ölü sayılır.
const FRAME_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceSignalState {
    /// `(buy_vol − sell_vol) / (buy_vol + sell_vol)` ∈ [−1, +1]; 0 = nötr.
    pub imbalance: f64,
    /// Ham CVD: `buy_vol − sell_vol` (lot cinsinden, pencere dahilinde).
    pub cvd: f64,
    /// `true` = warmup tamamlanmadı, sinyal güvenilmez.
    pub warmup: bool,
}

impl Default for BinanceSignalState {
    fn default() -> Self {
        Self { imbalance: 0.0, cvd: 0.0, warmup: true }
    }
}

pub type SharedSignalState = Arc<RwLock<BinanceSignalState>>;

pub fn new_shared_state() -> SharedSignalState {
    Arc::new(RwLock::new(BinanceSignalState::default()))
}

/// Binance USD-M Futures aggTrade payload (yalnızca tüketilen alanlar).
#[derive(Debug, Deserialize)]
struct AggTrade {
    #[serde(rename = "E")]
    event_time_ms: u64,
    #[serde(rename = "q")]
    qty: String,
    /// `true` = satıcı agresif (satış baskısı), `false` = alıcı agresif (alım baskısı).
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[derive(Clone, Copy)]
struct TradeEntry {
    ts_ms: u64,
    qty: f64,
    is_buy: bool,
}

/// 3 saniyelik kayan CVD hesaplayıcısı.
struct CvdEngine {
    window_ms: u64,
    trades: VecDeque<TradeEntry>,
    buy_vol: f64,
    sell_vol: f64,
    total_count: usize,
}

impl CvdEngine {
    fn new() -> Self {
        Self {
            window_ms: (WINDOW_S * 1000.0) as u64,
            trades: VecDeque::new(),
            buy_vol: 0.0,
            sell_vol: 0.0,
            total_count: 0,
        }
    }

    fn ingest(&mut self, ts_ms: u64, qty: f64, is_buy: bool) {
        self.trades.push_back(TradeEntry { ts_ms, qty, is_buy });
        if is_buy { self.buy_vol += qty; } else { self.sell_vol += qty; }
        self.total_count += 1;

        let cutoff = ts_ms.saturating_sub(self.window_ms);
        while let Some(front) = self.trades.front() {
            if front.ts_ms >= cutoff { break; }
            let e = self.trades.pop_front().unwrap();
            if e.is_buy { self.buy_vol = (self.buy_vol - e.qty).max(0.0); }
            else        { self.sell_vol = (self.sell_vol - e.qty).max(0.0); }
        }
    }

    fn snapshot(&self) -> (f64, f64, bool) {
        let total = self.buy_vol + self.sell_vol;
        let imbalance = if total > 0.0 { (self.buy_vol - self.sell_vol) / total } else { 0.0 };
        let cvd = self.buy_vol - self.sell_vol;
        let warmup = self.total_count < WARMUP_TRADES;
        (imbalance, cvd, warmup)
    }
}

/// Binance aggTrade WS görevi; kopuşta exponential backoff ile yeniden bağlanır,
/// disconnect süresince `imbalance = 0.0`, `warmup = true`.
pub async fn run_binance_signal(symbol: &str, state: SharedSignalState, bot_id: i64) {
    let url = format!("wss://stream.binance.com:9443/ws/{symbol}@aggTrade");
    let label = bot_id.to_string();
    let mut backoff = 1u64;
    loop {
        ipc::log_line(
            &label,
            format!("🛰️  Binance CVD ws bağlanıyor (symbol={symbol}) → {url}"),
        );
        match connect_stream(&url, &state, &label).await {
            Ok(()) => ipc::log_line(&label, format!("⚠️  Binance ws kapandı, {backoff}s içinde yeniden bağlanılacak")),
            Err(e) => ipc::log_line(&label, format!("❌ Binance ws hatası: {e} ({backoff}s içinde yeniden)")),
        }
        {
            let mut s = state.write().await;
            *s = BinanceSignalState::default();
        }
        sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

async fn connect_stream(
    url: &str,
    state: &SharedSignalState,
    label: &str,
) -> Result<(), anyhow::Error> {
    let (ws_stream, _) =
        match timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await {
            Ok(res) => res?,
            Err(_) => return Err(anyhow::anyhow!("connect_async timeout (10s)")),
        };
    let (mut write, mut read) = ws_stream.split();
    let mut engine = CvdEngine::new();
    let mut prev_warmup = true;

    loop {
        let next = match timeout(FRAME_IDLE_TIMEOUT, read.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => return Ok(()),
            Err(_) => return Err(anyhow::anyhow!(
                "binance ws idle > {}s (aggTrade frame yok)",
                FRAME_IDLE_TIMEOUT.as_secs()
            )),
        };
        let text = match next? {
            Message::Ping(payload) => { let _ = write.send(Message::Pong(payload)).await; continue; }
            Message::Close(_) => return Ok(()),
            Message::Text(t) => t,
            _ => continue,
        };

        let trade: AggTrade = match serde_json::from_str(&text) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let qty: f64 = match trade.qty.parse() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, qty=%trade.qty, "binance aggTrade qty parse başarısız");
                continue;
            }
        };

        // is_buyer_maker = true → satıcı agresif → SELL baskısı
        engine.ingest(trade.event_time_ms, qty, !trade.is_buyer_maker);
        let (imbalance, cvd, warmup) = engine.snapshot();
        {
            let mut s = state.write().await;
            s.imbalance = imbalance;
            s.cvd = cvd;
            s.warmup = warmup;
        }

        if prev_warmup && !warmup {
            prev_warmup = false;
            ipc::log_line(
                label,
                format!("🟢 Binance CVD warmup tamamlandı → imbalance={imbalance:+.3} cvd={cvd:.3}"),
            );
        }
    }
}
