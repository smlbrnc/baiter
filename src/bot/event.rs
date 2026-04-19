//! Polymarket WS event handler dispatch.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::db;
use crate::engine::{
    absorb_trade_matched, outcome_from_asset_id, simulate_passive_fills, update_best,
    MarketSession,
};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::{PolymarketEvent, PriceChangeLevel};
use crate::time::now_ms;
use crate::types::{Outcome, RunMode};

struct TradeMsg {
    trade_id: String,
    market: String,
    asset_id: String,
    side: Option<String>,
    outcome: Option<String>,
    size: f64,
    price: f64,
    status: String,
    fee_rate_bps: Option<f64>,
    timestamp_ms: u64,
    raw: Arc<serde_json::Value>,
}

struct OrderMsg {
    order_id: String,
    market: String,
    asset_id: String,
    side: String,
    outcome: Option<String>,
    original_size: Option<f64>,
    size_matched: Option<f64>,
    price: Option<f64>,
    order_type: Option<String>,
    status: String,
    lifecycle_type: String,
    timestamp_ms: u64,
    raw: Arc<serde_json::Value>,
}

struct ResolvedMsg {
    market: String,
    winning_outcome: String,
    winning_asset_id: Option<String>,
    timestamp_ms: u64,
}

/// WS event'ini ilgili sub-handler'a yönlendir. Sync (Rule 1); DB I/O her
/// handler içinden `db::*::persist_*` → `spawn_db` ile arka plana atılır.
pub fn handle_event(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    run_mode: RunMode,
    ev: PolymarketEvent,
) {
    match ev {
        PolymarketEvent::BestBidAsk {
            asset_id, best_bid, best_ask, ..
        } => on_best_bid_ask(sess, pool, run_mode, &asset_id, best_bid, best_ask),
        PolymarketEvent::Book {
            asset_id, bids, asks, ..
        } => on_book_snapshot(sess, pool, run_mode, &asset_id, &bids, &asks),
        PolymarketEvent::PriceChange { changes, .. } => {
            on_price_change(sess, pool, run_mode, &changes)
        }
        PolymarketEvent::Trade {
            trade_id, market, asset_id, side, outcome, size, price, status,
            fee_rate_bps, timestamp_ms, raw,
        } => on_trade(
            sess, pool,
            TradeMsg {
                trade_id, market, asset_id, side, outcome, size, price, status,
                fee_rate_bps, timestamp_ms, raw,
            },
        ),
        PolymarketEvent::Order {
            order_id, market, asset_id, side, outcome, original_size, size_matched,
            price, order_type, status, lifecycle_type, timestamp_ms, raw,
        } => on_order(
            sess, pool,
            OrderMsg {
                order_id, market, asset_id, side, outcome, original_size, size_matched,
                price, order_type, status, lifecycle_type, timestamp_ms, raw,
            },
        ),
        PolymarketEvent::MarketResolved {
            market, winning_outcome, winning_asset_id, timestamp_ms,
        } => on_market_resolved(
            sess, pool,
            ResolvedMsg { market, winning_outcome, winning_asset_id, timestamp_ms },
        ),
    }
}

fn on_best_bid_ask(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    run_mode: RunMode,
    asset_id: &str,
    best_bid: f64,
    best_ask: f64,
) {
    update_best(sess, asset_id, best_bid, best_ask);
    after_book_update(sess, pool, run_mode);
}

fn on_book_snapshot(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    run_mode: RunMode,
    asset_id: &str,
    bids: &[f64],
    asks: &[f64],
) {
    // WS array sıralamasına güvenmeden best_bid = max(bids), best_ask = min(asks).
    let best_bid = bids.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let best_ask = asks.iter().copied().fold(f64::INFINITY, f64::min);
    if best_bid.is_finite() && best_ask.is_finite() {
        update_best(sess, asset_id, best_bid, best_ask);
        after_book_update(sess, pool, run_mode);
    }
}

fn on_price_change(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    run_mode: RunMode,
    changes: &[PriceChangeLevel],
) {
    let mut any_update = false;
    for ch in changes {
        if let (Some(bb), Some(ba)) = (ch.best_bid, ch.best_ask) {
            update_best(sess, &ch.asset_id, bb, ba);
            any_update = true;
        }
    }
    if any_update {
        after_book_update(sess, pool, run_mode);
    }
}

