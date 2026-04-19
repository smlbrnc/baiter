//! Binance USD-M Futures aggTrade sinyal katmanı (§14).
//!
//! WebSocket'ten aggTrade akışı; sliding-window CVD + Hawkes BSI + OFI
//! birleşip [0,10] aralığına z-score ile haritalanır → `signal_score`.
//! Warmup (N<300) ve bağlantı koptuğunda nötr `5.0`.

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
use crate::time::now_ms;

/// Strateji katmanına açılan anlık sinyal durumu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceSignalState {
    pub cvd: f64,
    pub bsi: f64,
    pub ofi: f64,
    pub signal_score: f64,
    pub warmup: bool,
    pub updated_at_ms: u64,
    pub connected: bool,
}

impl Default for BinanceSignalState {
    fn default() -> Self {
        Self {
            cvd: 0.0,
            bsi: 0.0,
            ofi: 0.0,
            signal_score: 5.0,
            warmup: true,
            updated_at_ms: 0,
            connected: false,
        }
    }
}

pub type SharedSignalState = Arc<RwLock<BinanceSignalState>>;

pub fn new_shared_state() -> SharedSignalState {
    Arc::new(RwLock::new(BinanceSignalState::default()))
}

/// `effective_score = 5 + (signal_score - 5) * (signal_weight / 10)` (§14.3).
pub fn effective_score(signal_score: f64, signal_weight: f64) -> f64 {
    5.0 + (signal_score - 5.0) * (signal_weight / 10.0)
}

/// CVD kayan penceresi market aralığına göre (§14.2).
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
    /// `m=true` → buyer is market maker → taker satış; `false` → taker alış.
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
const HAWKES_KAPPA: f64 = 0.1;

/// aggTrade işleyici — sliding-window CVD + BSI + OFI + signal_score (§14.2-14.3).
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
        let warmup = self.ofi_history.len() < MAX_STATS;
        let signal_score = if warmup {
            NEUTRAL
        } else {
            let n = self.ofi_history.len() as f64;
            let mean = self.ofi_history.iter().sum::<f64>() / n;
            let var = self
                .ofi_history
                .iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>()
                / n;
            let std = var.sqrt().max(1e-9);
            let z = ((ofi - mean) / std).clamp(-3.0, 3.0);
            // z ∈ [-3, 3] → [0, 10] (0.1 step).
            ((z + 3.0) / 6.0 * 100.0).round() / 10.0
        };

        (self.cvd, self.bsi, ofi, signal_score, warmup)
    }
}

/// aggTrade frame'leri arasında izin verilen maksimum sessizlik.
/// BTC/ETH için saniyede onlarca işlem akar; 60 sn boşluk = ölü WS.
const FRAME_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Binance aggTrade WebSocket görevini başlatır; bağlantı koptuğunda
/// exponential backoff ile yeniden bağlanır, kopuk süre boyunca
/// `signal_score = 5.0`.
///
/// `bot_id` log etiketi olarak kullanılır.
pub async fn run_binance_signal(
    symbol: &str,
    interval: Interval,
    state: SharedSignalState,
    bot_id: i64,
) {
    let url = format!("wss://fstream.binance.com/ws/{symbol}@aggTrade");
    let label = bot_id.to_string();
    ipc::log_line(
        &label,
        format!("🛰️  Binance signal task starting (symbol={symbol}, interval={interval:?})"),
    );
    let mut backoff = 1u64;
    loop {
        ipc::log_line(&label, format!("🛰️  Binance ws connecting → {url}"));
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
            s.connected = false;
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
    ipc::log_line(label, "🛰️  Binance ws connected (warmup başladı)");

    let mut computer = SignalComputer::new(interval);
    {
        let mut s = state.write().await;
        s.connected = true;
        s.warmup = true;
        s.signal_score = NEUTRAL;
    }

    let mut prev_warmup = true;
    let mut trade_count: u64 = 0;
    let mut last_progress_log: u64 = 0;
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
            // Pong / Binary / Frame → görmezden gel.
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
        trade_count += 1;
        let (cvd, bsi, ofi, score, warmup) = computer.snapshot();
        {
            let mut s = state.write().await;
            s.cvd = cvd;
            s.bsi = bsi;
            s.ofi = ofi;
            s.signal_score = score;
            s.warmup = warmup;
            s.updated_at_ms = now_ms();
            s.connected = true;
        }

        if warmup && trade_count - last_progress_log >= 100 {
            last_progress_log = trade_count;
            ipc::log_line(
                label,
                format!(
                    "🛰️  Binance warmup {}/{MAX_STATS} (cvd={cvd:.3} bsi={bsi:.3} ofi={ofi:+.3})",
                    computer.ofi_history.len(),
                ),
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_score_zero_weight_returns_neutral() {
        assert!((effective_score(8.0, 0.0) - 5.0).abs() < 1e-9);
        assert!((effective_score(2.0, 0.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn effective_score_full_weight_returns_raw() {
        assert!((effective_score(8.0, 10.0) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn effective_score_half_weight() {
        assert!((effective_score(8.0, 5.0) - 6.5).abs() < 1e-9);
    }

    #[test]
    fn signal_computer_warmup() {
        let mut c = SignalComputer::new(Interval::M5);
        c.ingest(1000, 1.0, true);
        let (cvd, _, _, score, warmup) = c.snapshot();
        assert_eq!(cvd, 1.0);
        assert_eq!(score, NEUTRAL);
        assert!(warmup);
    }

    #[test]
    fn signal_computer_cvd_window_expiry() {
        let mut c = SignalComputer::new(Interval::M5); // 60s window
        c.ingest(1_000, 10.0, true);
        c.ingest(2_000, 5.0, false);
        assert!((c.cvd - 5.0).abs() < 1e-9);
        // 62s sonra ilk trade düşmeli (cutoff = 62000 - 60000 = 2000).
        c.ingest(62_000, 1.0, true);
        assert!((c.cvd - (5.0 - 10.0 + 1.0)).abs() < 1e-9);
    }
}
