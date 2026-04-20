//! Hedge re-pricing akışı — cancel sonrası imbalance bazlı re-place (doc §9).

use crate::strategy::{planned_buy_gtc, Decision};
use crate::types::Outcome;

use super::state::{HarvestContext, HarvestState, HEDGE_REASON_PREFIX};

pub fn handle(filled_side: Outcome, ctx: &HarvestContext) -> (HarvestState, Decision) {
    // Cancel response hâlâ bekleniyor.
    if ctx.hedge_order().is_some() {
        return (HarvestState::HedgeUpdating { filled_side }, Decision::NoOp);
    }

    // §9 adım 6a vs 6b: hedge gitti → imbalance'a göre dallan.
    //   cancel_ok  → shares aynı, imbalance ≈ filled_shares → re-place.
    //   cancel_race → hedge fill oldu, imbalance ≈ 0 → pair tamamlandı.
    let imbalance = ctx.shares(filled_side) - ctx.shares(filled_side.opposite());
    if imbalance.abs() < ctx.api_min_order_size {
        return (HarvestState::PairComplete, Decision::NoOp);
    }

    let hedge_side = filled_side.opposite();
    let target = ctx.snap_clamp(ctx.avg_threshold - ctx.avg_filled(filled_side));
    if !ctx.price_in_band(target) {
        // `avg_threshold − avg_filled` bandın altında/üstünde → re-price imkânsız,
        // pozisyonu olduğu gibi bırak; sonraki tick avg düştüğünde tekrar tetiklenir.
        return (
            HarvestState::PositionOpen { filled_side },
            Decision::NoOp,
        );
    }
    let size = imbalance.abs();
    let order = planned_buy_gtc(
        hedge_side,
        ctx.token_id(hedge_side),
        target,
        size,
        format!(
            "{}{}",
            HEDGE_REASON_PREFIX,
            HarvestContext::outcome_str(hedge_side)
        ),
    );
    (
        HarvestState::PositionOpen { filled_side },
        Decision::PlaceOrders(vec![order]),
    )
}
