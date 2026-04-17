//! Zaman yardımcıları: unix ms/sec, T-15, zone_pct.
//!
//! Referans: [docs/bot-platform-mimari.md §4 §15](../../../docs/bot-platform-mimari.md).

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

/// Market başlangıcından 15 saniye önce (T-15) unix sn döner.
pub fn t_minus_15(market_start_ts: u64) -> u64 {
    market_start_ts.saturating_sub(15)
}

/// Market penceresinin % ilerleme oranı [0.0, 1.0].
///
/// `start_ts` ve `end_ts` unix saniye; `now` hesaba dahil edilerek pencere
/// içindeki ilerleme yüzdesi döner. Pencere öncesi 0.0, sonrası 1.0.
pub fn zone_pct(start_ts: u64, end_ts: u64, now: u64) -> f64 {
    if now <= start_ts {
        return 0.0;
    }
    if now >= end_ts {
        return 1.0;
    }
    let total = (end_ts - start_ts) as f64;
    if total <= 0.0 {
        return 1.0;
    }
    (now - start_ts) as f64 / total
}

/// Bölge hesabı — `MarketZone` enum ile uyumlu yüzde eşikleri.
///
/// Eşikler mimari §15 ile birebir: DeepTrade < 10 %, NormalTrade < 50 %,
/// AggTrade < 90 %, FakTrade < 97 %, StopTrade ≥ 97 %.
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
