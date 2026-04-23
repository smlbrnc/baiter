//! IPC — bot → supervisor event/log köprüsü (§5.1, §5.2).
//!
//! Bot süreçleri stdout'a iki tür satır yazar:
//! 1. `[HH:MM:SS.mmm] [bot_label] mesaj` — sade log; supervisor `info`/`warn`/`error`
//!    seviyesine göre `logs` tablosuna yazar.
//! 2. `[[EVENT]] {json}` — `FrontendEvent` payload'ı; supervisor parse eder ve SSE'ya yayar.
//!
//! Her iki çağrı `init_async_writer()` sonrası non-blocking: hot path satırı
//! ön-formatlar, bounded mpsc'ye `try_send`'ler ve döner. Dedicated drain task
//! batch'leyip stdout'a tek `write_all + flush` ile basar; ordering korunur.
//! Init yoksa veya kanal doluysa sync fallback (eski davranış).

use std::io::{self, Write};
use std::sync::OnceLock;

use chrono::Utc;
use chrono_tz::America::New_York;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

use crate::types::{Outcome, Side};

/// `[[EVENT]] ` prefix'i — supervisor bunu görünce satırı event parser'a yönlendirir.
pub const EVENT_PREFIX: &str = "[[EVENT]] ";

static LOG_TX: OnceLock<mpsc::Sender<Vec<u8>>> = OnceLock::new();

/// Async stdout writer'ı kurar. `bot::run()` başında bir kere çağrılır.
/// Tekrar çağrılırsa no-op (`OnceLock::set` Err). Drain task ömür boyu çalışır;
/// supervisor pipe kapanırsa task tokio runtime ile birlikte sonlanır.
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

/// `[HH:MM:SS.mmm] [bot_label] mesaj` formatlı tek satır metin log (ET zaman dilimi).
pub fn log_line(bot_label: &str, msg: impl AsRef<str>) {
    let ts = Utc::now().with_timezone(&New_York).format("%H:%M:%S%.3f");
    let line = format!("[{ts}] [{bot_label}] {}\n", msg.as_ref());
    write_line(line.into_bytes());
}

/// Supervisor → frontend SSE ile taşınan event tipleri.
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
    /// 1 sn cadence: book + composite sinyal verilerini tek event'te taşır.
    /// `slug` ile session'a bağlanır; frontend REST polling'i kaldırabilir.
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
    /// 1 sn cadence: PnL snapshot'ı — frontend REST polling'in yerine geçer.
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

/// `FrontendEvent`'i stdout'a `[[EVENT]] <json>\n` olarak yazar.
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

/// Tek satırı `FrontendEvent`'e parse eder; prefix yoksa veya JSON bozuksa `None`.
pub fn parse_event_line(line: &str) -> Option<FrontendEvent> {
    let rest = line.strip_prefix(EVENT_PREFIX)?;
    serde_json::from_str::<FrontendEvent>(rest.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_order_placed() {
        let ev = FrontendEvent::OrderPlaced {
            bot_id: 7,
            order_id: "0xff35".into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.57,
            size: 10.0,
            order_type: "GTC".into(),
            status: "live".into(),
            ts_ms: 1_766_789_469_958,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let line = format!("{EVENT_PREFIX}{json}");
        let parsed = parse_event_line(&line).expect("must parse");
        match parsed {
            FrontendEvent::OrderPlaced { order_id, price, .. } => {
                assert_eq!(order_id, "0xff35");
                assert!((price - 0.57).abs() < 1e-9);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn non_event_line_returns_none() {
        assert!(parse_event_line("regular log line").is_none());
        assert!(parse_event_line("[[EVENT]] not-json").is_none());
    }

    #[test]
    fn all_variants_serialize() {
        let variants = vec![
            FrontendEvent::BotStarted {
                bot_id: 1,
                name: "x".into(),
                slug: "btc-updown-5m-0".into(),
                ts_ms: 0,
            },
            FrontendEvent::BotStopped {
                bot_id: 1,
                ts_ms: 0,
                reason: "sigterm".into(),
            },
            FrontendEvent::Error {
                bot_id: 1,
                message: "test".into(),
                ts_ms: 0,
            },
        ];
        for v in variants {
            let s = serde_json::to_string(&v).unwrap();
            assert!(s.starts_with('{'));
            let back: FrontendEvent = serde_json::from_str(&s).unwrap();
            let _ = format!("{back:?}");
        }
    }
}
