//! `PositionOpen` — avg-down + pyramid + hedge drift / missing / flip
//! (doc §7, §8, §9, §10, §11).
//!
//! Hedge tarafı dinamik: `majority_side().opposite()`. Avg-down ve pyramid'ler
//! atomic `Decision::CancelAndPlace` ile hedge re-place'i ile birlikte gönderilir.

use crate::strategy::{planned_buy_gtc, Decision, PlannedOrder, MIN_NOTIONAL_USD};
use crate::time::MarketZone;
use crate::types::{OrderType, Outcome, Side};

use super::state::{
    avg_down_reason, hedge_reason, pyramid_reason, HarvestContext, HarvestState,
};

/// Opportunistic taker hedge reason prefix — pasif `hedge:*` GTC'lerden ayrı
/// tutulur (FAK olduğu için kitapta kalmaz, fakat log/audit için ayrı namespace).
pub const TAKER_HEDGE_REASON_PREFIX: &str = "harvest_v2:taker_hedge:";

fn taker_hedge_reason(side: Outcome) -> String {
    format!("{TAKER_HEDGE_REASON_PREFIX}{}", side.as_lowercase())
}

pub fn handle(filled_side: Outcome, ctx: &HarvestContext) -> (HarvestState, Decision) {
    let same = HarvestState::PositionOpen { filled_side };

    if ctx.profit_locked() {
        return (HarvestState::ProfitLocked { filled_side }, Decision::NoOp);
    }

    // P5 (yeni): Opportunistic profit-lock taker hedge.
    // Pasif GTC hedge (`build_hedge`) `avg_threshold − avg_majority` fiyatına
    // çakılı; market geçici olarak `avg_majority + best_ask(hedge) < 1.0` lock
    // fırsatı sunduğunda bu fırsat kaçırılıyordu (bkz. btc-updown-5m-1776845700).
    // FAK BUY ile parity'e getirip garanti pozitif lock'u anında yakala.
    // FAK olduğu için kitapta kalmaz → balance lock / spam riski yok.
    if let Some(taker_hedge) = try_opportunistic_taker_hedge(ctx) {
        return (same, Decision::PlaceOrders(vec![taker_hedge]));
    }

    let majority = ctx.majority_side();

    // §11: stale avg/pyramid GTC'ler.
    let stale = ctx.stale_avg_or_pyramid_ids();
    if !stale.is_empty() {
        return (same, Decision::CancelOrders(stale));
    }

    let opposed = ctx.signal_opposed_avg_ids();
    if !opposed.is_empty() {
        return (same, Decision::CancelOrders(opposed));
    }

    let hedge_side = majority
        .map(|m| m.opposite())
        .unwrap_or_else(|| filled_side.opposite());

    // Hedge tarafı flip olduysa eski hedge cancel + (varsa) yeni hedge place.
    if let Some(stale_hedge) = ctx.hedge_order(hedge_side.opposite()) {
        let cancels = vec![stale_hedge.id.clone()];
        return match build_hedge(hedge_side, ctx) {
            Some(replacement) => (
                same,
                Decision::CancelAndPlace {
                    cancels,
                    places: vec![replacement],
                },
            ),
            None => (same, Decision::CancelOrders(cancels)),
        };
    }

    // §9: hedge yoksa re-place.
    if ctx.hedge_order(hedge_side).is_none() {
        return replace_missing_hedge(hedge_side, ctx);
    }

    // §9: hedge fiyat veya size sapması.
    if let Some(cancel) = hedge_needs_replace(hedge_side, ctx) {
        return match build_hedge(hedge_side, ctx) {
            Some(replacement) => (
                same,
                Decision::CancelAndPlace {
                    cancels: vec![cancel],
                    places: vec![replacement],
                },
            ),
            None => (same, Decision::NoOp),
        };
    }

    match ctx.zone {
        MarketZone::NormalTrade => {
            if let Some(avg_order) = try_avg_down(filled_side, ctx) {
                return atomic_avg_with_hedge(filled_side, ctx, avg_order);
            }
        }
        MarketZone::AggTrade | MarketZone::FakTrade => {
            if let Some(pyramid_order) = try_pyramid(filled_side, ctx) {
                return atomic_avg_with_hedge(filled_side, ctx, pyramid_order);
            }
        }
        MarketZone::DeepTrade => {}
        MarketZone::StopTrade => {
            unreachable!("harvest::decide() short-circuits StopTrade before reaching position_open")
        }
    }
    (same, Decision::NoOp)
}

