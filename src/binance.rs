//! Binance USD-M Futures aggTrade sinyal katmanı (§14).
//!
//! - WebSocket: `wss://fstream.binance.com/ws/<symbol>@aggTrade`.
//! - CVD (kayan pencere), BSI (Hawkes bozunum), OFI (sayı oranı) → `signal_score`.
//! - Warmup (N<300 örnek) → `signal_score = 5.0` (nötr).
//! - Reconnect kopuk süre boyunca `signal_score = 5.0`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::sleep;
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

/// `effective_score` formülü (§14.3).
pub fn effective_score(signal_score: f64, signal_weight: f64) -> f64 {
    5.0 + (signal_score - 5.0) * (signal_weight / 10.0)
}

/// Paylaşılabilir sinyal state + task handle.
pub type SharedSignalState = Arc<RwLock<BinanceSignalState>>;

pub fn new_shared_state() -> SharedSignalState {
    Arc::new(RwLock::new(BinanceSignalState::default()))
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

/// aggTrade olayı — Binance USD-M Futures şeması.
#[derive(Debug, Clone, Deserialize)]
struct AggTrade {
    #[serde(rename = "E")]
    event_time_ms: u64,
    #[serde(rename = "p", default)]
    #[allow(dead_code)]
    price: String,
    #[serde(rename = "q")]
    qty: String,
    #[serde(rename = "m")]
    /// true = buyer is market maker → taker satış; false = taker alış.
    is_buyer_maker: bool,
}

#[derive(Debug, Clone, Copy)]
struct TradeEntry {
    ts_ms: u64,
    delta: f64, // +q (buy) veya -q (sell)
    is_buy: bool,
}

/// aggTrade işleyici — kayan pencere CVD + BSI + OFI + signal_score (§14.2-14.3).
pub struct SignalComputer {
    window_ms: u64,
    window_trades: VecDeque<TradeEntry>,
    cvd: f64,
    buy_count: u64,
    sell_count: u64,
    // BSI Hawkes
    bsi: f64,
    last_ts_ms: Option<u64>,
    kappa: f64,
    // Rolling z-score stats
    ofi_history: VecDeque<f64>,
    max_stats: usize,
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
            kappa: 0.1,
            ofi_history: VecDeque::new(),
            max_stats: 300,
        }
    }

    pub fn ingest(&mut self, ts_ms: u64, qty: f64, is_buy: bool) {
        let delta = if is_buy { qty } else { -qty };

        // CVD pencere güncellemesi
        self.window_trades.push_back(TradeEntry {
            ts_ms,
            delta,
            is_buy,
        });
        self.cvd += delta;
        if is_buy {
            self.buy_count += 1;
        } else {
            self.sell_count += 1;
        }

        let cutoff = ts_ms.saturating_sub(self.window_ms);
        while let Some(front) = self.window_trades.front() {
            if front.ts_ms < cutoff {
                let entry = self.window_trades.pop_front().unwrap();
                self.cvd -= entry.delta;
                if entry.is_buy {
                    self.buy_count = self.buy_count.saturating_sub(1);
                } else {
                    self.sell_count = self.sell_count.saturating_sub(1);
                }
            } else {
                break;
            }
        }

        // BSI Hawkes
        if let Some(prev) = self.last_ts_ms {
            let dt = (ts_ms.saturating_sub(prev)) as f64 / 1000.0;
            self.bsi = self.bsi * (-self.kappa * dt).exp() + delta;
        } else {
            self.bsi = delta;
        }
        self.last_ts_ms = Some(ts_ms);

        // OFI güncelle
        let total = (self.buy_count + self.sell_count) as f64;
        let ofi = if total > 0.0 {
            (self.buy_count as f64 - self.sell_count as f64) / total
        } else {
            0.0
        };

        self.ofi_history.push_back(ofi);
        if self.ofi_history.len() > self.max_stats {
            self.ofi_history.pop_front();
        }
    }

    pub fn snapshot(&self) -> (f64, f64, f64, f64, bool) {
        let total = (self.buy_count + self.sell_count) as f64;
        let ofi = if total > 0.0 {
            (self.buy_count as f64 - self.sell_count as f64) / total
        } else {
            0.0
        };

        let warmup = self.ofi_history.len() < self.max_stats;
        let signal_score = if warmup {
            5.0
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
            ((z + 3.0) / 6.0 * 10.0 * 10.0).round() / 10.0
        };

        (self.cvd, self.bsi, ofi, signal_score, warmup)
    }
}

