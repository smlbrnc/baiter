//! Binance USD-M Futures aggTrade sinyal katmanı (§14): sliding-window CVD +
//! Hawkes BSI + OFI → `signal_score` ∈ [0, 10]. Warmup ve bağlantı koptuğunda
//! nötr `5.0`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;

use crate::ipc;
use crate::slug::Interval;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceSignalState {
    pub cvd: f64,
    pub bsi: f64,
    pub ofi: f64,
    pub signal_score: f64,
}

impl Default for BinanceSignalState {
    fn default() -> Self {
        Self {
            cvd: 0.0,
            bsi: 0.0,
            ofi: 0.0,
            signal_score: 5.0,
        }
    }
}

pub type SharedSignalState = Arc<RwLock<BinanceSignalState>>;

pub fn new_shared_state() -> SharedSignalState {
    Arc::new(RwLock::new(BinanceSignalState::default()))
}

/// CVD kayan penceresi (§14.2).
fn cvd_window_secs(interval: Interval) -> u64 {
    match interval {
        Interval::M5 => 60,
        Interval::M15 => 180,
        Interval::H1 => 600,
        Interval::H4 => 1800,
    }
}

/// Binance USD-M Futures aggTrade payload (yalnızca tüketilen alanlar).
#[derive(Debug, Clone, Deserialize)]
struct AggTrade {
    #[serde(rename = "E")]
    event_time_ms: u64,
    #[serde(rename = "q")]
    qty: String,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[derive(Debug, Clone, Copy)]
struct TradeEntry {
    ts_ms: u64,
    delta: f64,
    is_buy: bool,
}

const NEUTRAL: f64 = 5.0;
const MAX_STATS: usize = 300;
const WARMUP_TRADES: usize = 30;
const HAWKES_KAPPA: f64 = 0.1;

pub struct SignalComputer {
    window_ms: u64,
    window_trades: VecDeque<TradeEntry>,
    cvd: f64,
    buy_count: u64,
    sell_count: u64,
    bsi: f64,
    last_ts_ms: Option<u64>,
    ofi_history: VecDeque<f64>,
}

impl SignalComputer {
    pub fn new(interval: Interval) -> Self {
        Self {
            window_ms: cvd_window_secs(interval) * 1000,
            window_trades: VecDeque::new(),
            cvd: 0.0,
            buy_count: 0,
            sell_count: 0,
            bsi: 0.0,
            last_ts_ms: None,
            ofi_history: VecDeque::with_capacity(MAX_STATS),
        }
    }

    pub fn ingest(&mut self, ts_ms: u64, qty: f64, is_buy: bool) {
        let delta = if is_buy { qty } else { -qty };

        self.window_trades.push_back(TradeEntry { ts_ms, delta, is_buy });
        self.cvd += delta;
        if is_buy {
            self.buy_count += 1;
        } else {
            self.sell_count += 1;
        }

        let cutoff = ts_ms.saturating_sub(self.window_ms);
        while let Some(front) = self.window_trades.front() {
            if front.ts_ms >= cutoff {
                break;
            }
            let entry = self.window_trades.pop_front().unwrap();
            self.cvd -= entry.delta;
            if entry.is_buy {
                self.buy_count = self.buy_count.saturating_sub(1);
            } else {
                self.sell_count = self.sell_count.saturating_sub(1);
            }
        }

        // Hawkes BSI: önceki BSI üstel bozunum + yeni delta.
        self.bsi = match self.last_ts_ms {
            Some(prev) => {
                let dt = ts_ms.saturating_sub(prev) as f64 / 1000.0;
                self.bsi * (-HAWKES_KAPPA * dt).exp() + delta
            }
            None => delta,
        };
        self.last_ts_ms = Some(ts_ms);

        self.ofi_history.push_back(self.current_ofi());
        if self.ofi_history.len() > MAX_STATS {
            self.ofi_history.pop_front();
        }
    }

    fn current_ofi(&self) -> f64 {
        let total = (self.buy_count + self.sell_count) as f64;
        if total > 0.0 {
            (self.buy_count as f64 - self.sell_count as f64) / total
        } else {
            0.0
        }
    }

