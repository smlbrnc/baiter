//! Harvest strategy — sinyal güdümlü dual açılış + averaging FSM.
//!
//! `Pending → OpenDual{deadline} → {SingleLeg | DoubleLeg} → Done`.
//! Detay: [docs/strategies.md §2](../../../docs/strategies.md).

use crate::strategy::{Decision, DecisionEngine, OpenOrder, PlannedOrder};

pub mod double;
pub mod dual;
pub mod profit_lock;
pub mod single;
pub mod state;

pub use dual::dual_prices;
pub use state::{HarvestContext, HarvestState, MAX_POSITION_SIZE};

pub struct HarvestEngine;

impl DecisionEngine for HarvestEngine {
    type State = HarvestState;
    type Ctx<'a> = HarvestContext<'a>;

    fn decide(state: Self::State, ctx: &Self::Ctx<'_>) -> (Self::State, Decision) {
        decide(state, ctx)
    }
}

/// Açık emirler için `CancelOrders` veya `NoOp`.
pub(crate) fn cancel_ids(open_orders: &[OpenOrder]) -> Decision {
    if open_orders.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(open_orders.iter().map(|o| o.id.clone()).collect())
    }
}

/// (place, cancel) listelerinden tek bir `Decision` türetir.
fn merge_decision(place: Vec<PlannedOrder>, cancel: Vec<String>) -> Decision {
    match (place.is_empty(), cancel.is_empty()) {
        (true, true) => Decision::NoOp,
        (false, true) => Decision::PlaceOrders(place),
        (true, false) => Decision::CancelOrders(cancel),
        (false, false) => Decision::Batch { cancel, place },
    }
}

pub(crate) fn decide(state: HarvestState, ctx: &HarvestContext) -> (HarvestState, Decision) {
    match state {
        HarvestState::Pending => dual::open_dual(ctx),
        HarvestState::OpenDual { deadline_ms } => dual::evaluate_open_dual(ctx, deadline_ms),
        HarvestState::SingleLeg {
            filled_side,
            entered_at_ms,
        } => single::single_leg(filled_side, entered_at_ms, ctx),
        HarvestState::DoubleLeg => double::double_leg(ctx),
        HarvestState::ProfitLock | HarvestState::Done => (HarvestState::Done, Decision::NoOp),
    }
}

#[cfg(test)]
mod tests;
