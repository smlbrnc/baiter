//! IPC — bot → supervisor event/log köprüsü (§5.1, §5.2).
//!
//! Bot süreçleri stdout'a iki tür satır yazar:
//! 1. `[HH:MM:SS.mmm] [bot_label] mesaj` — sade log; supervisor `info`/`warn`/`error`
//!    seviyesine göre `logs` tablosuna yazar.
//! 2. `[[EVENT]] {json}` — `FrontendEvent` payload'ı; supervisor parse eder ve SSE'ye yayar.

use std::io::{self, Write};

use chrono::Utc;
use chrono_tz::America::New_York;
use serde::{Deserialize, Serialize};

use crate::types::{Outcome, Side};

/// `[[EVENT]] ` prefix'i — supervisor bunu görünce satırı event parser'a yönlendirir.
pub const EVENT_PREFIX: &str = "[[EVENT]] ";

/// `[HH:MM:SS.mmm] [bot_label] mesaj` formatlı tek satır metin log (ET zaman dilimi).
pub fn log_line(bot_label: &str, msg: impl AsRef<str>) {
    let ts = Utc::now().with_timezone(&New_York).format("%H:%M:%S%.3f");
    let stdout = io::stdout();
    let mut h = stdout.lock();
    let _ = writeln!(h, "[{ts}] [{bot_label}] {}", msg.as_ref());
    let _ = h.flush();
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
        yes_token_id: String,
        no_token_id: String,
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
    BestBidAsk {
        bot_id: i64,
        yes_best_bid: f64,
        yes_best_ask: f64,
        no_best_bid: f64,
        no_best_ask: f64,
        ts_ms: u64,
    },
    SignalUpdate {
        bot_id: i64,
        symbol: String,
        signal_score: f64,
        bsi: f64,
        ofi: f64,
        cvd: f64,
        ts_ms: u64,
    },
    RtdsUpdate {
        bot_id: i64,
        current_price: f64,
        window_open_price: Option<f64>,
        window_delta_bps: f64,
        ts_ms: u64,
    },
    /// 1 sn cadence: BestBidAsk + SignalUpdate verilerini tek event'te birleştirir.
    /// `slug` ile session'a bağlanır; frontend REST polling'i kaldırabilir.
    TickSnapshot {
        bot_id: i64,
        slug: String,
        yes_best_bid: f64,
        yes_best_ask: f64,
        no_best_bid: f64,
        no_best_ask: f64,
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
        shares_yes: f64,
        shares_no: f64,
        pnl_if_up: f64,
        pnl_if_down: f64,
        mtm_pnl: f64,
        pair_count: f64,
        avg_yes: Option<f64>,
        avg_no: Option<f64>,
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
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{EVENT_PREFIX}{json}");
    let _ = handle.flush();
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
