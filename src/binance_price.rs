//! Binance Spot bookTicker fiyat oracle — Binance Latency stratejisi için.
//!
//! Sub-second BTC/USDT bid/ask snapshot (mid price) tutar. WS kopuşunda
//! exponential backoff ile yeniden bağlanır; disconnect süresince `None`
//! döner (strateji NoOp yapar).
//!
//! Strateji `BinanceLatency` bunu kullanarak:
//!   - `start_ts` anındaki snapshot fiyatı kaydeder (`btc_open_price`)
//!   - Her tick'te mevcut fiyatı okur (`btc_current_price`)
//!   - `delta = current − open` → eşik aşılırsa BUY (UP/DOWN seçer)
//!
//! Backtest sonucu (Bot 91, 64h, 665 session): `sig_thr=$50, mt=10, cd=3s`
//! → ROI +%4.80, NET +$8323, yıllık ~$1.14M.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;

use crate::ipc;

const FRAME_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// BTC/USDT mid price snapshot. `None` = WS bağlı değil veya henüz veri yok.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinancePriceState {
    /// En son alınan mid price ((bid + ask) / 2). `None` = veri yok.
    pub mid: Option<f64>,
    /// Snapshot zamanı (epoch ms). `0` = veri yok.
    pub ts_ms: u64,
}

pub type SharedBinancePrice = Arc<RwLock<BinancePriceState>>;

pub fn new_shared_state() -> SharedBinancePrice {
    Arc::new(RwLock::new(BinancePriceState::default()))
}

/// Binance Spot bookTicker payload (yalnızca tüketilen alanlar).
#[derive(Debug, Deserialize)]
struct BookTicker {
    #[serde(rename = "b")]
    bid: String,
    #[serde(rename = "a")]
    ask: String,
}

/// Binance Spot bookTicker WS görevi; kopuşta exponential backoff ile
/// yeniden bağlanır. Disconnect süresince `mid = None`.
pub async fn run_binance_price(symbol: &str, state: SharedBinancePrice, bot_id: i64) {
    let url = format!(
        "wss://stream.binance.com:9443/ws/{}@bookTicker",
        symbol.to_ascii_lowercase()
    );
    let label = bot_id.to_string();
    let mut backoff = 1u64;
    loop {
        ipc::log_line(
            &label,
            format!("📈 Binance price ws bağlanıyor (symbol={symbol}) → {url}"),
        );
        match connect_stream(&url, &state, &label).await {
            Ok(()) => ipc::log_line(
                &label,
                format!("⚠️  Binance price ws kapandı, {backoff}s içinde yeniden bağlanılacak"),
            ),
            Err(e) => ipc::log_line(
                &label,
                format!("❌ Binance price ws hatası: {e} ({backoff}s içinde yeniden)"),
            ),
        }
        {
            let mut s = state.write().await;
            *s = BinancePriceState::default();
        }
        sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

async fn connect_stream(
    url: &str,
    state: &SharedBinancePrice,
    _label: &str,
) -> Result<(), anyhow::Error> {
    let (ws_stream, _) = match timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await
    {
        Ok(res) => res?,
        Err(_) => return Err(anyhow::anyhow!("connect_async timeout (10s)")),
    };
    let (mut write, mut read) = ws_stream.split();

    loop {
        let next = match timeout(FRAME_IDLE_TIMEOUT, read.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => return Ok(()),
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "binance price ws idle > {}s",
                    FRAME_IDLE_TIMEOUT.as_secs()
                ))
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

        let bt: BookTicker = match serde_json::from_str(&text) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let bid: f64 = match bt.bid.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ask: f64 = match bt.ask.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if bid <= 0.0 || ask <= 0.0 {
            continue;
        }
        let mid = (bid + ask) * 0.5;
        let now = crate::time::now_ms();
        let mut s = state.write().await;
        s.mid = Some(mid);
        s.ts_ms = now;
    }
}

/// Sync snapshot okuma — strateji `decide()` içinde kullanmak için
/// (write lock blokken read lock yine alınabilir; Tokio RwLock ile
/// `try_read` non-blocking).
pub fn try_read_mid(state: &SharedBinancePrice) -> Option<f64> {
    state.try_read().ok().and_then(|s| s.mid)
}
