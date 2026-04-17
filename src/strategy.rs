//! Strateji enum'u, MetricMask, MarketZone haritası ve Decision tipi.
//!
//! Alt modüller: `metrics` (yardımcı), `harvest` (tam FSM),
//! `dutch_book` + `prism` (stub).
//!
//! Referans: [docs/bot-platform-mimari.md §11 §15](../../../docs/bot-platform-mimari.md).

use serde::{Deserialize, Serialize};

use crate::time::MarketZone;
use crate::types::{OrderType, Outcome, Side, Strategy};

pub mod dutch_book;
pub mod harvest;
pub mod metrics;
pub mod prism;

/// Strategy başına hangi metrikler hesaplanmalı.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct MetricMask {
    pub imbalance: bool,
    pub imbalance_cost: bool,
    pub avgsum: bool,
    pub profit: bool,
    pub sum_volume: bool,
    pub binance_signal: bool,
}

impl MetricMask {
    /// Geçerlilik: `profit == true` ⇒ `avgsum == true`.
    pub const fn is_valid(self) -> bool {
        !(self.profit && !self.avgsum)
    }
}

/// Strateji metrik maskesi (mimari §11).
pub fn required_metrics(strategy: Strategy) -> MetricMask {
    match strategy {
        Strategy::DutchBook => MetricMask {
            imbalance: true,
            imbalance_cost: true,
            avgsum: true,
            profit: true,
            sum_volume: true,
            binance_signal: true,
        },
        Strategy::Harvest => MetricMask {
            imbalance: true,
            imbalance_cost: false,
            avgsum: true,
            profit: false,
            sum_volume: true,
            binance_signal: true,
        },
        Strategy::Prism => MetricMask {
            imbalance: false,
            imbalance_cost: false,
            avgsum: true,
            profit: true,
            sum_volume: true,
            binance_signal: true,
        },
    }
}

/// Bölge başına sinyal aktifliği (§15.3).
#[derive(Debug, Clone, Copy)]
pub struct ZoneSignalMap(pub [bool; 5]);

impl ZoneSignalMap {
    pub const HARVEST: ZoneSignalMap = ZoneSignalMap([true, true, true, true, false]);
    pub const DUTCH_BOOK: ZoneSignalMap = ZoneSignalMap([true, true, true, true, false]);
    pub const PRISM: ZoneSignalMap = ZoneSignalMap([false, true, true, true, false]);

    pub fn is_active(&self, zone: MarketZone) -> bool {
        let idx = match zone {
            MarketZone::DeepTrade => 0,
            MarketZone::NormalTrade => 1,
            MarketZone::AggTrade => 2,
            MarketZone::FakTrade => 3,
            MarketZone::StopTrade => 4,
        };
        self.0[idx]
    }
}

/// Decide() döndüğü aksiyon — engine tarafından yürütülür.
#[derive(Debug, Clone)]
pub enum Decision {
    NoOp,
    PlaceOrders(Vec<PlannedOrder>),
    CancelOrders(Vec<String>),
    Batch {
        cancel: Vec<String>,
        place: Vec<PlannedOrder>,
    },
    Complete,
}

/// Strateji motoru tarafından üretilen emir planı.
/// Engine bunu hem Live (CLOB REST) hem DryRun (Simulator) modunda yürütür.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedOrder {
    pub outcome: Outcome,
    pub token_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub order_type: OrderType,
    pub reason: String,
}

/// Emir boyutu formülü — tüm stratejiler (strategies.md § emir boyutu).
pub fn order_size(order_usdc: f64, price: f64, api_min_order_size: f64) -> f64 {
    let base = (order_usdc / price.max(1e-9)).ceil();
    base.max(api_min_order_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_validity() {
        assert!(MetricMask::default().is_valid());
        assert!(!MetricMask {
            profit: true,
            avgsum: false,
            ..Default::default()
        }
        .is_valid());
        assert!(MetricMask {
            profit: true,
            avgsum: true,
            ..Default::default()
        }
        .is_valid());
    }

    #[test]
    fn harvest_mask() {
        let m = required_metrics(Strategy::Harvest);
        assert!(m.avgsum);
        assert!(!m.profit);
        assert!(m.imbalance);
    }

    #[test]
    fn order_size_respects_min() {
        let sz = order_size(5.0, 0.5, 1.0);
        assert_eq!(sz, 10.0);
        let sz2 = order_size(0.1, 0.99, 5.0);
        assert_eq!(sz2, 5.0);
    }

    #[test]
    fn zone_signal_stop_trade_inactive_for_harvest() {
        assert!(!ZoneSignalMap::HARVEST.is_active(MarketZone::StopTrade));
        assert!(ZoneSignalMap::HARVEST.is_active(MarketZone::NormalTrade));
    }
}
