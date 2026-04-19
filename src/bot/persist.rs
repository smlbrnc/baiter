//! DB persistence helper'ları — orders/trades fire-and-forget yazımları
//! `event.rs` ve `engine::executor` içinde, periyodik PnL snapshot ise burada.
//!
//! Tüm yazımlar `tokio::spawn` ile non-blocking çalışır (§⚡ Kural 4).

use sqlx::SqlitePool;

use crate::db;
use crate::engine::MarketSession;

/// `pnl_snapshots` tablosuna tek satır yazar — fire-and-forget (§⚡ Kural 4).
///
/// `window.rs` içinde 5 sn aralıkla çağrılır.
pub fn snapshot_pnl(pool: &SqlitePool, sess: &MarketSession) {
    if sess.market_session_id == 0 {
        return;
    }
    let pool = pool.clone();
    let bot_id = sess.bot_id;
    let market_session_id = sess.market_session_id;
    let pnl = sess.pnl();
    let pair_count = sess.metrics.pair_count();
    tokio::spawn(async move {
        if let Err(e) = db::pnl::insert_pnl_snapshot(
            &pool,
            bot_id,
            market_session_id,
            pnl.cost_basis,
            pnl.fee_total,
            pnl.shares_yes,
            pnl.shares_no,
            pnl.pnl_if_up,
            pnl.pnl_if_down,
            pnl.mtm_pnl,
            pair_count,
        )
        .await
        {
            tracing::warn!(error=%e, "pnl_snapshot insert failed");
        }
    });
}
