//! Strateji tick + execute + log/IPC dispatch (decide→execute arasında sync I/O yok).

use crate::engine::{execute, ExecuteOutput, MarketSession};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::CancelResponse;
use crate::strategy::Decision;
use crate::time::now_ms;
use crate::types::RunMode;

use super::ctx::Ctx;
use super::signal::decision_composite;

/// Composite skor `[0, 10]`; 5.0 = nötr.
const NEUTRAL_COMPOSITE: f64 = 5.0;

/// `δ = (composite − 5) / 5 ∈ [−1, +1]`.
fn delta_from_composite(composite: f64) -> f64 {
    (composite - NEUTRAL_COMPOSITE) / NEUTRAL_COMPOSITE
}

fn cancel_ids_for_log(decision: &Decision) -> &[String] {
    match decision {
        Decision::CancelOrders(ids) => ids.as_slice(),
        Decision::CancelAndPlace { cancels, .. } => cancels.as_slice(),
        Decision::NoOp | Decision::PlaceOrders(_) => &[],
    }
}

/// Composite signal → decide → execute; başarılı execute sonrası `hot_path_latency` log.
pub async fn tick(ctx: &Ctx, sess: &mut MarketSession) {
    let recv_at = std::time::Instant::now();
    let server_ts = sess.last_book_server_ts_ms;
    // imbalance_opt → bsi slot; ofi/cvd artık None (eski alanlar kaldırıldı)
    let (composite, signal_ready, imbalance_opt, _, _) = decision_composite(ctx).await;
    let btc_spot = crate::binance_price::try_read_mid(&ctx.binance_price_state);
    let decision = sess.tick(
        &ctx.cfg,
        now_ms(),
        composite,
        signal_ready,
        imbalance_opt,
        None,
        None,
        btc_spot,
    );
    let bot_id = ctx.bot_id;
    let label = ctx.bot_label.as_ref();

    if matches!(decision, Decision::NoOp) {
        return;
    }

    let cancel_ids: Vec<String> = cancel_ids_for_log(&decision).to_vec();
    let out = match execute(sess, &ctx.executor, decision).await {
        Ok(out) => out,
        Err(e) => {
            tracing::error!(bot_id, error = %e, "execute failed in tick");
            ipc::log_line(label, format!("❌ execute failed: {e}"));
            return;
        }
    };

    if !out.placed.is_empty() || !out.canceled.is_empty() {
        let server_to_post_ms = if server_ts > 0 {
            now_ms().saturating_sub(server_ts)
        } else {
            0
        };
        tracing::info!(
            bot_id,
            server_to_post_ms,
            decide_to_post_us = recv_at.elapsed().as_micros() as u64,
            placed = out.placed.len(),
            canceled = out.canceled.iter().map(|c| c.canceled.len()).sum::<usize>(),
            "hot_path_latency"
        );
    }

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

    log_cancel(&cancel_ids, &out.canceled, label);
    log_placements(&out, composite, label);
    emit_order_events(bot_id, &out);
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

fn log_cancel(cancel_ids: &[String], canceled: &[CancelResponse], label: &str) {
    if cancel_ids.is_empty() {
        return;
    }
    let canceled_n: usize = canceled.iter().map(|c| c.canceled.len()).sum();
    let not_canceled_n: usize = canceled
        .iter()
        .filter_map(|c| c.not_canceled.as_object().map(|m| m.len()))
        .sum();
    ipc::log_line(
        label,
        format!(
            "🚫 cancel ids={cancel_ids:?} → canceled={canceled_n} not_canceled={not_canceled_n}"
        ),
    );
}

fn log_placements(out: &ExecuteOutput, composite: f64, label: &str) {
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
