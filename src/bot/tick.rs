//! Strateji tick + state-transition logu + place/cancel logu.
//!
//! ⚡ Kural 1 (kritik yol sıfır blok): `decide → execute (POST/DELETE)`
//! arasına **hiçbir** sync I/O (log flush, IPC emit) girmez. Tüm log ve
//! frontend push'lar `execute` döndükten sonra `tokio::spawn` ile arkaplana
//! atılır; ana task hemen yeni event'e döner.

use crate::binance;
use crate::engine::{execute, ExecuteOutput, MarketSession};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::CancelResponse;
use crate::strategy::harvest::HarvestState;
use crate::strategy::Decision;
use crate::time::now_ms;
use crate::types::Outcome;

use super::ctx::Ctx;

/// State-transition logu için `tick` sırasında alınan snapshot — `sess`'in
/// log fonksiyonu çağrıldığı anki halini taşır, böylece logging arkaplan
/// task'ında session'a referans tutmaz.
struct StateLogSnapshot {
    shares_yes: f64,
    shares_no: f64,
    avg_yes: f64,
    avg_no: f64,
    yes_best_ask: f64,
    no_best_ask: f64,
    avg_threshold: f64,
}

/// `tick` arkaplan log/emit task'ına aktarılan tüm bağlam. Tüm alanlar
/// owned/Copy olduğundan `'static` task'a güvenle taşınır.
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

/// Strateji çağrısı + decision execute.
///
/// `bot/window.rs::run_trading_loop` içinden iki kanaldan tetiklenir:
/// - **Event-driven**: her WS event sonrası (Critical Path Zero Block).
/// - **Periyodik (1 sn)**: WS akışı sessiz olsa bile Binance signal
///   değişimleri için safety net.
pub async fn tick(ctx: &Ctx, sess: &mut MarketSession) {
    let signal_score = ctx.signal_state.read().await.signal_score;
    let es = binance::effective_score(signal_score, ctx.cfg.signal_weight);
    let prev_state = sess.harvest_state;
    let decision = sess.tick(&ctx.cfg, now_ms(), es);
    let bot_id = ctx.bot_id;
    let post_state = sess.harvest_state;
    let snap = StateLogSnapshot {
        shares_yes: sess.metrics.shares_yes,
        shares_no: sess.metrics.shares_no,
        avg_yes: sess.metrics.avg_yes,
        avg_no: sess.metrics.avg_no,
        yes_best_ask: sess.yes_best_ask,
        no_best_ask: sess.no_best_ask,
        avg_threshold: ctx.cfg.strategy_params.harvest_avg_threshold(),
    };

    if matches!(decision, Decision::NoOp) {
        let log_ctx = TickLogCtx {
            bot_id,
            label: bot_id.to_string(),
            prev_state,
            post_state,
            decision,
            signal_score,
            es,
            snap,
        };
        tokio::spawn(async move {
            log_state_transition(&log_ctx);
        });
        return;
    }

    let decision_for_log = decision.clone();
    let out = match execute(sess, &ctx.executor, decision).await {
        Ok(out) => out,
        Err(e) => {
            // Strateji state ileri taşındı (decide tamamlandı), POST/DELETE
            // başarısız oldu. Hata yüzeye çıkmalı; bir sonraki tick'te FSM
            // tutarsız yeniden değerlendirme yapabilir.
            tracing::error!(bot_id, error = %e, "execute failed in tick");
            ipc::log_line(
                &bot_id.to_string(),
                format!("❌ execute failed: {e}"),
            );
            return;
        }
    };

    let log_ctx = TickLogCtx {
        bot_id,
        label: bot_id.to_string(),
        prev_state,
        post_state,
        decision: decision_for_log,
        signal_score,
        es,
        snap,
    };
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

/// §5.2: OpenDual giriş/çıkış + Averaging timeout + ProfitLock geçişlerini
/// görselleştir. Sadece log etkisi vardır; akışı değiştirmez.
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
                        "🎯 OpenDual signal_score={:.2} effective_score={:.2} → up_bid={:.2} down_bid={:.2} deadline={}ms",
                        signal_score, es, up, down, deadline_ms
                    ),
                );
            }
        }
        (HarvestState::OpenDual { .. }, HarvestState::SingleLeg { filled_side }) => {
            let yes_filled = snap.shares_yes > 0.0;
            let no_filled = snap.shares_no > 0.0;
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
                    "🔒 ProfitLock triggered: first_leg({})={:.4} + hedge_leg({})={:.4} = {:.4} ≤ threshold({:.2}) → FAK",
                    filled_side.as_str(),
                    first_leg,
                    filled_side.opposite().as_str().to_uppercase(),
                    hedge_leg,
                    first_leg + hedge_leg,
                    snap.avg_threshold,
                ),
            );
        }
        _ => {}
    }
}

/// §5.5: cancel logu (DELETE /order ({n} ids) ids=[..]) — POST/DELETE
/// sonrası arka planda yazılır.
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

fn log_cancel_responses(canceled: &[CancelResponse], label: &str) {
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

fn log_placements(out: &ExecuteOutput, signal_score: f64, es: f64, label: &str) {
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
