//! DB persistence helper'ları — fire-and-forget yazımlar (§⚡ Kural 4).
//!
//! Periyodik PnL/tick snapshot'ları + DryRun fill yazımı buradan; user-WS
//! kaynaklı orders/trades yazımları `event.rs` içinden tetiklenir.

use sqlx::SqlitePool;

use crate::db;
use crate::engine::{ExecutedOrder, MarketSession, DRYRUN_FEE_RATE};
use crate::time::now_ms;

use super::ctx::Ctx;
use super::signal::observed_snapshot;

/// `pnl_snapshots` tablosuna tek satır yazar — `window.rs` 1 sn cadence'inden.
pub fn snapshot_pnl(pool: &SqlitePool, sess: &MarketSession) {
    if sess.market_session_id == 0 {
        return;
    }
    let pool = pool.clone();
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
        avg_yes: sess.metrics.avg_yes,
        avg_no: sess.metrics.avg_no,
        ts_ms: 0, // DB tarafı now_ms() kullanır.
    };
    let bot_id = sess.bot_id;
    let market_session_id = sess.market_session_id;
    db::spawn_db("pnl_snapshot insert", async move {
        db::pnl::insert_pnl_snapshot(&pool, bot_id, market_session_id, &snap).await
    });
}

/// `market_ticks` tablosuna 1 sn cadence BBA + composite signal snapshot'ı yazar.
pub async fn snapshot_tick(ctx: &Ctx, sess: &MarketSession) {
    if sess.market_session_id == 0 {
        return;
    }
    let sig = observed_snapshot(ctx, sess).await;
    let tick = db::MarketTick {
        yes_best_bid: sess.yes_best_bid,
        yes_best_ask: sess.yes_best_ask,
        no_best_bid: sess.no_best_bid,
        no_best_ask: sess.no_best_ask,
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

/// RTDS pencere açılışını `market_sessions`'a bir kez yazar (fire-and-forget).
/// `true` döndüğünde çağırıcı aynı pencerede tekrar çağırmamalı.
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

/// DryRun fill → `trades` tablosuna fire-and-forget. `trader_side` =
/// `"TAKER"` (immediate match) | `"MAKER"` (passive fill).
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
    let source = match trader_side {
        "MAKER" => "dryrun_passive",
        _ => "dryrun_taker",
    };
    let raw = serde_json::json!({
        "source": source,
        "reason": p.reason,
        "order_type": p.order_type.as_str(),
    });
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
        raw_payload: Some(raw.to_string()),
    };
    db::trades::persist_trade(pool, record, "dryrun fill upsert_trade");
}