    pub fn snapshot(&self) -> (f64, f64, f64, f64, bool) {
        let ofi = self.current_ofi();
        let warmup = self.ofi_history.len() < WARMUP_TRADES;
        let signal_score = if warmup { NEUTRAL } else { ofi_to_score(ofi) };
        (self.cvd, self.bsi, ofi, signal_score, warmup)
    }
}

/// OFI (`[−1, +1]`) → `[0, 10]` skoru (5.0 = nötr). Piecewise-linear, absolute
/// kalibrasyon; BTC/ETH futures dağılımına göre (|ofi|≈0.30 güçlü, ≈0.50 ekstrem).
#[inline]
pub fn ofi_to_score(ofi: f64) -> f64 {
    let d = ofi.abs().min(1.0);
    let sgn = ofi.signum();
    let score_delta = if d < 0.05 {
        d * 8.0
    } else if d < 0.15 {
        0.40 + (d - 0.05) * 11.0
    } else if d < 0.30 {
        1.50 + (d - 0.15) * 10.0
    } else if d < 0.50 {
        3.00 + (d - 0.30) * 9.0
    } else {
        4.80 + (d - 0.50) * 0.4
    };
    (5.0 + sgn * score_delta).clamp(0.0, 10.0)
}

/// 60 sn frame yoksa WS ölü sayılır (BTC/ETH'de saniyede onlarca trade akar).
const FRAME_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Binance aggTrade WS görevi; kopuşta exponential backoff ile yeniden bağlanır,
/// disconnect süresince `signal_score = 5.0`.
pub async fn run_binance_signal(
    symbol: &str,
    interval: Interval,
    state: SharedSignalState,
    bot_id: i64,
) {
    let url = format!("wss://fstream.binance.com/ws/{symbol}@aggTrade");
    let label = bot_id.to_string();
    let mut backoff = 1u64;
    loop {
        ipc::log_line(
            &label,
            format!("🛰️  Binance ws connecting (symbol={symbol}, interval={interval:?}) → {url}"),
        );
        match connect_stream(&url, interval, &state, &label).await {
            Ok(()) => ipc::log_line(
                &label,
                format!("⚠️  Binance ws closed, reconnect in {backoff}s"),
            ),
            Err(e) => ipc::log_line(
                &label,
                format!("❌ Binance ws error: {e} (reconnect in {backoff}s)"),
            ),
        }
        {
            let mut s = state.write().await;
            s.signal_score = NEUTRAL;
        }
        sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

async fn connect_stream(
    url: &str,
    interval: Interval,
    state: &SharedSignalState,
    label: &str,
) -> Result<(), anyhow::Error> {
    let (ws_stream, _) = match timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await
    {
        Ok(res) => res?,
        Err(_) => return Err(anyhow::anyhow!("connect_async timeout (10s)")),
    };
    let (mut write, mut read) = ws_stream.split();
    let mut computer = SignalComputer::new(interval);
    let mut prev_warmup = true;
    loop {
        let next = match timeout(FRAME_IDLE_TIMEOUT, read.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => return Ok(()),
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "binance ws idle > {}s (no aggTrade frames)",
                    FRAME_IDLE_TIMEOUT.as_secs()
                ));
            }
        };
        let text = match next? {
            Message::Ping(payload) => {
                let _ = write.send(Message::Pong(payload)).await;
                continue;
            }
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
                tracing::warn!(error=%e, qty=%trade.qty, "binance aggTrade qty parse failed");
                continue;
            }
        };

        computer.ingest(trade.event_time_ms, qty, !trade.is_buyer_maker);
        let (cvd, bsi, ofi, score, warmup) = computer.snapshot();
        {
            let mut s = state.write().await;
            s.cvd = cvd;
            s.bsi = bsi;
            s.ofi = ofi;
            s.signal_score = score;
        }

        if prev_warmup && !warmup {
            prev_warmup = false;
            ipc::log_line(
                label,
                format!(
                    "🟢 Binance warmup complete → signal_score={score:.2} cvd={cvd:.3} bsi={bsi:.3} ofi={ofi:+.3}"
                ),
            );
        }
    }
}
