//! Graceful shutdown + ortak `cancel_all` yardımcısı.

use std::io::Write;

use crate::db;
use crate::engine::Executor;
use crate::ipc::{self, FrontendEvent};
use crate::time::now_ms;

use super::ctx::Ctx;

/// SIGTERM/SIGINT'ten sonra: cancel_all → DB STOPPED → frontend BotStopped.
pub async fn graceful_shutdown(ctx: &Ctx, reason: &str) {
    cancel_all_open(ctx, "shutdown").await;
    if let Err(e) = db::set_bot_state(&ctx.pool, ctx.bot_id, "STOPPED").await {
        tracing::warn!(bot_id = ctx.bot_id, error=%e, "set_bot_state STOPPED failed");
    }
    ipc::emit(&FrontendEvent::BotStopped {
        bot_id: ctx.bot_id,
        ts_ms: now_ms(),
        reason: reason.into(),
    });
    if let Err(e) = std::io::stdout().flush() {
        tracing::warn!(error=%e, "stdout flush failed during shutdown");
    }
}

/// Live mod ise CLOB'da tüm açık emirleri iptal eder; DryRun no-op.
///
/// `where_label` log konteksti (örn. "shutdown" / "window boundary").
pub async fn cancel_all_open(ctx: &Ctx, where_label: &str) {
    let label = ctx.bot_id.to_string();
    let Executor::Live(live) = &ctx.executor else {
        return;
    };
    ipc::log_line(&label, format!("🚫 cancel_all ({where_label})"));
    match live.client.cancel_all().await {
        Ok(resp) => ipc::log_line(
            &label,
            format!(
                "    canceled={:?} not_canceled={}",
                resp.canceled, resp.not_canceled
            ),
        ),
        Err(e) => {
            ipc::log_line(&label, format!("    cancel_all error: {e}"));
            tracing::warn!(error=%e, where_label=%where_label, "cancel_all failed");
        }
    }
}
