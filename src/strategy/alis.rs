//! Alis stratejisi — iskelet.
//!
//! State + decide stub'u. Henüz emir üretmez (`Decision::NoOp`); gerçek FSM
//! ve giriş/çıkış kuralları buraya eklenecek.

use serde::{Deserialize, Serialize};

use super::common::{Decision, StrategyContext};

/// Alis FSM state'i. İlk implementasyon iki durumla başlar; pencere bitince
/// `Done`'a geçer ve tick'ler NoOp döner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AlisState {
    #[default]
    Pending,
    Done,
}

/// Alis karar motoru — saf fonksiyon (no internal mutation): `(state, ctx)
/// → (next_state, decision)`.
pub struct AlisEngine;

impl AlisEngine {
    pub fn decide(state: AlisState, _ctx: &StrategyContext<'_>) -> (AlisState, Decision) {
        // TODO: Alis stratejisinin giriş/avg/hedge kuralları buraya gelecek.
        (state, Decision::NoOp)
    }
}
