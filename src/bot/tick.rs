//! Strateji tick + state-transition logu + place/cancel logu.
//!
//! ⚡ Kural 1: `decide → execute (POST/DELETE)` arasına sync I/O girmez. Tüm
//! log/IPC push'lar `execute` döndükten sonra `tokio::spawn` ile arkaplana atılır.

use crate::engine::{execute, ExecuteOutput, MarketSession};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::CancelResponse;
use crate::rtds;
use crate::strategy::harvest::HarvestState;
use crate::strategy::Decision;
use crate::time::now_ms;
use crate::types::{Outcome, RunMode};

use super::ctx::Ctx;

/// State-transition logu için `tick` sırasındaki session snapshot — `'static`
/// task'a güvenle taşınır (tüm alanlar `Copy`).
#[derive(Clone, Copy)]
struct StateLogSnapshot {
    avg_yes: f64,
    avg_no: f64,
    yes_best_ask: f64,
    no_best_ask: f64,
    avg_threshold: f64,
    imbalance: f64,
}

struct TickLogCtx {
    bot_id: i64,
    label: String,
    prev_state: HarvestState,
    post_state: HarvestState,
    decision: Decision,
    signal_score: f64,
    es: f64,
    snap: StateLogSnapshot,
}

/// Strateji çağrısı + decision execute. Composite sinyal akışı:
/// `RTDS window_delta_score + Binance signal_score → effective_composite → sess.tick`.
pub async fn tick(ctx: &Ctx, sess: &mut MarketSession) {
    let binance_score = ctx.signal_state.read().await.signal_score;
    let window_score = if ctx.cfg.strategy_params.rtds_enabled_or_default() {
        let rtds_snap = ctx.rtds_state.read().await;
        let interval_secs = sess.end_ts.saturating_sub(sess.start_ts);
        rtds::window_delta_score(
            rtds_snap.window_delta_bps,
            rtds::interval_scale(interval_secs),
        )
    } else {
        5.0
    };
    let composite = rtds::composite_score(
        window_score,
        binance_score,
        ctx.cfg.strategy_params.window_delta_weight_or_default(),
    );
    let es = rtds::effective_composite(composite, ctx.cfg.signal_weight);
    let signal_score = composite;
    let prev_state = sess.harvest_state;
    let decision = sess.tick(&ctx.cfg, now_ms(), es);
    let bot_id = ctx.bot_id;
    let post_state = sess.harvest_state;
    let snap = StateLogSnapshot {
        avg_yes: sess.metrics.avg_yes,
        avg_no: sess.metrics.avg_no,
        yes_best_ask: sess.yes_best_ask,
        no_best_ask: sess.no_best_ask,
        avg_threshold: ctx.cfg.strategy_params.harvest_avg_threshold(),
        imbalance: sess.metrics.imbalance,
    };

    let make_log_ctx = |decision: Decision| TickLogCtx {
        bot_id,
        label: bot_id.to_string(),
        prev_state,
        post_state,
        decision,
        signal_score,
        es,
        snap,
    };

    if matches!(decision, Decision::NoOp) {
        let log_ctx = make_log_ctx(decision);
        tokio::spawn(async move {
            log_state_transition(&log_ctx);
        });
        return;
    }

    let decision_for_log = decision.clone();
    let out = match execute(sess, &ctx.executor, decision).await {
        Ok(out) => out,
        Err(e) => {
            // Strateji state ilerledi (decide tamamlandı), POST/DELETE başarısız.
            // Hata yüzeye çıkmalı; sonraki tick'te FSM yeniden değerlendirme yapar.
            tracing::error!(bot_id, error = %e, "execute failed in tick");
            ipc::log_line(&bot_id.to_string(), format!("❌ execute failed: {e}"));
            return;
        }
    };

    // DryRun taker (immediate match) → trades. Passive fill'ler `event.rs`'de yazılır.
    if ctx.cfg.run_mode == RunMode::Dryrun {
        for ex in &out.placed {
            if ex.filled {
                let fp = ex.fill_price.unwrap_or(ex.planned.price);
                let fs = ex.fill_size.unwrap_or(ex.planned.size);
                super::persist::persist_dryrun_fill(&ctx.pool, sess, ex, fp, fs, "TAKER");
            }
        }
    }

    let log_ctx = make_log_ctx(decision_for_log);
    tokio::spawn(async move {
        log_state_transition(&log_ctx);
        log_cancel_request(&log_ctx.decision, &log_ctx.label);
        log_cancel_responses(&out.canceled, &log_ctx.label);
        log_placements(&out, log_ctx.signal_score, log_ctx.es, &log_ctx.label);
        emit_order_events(log_ctx.bot_id, &out);
    });
}

