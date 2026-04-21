//! Strateji tick + state-transition logu + place/cancel logu.
//!
//! ⚡ Kural 1: `decide → execute (POST/DELETE)` arasına sync I/O girmez. Tüm
//! log/IPC push'lar `execute` döndükten sonra `tokio::spawn` ile arkaplana atılır.

use crate::engine::{execute, ExecuteOutput, MarketSession};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::CancelResponse;
use crate::strategy::harvest::HarvestState;
use crate::strategy::Decision;
use crate::time::now_ms;
use crate::types::{Outcome, RunMode};

use super::ctx::Ctx;
use super::signal::decision_composite;

/// Composite skor `[0, 10]`; 5.0 = nötr (long/short eşit ağırlık).
/// `decide()` bu noktada opener/pyramid yönünü flip etmez.
const NEUTRAL_COMPOSITE: f64 = 5.0;

/// Doc §3: `δ = (composite − 5) / 5 ∈ [−1, +1]`. `decide()` ve log
/// helper'ları bu faktörü ortak kullansın diye tek noktaya alındı.
fn delta_from_composite(composite: f64) -> f64 {
    (composite - NEUTRAL_COMPOSITE) / NEUTRAL_COMPOSITE
}

/// State-transition logu için `tick` sırasındaki session snapshot — `'static`
/// task'a güvenle taşınır (tüm alanlar `Copy`).
#[derive(Clone, Copy)]
struct StateLogSnapshot {
    avg_yes: f64,
    avg_no: f64,
    avg_threshold: f64,
    imbalance: f64,
}

struct TickLogCtx {
    bot_id: i64,
    label: String,
    prev_state: HarvestState,
    post_state: HarvestState,
    decision: Decision,
    composite: f64,
    snap: StateLogSnapshot,
}

