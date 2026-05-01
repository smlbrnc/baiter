//! OKX spot işlemlerinden çift EMA fiyat momentum sinyali.
//!
//! signal.md Katman 2:
//!   ema_fast = 0.40 × fiyat + 0.60 × ema_fast   # α=0.40 → ~2 sn
//!   ema_slow = 0.10 × fiyat + 0.90 × ema_slow   # α=0.10 → ~10 sn
//!   momentum_bps = (ema_fast − ema_slow) / ema_slow × 10_000

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

use crate::ipc;

const EMA_FAST_ALPHA: f64 = 0.40;
const EMA_SLOW_ALPHA: f64 = 0.10;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const FRAME_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxSignalState {
    /// `(ema_fast − ema_slow) / ema_slow × 10_000`; 0 = nötr.
    pub momentum_bps: f64,
    /// Warmup: ilk fiyat gelmeden `true`.
    pub warmup: bool,
}

impl Default for OkxSignalState {
    fn default() -> Self {
        Self { momentum_bps: 0.0, warmup: true }
    }
}

pub type SharedOkxState = Arc<RwLock<OkxSignalState>>;

pub fn new_shared_state() -> SharedOkxState {
    Arc::new(RwLock::new(OkxSignalState::default()))
}

/// Çift EMA momentum hesaplayıcısı.
struct EmaEngine {
    ema_fast: Option<f64>,
    ema_slow: Option<f64>,
}

impl EmaEngine {
    fn new() -> Self {
        Self { ema_fast: None, ema_slow: None }
    }

    fn ingest(&mut self, price: f64) {
        match (self.ema_fast, self.ema_slow) {
            (None, _) | (_, None) => {
                self.ema_fast = Some(price);
                self.ema_slow = Some(price);
            }
            (Some(fast), Some(slow)) => {
                self.ema_fast = Some(EMA_FAST_ALPHA * price + (1.0 - EMA_FAST_ALPHA) * fast);
                self.ema_slow = Some(EMA_SLOW_ALPHA * price + (1.0 - EMA_SLOW_ALPHA) * slow);
            }
        }
    }

    fn snapshot(&self) -> (f64, bool) {
        match (self.ema_fast, self.ema_slow) {
            (Some(fast), Some(slow)) if slow > 0.0 => {
                let momentum_bps = (fast - slow) / slow * 10_000.0;
                (momentum_bps, false)
            }
            _ => (0.0, true),
        }
    }
}

/// OKX WS tick payload (trades channel).
#[derive(Debug, Deserialize)]
struct OkxTradeMsg {
    #[serde(default)]
    data: Vec<OkxTrade>,
}

#[derive(Debug, Deserialize)]
struct OkxTrade {
    /// Son işlem fiyatı (string olarak gelir, ör. "97000.5").
    px: String,
}

/// OKX public trades WS görevi; kopuşta exponential backoff ile yeniden bağlanır.
pub async fn run_okx_signal(inst_id: &str, state: SharedOkxState, bot_id: i64) {
    let ws_url = "wss://ws.okx.com:8443/ws/v5/public";
    let label = bot_id.to_string();
    let mut backoff = 1u64;
    loop {
        ipc::log_line(
            &label,
            format!("📡 OKX EMA ws bağlanıyor (instId={inst_id}) → {ws_url}"),
        );
        match connect_stream(ws_url, inst_id, &state, &label).await {
            Ok(()) => ipc::log_line(&label, format!("⚠️  OKX ws kapandı, {backoff}s içinde yeniden bağlanılacak")),
            Err(e) => ipc::log_line(&label, format!("❌ OKX ws hatası: {e} ({backoff}s içinde yeniden)")),
        }
        {
            let mut s = state.write().await;
            *s = OkxSignalState::default();
        }
        sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

async fn connect_stream(
    ws_url: &str,
    inst_id: &str,
    state: &SharedOkxState,
    label: &str,
) -> Result<(), anyhow::Error> {
    let (ws_stream, _) =
        match timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(ws_url)).await {
            Ok(res) => res?,
            Err(_) => return Err(anyhow::anyhow!("OKX connect_async timeout (10s)")),
        };
    let (mut write, mut read) = ws_stream.split();

    let subscribe = serde_json::json!({
        "op": "subscribe",
        "args": [{ "channel": "trades", "instId": inst_id }]
    })
    .to_string();
    write
        .send(Message::Text(Utf8Bytes::from(subscribe)))
        .await
        .map_err(|e| anyhow::anyhow!("OKX subscribe gönderme hatası: {e}"))?;

    let mut engine = EmaEngine::new();
    let mut prev_warmup = true;

    loop {
        let next = match timeout(FRAME_IDLE_TIMEOUT, read.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => return Ok(()),
            Err(_) => return Err(anyhow::anyhow!(
                "OKX ws idle > {}s (trades frame yok)",
                FRAME_IDLE_TIMEOUT.as_secs()
            )),
        };
        let text = match next? {
            Message::Ping(payload) => { let _ = write.send(Message::Pong(payload)).await; continue; }
            Message::Close(_) => return Ok(()),
            Message::Text(t) => t,
            _ => continue,
        };

        let msg: OkxTradeMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };

        for trade in &msg.data {
            let price: f64 = match trade.px.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            engine.ingest(price);
        }

        let (momentum_bps, warmup) = engine.snapshot();
        {
            let mut s = state.write().await;
            s.momentum_bps = momentum_bps;
            s.warmup = warmup;
        }

        if prev_warmup && !warmup {
            prev_warmup = false;
            ipc::log_line(
                label,
                format!("🟢 OKX EMA warmup tamamlandı → momentum_bps={momentum_bps:+.2}"),
            );
        }
    }
}
