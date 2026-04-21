//! Harvest v2 — bölge bazlı dual davranış FSM
//! (`Pending → OpenPair → PositionOpen → PairComplete → Done`).

use crate::strategy::{Decision, DecisionEngine, OpenOrder};
use crate::time::MarketZone;

pub mod open_pair;
pub mod position_open;
pub mod state;

pub use state::{is_averaging_like, HarvestContext, HarvestState};

pub struct HarvestEngine;

impl DecisionEngine for HarvestEngine {
    type State = HarvestState;
    type Ctx<'a> = HarvestContext<'a>;

    fn decide(state: Self::State, ctx: &Self::Ctx<'_>) -> (Self::State, Decision) {
        decide(state, ctx)
    }
}

pub(crate) fn cancel_all(open_orders: &[OpenOrder]) -> Decision {
    if open_orders.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(open_orders.iter().map(|o| o.id.clone()).collect())
    }
}

pub(crate) fn decide(state: HarvestState, ctx: &HarvestContext) -> (HarvestState, Decision) {
    if matches!(ctx.zone, MarketZone::StopTrade) {
        return stop_trade(state, ctx);
    }
    match state {
        HarvestState::Pending => open_pair::pending(ctx),
        HarvestState::OpenPair => open_pair::monitor(ctx),
        HarvestState::PositionOpen { filled_side } => position_open::handle(filled_side, ctx),
        HarvestState::ProfitLocked { filled_side } => {
            (HarvestState::ProfitLocked { filled_side }, Decision::NoOp)
        }
        HarvestState::PairComplete => (HarvestState::Done, cancel_all(ctx.open_orders)),
        HarvestState::Done => (HarvestState::Done, Decision::NoOp),
    }
}

fn stop_trade(state: HarvestState, ctx: &HarvestContext) -> (HarvestState, Decision) {
    match state {
        HarvestState::Done => (HarvestState::Done, Decision::NoOp),
        _ => (HarvestState::Done, cancel_all(ctx.open_orders)),
    }
}