/// Avg-down/pyramid + hedge re-place'i atomic gönderir; hedge tarafı projeksiyon
/// sonrası majority'ye göre belirlenir.
fn atomic_avg_with_hedge(
    filled_side: Outcome,
    ctx: &HarvestContext,
    new_order: PlannedOrder,
) -> (HarvestState, Decision) {
    let same = HarvestState::PositionOpen { filled_side };
    let mut cancels = Vec::new();
    let mut places = vec![new_order.clone()];

    for side in [Outcome::Up, Outcome::Down] {
        if let Some(hedge) = ctx.hedge_order(side) {
            cancels.push(hedge.id.clone());
        }
    }

    if let Some(replacement) = build_hedge_with_projected_fill(ctx, &new_order) {
        places.push(replacement);
    }

    if cancels.is_empty() {
        (same, Decision::PlaceOrders(places))
    } else {
        (same, Decision::CancelAndPlace { cancels, places })
    }
}

/// Missing hedge tespitinde re-place; band dışı ise `PairComplete`.
fn replace_missing_hedge(
    hedge_side: Outcome,
    ctx: &HarvestContext,
) -> (HarvestState, Decision) {
    let filled_side = hedge_side.opposite();
    match build_hedge(hedge_side, ctx) {
        Some(order) => (
            HarvestState::PositionOpen { filled_side },
            Decision::PlaceOrders(vec![order]),
        ),
        None => (HarvestState::PairComplete, Decision::NoOp),
    }
}

/// Share-balanced hedge planı.
/// - Fiyat: `avg_threshold − avg_filled(majority)`.
/// - Size: `shares(majority) − shares(hedge_side)`, `MIN_NOTIONAL_USD/price` ve `api_min` ile alttan clamp.
fn build_hedge(hedge_side: Outcome, ctx: &HarvestContext) -> Option<PlannedOrder> {
    let majority = hedge_side.opposite();
    let raw_price = ctx.avg_threshold - ctx.avg_filled(majority);
    if !ctx.price_in_band(raw_price) {
        return None;
    }
    let target = ctx.snap_clamp(raw_price);
    let target_size = (ctx.shares(majority) - ctx.shares(hedge_side)).max(0.0);
    let size = target_size
        .max(MIN_NOTIONAL_USD / target)
        .max(ctx.api_min_order_size);
    if size <= 0.0 {
        return None;
    }
    Some(planned_buy_gtc(
        hedge_side,
        ctx.token_id(hedge_side),
        target,
        size,
        hedge_reason(hedge_side),
    ))
}

/// Yeni avg/pyramid emrinin tam dolduğu varsayımı ile hedge planı; balanced çıkarsa `None`.
fn build_hedge_with_projected_fill(
    ctx: &HarvestContext,
    new_order: &PlannedOrder,
) -> Option<PlannedOrder> {
    let projected_up = ctx.shares(Outcome::Up)
        + if new_order.outcome == Outcome::Up {
            new_order.size
        } else {
            0.0
        };
    let projected_down = ctx.shares(Outcome::Down)
        + if new_order.outcome == Outcome::Down {
            new_order.size
        } else {
            0.0
        };
    let diff = projected_up - projected_down;
    if diff.abs() < ctx.api_min_order_size {
        return None;
    }
    let projected_majority = if diff > 0.0 {
        Outcome::Up
    } else {
        Outcome::Down
    };
    let hedge_side = projected_majority.opposite();

    let cost_majority_after = ctx.cost_filled(projected_majority)
        + if new_order.outcome == projected_majority {
            new_order.price * new_order.size
        } else {
            0.0
        };
    let shares_majority_after = if projected_majority == Outcome::Up {
        projected_up
    } else {
        projected_down
    };
    let shares_hedge_after = if hedge_side == Outcome::Up {
        projected_up
    } else {
        projected_down
    };
    if shares_majority_after <= 0.0 {
        return None;
    }
    let avg_majority_after = cost_majority_after / shares_majority_after;

    let raw_price = ctx.avg_threshold - avg_majority_after;
    if !ctx.price_in_band(raw_price) {
        return None;
    }
    let target = ctx.snap_clamp(raw_price);
    let target_size = (shares_majority_after - shares_hedge_after).max(0.0);
    let size = target_size
        .max(MIN_NOTIONAL_USD / target)
        .max(ctx.api_min_order_size);
    if size <= 0.0 {
        return None;
    }
    Some(planned_buy_gtc(
        hedge_side,
        ctx.token_id(hedge_side),
        target,
        size,
        hedge_reason(hedge_side),
    ))
}

