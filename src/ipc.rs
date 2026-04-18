//! IPC — bot → supervisor event bridge.
//!
//! Bot process'leri kritik event'leri stdout'a tek satır JSON olarak yazar:
//! `[[EVENT]] {"kind":"Fill", ...}\n`
//! Supervisor `ChildStdout` satırlarını okurken bu prefix'e göre ayırır.
//!
//! Referans: [docs/bot-platform-mimari.md §5.1 §5.2](../../../docs/bot-platform-mimari.md).

use std::io::{self, Write};

use chrono::Utc;
use chrono_tz::America::New_York;
use serde::{Deserialize, Serialize};

use crate::types::{Outcome, Side};

/// Stdout event prefix'i — supervisor bu prefix ile satırları parser'a yönlendirir.
pub const EVENT_PREFIX: &str = "[[EVENT]] ";

/// Mimari §5.1 — `[HH:MM:SS.mmm] [bot_label] mesaj` formatlı tek satır metin log.
///
/// Stdout'a yazılır; supervisor `[[EVENT]]` olmayan satırları logs tablosuna `info`
/// seviyesiyle (veya satır başında `WARN`/`ERROR` belirteci varsa o seviyeyle) yazar.
pub fn log_line(bot_label: &str, msg: impl AsRef<str>) {
    // Mimari §5.1 örnekleri ET (America/New_York) zaman dilimindedir.
    let ts = Utc::now()
        .with_timezone(&New_York)
        .format("%H:%M:%S%.3f");
    let stdout = io::stdout();
    let mut h = stdout.lock();
    let _ = writeln!(h, "[{ts}] [{bot_label}] {}", msg.as_ref());
    let _ = h.flush();
}

/// `log_line` makro — `format!` benzeri kullanım için.
#[macro_export]
macro_rules! log_line {
    ($label:expr, $($arg:tt)*) => {
        $crate::ipc::log_line($label, format!($($arg)*))
    };
}

/// Supervisor → frontend SSE ile taşınan event tipleri.
///
/// `serde_json` ile tek satırda (newline'sız) serialize edilir.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FrontendEvent {
    /// Bot başlatıldı veya yeniden başlatıldı.
    BotStarted {
        bot_id: i64,
        name: String,
        slug: String,
        ts_ms: u64,
    },
    /// Bot normal şekilde durdu (SIGTERM).
    BotStopped {
        bot_id: i64,
        ts_ms: u64,
        reason: String,
    },
    /// Yeni market penceresi açıldı (startDate-endDate).
    SessionOpened {
        bot_id: i64,
        slug: String,
        start_ts: u64,
        end_ts: u64,
        yes_token_id: String,
        no_token_id: String,
    },
    /// Market penceresi çözümlendi (market_resolved).
    SessionResolved {
        bot_id: i64,
        slug: String,
        winning_outcome: String,
        ts_ms: u64,
    },
    /// Emir gönderildi (POST /order döndü).
    OrderPlaced {
        bot_id: i64,
        order_id: String,
        outcome: Outcome,
        side: Side,
        price: f64,
        size: f64,
        order_type: String,
        ts_ms: u64,
    },
    /// Emir iptal edildi.
    OrderCanceled {
        bot_id: i64,
        order_id: String,
        ts_ms: u64,
    },
    /// Trade fill event'i (User WS `trade` MATCHED + sonraki statuslar).
    Fill {
        bot_id: i64,
        trade_id: String,
        outcome: Outcome,
        price: f64,
        size: f64,
        status: String,
        ts_ms: u64,
    },
    /// Market WS `best_bid_ask` snapshot'ı (frontend PriceChart).
    BestBidAsk {
        bot_id: i64,
        yes_best_bid: f64,
        yes_best_ask: f64,
        no_best_bid: f64,
        no_best_ask: f64,
        ts_ms: u64,
    },
    /// MarketZone geçişi (DeepTrade → NormalTrade → …).
    ZoneChanged {
        bot_id: i64,
        zone: String,
        zone_pct: f64,
        ts_ms: u64,
    },
    /// Binance sinyal skor güncelleme.
    SignalUpdate {
        bot_id: i64,
        symbol: String,
        signal_score: f64,
        bsi: f64,
        ofi: f64,
        cvd: f64,
        ts_ms: u64,
    },
    /// Strateji FSM geçişi
    /// (örn. `Pending → OpenDual{deadline}`, `OpenDual → SingleLeg{Up}`,
    /// `SingleLeg → ProfitLock`, `ProfitLock → Done`).
    StateChanged {
        bot_id: i64,
        state: String,
        ts_ms: u64,
    },
    /// Genel hata / uyarı.
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

/// Log satırından `FrontendEvent` parse eder (supervisor tarafında kullanılır).
///
/// - `[[EVENT]] ` prefix'i yoksa `None` döner.
/// - JSON parse hatası `None` döner (log satırı hata olarak yutulmaz, üst katman loglar).
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
            ts_ms: 1_766_789_469_958,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let line = format!("{EVENT_PREFIX}{json}");
        let parsed = parse_event_line(&line).expect("must parse");
        match parsed {
            FrontendEvent::OrderPlaced {
                order_id, price, ..
            } => {
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
            FrontendEvent::ZoneChanged {
                bot_id: 1,
                zone: "NormalTrade".into(),
                zone_pct: 0.35,
                ts_ms: 0,
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
