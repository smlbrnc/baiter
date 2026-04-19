//! DB persistence helper'ları — orders/trades fire-and-forget yazımları
//! `event.rs` ve `engine::executor` içinde, periyodik PnL snapshot ise burada.
//!
//! Tüm yazımlar `tokio::spawn` ile non-blocking çalışır (§⚡ Kural 4).

use sqlx::SqlitePool;

use crate::db;
use crate::engine::MarketSession;

/// `pnl_snapshots` tablosuna tek satır yazar — fire-and-forget (§⚡ Kural 4).
///
/// `window.rs` içinde 1 sn aralıkla çağrılır (frontend_timer ile aynı cadence).
pub fn snapshot_pnl(pool: &SqlitePool, sess: &MarketSession) {
    if sess.market_session_id == 0 {
        return;
    }
    let pool = pool.clone();
    let bot_id = sess.bot_id;
    let market_session_id = sess.market_session_id;
    let pnl = sess.pnl();
    let snap = db::pnl::PnlSnapshot {
        cost_basis: pnl.cost_basis,
        fee_total: pnl.fee_total,
        shares_yes: pnl.shares_yes,
        shares_no: pnl.shares_no,
        pnl_if_up: pnl.pnl_if_up,
        pnl_if_down: pnl.pnl_if_down,
        mtm_pnl: pnl.mtm_pnl,
        pair_count: sess.metrics.pair_count(),
        ts_ms: 0, // DB tarafı now_ms() kullanır.
    };
    db::spawn_db("pnl_snapshot insert", async move {
        db::pnl::insert_pnl_snapshot(&pool, bot_id, market_session_id, &snap).await
    });
}
