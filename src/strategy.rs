//! Strateji enum'u, MarketZone haritası ve Decision tipi.
//!
//! Alt modüller: `metrics` (PnL + agg), `harvest` (tam FSM), `order` (OpenOrder).
//! `Strategy::DutchBook` ve `Strategy::Prism` API/DB sözleşmesini bozmamak için
//! enum'da kalır; bot/ctx.rs `Strategy::Harvest` dışındaki seçimleri start aşamasında
//! `AppError::Config` ile reddeder (doc §11 sözleşmesi).
//!
//! Referans: [docs/bot-platform-mimari.md §11 §15](../../../docs/bot-platform-mimari.md).

use serde::{Deserialize, Serialize};

use crate::time::MarketZone;
use crate::types::{OrderType, Outcome, Side};

pub mod dutch_book;
pub mod harvest;
pub mod metrics;
pub mod order;
pub mod prism;

pub use order::{planned_buy_gtc, OpenOrder};

/// Bölge başına sinyal aktifliği (doc §15.3). Yalnızca aktif strateji `Harvest`.
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

/// Strateji "decide step" sözleşmesi — her FSM strateji
/// `(State, &Context) → (State, Decision)` imzasını sağlamalıdır.
///
/// `MarketSession::tick` `cfg.strategy` üzerinden uygun marker'ı seçer ve
/// karşılığında bu trait'i çağırır. Mevcut `harvest::HarvestEngine` tek aktif
/// implementor; `dutch_book`/`prism` gerçek FSM doldurulduğunda kendi
/// marker'larını ekleyip aynı trait'i sağlayacaklardır.
pub trait DecisionEngine {
    type State;
    type Ctx<'a>;
    fn decide(state: Self::State, ctx: &Self::Ctx<'_>) -> (Self::State, Decision);
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

/// Averaging cooldown **default** değeri (ms). Asıl değer her bot için
/// `BotConfig::cooldown_threshold` alanından okunur ve strateji
/// context'lerine geçirilir; bu sabit yalnızca testler ve fallback amaçlıdır.
pub const COOLDOWN_THRESHOLD_DEFAULT: u64 = 30_000;

#[cfg(test)]
mod tests {
    use super::*;

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
