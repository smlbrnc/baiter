//! Dutch Book strategy — TBD stub (Faz 14).
//!
//! Ayrıntılı FSM için [docs/strategies.md §1](../../../docs/strategies.md)
//! `strategies.md` TBD alanları doldurulduktan sonra Faz 14'te implement edilir.

use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::Decision;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DutchBookState {
    Pending,
    Done,
}

#[derive(Debug, Clone)]
pub struct DutchBookContext<'a> {
    pub metrics: &'a StrategyMetrics,
    /// Global emir taban fiyatı — strateji içi proaktif clamp için (engine guard zaten reject eder).
    pub min_price: f64,
    /// Global emir tavan fiyatı — strateji içi proaktif clamp için.
    pub max_price: f64,
    /// Averaging cooldown (ms) — bot config'den gelir; ilerideki averaging mantığı için.
    pub cooldown_threshold: u64,
}

/// Faz 14'te doldurulacak. Şu anda herhangi bir aksiyon üretmez.
pub fn decide(state: DutchBookState, _ctx: &DutchBookContext) -> (DutchBookState, Decision) {
    tracing::warn!("dutch_book stratejisi henüz implement edilmedi (strategies.md §1 TBD)");
    (state, Decision::NoOp)
}
