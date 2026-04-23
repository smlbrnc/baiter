//! DB persistence helper'ları — fire-and-forget yazımlar (PnL/tick snapshot + DryRun fill).

use sqlx::SqlitePool;

use crate::db;
use crate::engine::{ExecutedOrder, MarketSession, DRYRUN_FEE_RATE};
use crate::ipc::{self, FrontendEvent};
use crate::strategy::metrics::MarketPnL;
use crate::time::now_ms;

use super::ctx::Ctx;
use super::signal::SignalSnapshot;

/// 1 sn cadence: `pnl_snapshots` insert + `PnlUpdate` SSE emit.
pub fn snapshot_pnl(pool: &SqlitePool, sess: &MarketSession) {
    if sess.market_session_id == 0 {
        return;
    }
    let pool = pool.clone();
    let pnl = sess.pnl();
    let pair_count = sess.metrics.pair_count();
    let avg_up = sess.metrics.avg_up;
    let avg_down = sess.metrics.avg_down;
    let ts_ms = now_ms();
    let snap = db::pnl::PnlSnapshot {
        cost_basis: pnl.cost_basis,
        fee_total: pnl.fee_total,
        up_filled: pnl.up_filled,
        down_filled: pnl.down_filled,
        pnl_if_up: pnl.pnl_if_up,
        pnl_if_down: pnl.pnl_if_down,
        mtm_pnl: pnl.mtm_pnl,
        pair_count,
        avg_up,
        avg_down,
        ts_ms: ts_ms as i64,
    };
    ipc::emit(&build_pnl_event(
        sess.bot_id,
        &sess.slug,
        &pnl,
        pair_count,
        avg_up,
        avg_down,
        ts_ms,
    ));
    let bot_id = sess.bot_id;
    let market_session_id = sess.market_session_id;
    db::spawn_db("pnl_snapshot insert", async move {
        db::pnl::insert_pnl_snapshot(&pool, bot_id, market_session_id, &snap).await
    });
}

fn build_pnl_event(
    bot_id: i64,
    slug: &str,
    pnl: &MarketPnL,
    pair_count: f64,
    avg_up: f64,
    avg_down: f64,
    ts_ms: u64,
) -> FrontendEvent {
    FrontendEvent::PnlUpdate {
        bot_id,
        slug: slug.to_string(),
        cost_basis: pnl.cost_basis,
        fee_total: pnl.fee_total,
        up_filled: pnl.up_filled,
        down_filled: pnl.down_filled,
        pnl_if_up: pnl.pnl_if_up,
        pnl_if_down: pnl.pnl_if_down,
        mtm_pnl: pnl.mtm_pnl,
        pair_count,
        avg_up: Some(avg_up),
        avg_down: Some(avg_down),
        ts_ms,
    }
}

/// 1 sn cadence: `market_ticks` insert (BBA + composite + RTDS alt sinyalleri).
pub fn snapshot_tick(ctx: &Ctx, sess: &MarketSession, sig: &SignalSnapshot) {
    if sess.market_session_id == 0 {
        return;
    }
    let tick = db::MarketTick {
        up_best_bid: sess.up_best_bid,
        up_best_ask: sess.up_best_ask,
        down_best_bid: sess.down_best_bid,
        down_best_ask: sess.down_best_ask,
        signal_score: sig.composite,
        bsi: sig.bsi,
        ofi: sig.ofi,
        cvd: sig.cvd,
        ts_ms: now_ms() as i64,
    };
    db::ticks::persist_tick(
        &ctx.pool,
        sess.bot_id,
        sess.market_session_id,
        tick,
        "market_tick insert",
    );
}

/// RTDS pencere açılışını `market_sessions`'a tek sefer yazar; `true` ise tekrar çağırma.
pub async fn maybe_persist_rtds_window_open(ctx: &Ctx, sess: &MarketSession) -> bool {
    let (price, ts_ms) = {
        let snap = ctx.rtds_state.read().await;
        match (snap.window_open_price, snap.window_open_ts_ms) {
            (Some(p), Some(t)) => (p, t),
            _ => return false,
        }
    };
    let pool = ctx.pool.clone();
    let session_id = sess.market_session_id;
    let ts_ms_i = ts_ms as i64;
    db::spawn_db("rtds_window_open update", async move {
        db::sessions::set_rtds_window_open(&pool, session_id, price, ts_ms_i).await
    });
    true
}

/// DryRun fill → `trades` insert; `trader_side` = `"TAKER"` (immediate) | `"MAKER"` (passive).
pub fn persist_dryrun_fill(
    pool: &SqlitePool,
    sess: &MarketSession,
    ex: &ExecutedOrder,
    fill_price: f64,
    fill_size: f64,
    trader_side: &'static str,
) {
    if sess.market_session_id == 0 {
        return;
    }
    let p = &ex.planned;
    let fee = fill_price * fill_size * DRYRUN_FEE_RATE;
    let record = db::trades::TradeRecord {
        trade_id: format!("dryrun:{}", ex.order_id),
        bot_id: sess.bot_id,
        market_session_id: Some(sess.market_session_id),
        market: Some(sess.condition_id.clone()),
        asset_id: Some(p.token_id.clone()),
        taker_order_id: None,
        maker_orders: None,
        trader_side: Some(trader_side.to_string()),
        side: Some(p.side.as_str().to_string()),
        outcome: Some(p.outcome.as_str().to_string()),
        size: fill_size,
        price: fill_price,
        status: "MATCHED".to_string(),
        fee,
        ts_ms: now_ms() as i64,
    };
    db::trades::persist_trade(pool, record, "dryrun fill upsert_trade");
}
