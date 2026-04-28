//! Zaman yardımcıları: unix ms/sec, T-15, `zone_pct`, `MarketZone`.

use std::time::{SystemTime, UNIX_EPOCH};

/// Güncel unix zaman — milisaniye.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis() as u64
}

/// Güncel unix zaman — saniye.
pub fn now_secs() -> u64 {
    now_ms() / 1000
}

/// Market başlangıcından 15 saniye önce (T-15) unix sn.
pub fn t_minus_15(market_start_ts: u64) -> u64 {
    market_start_ts.saturating_sub(15)
}

/// Market penceresinin % ilerleme oranı [0.0, 1.0]; `start_ts`/`end_ts`/`now` unix saniye.
pub fn zone_pct(start_ts: u64, end_ts: u64, now: u64) -> f64 {
    if now <= start_ts {
        return 0.0;
    }
    if now >= end_ts {
        return 1.0;
    }
    (now - start_ts) as f64 / (end_ts - start_ts) as f64
}

/// Market penceresi % bazlı bölge eşikleri (interval-agnostic).
///
/// Aynı eşikler tüm interval'lerde geçerli; süre olarak farklı görünür:
/// - 5m  (300s):  Deep <30s,  Normal <225s, Agg <270s, Fak <294s, Stop ≥294s
/// - 15m (900s):  Deep <90s,  Normal <675s, Agg <810s, Fak <882s, Stop ≥882s
/// - 1h  (3600s): Deep <6m,   Normal <45m,  Agg <54m,  Fak <58m48s, Stop ≥58m48s
/// - 4h  (14400s):Deep <24m,  Normal <3h,   Agg <3h36m,Fak <3h55m12s,Stop ≥3h55m12s
///
/// Bantlar: DeepTrade < 10 %, NormalTrade < 75 %, AggTrade < 90 %,
/// FakTrade < 98 %, StopTrade ≥ 98 %.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MarketZone {
    DeepTrade,
    NormalTrade,
    AggTrade,
    FakTrade,
    StopTrade,
}

impl MarketZone {
    pub fn from_pct(pct: f64) -> Self {
        if pct < 0.10 {
            Self::DeepTrade
        } else if pct < 0.75 {
            Self::NormalTrade
        } else if pct < 0.90 {
            Self::AggTrade
        } else if pct < 0.98 {
            Self::FakTrade
        } else {
            Self::StopTrade
        }
    }
}
