//! Strateji tick + state-transition logu + place/cancel logu.

use crate::binance;
use crate::engine::{execute, MarketSession};
use crate::ipc::{self, FrontendEvent};
use crate::strategy::harvest::HarvestState;
use crate::strategy::Decision;
use crate::time::now_ms;
use crate::types::Outcome;

use super::ctx::Ctx;

/// 500 ms cadence'inde strateji çağrısı + decision execute.
pub async fn tick(ctx: &Ctx, sess: &mut MarketSession) {
    let snap = ctx.signal_state.read().await;
    let es = binance::effective_score(snap.signal_score, ctx.cfg.signal_weight);
    let prev_state = sess.harvest_state;
    let decision = sess.tick(&ctx.cfg, now_ms(), es);
    let label = ctx.bot_id.to_string();

    log_state_transition(
        ctx,
        sess,
        prev_state,
        &decision,
        snap.signal_score,
        es,
        &label,
    );

    if matches!(decision, Decision::NoOp) {
        return;
    }

    log_cancel_request(&decision, &label);

    let Ok(out) = execute(sess, &ctx.executor, decision).await else {
        return;
    };

    log_cancel_responses(&out.canceled, &label);
    log_placements(&out, snap.signal_score, es, &label);

    for ex in out.placed.into_iter().filter(|e| e.filled) {
        ipc::emit(&FrontendEvent::OrderPlaced {
            bot_id: ctx.bot_id,
            order_id: ex.order_id,
            outcome: ex.planned.outcome,
            side: ex.planned.side,
            price: ex.planned.price,
            size: ex.planned.size,
            order_type: format!("{:?}", ex.planned.order_type),
            ts_ms: now_ms(),
        });
    }
}

/// §5.2: OpenDual giriş/çıkış + Averaging timeout + ProfitLock geçişlerini
/// görselleştir. Sadece log etkisi vardır; akışı değiştirmez.
fn log_state_transition(
    ctx: &Ctx,
    sess: &MarketSession,
    prev: HarvestState,
    decision: &Decision,
    signal_score: f64,
    es: f64,
    label: &str,
) {
    match (prev, sess.harvest_state) {
        (HarvestState::Pending, HarvestState::OpenDual { deadline_ms }) => {
            if let Decision::PlaceOrders(orders) = decision {
                let up = orders
                    .iter()
                    .find(|o| matches!(o.outcome, Outcome::Up))
                    .map(|o| o.price)
                    .unwrap_or(0.0);
                let down = orders
                    .iter()
                    .find(|o| matches!(o.outcome, Outcome::Down))
                    .map(|o| o.price)
                    .unwrap_or(0.0);
                ipc::log_line(
                    label,
                    format!(
                        "🎯 OpenDual signal_score={:.2} effective_score={:.2} → up_bid={:.2} down_bid={:.2} deadline={}ms",
                        signal_score, es, up, down, deadline_ms
                    ),
                );
            }
        }
        (HarvestState::OpenDual { .. }, HarvestState::SingleLeg { filled_side }) => {
            let yes_filled = sess.metrics.shares_yes > 0.0;
            let no_filled = sess.metrics.shares_no > 0.0;
            if yes_filled && no_filled {
                ipc::log_line(
                    label,
                    format!(
                        "🔀 OpenDual both filled → SingleLeg{{by_signal={}}}",
                        filled_side.as_str()
                    ),
                );
            } else {
                ipc::log_line(
                    label,
                    format!(
                        "⏰ OpenDual timeout (one_fill={}) → cancelling counter side",
                        filled_side.as_str()
                    ),
                );
            }
        }
        (HarvestState::OpenDual { .. }, HarvestState::Pending) => {
            ipc::log_line(
                label,
                "⏰ OpenDual timeout (no_fill) → cancelling 2 orders, reopening",
            );
        }
        (HarvestState::SingleLeg { filled_side }, HarvestState::SingleLeg { .. }) => {
            if let Decision::CancelOrders(ids) = decision {
                ipc::log_line(
                    label,
                    format!(
                        "🔁 Averaging timeout (side={}) → cancelling {} order(s), will retry",
                        filled_side.as_str(),
                        ids.len()
                    ),
                );
            }
        }
        (HarvestState::SingleLeg { filled_side }, HarvestState::ProfitLock) => {
            // §5.2: ProfitLock tetiklendi — first_leg + hedge_leg ≤ avg_threshold.
            let avg_threshold = ctx.cfg.strategy_params.harvest_avg_threshold();
            let first_leg = match filled_side {
                Outcome::Up => sess.metrics.avg_yes,
                Outcome::Down => sess.metrics.avg_no,
            };
            let hedge_leg = match filled_side {
                Outcome::Up => sess.no_best_ask,
                Outcome::Down => sess.yes_best_ask,
            };
            ipc::log_line(
                label,
                format!(
                    "🔒 ProfitLock triggered: first_leg({})={:.4} + hedge_leg({})={:.4} = {:.4} ≤ threshold({:.2}) → FAK",
                    filled_side.as_str(),
                    first_leg,
                    filled_side.opposite().as_str().to_uppercase(),
                    hedge_leg,
                    first_leg + hedge_leg,
                    avg_threshold,
                ),
            );
        }
        _ => {}
    }
}

/// §5.5: cancel önce log'lansın (DELETE /order ({n} ids) ids=[..]).
fn log_cancel_request(decision: &Decision, label: &str) {
    let cancel_ids: &[String] = match decision {
        Decision::CancelOrders(ids) => ids,
        Decision::Batch { cancel, .. } => cancel,
        _ => return,
    };
    if cancel_ids.is_empty() {
        return;
    }
    ipc::log_line(
        label,
        format!(
            "🚫 DELETE /order ({} ids) ids={:?}",
            cancel_ids.len(),
            cancel_ids
        ),
    );
}

fn log_cancel_responses(canceled: &[crate::polymarket::clob::CancelResponse], label: &str) {
    for c in canceled {
        ipc::log_line(
            label,
            format!(
                "    canceled={:?} not_canceled={}",
                c.canceled, c.not_canceled
            ),
        );
    }
}

fn log_placements(out: &crate::engine::ExecuteOutput, signal_score: f64, es: f64, label: &str) {
    for ex in &out.placed {
        let status = if ex.filled { "matched" } else { "live" };
        ipc::log_line(
            label,
            format!(
                "✅ orderType={} side={} outcome={} size={} price={} | status={} | reason={} | signal={:.2}(eff {:.2})",
                ex.planned.order_type.as_str(),
                ex.planned.side.as_str(),
                ex.planned.outcome.as_str(),
                ex.planned.size,
                ex.planned.price,
                status,
                ex.planned.reason,
                signal_score,
                es,
            ),
        );
    }
}
