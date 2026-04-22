//! Elis stratejisi — iskelet.
//!
//! State + decide stub'u. Henüz emir üretmez (`Decision::NoOp`); gerçek FSM
//! ve giriş/çıkış kuralları buraya eklenecek.

use serde::{Deserialize, Serialize};

use super::common::{Decision, StrategyContext};

/// Elis FSM state'i. İlk implementasyon iki durumla başlar; pencere bitince
/// `Done`'a geçer ve tick'ler NoOp döner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ElisState {
    #[default]
    Pending,
    Done,
}

/// Elis karar motoru — saf fonksiyon: `(state, ctx) → (next_state, decision)`.
pub struct ElisEngine;

impl ElisEngine {
    pub fn decide(state: ElisState, _ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        // TODO: Elis stratejisinin giriş/avg/hedge kuralları buraya gelecek.
        (state, Decision::NoOp)
    }
}
