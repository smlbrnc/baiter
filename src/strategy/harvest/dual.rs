//! OpenDual fiyatı + Pending/OpenDual aşaması.

use crate::strategy::{order_size, planned_buy_gtc, Decision, OpenOrder};
use crate::types::Outcome;

use super::state::{HarvestContext, HarvestState};

/// Nötr (sinyal=5) OpenDual başlangıç fiyatı — Up/Down simetrik.
const DUAL_BASE_PRICE: f64 = 0.50;
/// Sinyal saturasyonunda (delta=±1) `DUAL_BASE_PRICE` etrafında uygulanan max kayma.
const DUAL_PRICE_SPREAD: f64 = 0.25;
/// `effective_score` aralığı [0, 10]; delta = (s − NEUTRAL) / NEUTRAL ∈ [−1, +1].
const SCORE_NEUTRAL: f64 = 5.0;

/// Sinyale göre simetrik OpenDual fiyatları — toplam her zaman `1.00`.
///
/// `s = effective_score` ∈ [0, 10], nötr 5; `delta = (s − 5) / 5` ∈ [−1, +1].
/// - `up_bid   = 0.50 + delta · 0.25`  → s=10 ⇒ 0.75, s=0 ⇒ 0.25, s=5 ⇒ 0.50
/// - `down_bid = 0.50 − delta · 0.25`  → s=10 ⇒ 0.25, s=0 ⇒ 0.75, s=5 ⇒ 0.50
/// - `up_bid + down_bid = 1.00` her durumda → dual fazda ProfitLock asla tetiklenmez.
/// - Çıktı `tick_size`'a snap edilir.
pub fn dual_prices(effective_score: f64, tick_size: f64) -> (f64, f64) {
    let snap = |p: f64| (p / tick_size).round() * tick_size;
    let delta = (effective_score - SCORE_NEUTRAL) / SCORE_NEUTRAL;
    let up_raw = DUAL_BASE_PRICE + delta * DUAL_PRICE_SPREAD;
    let down_raw = DUAL_BASE_PRICE - delta * DUAL_PRICE_SPREAD;
    (snap(up_raw), snap(down_raw))
}

pub fn open_dual(ctx: &HarvestContext) -> (HarvestState, Decision) {
    // Book-ready gate: market quote'u gelmeden emir spam'lamayalım.
    if ctx.yes_best_bid <= 0.0 || ctx.no_best_bid <= 0.0 {
        return (HarvestState::Pending, Decision::NoOp);
    }
    let (up_bid, down_bid) = dual_prices(ctx.effective_score, ctx.tick_size);
    let leg = |outcome: Outcome, price: f64, reason: &str| {
        let size = order_size(ctx.order_usdc, price, ctx.api_min_order_size);
        planned_buy_gtc(outcome, ctx.token_id(outcome), price, size, reason)
    };
    let orders = vec![
        leg(Outcome::Up, up_bid, "harvest:open_dual:yes"),
        leg(Outcome::Down, down_bid, "harvest:open_dual:no"),
    ];

    let deadline_ms = ctx.now_ms + ctx.dual_timeout;
    (
        HarvestState::OpenDual { deadline_ms },
        Decision::PlaceOrders(orders),
    )
}

pub fn evaluate_open_dual(ctx: &HarvestContext, deadline_ms: u64) -> (HarvestState, Decision) {
    let yes_filled = ctx.metrics.shares_yes > 0.0;
    let no_filled = ctx.metrics.shares_no > 0.0;
    let timed_out = ctx.now_ms >= deadline_ms;

    match (yes_filled, no_filled, timed_out) {
        (true, true, _) => {
            let side = if ctx.effective_score >= 5.0 {
                Outcome::Up
            } else {
                Outcome::Down
            };
            (
                HarvestState::SingleLeg { filled_side: side },
                cancel_open(ctx.open_orders),
            )
        }
        (true, false, true) => (
            HarvestState::SingleLeg {
                filled_side: Outcome::Up,
            },
            cancel_open(ctx.open_orders),
        ),
        (false, true, true) => (
            HarvestState::SingleLeg {
                filled_side: Outcome::Down,
            },
            cancel_open(ctx.open_orders),
        ),
        (false, false, true) => (HarvestState::Pending, cancel_open(ctx.open_orders)),
        _ => (HarvestState::OpenDual { deadline_ms }, Decision::NoOp),
    }
}

fn cancel_open(open_orders: &[OpenOrder]) -> Decision {
    super::cancel_ids(open_orders)
}
