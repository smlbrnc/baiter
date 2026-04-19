//! ProfitLock FAK çıktısı.

use crate::strategy::{Decision, PlannedOrder};
use crate::types::{OrderType, Outcome, Side};

use super::state::{HarvestContext, HarvestState};

pub fn profit_lock_fak(ctx: &HarvestContext) -> (HarvestState, Decision) {
    let imb = ctx.metrics.imbalance;
    if imb.abs() < f64::EPSILON {
        return (HarvestState::ProfitLock, Decision::NoOp);
    }
    // imb > 0 ⇒ YES tarafında fazla → karşı tarafa (NO) FAK; tersi simetrik.
    let excess_side = if imb > 0.0 { Outcome::Up } else { Outcome::Down };
    let hedge_side = excess_side.opposite();
    let fak = PlannedOrder {
        outcome: hedge_side,
        token_id: ctx.token_id(hedge_side).to_string(),
        side: Side::Buy,
        price: ctx.best_ask(hedge_side),
        size: imb.abs(),
        order_type: OrderType::Fak,
        reason: "harvest:profit_lock:fak".to_string(),
    };
    (HarvestState::ProfitLock, Decision::PlaceOrders(vec![fak]))
}