fn emit_order_events(bot_id: i64, out: &ExecuteOutput) {
    for ex in &out.placed {
        let status = if ex.filled { "matched" } else { "live" };
        ipc::emit(&FrontendEvent::OrderPlaced {
            bot_id,
            order_id: ex.order_id.clone(),
            outcome: ex.planned.outcome,
            side: ex.planned.side,
            price: ex.planned.price,
            size: ex.planned.size,
            order_type: ex.planned.order_type.as_str().to_string(),
            status: status.into(),
            ts_ms: now_ms(),
        });
    }
    for c in &out.canceled {
        for id in &c.canceled {
            ipc::emit(&FrontendEvent::OrderCanceled {
                bot_id,
                order_id: id.clone(),
                ts_ms: now_ms(),
            });
        }
    }
}

/// §5.2: OpenDual giriş/çıkış + Averaging timeout + ProfitLock geçişleri.
fn log_state_transition(c: &TickLogCtx) {
    let TickLogCtx {
        prev_state: prev,
        post_state: post,
        decision,
        signal_score,
        es,
        label,
        snap,
        ..
    } = c;
    let signal_score = *signal_score;
    let es = *es;
    let label = label.as_str();
    match (*prev, *post) {
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
                        "🎯 OpenDual signal_score={signal_score:.2} effective_score={es:.2} → up_bid={up:.2} down_bid={down:.2} deadline={deadline_ms}ms"
                    ),
                );
            }
        }
        (HarvestState::OpenDual { .. }, HarvestState::SingleLeg { filled_side, .. }) => {
            ipc::log_line(
                label,
                format!(
                    "⏰ OpenDual timeout (one_fill={}) → cancelling counter side → SingleLeg",
                    filled_side.as_str()
                ),
            );
        }
        (HarvestState::OpenDual { .. }, HarvestState::DoubleLeg) => {
            ipc::log_line(
                label,
                format!(
                    "🔀 OpenDual both filled → DoubleLeg (avg_yes={:.4} avg_no={:.4})",
                    snap.avg_yes, snap.avg_no
                ),
            );
        }
        (HarvestState::DoubleLeg, HarvestState::DoubleLeg) => {
            if let Decision::CancelOrders(ids) = decision {
                ipc::log_line(
                    label,
                    format!(
                        "🔁 DoubleLeg averaging timeout → cancelling {} order(s)",
                        ids.len()
                    ),
                );
            }
        }
        (HarvestState::DoubleLeg, HarvestState::Done) => {
            ipc::log_line(
                label,
                format!(
                    "🔒 DoubleLeg ProfitLock: avg_sum={:.4} ≤ threshold({:.2}), imbalance={:+.2} → Done",
                    snap.avg_yes + snap.avg_no,
                    snap.avg_threshold,
                    snap.imbalance,
                ),
            );
        }
        (HarvestState::OpenDual { .. }, HarvestState::Pending) => {
            ipc::log_line(label, "⏰ OpenDual timeout (no_fill) → cancelling 2 orders, reopening");
        }
        (HarvestState::SingleLeg { filled_side, .. }, HarvestState::SingleLeg { .. }) => {
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
        (HarvestState::SingleLeg { filled_side, .. }, HarvestState::Done) => {
            let first_leg = match filled_side {
                Outcome::Up => snap.avg_yes,
                Outcome::Down => snap.avg_no,
            };
            let hedge_leg = match filled_side {
                Outcome::Up => snap.no_best_ask,
                Outcome::Down => snap.yes_best_ask,
            };
            ipc::log_line(
                label,
                format!(
                    "🔒 ProfitLock triggered: first_leg({})={first_leg:.4} + hedge_leg({})={hedge_leg:.4} = {:.4} ≤ threshold({:.2}) → FAK",
                    filled_side.as_str(),
                    filled_side.opposite().as_str(),
                    first_leg + hedge_leg,
                    snap.avg_threshold,
                ),
            );
        }
        _ => {}
    }
}

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
        format!("🚫 DELETE /order ({} ids) ids={:?}", cancel_ids.len(), cancel_ids),
    );
}

fn log_cancel_responses(canceled: &[CancelResponse], label: &str) {
    for c in canceled {
        ipc::log_line(
            label,
            format!("    canceled={:?} not_canceled={}", c.canceled, c.not_canceled),
        );
    }
}

fn log_placements(out: &ExecuteOutput, signal_score: f64, es: f64, label: &str) {
    for ex in &out.placed {
        let status = if ex.filled { "matched" } else { "live" };
        ipc::log_line(
            label,
            format!(
                "✅ orderType={} side={} outcome={} size={} price={} | status={status} | reason={} | signal={signal_score:.2}(eff {es:.2})",
                ex.planned.order_type.as_str(),
                ex.planned.side.as_str(),
                ex.planned.outcome.as_str(),
                ex.planned.size,
                ex.planned.price,
                ex.planned.reason,
            ),
        );
    }
}