/// Binance aggTrade WebSocket görevini başlatır; state'i güncelle.
/// Kopma süresince `signal_score` nötr (`5.0`) kalır.
///
/// `bot_id` structured log etiketi olarak kullanılır (frontend log akışı için).
pub async fn run_binance_signal(
    symbol: &str,
    interval: Interval,
    state: SharedSignalState,
    bot_id: i64,
) {
    let url = format!("wss://fstream.binance.com/ws/{}@aggTrade", symbol);
    let label = bot_id.to_string();
    ipc::log_line(
        &label,
        format!("🛰️  Binance signal task starting (symbol={symbol}, interval={interval:?})"),
    );
    let mut backoff = 1u64;
    loop {
        ipc::log_line(&label, format!("🛰️  Binance ws connecting → {url}"));
        match connect_stream(&url, interval, &state, &label).await {
            Ok(()) => {
                ipc::log_line(
                    &label,
                    format!("⚠️  Binance ws closed, reconnect in {backoff}s"),
                );
            }
            Err(e) => {
                ipc::log_line(
                    &label,
                    format!("❌ Binance ws error: {e} (reconnect in {backoff}s)"),
                );
            }
        }
        {
            let mut s = state.write().await;
            s.connected = false;
            s.signal_score = 5.0; // nötr — bağlantı yokken
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
    let connect = tokio_tungstenite::connect_async(url);
    let (ws_stream, _) = match tokio::time::timeout(Duration::from_secs(10), connect).await {
        Ok(res) => res?,
        Err(_) => return Err(anyhow::anyhow!("connect_async timeout (10s)")),
    };
    let (_write, mut read) = ws_stream.split();
    ipc::log_line(label, "🛰️  Binance ws connected (warmup başladı)".to_string());

    let mut computer = SignalComputer::new(interval);
    {
        let mut s = state.write().await;
        s.connected = true;
        s.warmup = true;
        s.signal_score = 5.0;
    }

    let mut prev_warmup = true;
    let mut trade_count: u64 = 0;
    let mut last_progress_log: u64 = 0;
    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let Message::Text(t) = msg {
            let trade: AggTrade = match serde_json::from_str(&t) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let qty: f64 = trade.qty.parse().unwrap_or(0.0);
            let is_buy = !trade.is_buyer_maker;
            computer.ingest(trade.event_time_ms, qty, is_buy);
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

            // Warmup ilerleme: her 100 trade'de bir.
            if warmup && trade_count - last_progress_log >= 100 {
                last_progress_log = trade_count;
                ipc::log_line(
                    label,
                    format!(
                        "🛰️  Binance warmup {}/300 (cvd={:.3} bsi={:.3} ofi={:+.3})",
                        computer.ofi_history.len(),
                        cvd,
                        bsi,
                        ofi
                    ),
                );
            }
            // Warmup tamamlandı.
            if prev_warmup && !warmup {
                prev_warmup = false;
                ipc::log_line(
                    label,
                    format!(
                        "🟢 Binance warmup complete → signal_score={:.2} cvd={:.3} bsi={:.3} ofi={:+.3}",
                        score, cvd, bsi, ofi
                    ),
                );
            }
        }
    }
    Ok(())
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
        // 5.0 + (8.0 - 5.0) * 0.5 = 6.5
        assert!((effective_score(8.0, 5.0) - 6.5).abs() < 1e-9);
    }

    #[test]
    fn signal_computer_warmup() {
        let mut c = SignalComputer::new(Interval::M5);
        c.ingest(1000, 1.0, true);
        let (cvd, _, _, score, warmup) = c.snapshot();
        assert_eq!(cvd, 1.0);
        assert_eq!(score, 5.0);
        assert!(warmup);
    }

    #[test]
    fn signal_computer_cvd_window_expiry() {
        let mut c = SignalComputer::new(Interval::M5); // 60s window
        c.ingest(1_000, 10.0, true);
        c.ingest(2_000, 5.0, false);
        assert!((c.cvd - 5.0).abs() < 1e-9);
        // 61s geçti → ilk trade düşmeli (cutoff = 62000 - 60000 = 2000, front.ts_ms=1000 < 2000)
        c.ingest(62_000, 1.0, true);
        assert!((c.cvd - (5.0 - 10.0 + 1.0)).abs() < 1e-9);
    }
}
