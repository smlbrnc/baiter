//! Strateji tick + place/cancel logu.
//!
//! ⚡ Kural 1: `decide → execute (POST/DELETE)` arasına sync I/O girmez. Tüm
//! log/IPC push'lar `execute` döndükten sonra `tokio::spawn` ile arkaplana atılır.

use crate::engine::{execute, ExecuteOutput, MarketSession};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::CancelResponse;
use crate::strategy::Decision;
use crate::time::now_ms;
use crate::types::RunMode;

use super::ctx::Ctx;
use super::signal::decision_composite;

/// Composite skor `[0, 10]`; 5.0 = nötr (long/short eşit ağırlık).
const NEUTRAL_COMPOSITE: f64 = 5.0;

/// `δ = (composite − 5) / 5 ∈ [−1, +1]`. Log helper'ları ortak kullansın diye
/// tek noktada.
fn delta_from_composite(composite: f64) -> f64 {
    (composite - NEUTRAL_COMPOSITE) / NEUTRAL_COMPOSITE
}

/// Strateji çağrısı + decision execute.
pub async fn tick(ctx: &Ctx, sess: &mut MarketSession) {
    // Sinyal hazırlığı: RTDS aktif iken pencere açılış fiyatı yakalanana
    // kadar opener basılmaz (composite skoru sadece Binance OFI'a düşmesin).
    let (composite, signal_ready) = decision_composite(ctx, sess).await;
    let decision = sess.tick(&ctx.cfg, now_ms(), composite, signal_ready);
    let bot_id = ctx.bot_id;
    let label = bot_id.to_string();

    if matches!(decision, Decision::NoOp) {
        return;
    }

    let decision_for_log = decision.clone();
    let out = match execute(sess, &ctx.executor, decision).await {
        Ok(out) => out,
        Err(e) => {
            tracing::error!(bot_id, error = %e, "execute failed in tick");
            ipc::log_line(&label, format!("❌ execute failed: {e}"));
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

    tokio::spawn(async move {
        log_cancel_request(&decision_for_log, &label);
        log_cancel_responses(&out.canceled, &label);
        log_placements(&out, composite, &label);
        emit_order_events(bot_id, &out);
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
