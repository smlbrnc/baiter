//! `Pending` açılışı ve `OpenPair` fill monitoring (doc §4, §5).

use crate::strategy::{order_size, planned_buy_gtc, Decision};
use crate::types::Outcome;

use super::state::{
    HarvestContext, HarvestState, HEDGE_REASON_PREFIX, OPEN_REASON_PREFIX,
};

/// `Pending` → `OpenPair`: sinyal yönüne göre opener + ProfitLock hedge (doc §5).
pub fn pending(ctx: &HarvestContext) -> (HarvestState, Decision) {
    // Sinyal hazır değil (RTDS window_open daha gelmedi) → opener basma.
    // Bir sonraki tick'te (RTDS event'i set olduktan sonra) tekrar denenir.
    if !ctx.signal_ready {
        return (HarvestState::Pending, Decision::NoOp);
    }
    if ctx.yes_best_bid <= 0.0 || ctx.no_best_bid <= 0.0 {
        return (HarvestState::Pending, Decision::NoOp);
    }
    let (open_side, open_price) = open_price(ctx);
    let hedge_side = open_side.opposite();
    let hedge_price = ctx.snap_clamp(ctx.avg_threshold - open_price);

    // Hedge size opener size'ına eşit: pair'i her zaman dengeli açar, opener fill'inde
    // PairComplete'a `imbalance ≈ 0` ile gidilir (doc §5 + §10 imbalance kuralı).
    let open_size = order_size(ctx.order_usdc, open_price, ctx.api_min_order_size);
    let hedge_size = open_size;

    let orders = vec![
        planned_buy_gtc(
            open_side,
            ctx.token_id(open_side),
            open_price,
            open_size,
            format!("{}{}", OPEN_REASON_PREFIX, open_side.as_lowercase()),
        ),
        planned_buy_gtc(
            hedge_side,
            ctx.token_id(hedge_side),
            hedge_price,
            hedge_size,
            format!("{}{}", HEDGE_REASON_PREFIX, hedge_side.as_lowercase()),
        ),
    ];
    (HarvestState::OpenPair, Decision::PlaceOrders(orders))
}

/// `OpenPair` monitor: shares durumuna göre `PositionOpen`/`PairComplete`/`OpenPair`
/// transition (doc §4). Decision daima `NoOp` — akış karar anlarını tetiklemez.
pub fn monitor(ctx: &HarvestContext) -> (HarvestState, Decision) {
    let up_filled = ctx.shares(Outcome::Up) > 0.0;
    let down_filled = ctx.shares(Outcome::Down) > 0.0;
    let next = match (up_filled, down_filled) {
        (true, true) => HarvestState::PairComplete,
        (true, false) => HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        (false, true) => HarvestState::PositionOpen {
            filled_side: Outcome::Down,
        },
        (false, false) => HarvestState::OpenPair,
    };
    (next, Decision::NoOp)
}

/// Doc §5 opener fiyatı + yön:
/// - Nötr (score ≈ 5)        → `Up @ yes_bid` (pasif maker)
/// - Bullish (score > 5)     → `Up @ yes_ask + delta(Up)` (taker)
/// - Bearish (score < 5)     → `Down @ no_ask + |delta(Down)|` (taker)
fn open_price(ctx: &HarvestContext) -> (Outcome, f64) {
    let diff = ctx.effective_score - 5.0;
    let (side, raw) = if diff.abs() < f64::EPSILON {
        (Outcome::Up, ctx.yes_best_bid)
    } else if diff > 0.0 {
        (Outcome::Up, ctx.yes_best_ask + ctx.delta(Outcome::Up))
    } else {
        (
            Outcome::Down,
            ctx.no_best_ask + ctx.delta(Outcome::Down).abs(),
        )
    };
    (side, ctx.snap_clamp(raw))
}
