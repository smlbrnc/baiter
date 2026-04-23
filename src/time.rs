//! Zaman yardımcıları: unix ms/sec, T-15, `zone_pct`, `MarketZone` (§4, §15).

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
/// - 5m  (300s):  Deep <30s,  Normal <150s, Agg <270s, Fak <291s, Stop ≥291s
/// - 15m (900s):  Deep <90s,  Normal <450s, Agg <810s, Fak <873s, Stop ≥873s
/// - 1h  (3600s): Deep <6m,   Normal <30m,  Agg <54m,  Fak <58m,  Stop ≥58m
/// - 4h  (14400s):Deep <24m,  Normal <2h,   Agg <3h36m,Fak <3h53m,Stop ≥3h53m
///
/// Bantlar (§15): DeepTrade < 10 %, NormalTrade < 50 %, AggTrade < 90 %,
/// FakTrade < 97 %, StopTrade ≥ 97 %.
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
        } else if pct < 0.50 {
            Self::NormalTrade
        } else if pct < 0.90 {
            Self::AggTrade
        } else if pct < 0.97 {
            Self::FakTrade
        } else {
            Self::StopTrade
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_pct_before_start() {
        assert_eq!(zone_pct(100, 200, 50), 0.0);
        assert_eq!(zone_pct(100, 200, 100), 0.0);
    }

    #[test]
    fn zone_pct_midpoint() {
        assert_eq!(zone_pct(100, 200, 150), 0.5);
    }

    #[test]
    fn zone_pct_after_end() {
        assert_eq!(zone_pct(100, 200, 250), 1.0);
    }

    #[test]
    fn market_zone_thresholds() {
        assert_eq!(MarketZone::from_pct(0.0), MarketZone::DeepTrade);
        assert_eq!(MarketZone::from_pct(0.09), MarketZone::DeepTrade);
        assert_eq!(MarketZone::from_pct(0.10), MarketZone::NormalTrade);
        assert_eq!(MarketZone::from_pct(0.49), MarketZone::NormalTrade);
        assert_eq!(MarketZone::from_pct(0.50), MarketZone::AggTrade);
        assert_eq!(MarketZone::from_pct(0.89), MarketZone::AggTrade);
        assert_eq!(MarketZone::from_pct(0.90), MarketZone::FakTrade);
        assert_eq!(MarketZone::from_pct(0.96), MarketZone::FakTrade);
        assert_eq!(MarketZone::from_pct(0.97), MarketZone::StopTrade);
        assert_eq!(MarketZone::from_pct(1.0), MarketZone::StopTrade);
    }

    #[test]
    fn t_minus_15_arithmetic() {
        assert_eq!(t_minus_15(1_000), 985);
        assert_eq!(t_minus_15(10), 0);
    }
}
