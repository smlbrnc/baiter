//! Harvest strategy — sinyal güdümlü dual açılış + averaging FSM.
//!
//! Durumlar: [Pending] → [OpenDual{deadline}] → [SingleLeg] → [ProfitLock] → [Done]
//!
//! - OpenDual fazı: Binance `effective_score`'a göre simetrik fiyat
//!   (`up_bid + down_bid = 1.00`); ProfitLock burada **tetiklenmez**.
//! - SingleLeg fazı: averaging GTC + ProfitLock (avg_threshold) korunur.
//!
//! Alt modüller:
//! - [`state`]        — `HarvestState`, `HarvestContext`, sabitler.
//! - [`dual`]         — OpenDual fiyatı + Pending/OpenDual aşaması.
//! - [`single`]       — SingleLeg averaging.
//! - [`profit_lock`]  — ProfitLock FAK çıktısı.
//!
//! Referans: [docs/strategies.md §2](../../../docs/strategies.md).

use crate::strategy::{Decision, DecisionEngine, OpenOrder};

pub mod dual;
pub mod profit_lock;
pub mod single;
pub mod state;

pub use dual::dual_prices;
pub use state::{HarvestContext, HarvestState, MAX_POSITION_SIZE};

/// `DecisionEngine` marker — `MarketSession::tick` `Strategy::Harvest` kolunda
/// kullanılır. Free `decide` fonksiyonu trait üzerinden de çağrılabilir.
pub struct HarvestEngine;

impl DecisionEngine for HarvestEngine {
    type State = HarvestState;
    type Ctx<'a> = HarvestContext<'a>;

    fn decide(state: Self::State, ctx: &Self::Ctx<'_>) -> (Self::State, Decision) {
        decide(state, ctx)
    }
}

/// Verilen açık emir slice'ından `CancelOrders` kararı üretir; boşsa `NoOp`.
pub(crate) fn cancel_ids(open_orders: &[OpenOrder]) -> Decision {
    if open_orders.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(open_orders.iter().map(|o| o.id.clone()).collect())
    }
}

/// Merkezi FSM fonksiyonu — her olay sonrası çağrılır.
pub fn decide(state: HarvestState, ctx: &HarvestContext) -> (HarvestState, Decision) {
    match state {
        HarvestState::Pending => dual::open_dual(ctx),
        HarvestState::OpenDual { deadline_ms } => dual::evaluate_open_dual(ctx, deadline_ms),
        HarvestState::SingleLeg { filled_side } => single::single_leg(filled_side, ctx),
        HarvestState::ProfitLock | HarvestState::Done => (HarvestState::Done, Decision::NoOp),
    }
}

#[cfg(test)]
mod tests;
