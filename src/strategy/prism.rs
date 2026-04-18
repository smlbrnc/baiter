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
    /// Global emir taban fiyatı — strateji içi proaktif clamp için (engine guard zaten reject eder).
    pub min_price: f64,
    /// Global emir tavan fiyatı — strateji içi proaktif clamp için.
    pub max_price: f64,
    /// Averaging cooldown (ms) — bot config'den gelir; ilerideki averaging mantığı için.
    pub cooldown_threshold: u64,
}

/// Faz 14'te doldurulacak.
pub fn decide(state: PrismState, _ctx: &PrismContext) -> (PrismState, Decision) {
    tracing::warn!("prism stratejisi henüz implement edilmedi (strategies.md §3 TBD)");
    (state, Decision::NoOp)
}
