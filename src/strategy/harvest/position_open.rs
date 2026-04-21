//! `PositionOpen` — NormalTrade averaging-down + AggTrade/FakTrade pyramiding +
//! hedge drift / missing-hedge / hedge-side flip tespiti (doc §7, §8, §9, §10, §11).
//!
//! Hedge tarafı **dinamik**: `majority_side().opposite()` ile her tick'te
//! yeniden hesaplanır. Pyramid karşı tarafa basıp imbalance flip ettiğinde
//! eski hedge cancel edilir, yeni hedge majority'nin tersine basılır
//! (Findings Öneri B / RİSK 1).
//!
//! Avg-down ve pyramid emirlerinden ÖNCE `would_lock_loss` projection gate
//! çalıştırılır: hipotetik fill'in `pair_avg_sum > avg_threshold` üreteceği
//! emirler reddedilir (RİSK 3).

use crate::strategy::{planned_buy_gtc, Decision, PlannedOrder, MIN_NOTIONAL_USD};
use crate::time::MarketZone;
use crate::types::Outcome;

use super::state::{
    avg_down_reason, hedge_reason, pyramid_reason, HarvestContext, HarvestState,
};

pub fn handle(filled_side: Outcome, ctx: &HarvestContext) -> (HarvestState, Decision) {
    let same = HarvestState::PositionOpen { filled_side };

    // ProfitLock: shares parity (`|shares_yes − shares_no| < api_min_order_size`)
    // ve her iki tarafta da fill. Share-balanced hedge sayesinde parity =>
    // avg_sum ≤ avg_threshold otomatik garanti.
    if ctx.profit_locked() {
        return (HarvestState::ProfitLocked { filled_side }, Decision::NoOp);
    }

    let majority = ctx.majority_side();

    // §11: stale avg/pyramid GTC'ler → cancel.
    let stale = ctx.stale_avg_or_pyramid_ids();
    if !stale.is_empty() {
        return (same, Decision::CancelOrders(stale));
    }

    // Hedge tarafı: majority varsa onun tersi; yoksa (dust imbalance) filled_side fallback.
    let hedge_side = majority
        .map(|m| m.opposite())
        .unwrap_or_else(|| filled_side.opposite());

    // RİSK 1: hedge tarafı flip olduysa eski (yanlış) tarafta kalan hedge cancel edilir.
    // Aynı tick'te yeni hedge atılırsa atomic CancelAndPlace, yoksa sadece cancel.
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

    // §9: hedge yoksa (cancel race / API hata / manuel) → re-place.
    if ctx.hedge_order(hedge_side).is_none() {
        return replace_missing_hedge(hedge_side, ctx);
    }

    // §9 adım 2-4: hedge fiyat veya size sapması → atomic cancel + re-place.
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

/// Avg-down ya da pyramid emri tetiklendiğinde hedge'i ANLIK olarak (aynı tick)
/// yeniden planla. Yeni hedge tarafı **projeksiyon sonrası majority** üzerinden
/// hesaplanır (pyramid karşı tarafa basıp imbalance flip edebilir). Eski hedge
/// (hangi tarafta olursa olsun) cancel + yeni hedge place tek `Decision::CancelAndPlace`
/// ile atomik gönderilir.
fn atomic_avg_with_hedge(
    filled_side: Outcome,
    ctx: &HarvestContext,
    new_order: PlannedOrder,
) -> (HarvestState, Decision) {
    let same = HarvestState::PositionOpen { filled_side };
    let mut cancels = Vec::new();
    let mut places = vec![new_order.clone()];

    // Mevcut hedge order'larının hepsini topla (her iki olası tarafta da olabilir).
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

/// Missing hedge (cancel race / API hata / manuel müdahale) tespitinde yeniden
/// hedge planla. `build_hedge` `None` dönerse (target band dışı) `PairComplete`'a
/// düşer; aksi halde `PositionOpen` korunur.
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

/// `shares_up = shares_down` invariant'ı için hedge planı (share-balanced).
/// `hedge_side` argüman olarak verilir → karşı taraf (`hedge_side.opposite()`)
/// majority'dir, share referansı odur.
///
/// - **Fiyat**: `avg_threshold − avg_filled(majority)` — hedge dolarsa
///   `avg_sum ≤ avg_threshold` invariant'ı korunur.
/// - **Size**: `shares(majority) − shares(hedge_side)`. `MIN_NOTIONAL_USD /
///   hedge_price` ve `api_min_order_size` ile alttan clamp.
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

/// Avg-down/pyramid emrinin TAM dolacağı varsayımı ile hedge'i taze metrikler
/// üzerinden planla. Hedge tarafı PROJEKSIYON sonrası majority'ye göre belirlenir;
/// pyramid karşı tarafa basıp dengeyi flip ettiğinde hedge de yeni majority'nin
/// tersine planlanır. Projeksiyon balanced çıkarsa (`majority = None`) yeni
/// hedge gerekmez (`None`).
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

/// Hedge için fiyat veya size sapması — cancel ID döndür. `hedge_side` argüman
/// olarak verilir; karşı taraf majority'dir.
///
/// - **Fiyat sapması**: `|hedge.price − (avg_threshold − avg(majority))| ≥ tick/2`.
/// - **Size sapması**: `|remaining − (shares(majority) − shares(hedge_side))|
///   ≥ api_min_order_size` (re-place sonrası order yine api_min'i geçmek zorunda).
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

/// Doc §7: `best_ask(filled) < avg_filled_side` + cooldown + açık avg yok.
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
    let size = crate::strategy::order_size(ctx.order_usdc, price, ctx.api_min_order_size);
    Some(planned_buy_gtc(
        filled_side,
        ctx.token_id(filled_side),
        price,
        size,
        avg_down_reason(filled_side),
    ))
}

/// Doc §8: rising_side'a `best_ask + |delta|` GTC. Tetik koşulu: `best_ask(rising) >
/// avg_filled(rising)` — trend tarafındaki VWAP anlık fiyatın altında kaldığı
/// sürece momentum'a katılır. RİSK 5: `rising_side` `Option` döner; 0.5 ± ε
/// dead zone'da pyramid skip.
fn try_pyramid(filled_side: Outcome, ctx: &HarvestContext) -> Option<PlannedOrder> {
    if !ctx.cooldown_ok() {
        return None;
    }
    let rising = ctx.rising_side()?;
    if ctx.has_open_pyramid(rising) {
        return None;
    }
    let ask = ctx.best_ask(rising);
    if ask <= 0.0 {
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
