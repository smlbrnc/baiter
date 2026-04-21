//! Strateji enum'u, `MarketZone` haritası ve `Decision` tipi (§11, §15).
//!
//! Alt modüller: [`metrics`] (PnL + agg), [`harvest`] (tam FSM), [`order`] (OpenOrder).
//! `Strategy::DutchBook` ve `Strategy::Prism` API/DB sözleşmesini bozmamak için
//! enum'da kalır; `bot/ctx.rs` `Harvest` dışındaki seçimleri start aşamasında
//! `AppError::Config` ile reddeder.

use serde::{Deserialize, Serialize};

use crate::time::MarketZone;
use crate::types::{OrderType, Outcome, Side};

pub mod dutch_book;
pub mod harvest;
pub mod metrics;
pub mod order;
pub mod prism;

pub use order::{planned_buy_gtc, OpenOrder};

/// Bölge başına sinyal aktifliği (§15.3). Mevcut implementasyon: `Harvest`.
#[derive(Debug, Clone, Copy)]
pub struct ZoneSignalMap(pub [bool; 5]);

impl ZoneSignalMap {
    pub const HARVEST: ZoneSignalMap = ZoneSignalMap([true, true, true, true, false]);

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

/// FSM strateji sözleşmesi: `(State, &Ctx) → (State, Decision)`.
/// `MarketSession::tick` `cfg.strategy` üzerinden uygun marker'ı seçer.
pub trait DecisionEngine {
    type State;
    type Ctx<'a>;
    fn decide(state: Self::State, ctx: &Self::Ctx<'_>) -> (Self::State, Decision);
}

/// `decide()` döndüğü aksiyon — engine tarafından yürütülür.
///
/// `CancelAndPlace` hedge re-pricing gibi senaryolarda eski emrin cancel'ı
/// ile yeni emrin placement'ı tek tick'te atomic olarak yapılsın diye ayrı
/// bir varyanttır. Executor önce cancel REST, sonra place REST sırasını
/// uygular; ayrı bir ara state beklenmez (doc §9 atomic re-price).
#[derive(Debug, Clone)]
pub enum Decision {
    NoOp,
    PlaceOrders(Vec<PlannedOrder>),
    CancelOrders(Vec<String>),
    CancelAndPlace {
        cancels: Vec<String>,
        places: Vec<PlannedOrder>,
    },
}

/// Strateji motorunun ürettiği emir planı; Live (CLOB REST) ve DryRun (Simulator)
/// modlarının ortak sözleşmesi.
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

/// Polymarket alt sınırı + bot tasarım kararı: tüm BUY emirlerinin notional'ı
/// (`size × price`) en az 1 USD olmalı. Çok düşük cost imbalance senaryolarında
/// (hedge re-place vs.) `(target_notional / price) < (1 / price)` ise size bu
/// minimuma yükseltilir; aksi takdirde emir Polymarket tarafından reddedilir
/// veya işlem başına fee oranı ekonomik anlamını yitirir.
pub const MIN_NOTIONAL_USD: f64 = 1.0;

/// Tüm stratejiler için emir boyutu formülü (`strategies.md § emir boyutu`).
/// `order_usdc` notional hedefi `price`'a bölünür; sonuç hem `api_min_order_size`
/// hem `MIN_NOTIONAL_USD / price` ile clamp edilir.
pub fn order_size(order_usdc: f64, price: f64, api_min_order_size: f64) -> f64 {
    let base = (order_usdc / price.max(1e-9)).ceil();
    let min_notional = MIN_NOTIONAL_USD / price.max(1e-9);
    base.max(api_min_order_size).max(min_notional)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_size_respects_min() {
        assert_eq!(order_size(5.0, 0.5, 1.0), 10.0);
        assert_eq!(order_size(0.1, 0.99, 5.0), 5.0);
    }

    #[test]
    fn order_size_clamps_to_min_notional_usd() {
        // order_usdc=0, api_min=0.1 → base=0 → fallback MIN_NOTIONAL_USD/price.
        let size = order_size(0.0, 0.50, 0.1);
        assert!(
            (size * 0.50 - MIN_NOTIONAL_USD).abs() < 1e-9,
            "notional={}",
            size * 0.50
        );
    }

    #[test]
    fn zone_signal_stop_trade_inactive_for_harvest() {
        assert!(!ZoneSignalMap::HARVEST.is_active(MarketZone::StopTrade));
        assert!(ZoneSignalMap::HARVEST.is_active(MarketZone::NormalTrade));
    }
}