fn after_book_update(sess: &mut MarketSession, pool: &SqlitePool, run_mode: RunMode) {
    maybe_log_book_ready(sess);
    if run_mode == RunMode::Dryrun {
        run_passive_fills_dryrun(sess, pool);
    }
}

fn on_trade(sess: &mut MarketSession, pool: &SqlitePool, ev: TradeMsg) {
    let bot_id = sess.bot_id;
    let label = bot_id.to_string();
    let status_upper = ev.status.to_ascii_uppercase();

    log_ws_trade_line(&label, &ev, &status_upper);

    let fee = ev
        .fee_rate_bps
        .map(|bps| ev.price * ev.size * bps / 10_000.0)
        .unwrap_or(0.0);
    let record = db::trades::TradeRecord::from_user_ws(db::trades::WsTradeInput {
        bot_id,
        market_session_id: sess.market_session_id,
        trade_id: ev.trade_id.clone(),
        market: ev.market,
        asset_id: ev.asset_id.clone(),
        side: ev.side,
        outcome: ev.outcome,
        size: ev.size,
        price: ev.price,
        status: status_upper.clone(),
        fee,
        ts_ms: ev.timestamp_ms as i64,
        raw: &ev.raw,
    });
    db::trades::persist_trade(pool, record, "user_ws upsert_trade");

    if status_upper != "MATCHED" {
        return;
    }
    let Some(outcome) = outcome_from_asset_id(sess, &ev.asset_id) else {
        return;
    };
    absorb_trade_matched(sess, outcome, ev.price, ev.size, fee);
    log_fill_and_position(&label, sess, outcome, ev.size, ev.price);

    ipc::emit(&FrontendEvent::Fill {
        bot_id,
        trade_id: ev.trade_id,
        outcome,
        price: ev.price,
        size: ev.size,
        status: status_upper,
        ts_ms: now_ms(),
    });
}

fn log_ws_trade_line(label: &str, ev: &TradeMsg, status_upper: &str) {
    let mut parts = vec![
        format!("id={}", ev.trade_id),
        format!("status={status_upper}"),
    ];
    if let Some(o) = ev.outcome.as_deref() {
        parts.push(format!("outcome={o}"));
    }
    parts.push(format!("size={}", ev.size));
    parts.push(format!("price={}", ev.price));
    if let Some(s) = ev.raw.get("taker_order_id").and_then(|v| v.as_str()) {
        parts.push(format!("taker_order_id={s}"));
    }
    if let Some(s) = ev.raw.get("trader_side").and_then(|v| v.as_str()) {
        parts.push(format!("trader_side={s}"));
    }
    ipc::log_line(label, format!("📬 WS trade | {}", parts.join(" ")));
}

fn log_fill_and_position(label: &str, sess: &MarketSession, outcome: Outcome, size: f64, price: f64) {
    ipc::log_line(
        label,
        format!("✅ fill_summary outcome={} size={size} price={price}", outcome.as_str()),
    );
    let imb = sess.metrics.imbalance;
    ipc::log_line(
        label,
        format!(
            "📊 [{:?}] Position: UP={}, DOWN={} (imbalance: {imb:+})",
            sess.strategy, sess.metrics.shares_yes, sess.metrics.shares_no
        ),
    );
}