/// Strateji çağrısı + decision execute. Sinyal akışı:
/// `RTDS (window_delta + lookahead × velocity) + Binance(absolute OFI)
///   → composite_score → sess.tick`.
/// Composite hem OpenPair opener fiyatını hem zona-duyarlı avg/pyramid kararlarını sürer.
pub async fn tick(ctx: &Ctx, sess: &mut MarketSession) {
    // Sinyal hazırlığı: RTDS aktif iken pencere açılış fiyatı yakalanana
    // kadar opener basılmaz. Aksi halde yeni pencerenin ilk 0.5-1 sn'sinde
    // composite skoru tamamen Binance OFI'a düşer ve eski pencerenin son
    // momentumu opener yönünü belirler (doc §3, §5; bot/tick gate).
    let (composite, signal_ready) = decision_composite(ctx, sess).await;
    let prev_state = sess.harvest_state;
    let decision = sess.tick(&ctx.cfg, now_ms(), composite, signal_ready);
    let bot_id = ctx.bot_id;
    let post_state = sess.harvest_state;
    let snap = StateLogSnapshot {
        avg_yes: sess.metrics.avg_yes,
        avg_no: sess.metrics.avg_no,
        avg_threshold: ctx.cfg.strategy_params.harvest_avg_threshold(),
        imbalance: sess.metrics.imbalance,
    };

    let make_log_ctx = |decision: Decision| TickLogCtx {
        bot_id,
        label: bot_id.to_string(),
        prev_state,
        post_state,
        decision,
        composite,
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
                super::persist::persist_dryrun_fill(
                    &ctx.pool,
                    sess,
                    ex,
                    ex.fill_price,
                    ex.fill_size,
                    "TAKER",
                );
            }
        }
    }

    let log_ctx = make_log_ctx(decision_for_log);
    tokio::spawn(async move {
        log_state_transition(&log_ctx);
        log_cancel_request(&log_ctx.decision, &log_ctx.label);
        log_cancel_responses(&out.canceled, &log_ctx.label);
        log_placements(&out, log_ctx.composite, &log_ctx.label);
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

/// Harvest v2 FSM state transition logu (doc §4).
fn log_state_transition(c: &TickLogCtx) {
    let TickLogCtx {
        prev_state: prev,
        post_state: post,
        decision,
        composite,
        label,
        snap,
        ..
    } = c;
    let composite = *composite;
    let label = label.as_str();
    match (*prev, *post) {
        (HarvestState::Pending, HarvestState::OpenPair) => {
            if let Decision::PlaceOrders(orders) = decision {
                let up = orders
                    .iter()
                    .find(|o| matches!(o.outcome, Outcome::Up))
                    .map(|o| o.price)
                    .expect("OpenPair invariant: UP leg present in PlaceOrders");
                let down = orders
                    .iter()
                    .find(|o| matches!(o.outcome, Outcome::Down))
                    .map(|o| o.price)
                    .expect("OpenPair invariant: DOWN leg present in PlaceOrders");
                let delta = delta_from_composite(composite);
                ipc::log_line(
                    label,
                    format!(
                        "🎯 OpenPair composite={composite:.2} δ={delta:+.3} → up={up:.2} down={down:.2}"
                    ),
                );
            }
        }
        (HarvestState::OpenPair, HarvestState::PositionOpen { filled_side }) => {
            ipc::log_line(
                label,
                format!(
                    "🎣 OpenPair single-leg filled (side={}) → PositionOpen",
                    filled_side.as_str()
                ),
            );
        }
        (HarvestState::OpenPair, HarvestState::PairComplete) => {
            ipc::log_line(
                label,
                format!(
                    "🔀 OpenPair both filled → PairComplete (avg_yes={:.4} avg_no={:.4})",
                    snap.avg_yes, snap.avg_no
                ),
            );
        }
        (HarvestState::PositionOpen { .. }, HarvestState::PairComplete) => {
            ipc::log_line(
                label,
                format!(
                    "🎯 Hedge passive fill → PairComplete (avg_yes={:.4} avg_no={:.4})",
                    snap.avg_yes, snap.avg_no
                ),
            );
        }
        (HarvestState::PairComplete, HarvestState::Done) => {
            ipc::log_line(
                label,
                format!(
                    "🔒 PairComplete: avg_sum={:.4} threshold={:.2} imbalance={:+.2} → Done",
                    snap.avg_yes + snap.avg_no,
                    snap.avg_threshold,
                    snap.imbalance,
                ),
            );
        }
        // Atomic drift re-price (CancelAndPlace) — state aynı kaldığı için
        // FSM transition arm'larına düşmez; Decision'a bakıp özel log basıyoruz.
        (
            HarvestState::PositionOpen { filled_side },
            HarvestState::PositionOpen { .. },
        ) => {
            if matches!(decision, Decision::CancelAndPlace { .. }) {
                ipc::log_line(
                    label,
                    format!(
                        "🔁 Hedge drift atomic re-price (side={}, avg={:.4}, threshold={:.2})",
                        filled_side.as_str(),
                        match filled_side {
                            Outcome::Up => snap.avg_yes,
                            Outcome::Down => snap.avg_no,
                        },
                        snap.avg_threshold,
                    ),
                );
            }
        }
        _ => {}
    }
}

fn log_cancel_request(decision: &Decision, label: &str) {
    let cancel_ids: &[String] = match decision {
        Decision::CancelOrders(ids) => ids,
        Decision::CancelAndPlace { cancels, .. } => cancels,
        Decision::NoOp | Decision::PlaceOrders(_) => return,
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

fn log_placements(out: &ExecuteOutput, composite: f64, label: &str) {
    // δ = (composite − 5) / 5 ∈ [−1, +1] — opener/pyramid fiyatının spread üzerinden ölçeklenmesini üreten faktör.
    // Avg-down emirlerinde fiyat best_bid'den verilir; δ bilgi amaçlıdır.
    let delta = delta_from_composite(composite);
    for ex in &out.placed {
        let status = if ex.filled { "matched" } else { "live" };
        ipc::log_line(
            label,
            format!(
                "✅ orderType={} side={} outcome={} size={} price={} | status={status} | reason={} | composite={composite:.2} δ={delta:+.3}",
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
