//! 1 sn cadence frontend emit: `TickSnapshot`.

use crate::engine::MarketSession;
use crate::ipc::{self, FrontendEvent};
use crate::time::now_ms;

use super::ctx::Ctx;
use super::signal::SignalSnapshot;

/// `TickSnapshot` (book + sinyal) emit'i; cadence caller'da.
pub fn emit_frontend_snapshot(ctx: &Ctx, sess: &MarketSession, sig: &SignalSnapshot) {
    let ts_ms = now_ms();

    ipc::emit(&FrontendEvent::TickSnapshot {
        bot_id: ctx.bot_id,
        slug: sess.slug.clone(),
        up_best_bid: sess.up_best_bid,
        up_best_ask: sess.up_best_ask,
        down_best_bid: sess.down_best_bid,
        down_best_ask: sess.down_best_ask,
        signal_score: sig.composite,
        imbalance: sig.imbalance,
        momentum_bps: sig.momentum_bps,
        skor: sig.skor,
        ts_ms,
    });
}