fn on_order(sess: &MarketSession, pool: &SqlitePool, ev: OrderMsg) {
    let bot_id = sess.bot_id;
    let label = bot_id.to_string();
    match ev.lifecycle_type.as_str() {
        "PLACEMENT" => {
            let mut parts = vec!["type=PLACEMENT".to_string()];
            if let Some(ot) = ev.order_type.as_deref().filter(|s| !s.is_empty()) {
                parts.push(format!("order_type={ot}"));
            }
            if !ev.status.is_empty() {
                parts.push(format!("status={}", ev.status));
            }
            parts.push(format!("id={}", ev.order_id));
            ipc::log_line(&label, format!("📬 WS order {}", parts.join(" ")));
        }
        "UPDATE" => {
            let mut parts = vec!["type=UPDATE".to_string(), format!("id={}", ev.order_id)];
            if let Some(sm) = ev.size_matched {
                parts.push(format!("size_matched={sm}"));
            }
            if let Some(at) = ev.raw.get("associate_trades") {
                parts.push(format!("associate_trades={at}"));
            }
            ipc::log_line(&label, format!("📬 WS order {}", parts.join(" ")));
        }
        "CANCELLATION" => {
            ipc::log_line(&label, format!("📬 WS order type=CANCELLATION id={}", ev.order_id));
        }
        _ => {}
    }

    let record = db::orders::OrderRecord::from_user_ws(db::orders::WsOrderInput {
        bot_id,
        market_session_id: sess.market_session_id,
        order_id: ev.order_id,
        market: ev.market,
        asset_id: ev.asset_id,
        side: ev.side,
        outcome: ev.outcome,
        original_size: ev.original_size,
        size_matched: ev.size_matched,
        price: ev.price,
        order_type: ev.order_type,
        status: ev.status,
        lifecycle_type: ev.lifecycle_type,
        ts_ms: ev.timestamp_ms as i64,
        raw: &ev.raw,
    });
    db::orders::persist_order(pool, record, "user_ws upsert_order");
}

fn on_market_resolved(sess: &MarketSession, pool: &SqlitePool, ev: ResolvedMsg) {
    let bot_id = sess.bot_id;
    let asset_part = ev
        .winning_asset_id
        .as_deref()
        .map(|a| format!(" | winning_asset_id={a}"))
        .unwrap_or_default();
    ipc::log_line(
        &bot_id.to_string(),
        format!(
            "🏆 market_resolved | market={} | winning_outcome={}{} | ts={}",
            ev.market, ev.winning_outcome, asset_part, ev.timestamp_ms
        ),
    );

    ipc::emit(&FrontendEvent::SessionResolved {
        bot_id,
        slug: sess.slug.clone(),
        winning_outcome: ev.winning_outcome.clone(),
        ts_ms: now_ms(),
    });

    let pool = pool.clone();
    let ResolvedMsg { market, winning_outcome, winning_asset_id, .. } = ev;
    db::spawn_db("market_resolved upsert", async move {
        db::markets::upsert_market_resolved(
            &pool,
            &market,
            &winning_outcome,
            winning_asset_id.as_deref(),
            now_ms() as i64,
            None,
        )
        .await
    });
}

/// İlk kez her iki taraf book'u dolduğunda tek seferlik bilgi logu.
fn maybe_log_book_ready(sess: &mut MarketSession) {
    if sess.book_ready_logged {
        return;
    }
    if sess.yes_best_bid > 0.0 && sess.no_best_bid > 0.0 {
        ipc::log_line(
            &sess.bot_id.to_string(),
            format!(
                "📚 Market book ready: yes_bid={:.4} yes_ask={:.4} no_bid={:.4} no_ask={:.4}",
                sess.yes_best_bid, sess.yes_best_ask, sess.no_best_bid, sess.no_best_ask
            ),
        );
        sess.book_ready_logged = true;
    }
}

/// Açık emirleri yeni quote'larla karşılaştır, passive (maker) fill'leri uygula
/// ve `trades` tablosuna fire-and-forget yaz.
fn run_passive_fills_dryrun(sess: &mut MarketSession, pool: &SqlitePool) {
    let bot_id = sess.bot_id;
    let label = bot_id.to_string();
    for ex in simulate_passive_fills(sess) {
        let p = &ex.planned;
        let fp = ex.fill_price.expect("dryrun fill_price always set");
        let fs = ex.fill_size.expect("dryrun fill_size always set");
        ipc::log_line(
            &label,
            format!(
                "📥 passive_fill side={} outcome={} size={fs} price={fp:.4} reason={}",
                p.side.as_str(),
                p.outcome.as_str(),
                p.reason
            ),
        );
        super::persist::persist_dryrun_fill(pool, sess, &ex, fp, fs, "MAKER");
        ipc::emit(&FrontendEvent::Fill {
            bot_id,
            trade_id: ex.order_id.clone(),
            outcome: p.outcome,
            price: fp,
            size: fs,
            status: "MATCHED".to_string(),
            ts_ms: now_ms(),
        });
    }
}
