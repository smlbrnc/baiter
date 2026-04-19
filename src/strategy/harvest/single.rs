//! SingleLeg averaging — Harvest FSM'in en uzun fazı.

use crate::strategy::{order_size, planned_buy_gtc, Decision, OpenOrder, PlannedOrder};
use crate::time::MarketZone;
use crate::types::{Outcome, Side};

use super::profit_lock::profit_lock_fak;
use super::state::{HarvestContext, HarvestState};

pub fn single_leg(filled_side: Outcome, ctx: &HarvestContext) -> (HarvestState, Decision) {
    let first_leg = first_leg_avg(ctx, filled_side);
    let hedge_leg = ctx.best_ask(filled_side.opposite());

    // 1) ProfitLock öncelikli kontrol (averaging fazında korunur).
    if hedge_leg > 0.0 && first_leg + hedge_leg <= ctx.avg_threshold {
        return profit_lock_fak(ctx);
    }

    // 2) StopTrade bölgesinde yeni emir yok.
    if ctx.zone == MarketZone::StopTrade {
        return (HarvestState::SingleLeg { filled_side }, Decision::NoOp);
    }

    // 3) Açık averaging GTC varsa: cooldown_threshold'u geçtiyse cancel; aksi halde bekle.
    if let Some(decision) = handle_open_averaging(ctx, filled_side) {
        return (HarvestState::SingleLeg { filled_side }, decision);
    }

    // 4) Averaging koşulu.
    if let Some(order) = try_new_averaging(ctx, filled_side) {
        return (
            HarvestState::SingleLeg { filled_side },
            Decision::PlaceOrders(vec![order]),
        );
    }

    (HarvestState::SingleLeg { filled_side }, Decision::NoOp)
}

fn first_leg_avg(ctx: &HarvestContext, side: Outcome) -> f64 {
    match side {
        Outcome::Up => ctx.metrics.avg_yes,
        Outcome::Down => ctx.metrics.avg_no,
    }
}

/// Açık averaging GTC işlemleri (cancel veya bekle). `Some(decision)` döndürürse
/// caller bu kararı pas geçirmeli; `None` ise yeni averaging değerlendirmesi yapılır.
fn handle_open_averaging(ctx: &HarvestContext, filled_side: Outcome) -> Option<Decision> {
    let open_avg: Vec<&OpenOrder> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("harvest:averaging") && o.outcome == filled_side)
        .collect();
    if open_avg.is_empty() {
        return None;
    }
    // open_avg.is_empty() yukarıda False — `max()` daima `Some`.
    let max_age = open_avg
        .iter()
        .map(|o| ctx.now_ms.saturating_sub(o.placed_at_ms))
        .max()
        .expect("open_avg non-empty after early return");
    if max_age >= ctx.cooldown_threshold {
        let cancel_ids: Vec<String> = open_avg.iter().map(|o| o.id.clone()).collect();
        return Some(Decision::CancelOrders(cancel_ids));
    }
    Some(Decision::NoOp)
}

fn try_new_averaging(ctx: &HarvestContext, filled_side: Outcome) -> Option<PlannedOrder> {
    let first_best_leg = ctx.best_bid(filled_side);
    let pos_held = position_held_with_open(ctx, filled_side);

    let cooldown_ok = ctx.now_ms.saturating_sub(ctx.last_averaging_ms) >= ctx.cooldown_threshold;
    let price_fell = ctx.last_fill_price > 0.0 && first_best_leg < ctx.last_fill_price;
    let pos_ok = pos_held < ctx.max_position_size;

    if !(cooldown_ok && price_fell && pos_ok && first_best_leg > 0.0) {
        return None;
    }
    // Global price guard: averaging fiyatı [min_price, max_price] dışındaysa atlat.
    if first_best_leg < ctx.min_price || first_best_leg > ctx.max_price {
        return None;
    }
    let base = order_size(ctx.order_usdc, first_best_leg, ctx.api_min_order_size);
    let mult = ctx.signal_multiplier(filled_side);
    let effective = (base * mult).round().max(ctx.api_min_order_size);

    Some(planned_buy_gtc(
        filled_side,
        ctx.token_id(filled_side),
        first_best_leg,
        effective,
        format!("harvest:averaging:{:?}", filled_side),
    ))
}

/// `pos_held` = filled shares + aynı taraftaki açık BUY emirlerin notional size'ı.
/// `max_position_size` koruması LIVE emirleri de hesaba katmalı (aksi halde
/// kitapta birikmiş averaging GTC'leri sınır kontrolünden kaçar).
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
