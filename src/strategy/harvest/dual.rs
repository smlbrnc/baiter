//! OpenDual fiyatlama + Pending/OpenDual aşaması.

use crate::strategy::{order_size, planned_buy_gtc, Decision};
use crate::types::Outcome;

use super::cancel_ids;
use super::state::{HarvestContext, HarvestState};

/// OpenDual bid'leri — composite skoru doğrudan hedef olasılığa eşler.
/// Orderbook'tan bağımsız, tamamen sinyale göre fiyatlandırma.
///
/// - `up_price   = clamp(snap(composite / 10),       min, max)`
/// - `down_price = clamp(snap(1 − composite / 10),   min, max)`
///
/// composite=5 (nötr) → 0.50/0.50.
/// composite=10 (full UP) → 0.95/0.05 (clamp ile sınırlı).
/// composite=0 (full DOWN) → 0.05/0.95.
///
/// Sonuç: market fiyatına göre kayma değil, sinyalin söylediği "olasılık" kadar bid.
/// Sinyal yönünde fiyat market'in ötesindeyse taker (anında dolar);
/// karşı yönde fiyat market'in altındaysa pasif (sinyal doğrulanırsa dolar).
#[inline]
pub fn dual_prices(
    effective_score: f64,
    tick_size: f64,
    min_price: f64,
    max_price: f64,
) -> (f64, f64) {
    let inv_tick = tick_size.recip();
    let up_raw = (effective_score / 10.0).clamp(0.0, 1.0);
    let down_raw = 1.0 - up_raw;
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
        (true, true, _) => HarvestState::DoubleLeg,
        (true, false, true) => HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: ctx.now_ms,
        },
        (false, true, true) => HarvestState::SingleLeg {
            filled_side: Outcome::Down,
            entered_at_ms: ctx.now_ms,
        },
        (false, false, true) => HarvestState::Pending,
        _ => return (HarvestState::OpenDual { deadline_ms }, Decision::NoOp),
    };
    (next, cancel_ids(ctx.open_orders))
}