/// Hedge fiyat (`|drift| ≥ tick/2`) veya size (`|drift| ≥ api_min`) sapmasında cancel ID döndür.
fn hedge_needs_replace(hedge_side: Outcome, ctx: &HarvestContext) -> Option<String> {
    let majority = hedge_side.opposite();
    let hedge = ctx.hedge_order(hedge_side)?;
    let avg_majority = ctx.avg_filled(majority);
    if avg_majority <= 0.0 {
        return None;
    }
    let target_price = ctx.snap_clamp(ctx.avg_threshold - avg_majority);
    let target_size = (ctx.shares(majority) - ctx.shares(hedge_side)).max(0.0);
    let remaining = (hedge.size - hedge.size_matched).max(0.0);
    let price_drift = (hedge.price - target_price).abs() >= ctx.tick_size * 0.5;
    let size_drift = (remaining - target_size).abs() >= ctx.api_min_order_size;
    if !price_drift && !size_drift {
        return None;
    }
    Some(hedge.id.clone())
}

/// P5: Garantili profit-lock fırsatı varsa parity'e getirecek FAK BUY hedge planla.
///
/// Tetikleyici: majority pozisyon var, `avg_majority + best_ask(hedge_side) <
/// 1.0 − lock_min_profit_pct`. Pair maliyeti $1'in altına düşüyorsa fark net
/// kâr olur (Polymarket'te her UP+DOWN çift $1 öder).
///
/// Size: `shares(majority) − shares(hedge_side)` (parity hedefi).
/// Order: `FAK BUY @ snap(best_ask)` — best_ask seviyesindeki tüm liquidity'i
/// alır, fazlası iptal olur. Kitapta kalmaz → balance lock yok, repeat-safe.
fn try_opportunistic_taker_hedge(ctx: &HarvestContext) -> Option<PlannedOrder> {
    let majority = ctx.majority_side()?;
    let hedge_side = majority.opposite();
    let avg_majority = ctx.avg_filled(majority);
    if avg_majority <= 0.0 {
        return None;
    }
    let ask = ctx.best_ask(hedge_side);
    if ask <= 0.0 || !ctx.price_in_band(ask) {
        return None;
    }
    let pair_cost = avg_majority + ask;
    let max_pair_cost = 1.0 - ctx.lock_min_profit_pct;
    if pair_cost >= max_pair_cost {
        return None;
    }
    let target_size = ctx.shares(majority) - ctx.shares(hedge_side);
    if target_size < ctx.api_min_order_size {
        return None;
    }
    let price = ctx.snap_clamp(ask);
    let size = target_size
        .max(MIN_NOTIONAL_USD / price)
        .max(ctx.api_min_order_size);
    Some(PlannedOrder {
        outcome: hedge_side,
        token_id: ctx.token_id(hedge_side).to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Fak,
        reason: taker_hedge_reason(hedge_side),
    })
}

/// Doc §7: `best_ask(filled) < avg_filled_side` + cooldown + açık avg yok.
fn try_avg_down(filled_side: Outcome, ctx: &HarvestContext) -> Option<PlannedOrder> {
    if !ctx.cooldown_ok() {
        return None;
    }
    if ctx.has_open_avg(filled_side) {
        return None;
    }
    if !ctx.signal_supports(filled_side) {
        return None;
    }
    if ctx.position_cap_reached() {
        return None;
    }
    let ask = ctx.best_ask(filled_side);
    let avg = ctx.avg_filled(filled_side);
    if avg <= 0.0 || ask <= 0.0 || ask >= avg {
        return None;
    }
    let bid = ctx.best_bid(filled_side);
    if !ctx.price_in_band(bid) {
        return None;
    }
    let price = ctx.snap_clamp(bid);
    let size = crate::strategy::order_size(ctx.order_usdc, price, ctx.api_min_order_size);
    Some(planned_buy_gtc(
        filled_side,
        ctx.token_id(filled_side),
        price,
        size,
        avg_down_reason(filled_side),
    ))
}

/// Doc §8: rising_side'a `best_ask + |delta|` GTC; trend gate sadece `rising == filled_side`'da.
fn try_pyramid(filled_side: Outcome, ctx: &HarvestContext) -> Option<PlannedOrder> {
    if !ctx.cooldown_ok() {
        return None;
    }
    let rising = ctx.rising_side()?;
    if ctx.has_open_pyramid(rising) {
        return None;
    }
    if rising != filled_side && !ctx.opposite_pyramid_enabled {
        return None;
    }
    if !ctx.signal_supports(rising) {
        return None;
    }
    if ctx.position_cap_reached() {
        return None;
    }
    let ask = ctx.best_ask(rising);
    if ask <= 0.0 || !ctx.price_in_band(ask) {
        return None;
    }
    if rising == filled_side {
        let avg = ctx.avg_filled(rising);
        if avg <= 0.0 || ask <= avg {
            return None;
        }
    }
    let price = ctx.snap_clamp(ask + ctx.delta(rising).abs());
    let size = crate::strategy::order_size(ctx.order_usdc, price, ctx.api_min_order_size);
    Some(planned_buy_gtc(
        rising,
        ctx.token_id(rising),
        price,
        size,
        pyramid_reason(rising),
    ))
}
