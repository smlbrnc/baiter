//! Strateji modülü: `metrics`, ortak tipler ve 3 aktif strateji (Alis, Elis,
//! Aras). Engine dispatch'i `StrategyState` discriminated union üzerinden gider.

pub mod alis;
pub mod aras;
pub mod common;
pub mod elis;
pub mod metrics;

pub use common::{Decision, OpenOrder, PlannedOrder, StrategyContext};

use serde::{Deserialize, Serialize};

use crate::types::Strategy;

use alis::AlisState;
use aras::ArasState;
use elis::ElisState;

/// `MarketSession` FSM state'i — aktif stratejinin discriminated union'ı.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StrategyState {
    Alis(AlisState),
    Elis(ElisState),
    Aras(ArasState),
}

impl StrategyState {
    pub fn pending_for(strategy: Strategy) -> Self {
        match strategy {
            Strategy::Alis => Self::Alis(AlisState::default()),
            Strategy::Elis => Self::Elis(ElisState::default()),
            Strategy::Aras => Self::Aras(ArasState::default()),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Alis(_) => "Alis",
            Self::Elis(_) => "Elis",
            Self::Aras(_) => "Aras",
        }
    }

    /// `self → next` profit-lock'a giriyorsa `Some(label)`; etiket `ProfitLocked`
    /// event'ine `lock_method` olarak gider.
    pub fn lock_transition_label(&self, next: &Self) -> Option<&'static str> {
        match (self, next) {
            (Self::Alis(prev), Self::Alis(AlisState::Locked))
                if !matches!(prev, AlisState::Locked) =>
            {
                Some(match prev {
                    AlisState::OpenPlaced { .. } => "symmetric_fill",
                    _ => "passive_hedge_fill",
                })
            }
            (Self::Elis(prev), Self::Elis(ElisState::Locked))
                if !matches!(prev, ElisState::Locked) =>
            {
                Some("pair_lock")
            }
            _ => None,
        }
    }
}
