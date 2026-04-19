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
    let (token_id, price) = match hedge_side {
        Outcome::Up => (ctx.yes_token_id, ctx.yes_best_ask),
        Outcome::Down => (ctx.no_token_id, ctx.no_best_ask),
    };
    let fak = PlannedOrder {
        outcome: hedge_side,
        token_id: token_id.to_string(),
        side: Side::Buy,
        price,
        size: imb.abs(),
        order_type: OrderType::Fak,
        reason: "harvest:profit_lock:fak".to_string(),
    };
    (HarvestState::ProfitLock, Decision::PlaceOrders(vec![fak]))
}
