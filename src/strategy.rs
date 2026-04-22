//! Strateji modülü — `metrics` + ortak tipler + 3 aktif strateji (Alis, Elis, Aras).
//!
//! Her strateji kendi state enum'una ve karar motoruna sahip; engine
//! `StrategyState` discriminated union'ı üzerinden dispatch eder.

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

/// `MarketSession` içinde tutulan FSM state'i — aktif stratejinin discriminated union'ı.
/// `Strategy::from_default_state` her strateji tipi için Pending varyant üretir.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StrategyState {
    Alis(AlisState),
    Elis(ElisState),
    Aras(ArasState),
}

impl StrategyState {
    /// Bot başlangıcında `BotConfig.strategy`'ye göre default Pending state üret.
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
}
