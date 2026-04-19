//! SingleLeg averaging — Harvest FSM'in en uzun fazı.

use crate::strategy::{order_size, planned_buy_gtc, Decision, OpenOrder, PlannedOrder};
use crate::time::MarketZone;
use crate::types::{Outcome, Side};

use super::profit_lock::profit_lock_fak;
use super::state::{HarvestContext, HarvestState};

pub fn single_leg(
    filled_side: Outcome,
    entered_at_ms: u64,
    ctx: &HarvestContext,
) -> (HarvestState, Decision) {
    let same = HarvestState::SingleLeg {
        filled_side,
        entered_at_ms,
    };
    let first_leg = match filled_side {
        Outcome::Up => ctx.metrics.avg_yes,
        Outcome::Down => ctx.metrics.avg_no,
    };
    let hedge_leg = ctx.best_ask(filled_side.opposite());

    let warm = ctx.now_ms.saturating_sub(entered_at_ms) >= ctx.cooldown_threshold;
    if warm && hedge_leg > 0.0 && first_leg + hedge_leg <= ctx.avg_threshold {
        return profit_lock_fak(ctx);
    }

    if ctx.zone == MarketZone::StopTrade {
        return (same, Decision::NoOp);
    }

    if let Some(decision) = handle_open_averaging_for_side(ctx, filled_side) {
        return (same, decision);
    }

    let mult = ctx.signal_multiplier(filled_side);
    if let Some(order) = try_averaging_for_side(ctx, filled_side, mult) {
        return (same, Decision::PlaceOrders(vec![order]));
    }

    (same, Decision::NoOp)
}

/// Açık averaging GTC'leri için karar (cancel veya bekle). `Some` → caller pas
/// geçmeli; `None` → yeni averaging değerlendirilebilir.
pub(super) fn handle_open_averaging_for_side(
    ctx: &HarvestContext,
    side: Outcome,
) -> Option<Decision> {
    let open_avg: Vec<&OpenOrder> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("harvest:averaging") && o.outcome == side)
        .collect();
    if open_avg.is_empty() {
        return None;
    }
    let max_age = open_avg
        .iter()
        .map(|o| ctx.now_ms.saturating_sub(o.placed_at_ms))
        .max()
        .expect("non-empty");
    if max_age >= ctx.cooldown_threshold {
        let cancel_ids: Vec<String> = open_avg.iter().map(|o| o.id.clone()).collect();
        return Some(Decision::CancelOrders(cancel_ids));
    }
    Some(Decision::NoOp)
}

/// Tek tarafa averaging GTC üretmeye çalışır. `signal_mult` baz size'a uygulanır
/// (SingleLeg: `ctx.signal_multiplier(side)`; DoubleLeg: `1.0`).
pub(super) fn try_averaging_for_side(
    ctx: &HarvestContext,
    side: Outcome,
    signal_mult: f64,
) -> Option<PlannedOrder> {
    let first_best_leg = ctx.best_bid(side);
    let pos_held = position_held_with_open(ctx, side);
    let last_fill_price = match side {
        Outcome::Up => ctx.metrics.last_fill_price_yes,
        Outcome::Down => ctx.metrics.last_fill_price_no,
    };

    let cooldown_ok = ctx.now_ms.saturating_sub(ctx.last_averaging_ms) >= ctx.cooldown_threshold;
    let price_fell = last_fill_price > 0.0 && first_best_leg < last_fill_price;
    let pos_ok = pos_held < ctx.max_position_size;
    let price_in_band = first_best_leg >= ctx.min_price && first_best_leg <= ctx.max_price;

    if !(cooldown_ok && price_fell && pos_ok && first_best_leg > 0.0 && price_in_band) {
        return None;
    }
    let base = order_size(ctx.order_usdc, first_best_leg, ctx.api_min_order_size);
    let effective = (base * signal_mult).round().max(ctx.api_min_order_size);

    Some(planned_buy_gtc(
        side,
        ctx.token_id(side),
        first_best_leg,
        effective,
        format!("harvest:averaging:{:?}", side),
    ))
}

/// `pos_held` = filled shares + aynı taraftaki açık BUY emirlerin notional size'ı.
/// `max_position_size` LIVE emirleri de hesaba katar (book'taki averaging GTC'leri
/// sınır kontrolünden kaçmasın diye).
pub(super) fn position_held_with_open(ctx: &HarvestContext, side: Outcome) -> f64 {
    let filled = match side {
        Outcome::Up => ctx.metrics.shares_yes,
        Outcome::Down => ctx.metrics.shares_no,
    };
    let open: f64 = ctx
        .open_orders
        .iter()
        .filter(|o| o.outcome == side && o.side == Side::Buy)
        .map(|o| o.size)
        .sum();
    filled + open
}
