//! Strateji modülü: `metrics`, ortak tipler ve aktif stratejiler
//! (Alis, Elis, Bonereaper, Gravie). Engine dispatch'i `StrategyState`
//! discriminated union üzerinden gider.

pub mod alis;
pub mod bonereaper;
pub mod common;
pub mod elis;
pub mod gravie;
pub mod metrics;

pub use common::{Decision, OpenOrder, PlannedOrder, StrategyContext};

use serde::{Deserialize, Serialize};

use crate::types::Strategy;

use alis::AlisState;
use bonereaper::BonereaperState;
use elis::ElisState;
use gravie::GravieState;

/// `MarketSession` FSM state'i — aktif stratejinin discriminated union'ı.
///
/// Not: `Copy` değil çünkü `ElisState::Pending` 20-tick `Vec` içeriyor.
/// Engine `clone()` ile state geçişi yapar (her tick küçük heap kopyalama).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyState {
    Alis(AlisState),
    Elis(ElisState),
    Bonereaper(BonereaperState),
    Gravie(GravieState),
}

impl StrategyState {
    pub fn pending_for(strategy: Strategy) -> Self {
        match strategy {
            Strategy::Alis => Self::Alis(AlisState::default()),
            Strategy::Elis => Self::Elis(ElisState::default()),
            Strategy::Bonereaper => Self::Bonereaper(BonereaperState::default()),
            Strategy::Gravie => Self::Gravie(GravieState::default()),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Alis(_) => "Alis",
            Self::Elis(_) => "Elis",
            Self::Bonereaper(_) => "Bonereaper",
            Self::Gravie(_) => "Gravie",
        }
    }
}
