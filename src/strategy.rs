//! Strateji modülü: `metrics`, ortak tipler ve aktif stratejiler
//! (Bonereaper, Gravie). Engine dispatch'i `StrategyState`
//! discriminated union üzerinden gider.

pub mod bonereaper;
pub mod common;
pub mod gravie;
pub mod metrics;

pub use common::{Decision, OpenOrder, PlannedOrder, StrategyContext};

use serde::{Deserialize, Serialize};

use crate::types::Strategy;

use bonereaper::BonereaperState;
use gravie::GravieState;

/// `MarketSession` FSM state'i — aktif stratejinin discriminated union'ı.
///
/// Engine `clone()` ile state geçişi yapar (her tick küçük heap kopyalama).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyState {
    Bonereaper(BonereaperState),
    Gravie(GravieState),
}

impl StrategyState {
    pub fn pending_for(strategy: Strategy) -> Self {
        match strategy {
            Strategy::Bonereaper => Self::Bonereaper(BonereaperState::default()),
            Strategy::Gravie => Self::Gravie(GravieState::default()),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Bonereaper(_) => "Bonereaper",
            Self::Gravie(_) => "Gravie",
        }
    }
}
