//! Aras stratejisi — iskelet.
//!
//! State + decide stub'u. Henüz emir üretmez (`Decision::NoOp`); gerçek FSM
//! ve giriş/çıkış kuralları buraya eklenecek.

use serde::{Deserialize, Serialize};

use super::common::{Decision, StrategyContext};

/// Aras FSM state'i. İlk implementasyon iki durumla başlar; pencere bitince
/// `Done`'a geçer ve tick'ler NoOp döner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArasState {
    #[default]
    Pending,
    Done,
}

/// Aras karar motoru — saf fonksiyon: `(state, ctx) → (next_state, decision)`.
pub struct ArasEngine;

impl ArasEngine {
    pub fn decide(state: ArasState, _ctx: &StrategyContext<'_>) -> (ArasState, Decision) {
        // TODO: Aras stratejisinin giriş/avg/hedge kuralları buraya gelecek.
        (state, Decision::NoOp)
    }
}
