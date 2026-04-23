//! IPC — bot → supervisor event/log köprüsü (§5.1, §5.2).
//!
//! Bot süreçleri stdout'a iki satır türü yazar: `[HH:MM:SS.mmm] [bot] msg`
//! sade log ve `[[EVENT]] {json}` `FrontendEvent` payload'ı. Supervisor
//! prefix'e bakıp logs tablosuna ya da SSE kanalına yönlendirir.
//!
//! `init_async_writer()` sonrası hot path bounded mpsc'ye `try_send` yapıp
//! döner; drain task batch'leyip stdout'a basar (ordering korunur). Init
//! yoksa veya kanal doluysa sync stdout fallback.

use std::io::{self, Write};
use std::sync::OnceLock;

use chrono::Utc;
use chrono_tz::America::New_York;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

use crate::types::{Outcome, Side};

/// `[[EVENT]] ` prefix'i; supervisor bunu görünce event parser'a yönlendirir.
pub const EVENT_PREFIX: &str = "[[EVENT]] ";

static LOG_TX: OnceLock<mpsc::Sender<Vec<u8>>> = OnceLock::new();

/// Async stdout writer kurar; `bot::run()` başında bir kere çağrılır,
/// tekrar çağrı no-op.
pub fn init_async_writer() {
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(4096);
    if LOG_TX.set(tx).is_err() {
        return;
    }
    tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        let mut batch: Vec<u8> = Vec::with_capacity(16 * 1024);
        while let Some(first) = rx.recv().await {
            batch.clear();
            batch.extend_from_slice(&first);
            while let Ok(more) = rx.try_recv() {
                batch.extend_from_slice(&more);
                if batch.len() >= 64 * 1024 {
                    break;
                }
            }
            let _ = stdout.write_all(&batch).await;
            let _ = stdout.flush().await;
        }
    });
}

fn write_line(buf: Vec<u8>) {
    if let Some(tx) = LOG_TX.get() {
        match tx.try_send(buf) {
            Ok(()) => return,
            Err(TrySendError::Full(b)) | Err(TrySendError::Closed(b)) => {
                let stdout = io::stdout();
                let mut h = stdout.lock();
                let _ = h.write_all(&b);
                let _ = h.flush();
                return;
            }
        }
    }
    let stdout = io::stdout();
    let mut h = stdout.lock();
    let _ = h.write_all(&buf);
    let _ = h.flush();
}

/// `[HH:MM:SS.mmm ET] [bot] msg` formatlı tek satır log.
pub fn log_line(bot_label: &str, msg: impl AsRef<str>) {
    let ts = Utc::now().with_timezone(&New_York).format("%H:%M:%S%.3f");
    let line = format!("[{ts}] [{bot_label}] {}\n", msg.as_ref());
    write_line(line.into_bytes());
}

/// Supervisor → frontend SSE event tipleri.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FrontendEvent {
    BotStarted {
        bot_id: i64,
        name: String,
        slug: String,
        ts_ms: u64,
    },
    BotStopped {
        bot_id: i64,
        ts_ms: u64,
        reason: String,
    },
    SessionOpened {
        bot_id: i64,
        slug: String,
        start_ts: u64,
        end_ts: u64,
        up_token_id: String,
        down_token_id: String,
    },
    SessionResolved {
        bot_id: i64,
        slug: String,
        winning_outcome: String,
        ts_ms: u64,
    },
    OrderPlaced {
        bot_id: i64,
        order_id: String,
        outcome: Outcome,
        side: Side,
        price: f64,
        size: f64,
        order_type: String,
        status: String,
        ts_ms: u64,
    },
    OrderCanceled {
        bot_id: i64,
        order_id: String,
        ts_ms: u64,
    },
    Fill {
        bot_id: i64,
        trade_id: String,
        outcome: Outcome,
        side: String,
        price: f64,
        size: f64,
        status: String,
        ts_ms: u64,
    },
    RtdsUpdate {
        bot_id: i64,
        current_price: f64,
        window_open_price: Option<f64>,
        window_delta_bps: f64,
        ts_ms: u64,
    },
    /// 1 sn cadence: book + composite sinyal; `slug` ile session'a bağlanır.
    TickSnapshot {
        bot_id: i64,
        slug: String,
        up_best_bid: f64,
        up_best_ask: f64,
        down_best_bid: f64,
        down_best_ask: f64,
        signal_score: f64,
        bsi: f64,
        ofi: f64,
        cvd: f64,
        ts_ms: u64,
    },
    /// 1 sn cadence: PnL snapshot.
    PnlUpdate {
        bot_id: i64,
        slug: String,
        cost_basis: f64,
        fee_total: f64,
        up_filled: f64,
        down_filled: f64,
        pnl_if_up: f64,
        pnl_if_down: f64,
        mtm_pnl: f64,
        pair_count: f64,
        avg_up: Option<f64>,
        avg_down: Option<f64>,
        ts_ms: u64,
    },
    /// Alis stratejisi profit-lock şartını sağlayıp `Locked` state'ine geçti.
    /// `lock_method` lock'un nasıl tetiklendiğini söyler:
    /// - `"taker_fak"`: dominant + opp.best_ask ≤ avg_threshold → FAK ile kapatıldı.
    /// - `"passive_hedge_fill"`: pasif hedge emri doldu, ek emir yok.
    /// - `"symmetric_fill"`: aynı tick'te iki taraf da doldu.
    ///
    /// `expected_profit` = `pair_count − cost_basis − fee_total` (USDC).
    ProfitLocked {
        bot_id: i64,
        slug: String,
        avg_up: f64,
        avg_down: f64,
        expected_profit: f64,
        lock_method: String,
        ts_ms: u64,
    },
    Error {
        bot_id: i64,
        message: String,
        ts_ms: u64,
    },
}

/// `FrontendEvent` → `[[EVENT]] <json>\n` stdout satırı.
pub fn emit(ev: &FrontendEvent) {
    let json = match serde_json::to_string(ev) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!(error=%e, "event serialize failed");
            return;
        }
    };
    let line = format!("{EVENT_PREFIX}{json}\n");
    write_line(line.into_bytes());
}

/// Satırı `FrontendEvent`'e parse; prefix yoksa veya JSON bozuksa `None`.
pub fn parse_event_line(line: &str) -> Option<FrontendEvent> {
    let rest = line.strip_prefix(EVENT_PREFIX)?;
    serde_json::from_str::<FrontendEvent>(rest.trim()).ok()
}
