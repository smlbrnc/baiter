//! Prism strategy — TBD stub (Faz 14).
//!
//! Ayrıntılı FSM için [docs/strategies.md §3](../../../docs/strategies.md)
//! `strategies.md` TBD alanları doldurulduktan sonra Faz 14'te implement edilir.

use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::Decision;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrismState {
    Pending,
    Done,
}

#[derive(Debug, Clone)]
pub struct PrismContext<'a> {
    pub metrics: &'a StrategyMetrics,
}

/// Faz 14'te doldurulacak.
pub fn decide(state: PrismState, _ctx: &PrismContext) -> (PrismState, Decision) {
    tracing::warn!("prism stratejisi henüz implement edilmedi (strategies.md §3 TBD)");
    (state, Decision::NoOp)
}
