//! Strateji modülü: `metrics`, ortak tipler ve aktif stratejiler
//! (Bonereaper, Gravie, Arbitrage). Engine dispatch'i `StrategyState`
//! discriminated union üzerinden gider.

pub mod arbitrage;
pub mod bonereaper;
pub mod common;
pub mod gravie;
pub mod metrics;

pub use common::{Decision, OpenOrder, PlannedOrder, StrategyContext};

use serde::{Deserialize, Serialize};

use crate::types::Strategy;

use arbitrage::ArbitrageState;
use bonereaper::BonereaperState;
use gravie::GravieState;

/// `MarketSession` FSM state'i — aktif stratejinin discriminated union'ı.
///
/// Engine `clone()` ile state geçişi yapar (her tick küçük heap kopyalama).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyState {
    Bonereaper(BonereaperState),
    Gravie(GravieState),
    Arbitrage(ArbitrageState),
}

impl StrategyState {
    pub fn pending_for(strategy: Strategy) -> Self {
        match strategy {
            Strategy::Bonereaper => Self::Bonereaper(BonereaperState::default()),
            Strategy::Gravie => Self::Gravie(GravieState::default()),
            Strategy::Arbitrage => Self::Arbitrage(ArbitrageState::default()),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Bonereaper(_) => "Bonereaper",
            Self::Gravie(_) => "Gravie",
            Self::Arbitrage(_) => "Arbitrage",
        }
    }
}
