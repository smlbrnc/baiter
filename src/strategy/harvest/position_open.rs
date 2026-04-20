//! `PositionOpen` — NormalTrade averaging-down + AggTrade/FakTrade pyramiding +
//! hedge drift tespiti (doc §7, §8, §9, §10, §11).

use crate::strategy::{order_size, planned_buy_gtc, Decision, PlannedOrder};
use crate::time::MarketZone;
use crate::types::Outcome;

use super::state::{
    HarvestContext, HarvestState, AVG_DOWN_REASON_PREFIX, PYRAMID_REASON_PREFIX,
};

pub fn handle(filled_side: Outcome, ctx: &HarvestContext) -> (HarvestState, Decision) {
    let same = HarvestState::PositionOpen { filled_side };

    // §10: hedge passive fill → pair tamamlandı.
    if ctx.hedge_order().is_none() && ctx.shares(filled_side.opposite()) > 0.0 {
        return (HarvestState::PairComplete, Decision::NoOp);
    }

    // §11: stale avg/pyramid GTC'ler → cancel.
    let stale = ctx.stale_avg_or_pyramid_ids();
    if !stale.is_empty() {
        return (same, Decision::CancelOrders(stale));
    }

    // §9 adım 2-4: hedge drift → HedgeUpdating.
    if let Some(cancel) = hedge_drift_cancel(filled_side, ctx) {
        return (
            HarvestState::HedgeUpdating { filled_side },
            Decision::CancelOrders(vec![cancel]),
        );
    }

    match ctx.zone {
        MarketZone::NormalTrade => {
            if let Some(order) = try_avg_down(filled_side, ctx) {
                return (same, Decision::PlaceOrders(vec![order]));
            }
        }
        MarketZone::AggTrade | MarketZone::FakTrade => {
            if let Some(order) = try_pyramid(filled_side, ctx) {
                return (same, Decision::PlaceOrders(vec![order]));
            }
        }
        MarketZone::DeepTrade | MarketZone::StopTrade => {}
    }
    (same, Decision::NoOp)
}

/// Doc §9 adım 2-4: beklenen hedge = `avg_threshold − avg_filled_side`; mevcut
/// hedge bu fiyattan ≥ 1 tick sapmışsa cancel ID döndür.
fn hedge_drift_cancel(filled_side: Outcome, ctx: &HarvestContext) -> Option<String> {
    let hedge = ctx.hedge_order()?;
    let avg_filled = ctx.avg_filled(filled_side);
    if avg_filled <= 0.0 {
        return None;
    }
    let target = ctx.snap_clamp(ctx.avg_threshold - avg_filled);
    if (hedge.price - target).abs() < ctx.tick_size * 0.5 {
        return None;
    }
    Some(hedge.id.clone())
}

/// Doc §7: `best_ask(filled) < avg_filled_side` + cooldown + açık avg yok + band içi.
fn try_avg_down(filled_side: Outcome, ctx: &HarvestContext) -> Option<PlannedOrder> {
    if !ctx.cooldown_ok() {
        return None;
    }
    if ctx.has_open_avg(filled_side) {
        return None;
    }
    let ask = ctx.best_ask(filled_side);
    let avg = ctx.avg_filled(filled_side);
    if avg <= 0.0 || ask <= 0.0 || ask >= avg {
        return None;
    }
    let price = ctx.snap_clamp(ctx.best_bid(filled_side));
    if !ctx.price_in_band(price) {
        return None;
    }
    let size = order_size(ctx.order_usdc, price, ctx.api_min_order_size);
    Some(planned_buy_gtc(
        filled_side,
        ctx.token_id(filled_side),
        price,
        size,
        format!(
            "{}{}",
            AVG_DOWN_REASON_PREFIX,
            HarvestContext::outcome_str(filled_side)
        ),
    ))
}

/// Doc §8: rising_side'a `best_ask + |delta|` GTC. Rising == filled_side ise
/// `best_ask > last_fill` trend koşulu aranır; karşı taraf ilk pyramid'inde atlanır.
fn try_pyramid(filled_side: Outcome, ctx: &HarvestContext) -> Option<PlannedOrder> {
    if !ctx.cooldown_ok() {
        return None;
    }
    let rising = ctx.rising_side();
    if ctx.has_open_pyramid(rising) {
        return None;
    }
    let ask = ctx.best_ask(rising);
    if ask <= 0.0 {
        return None;
    }
    if rising == filled_side {
        let last = ctx.last_fill(rising);
        if last <= 0.0 || ask <= last {
            return None;
        }
    }
    let price = ctx.snap_clamp(ask + ctx.delta(rising).abs());
    if !ctx.price_in_band(price) {
        return None;
    }
    let size = order_size(ctx.order_usdc, price, ctx.api_min_order_size);
    Some(planned_buy_gtc(
        rising,
        ctx.token_id(rising),
        price,
        size,
        format!(
            "{}{}",
            PYRAMID_REASON_PREFIX,
            HarvestContext::outcome_str(rising)
        ),
    ))
}
