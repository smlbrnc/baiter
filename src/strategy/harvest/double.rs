//! DoubleLeg fazı — OpenDual'da iki taraf da dolduğunda devreye girer.
//!
//! - `avg_sum > avg_threshold` → her iki tarafa bağımsız averaging GTC
//!   (multiplier 1.0, `price_fell` zorunlu).
//! - `avg_sum ≤ avg_threshold` + `|imbalance| ≥ api_min_order_size` → eksik
//!   tarafa `size = |imbalance|` GTC (price_fell BYPASS).
//! - `avg_sum ≤ avg_threshold` + `|imbalance| < api_min_order_size` → **Done**.
//! - `StopTrade` bölgesinde NoOp + DoubleLeg kalır.
//! - `max_position_size` cap (8A) bir tarafı dondurursa o tarafa avg basılmaz.

use crate::strategy::{planned_buy_gtc, Decision};
use crate::time::MarketZone;
use crate::types::Outcome;

use super::merge_decision;
use super::single::{handle_open_averaging_for_side, position_held_with_open, try_averaging_for_side};
use super::state::{HarvestContext, HarvestState};

pub fn double_leg(ctx: &HarvestContext) -> (HarvestState, Decision) {
    let avg_sum_ok = ctx.metrics.avg_yes > 0.0
        && ctx.metrics.avg_no > 0.0
        && ctx.metrics.avg_sum <= ctx.avg_threshold;
    let balanced = ctx.metrics.imbalance.abs() < ctx.api_min_order_size;

    if avg_sum_ok && balanced {
        return (HarvestState::Done, Decision::NoOp);
    }
    if ctx.zone == MarketZone::StopTrade {
        return (HarvestState::DoubleLeg, Decision::NoOp);
    }
    if avg_sum_ok {
        return imbalance_close_decision(ctx);
    }

    let mut place = Vec::new();
    let mut cancel = Vec::new();
    for side in [Outcome::Up, Outcome::Down] {
        match handle_open_averaging_for_side(ctx, side) {
            Some(Decision::CancelOrders(ids)) => cancel.extend(ids),
            Some(_) => {}
            None => {
                if let Some(o) = try_averaging_for_side(ctx, side, 1.0) {
                    place.push(o);
                }
            }
        }
    }
    (HarvestState::DoubleLeg, merge_decision(place, cancel))
}

/// avg_sum eşiği sağlanmış ama imbalance varken eksik tarafa `size = |imbalance|`
/// GTC açar. price_fell bypass; signal_multiplier yok.
fn imbalance_close_decision(ctx: &HarvestContext) -> (HarvestState, Decision) {
    let imb = ctx.metrics.imbalance;
    let short_side = if imb < 0.0 {
        Outcome::Up
    } else {
        Outcome::Down
    };

    let cooldown_ok = ctx.now_ms.saturating_sub(ctx.last_averaging_ms) >= ctx.cooldown_threshold;
    let price = ctx.best_bid(short_side);
    let price_ok = price > 0.0 && price >= ctx.min_price && price <= ctx.max_price;

    let pos_held = position_held_with_open(ctx, short_side);
    let cap = (ctx.max_position_size - pos_held).max(0.0);
    let size = imb.abs().min(cap).round();
    let size_ok = size >= ctx.api_min_order_size;

    let order = if cooldown_ok && price_ok && size_ok {
        Some(planned_buy_gtc(
            short_side,
            ctx.token_id(short_side),
            price,
            size,
            format!("harvest:averaging:{:?}", short_side),
        ))
    } else {
        None
    };

    let cancel = match handle_open_averaging_for_side(ctx, short_side) {
        Some(Decision::CancelOrders(ids)) => ids,
        Some(_) => return (HarvestState::DoubleLeg, Decision::NoOp),
        None => Vec::new(),
    };
    let place = order.map(|o| vec![o]).unwrap_or_default();
    (HarvestState::DoubleLeg, merge_decision(place, cancel))
}
