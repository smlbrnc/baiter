//! OpenDual fiyatlama + Pending/OpenDual aşaması.

use crate::strategy::{order_size, planned_buy_gtc, Decision};
use crate::types::Outcome;

use super::cancel_ids;
use super::state::{HarvestContext, HarvestState};

const SCORE_NEUTRAL: f64 = 5.0;
const INV_NEUTRAL: f64 = 1.0 / SCORE_NEUTRAL;

/// OpenDual bid'leri. `yes` ve `no` parametreleri `(bid, ask)` tuple'ı.
///
/// - `δ = (effective_score − 5) / 5 ∈ [−1, +1]`
/// - `up_bid   = clamp(snap(yes_ask + δ · (yes_ask − yes_bid)), min, max)`
/// - `down_bid = clamp(snap(no_ask  − δ · (no_ask  − no_bid)),  min, max)`
///
/// δ=0 → ikisi de ask'ta (taker eşiği). δ=±1 → bir taraf agresif taker, diğer pasif maker.
#[inline]
pub fn dual_prices(
    effective_score: f64,
    yes: (f64, f64),
    no: (f64, f64),
    tick_size: f64,
    min_price: f64,
    max_price: f64,
) -> (f64, f64) {
    let (yes_bid, yes_ask) = yes;
    let (no_bid, no_ask) = no;
    let inv_tick = tick_size.recip();
    let delta = (effective_score - SCORE_NEUTRAL) * INV_NEUTRAL;
    let yes_spread = (yes_ask - yes_bid).max(0.0);
    let no_spread = (no_ask - no_bid).max(0.0);
    let up_raw = yes_ask + delta * yes_spread;
    let down_raw = no_ask - delta * no_spread;
    let up = ((up_raw * inv_tick).round() * tick_size).clamp(min_price, max_price);
    let down = ((down_raw * inv_tick).round() * tick_size).clamp(min_price, max_price);
    (up, down)
}

#[inline]
pub fn open_dual(ctx: &HarvestContext) -> (HarvestState, Decision) {
    if ctx.yes_best_bid <= 0.0 || ctx.no_best_bid <= 0.0 {
        return (HarvestState::Pending, Decision::NoOp);
    }

    let (up_bid, down_bid) = dual_prices(
        ctx.effective_score,
        (ctx.yes_best_bid, ctx.yes_best_ask),
        (ctx.no_best_bid, ctx.no_best_ask),
        ctx.tick_size,
        ctx.min_price,
        ctx.max_price,
    );

    let up_size = order_size(ctx.order_usdc, up_bid, ctx.api_min_order_size);
    let down_size = order_size(ctx.order_usdc, down_bid, ctx.api_min_order_size);

    let orders = vec![
        planned_buy_gtc(
            Outcome::Up,
            ctx.token_id(Outcome::Up),
            up_bid,
            up_size,
            "harvest:open_dual:yes",
        ),
        planned_buy_gtc(
            Outcome::Down,
            ctx.token_id(Outcome::Down),
            down_bid,
            down_size,
            "harvest:open_dual:no",
        ),
    ];

    (
        HarvestState::OpenDual {
            deadline_ms: ctx.now_ms + ctx.dual_timeout,
        },
        Decision::PlaceOrders(orders),
    )
}

#[inline]
pub fn evaluate_open_dual(ctx: &HarvestContext, deadline_ms: u64) -> (HarvestState, Decision) {
    let yes_filled = ctx.metrics.shares_yes > 0.0;
    let no_filled = ctx.metrics.shares_no > 0.0;
    let timed_out = ctx.now_ms >= deadline_ms;

    let next = match (yes_filled, no_filled, timed_out) {
        (true, true, _) => HarvestState::SingleLeg {
            filled_side: if ctx.effective_score >= SCORE_NEUTRAL {
                Outcome::Up
            } else {
                Outcome::Down
            },
        },
        (true, false, true) => HarvestState::SingleLeg {
            filled_side: Outcome::Up,
        },
        (false, true, true) => HarvestState::SingleLeg {
            filled_side: Outcome::Down,
        },
        (false, false, true) => HarvestState::Pending,
        _ => return (HarvestState::OpenDual { deadline_ms }, Decision::NoOp),
    };
    (next, cancel_ids(ctx.open_orders))
}
