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
    let binance_score = ctx.signal_state.read().await.signal_score;
    let rtds_enabled = ctx.cfg.strategy_params.rtds_enabled_or_default();
    let (window_score, signal_ready) = if rtds_enabled {
        let rtds_snap = ctx.rtds_state.read().await;
        // Sinyal hazırlığı: RTDS aktif iken pencere açılış fiyatı yakalanana
        // kadar opener basılmaz. Aksi halde yeni pencerenin ilk 0.5-1 sn'sinde
        // composite skoru tamamen Binance OFI'a düşer ve eski pencerenin son
        // momentumu opener yönünü belirler (doc §3, §5; bot/tick gate).
        let ready = rtds_snap.window_open_price.is_some();
        let interval_secs = sess.end_ts.saturating_sub(sess.start_ts);
        // Linear extrapolation: 3 sn sonraki bps ≈ kümülatif + velocity × dt.
        let lookahead = ctx.cfg.strategy_params.signal_lookahead_secs_or_default();
        let projected_bps =
            rtds_snap.window_delta_bps + rtds_snap.recent_velocity_bps_per_sec * lookahead;
        let score = rtds::window_delta_score(projected_bps, rtds::interval_scale(interval_secs));
        (score, ready)
    } else {
        (5.0, true)
    };
    let composite = rtds::composite_score(
        window_score,
        binance_score,
        ctx.cfg.strategy_params.window_delta_weight_or_default(),
    );
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
                    .unwrap_or(0.0);
                let down = orders
                    .iter()
                    .find(|o| matches!(o.outcome, Outcome::Down))
                    .map(|o| o.price)
                    .unwrap_or(0.0);
                let delta = (composite - 5.0) / 5.0;
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
        (HarvestState::PositionOpen { filled_side }, HarvestState::HedgeUpdating { .. }) => {
            ipc::log_line(
                label,
                format!(
                    "🔁 Hedge drift detected (side={}, avg={:.4}, threshold={:.2}) → HedgeUpdating",
                    filled_side.as_str(),
                    match filled_side {
                        Outcome::Up => snap.avg_yes,
                        Outcome::Down => snap.avg_no,
                    },
                    snap.avg_threshold,
                ),
            );
        }
        (HarvestState::HedgeUpdating { .. }, HarvestState::PositionOpen { filled_side }) => {
            ipc::log_line(
                label,
                format!(
                    "✳️ Hedge re-priced (side={}, imbalance={:+.2}) → PositionOpen",
                    filled_side.as_str(),
                    snap.imbalance,
                ),
            );
        }
        (HarvestState::HedgeUpdating { .. }, HarvestState::PairComplete) => {
            ipc::log_line(label, "🎯 Hedge cancel-race (hedge passive fill) → PairComplete");
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

fn log_placements(out: &ExecuteOutput, composite: f64, label: &str) {
    // δ = (composite − 5) / 5 ∈ [−1, +1] — opener/pyramid fiyatının spread üzerinden ölçeklenmesini üreten faktör.
    // Avg-down emirlerinde fiyat best_bid'den verilir; δ bilgi amaçlıdır.
    let delta = (composite - 5.0) / 5.0;
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
