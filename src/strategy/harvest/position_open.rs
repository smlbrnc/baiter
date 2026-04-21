//! `PositionOpen` — NormalTrade averaging-down + AggTrade/FakTrade pyramiding +
//! hedge drift / missing-hedge tespiti (doc §7, §8, §9, §10, §11).
//!
//! Hedge drift atomic `Decision::CancelAndPlace` ile aynı tick'te re-price
//! edilir; missing hedge `Decision::PlaceOrders([replacement])` ile yeniden
//! konur. Ayrı `HedgeUpdating` ara state'i yoktur.

use crate::strategy::{order_size, planned_buy_gtc, Decision, PlannedOrder};
use crate::time::MarketZone;
use crate::types::Outcome;

use super::state::{
    avg_down_reason, hedge_reason, pyramid_reason, HarvestContext, HarvestState,
};

pub fn handle(filled_side: Outcome, ctx: &HarvestContext) -> (HarvestState, Decision) {
    let same = HarvestState::PositionOpen { filled_side };
    let hedge_side = filled_side.opposite();

    // §10: hedge passive fill → pair tamamlandı.
    if ctx.hedge_order(hedge_side).is_none() && ctx.shares(hedge_side) > 0.0 {
        return (HarvestState::PairComplete, Decision::NoOp);
    }

    // §11: stale avg/pyramid GTC'ler → cancel.
    let stale = ctx.stale_avg_or_pyramid_ids();
    if !stale.is_empty() {
        return (same, Decision::CancelOrders(stale));
    }

    // §9: hedge yoksa (cancel race / API hata / manuel) → re-place.
    // Bot 2 / btc-updown-5m-1776766500 regresyonu: avg-down yığıldıkça
    // profit-lock kaçmasın diye missing hedge aynı tick'te yeniden konur.
    if ctx.hedge_order(hedge_side).is_none() {
        return replace_missing_hedge(filled_side, ctx);
    }

    // §9 adım 2-4: hedge drift → atomic cancel + re-place. Aynı tick içinde
    // eski hedge düşer, yeni hedge `0.98 − avg_filled` hedef fiyatıyla kitaba
    // girer. Yeni hedge planlanamıyorsa (target band dışı / imbalance dust)
    // eski hedge bırakılır — ProfitLock garantisini bozmamak için cancel-only
    // yapılmaz; bir sonraki tick'te avg değişince tekrar değerlendirilir.
    if let Some(cancel) = hedge_drift_cancel(filled_side, ctx) {
        return match build_hedge(filled_side, ctx) {
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
            if let Some(order) = try_avg_down(filled_side, ctx) {
                return (same, Decision::PlaceOrders(vec![order]));
            }
        }
        MarketZone::AggTrade | MarketZone::FakTrade => {
            if let Some(order) = try_pyramid(filled_side, ctx) {
                return (same, Decision::PlaceOrders(vec![order]));
            }
        }
        MarketZone::DeepTrade => {}
        MarketZone::StopTrade => {
            unreachable!("harvest::decide() short-circuits StopTrade before reaching position_open")
        }
    }
    (same, Decision::NoOp)
}

/// Missing hedge (cancel race / API hata / manuel müdahale) tespitinde yeniden
/// hedge planla. Imbalance dust ise `PairComplete`'a düşer.
fn replace_missing_hedge(
    filled_side: Outcome,
    ctx: &HarvestContext,
) -> (HarvestState, Decision) {
    let imbalance = ctx.shares(filled_side) - ctx.shares(filled_side.opposite());
    if imbalance.abs() < ctx.api_min_order_size {
        return (HarvestState::PairComplete, Decision::NoOp);
    }
    match build_hedge(filled_side, ctx) {
        Some(order) => (
            HarvestState::PositionOpen { filled_side },
            Decision::PlaceOrders(vec![order]),
        ),
        None => (HarvestState::PositionOpen { filled_side }, Decision::NoOp),
    }
}

/// Doc §9 adım 7: `imbalance.abs()` boyutlu `Buy GTC` hedge planı; target band
/// dışında veya imbalance dust ise `None`.
fn build_hedge(filled_side: Outcome, ctx: &HarvestContext) -> Option<PlannedOrder> {
    let imbalance = ctx.shares(filled_side) - ctx.shares(filled_side.opposite());
    if imbalance.abs() < ctx.api_min_order_size {
        return None;
    }
    let hedge_side = filled_side.opposite();
    let target = ctx.snap_clamp(ctx.avg_threshold - ctx.avg_filled(filled_side));
    if !ctx.price_in_band(target) {
        return None;
    }
    Some(planned_buy_gtc(
        hedge_side,
        ctx.token_id(hedge_side),
        target,
        imbalance.abs(),
        hedge_reason(hedge_side),
    ))
}

/// Doc §9 adım 2-4: beklenen hedge = `avg_threshold − avg_filled_side`; mevcut
/// hedge bu fiyattan ≥ 1 tick sapmışsa cancel ID döndür.
fn hedge_drift_cancel(filled_side: Outcome, ctx: &HarvestContext) -> Option<String> {
    let hedge = ctx.hedge_order(filled_side.opposite())?;
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
        avg_down_reason(filled_side),
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
        pyramid_reason(rising),
    ))
}
