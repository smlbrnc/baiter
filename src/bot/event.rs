//! Polymarket WS event handler dispatch.
//! In-memory state önce, DB persist `spawn_db` ile fire-and-forget.

use sqlx::SqlitePool;

use crate::db;
use crate::engine::{
    apply_live_fill, simulate_passive_fills, update_top_of_book, MarketSession,
};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::{
    fee_for_role, FeeParams, MarketResolvedPayload, OrderLifecycle, OrderPayload, PolymarketEvent,
    PriceChangeLevel, TradePayload, TradeStatus,
};
use crate::time::now_ms;
use crate::types::{Outcome, RunMode, Side};

pub fn handle_event(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    run_mode: RunMode,
    ev: PolymarketEvent,
) {
    match ev {
        PolymarketEvent::BestBidAsk { asset_id, best_bid, best_ask }
        | PolymarketEvent::Book { asset_id, best_bid, best_ask } => {
            update_top_of_book(sess, &asset_id, best_bid, best_ask);
            after_book_update(sess, pool, run_mode);
        }
        PolymarketEvent::PriceChange { changes } => {
            on_price_change(sess, pool, run_mode, &changes)
        }
        PolymarketEvent::Trade(t) => on_trade(sess, pool, &t),
        PolymarketEvent::Order(o) => on_order(sess, &o),
        PolymarketEvent::MarketResolved(r) => on_market_resolved(sess, pool, &r),
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
            update_top_of_book(sess, &ch.asset_id, bb, ba);
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

#[derive(Debug, Clone)]
enum FillRole {
    Maker { open_order_id: String },
    Taker { taker_order_id: Option<String> },
}

impl FillRole {
    fn is_taker(&self) -> bool {
        matches!(self, Self::Taker { .. })
    }

    fn open_order_id(&self) -> Option<&str> {
        match self {
            Self::Maker { open_order_id } => Some(open_order_id.as_str()),
            Self::Taker { taker_order_id } => taker_order_id.as_deref(),
        }
    }
}

#[derive(Debug, Clone)]
struct Fill {
    role: FillRole,
    outcome: Outcome,
    asset_id: String,
    side: Side,
    price: f64,
    size: f64,
}

impl Fill {
    fn fee(&self, rate: f64) -> f64 {
        fee_for_role(
            self.price,
            self.size,
            &FeeParams { rate, taker_only: true },
            self.role.is_taker(),
        )
    }
}

/// DB tek-satır trade view: bizim fill varsa per-fill aggregate, yoksa top-level (foreign).
struct PersistedTrade {
    outcome: Option<String>,
    asset_id: String,
    side: String,
    price: f64,
    size: f64,
}

impl PersistedTrade {
    fn from_top_level(ev: &TradePayload) -> Self {
        Self {
            outcome: ev.outcome.clone(),
            asset_id: ev.asset_id.clone(),
            side: ev.side.as_str().to_string(),
            price: ev.price,
            size: ev.size,
        }
    }

    fn from_fills(fills: &[Fill]) -> Self {
        let first = &fills[0];
        let sum_size: f64 = fills.iter().map(|f| f.size).sum();
        let sum_pxsz: f64 = fills.iter().map(|f| f.price * f.size).sum();
        let avg_price = sum_pxsz / sum_size.max(f64::EPSILON);
        Self {
            outcome: Some(first.outcome.as_str().to_string()),
            asset_id: first.asset_id.clone(),
            side: first.side.as_str().to_string(),
            price: avg_price,
            size: sum_size,
        }
    }
}

/// Fill attribution sadece `MATCHED`'te; `MINED/CONFIRMED` aynı `trade_id`'yi upsert eder (freeze).
fn on_trade(sess: &mut MarketSession, pool: &SqlitePool, ev: &TradePayload) {
    let label = sess.bot_id.to_string();

    let fills: Vec<Fill> = if ev.status.is_initial_match() {
        ipc::log_line(
            &label,
            format!(
                "WS trade | id={} status={} side={} size={} price={}",
                ev.trade_id,
                ev.status.as_str(),
                ev.side.as_str(),
                ev.size,
                ev.price,
            ),
        );
        extract_fills(sess, ev)
    } else {
        Vec::new()
    };

    let rate = sess.fee_rate;
    let fees: Vec<f64> = fills.iter().map(|f| f.fee(rate)).collect();
    let total_fee: f64 = fees.iter().sum();

    for (f, &fee) in fills.iter().zip(&fees) {
        apply_fill(sess, &label, &ev.trade_id, ev.status, f, fee);
    }

    let view = if fills.is_empty() {
        PersistedTrade::from_top_level(ev)
    } else {
        PersistedTrade::from_fills(&fills)
    };
    persist_trade(pool, sess, ev, &view, total_fee);
}

/// `owner == sess.owner_uuid` → TAKER (top-level); aksi halde MAKER (`maker_orders[]` filtreli).
fn extract_fills(sess: &MarketSession, ev: &TradePayload) -> Vec<Fill> {
    let Some(owner) = sess.owner_uuid.as_deref() else {
        return Vec::new();
    };
    if ev.owner.as_deref() == Some(owner) {
        extract_taker_fill(ev).into_iter().collect()
    } else {
        extract_maker_fills(ev, owner)
    }
}

fn extract_maker_fills(ev: &TradePayload, owner: &str) -> Vec<Fill> {
    ev.maker_orders
        .iter()
        .filter(|m| m.owner.as_deref() == Some(owner))
        .filter_map(|m| {
            let outcome = m.outcome.as_deref().and_then(Outcome::parse)?;
            Some(Fill {
                role: FillRole::Maker { open_order_id: m.order_id.clone() },
                outcome,
                asset_id: m.asset_id.clone(),
                side: m.side,
                price: m.price,
                size: m.matched_amount,
            })
        })
        .collect()
}

fn extract_taker_fill(ev: &TradePayload) -> Option<Fill> {
    let outcome = ev.outcome.as_deref().and_then(Outcome::parse)?;
    Some(Fill {
        role: FillRole::Taker {
            taker_order_id: ev.taker_order_id.clone(),
        },
        outcome,
        asset_id: ev.asset_id.clone(),
        side: ev.side,
        price: ev.price,
        size: ev.size,
    })
}

fn apply_fill(
    sess: &mut MarketSession,
    label: &str,
    trade_id: &str,
    status: TradeStatus,
    fill: &Fill,
    fee: f64,
) {
    apply_live_fill(sess, fill.outcome, fill.side, fill.price, fill.size, fee);
    record_fill_and_prune_if_full(sess, fill.role.open_order_id(), fill.size, label);
    log_fill_and_position(label, sess, fill.outcome, fill.size, fill.price);
    emit_fill(
        sess.bot_id,
        trade_id,
        fill.outcome,
        fill.side,
        fill.price,
        fill.size,
        status.as_str(),
    );
}

fn emit_fill(
    bot_id: i64,
    trade_id: &str,
    outcome: Outcome,
    side: Side,
    price: f64,
    size: f64,
    status: &str,
) {
    ipc::emit(&FrontendEvent::Fill {
        bot_id,
        trade_id: trade_id.to_string(),
        outcome,
        side: side.as_str().to_string(),
        price,
        size,
        status: status.to_string(),
        ts_ms: now_ms(),
    });
}

fn persist_trade(
    pool: &SqlitePool,
    sess: &MarketSession,
    ev: &TradePayload,
    view: &PersistedTrade,
    fee: f64,
) {
    let maker_orders_json = if ev.maker_orders.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(&ev.maker_orders)
                .expect("Vec<MakerOrder> serialization is infallible"),
        )
    };
    let record = db::trades::TradeRecord::from_user_ws(db::trades::WsTradeInput {
        bot_id: sess.bot_id,
        market_session_id: sess.market_session_id,
        trade_id: ev.trade_id.clone(),
        market: ev.market.clone(),
        asset_id: view.asset_id.clone(),
        side: Some(view.side.clone()),
        outcome: view.outcome.clone(),
        size: view.size,
        price: view.price,
        status: ev.status.as_str().to_string(),
        fee,
        ts_ms: ev.timestamp_ms as i64,
        taker_order_id: ev.taker_order_id.clone(),
        maker_orders_json,
        trader_side: ev.trader_side.clone(),
    });
    db::trades::persist_trade(pool, record, "user_ws upsert_trade");
}

/// Dust eşiği: erken prune kısmi fill attribution'ı bozar; min_order_size değil, gerçek dust.
const FILL_DUST_THRESHOLD: f64 = 0.01;

fn record_fill_and_prune_if_full(
    sess: &mut MarketSession,
    order_id: Option<&str>,
    fill_size: f64,
    label: &str,
) {
    let Some(id) = order_id else { return };
    let mut fully_filled = false;
    if let Some(o) = sess.open_orders.iter_mut().find(|o| o.id == id) {
        o.size_matched += fill_size;
        let remaining = (o.size - o.size_matched).max(0.0);
        fully_filled = remaining < FILL_DUST_THRESHOLD;
        if fully_filled {
            ipc::log_line(label, format!("order filled, pruning id={id}"));
        }
    }
    if fully_filled {
        sess.open_orders.retain(|o| o.id != id);
    }
}

fn log_fill_and_position(label: &str, sess: &MarketSession, outcome: Outcome, size: f64, price: f64) {
    let imb = sess.metrics.imbalance();
    ipc::log_line(
        label,
        format!(
            "fill {} size={size} price={price} | [{}] UP={} DOWN={} imb={imb:+}",
            outcome.as_str(),
            sess.state.label(),
            sess.metrics.up_filled,
            sess.metrics.down_filled,
        ),
    );
}

/// Cancellation lifecycle'ında lokal `open_orders`'tan prune et (MATCHED prune'u `on_trade`'te).
fn on_order(sess: &mut MarketSession, ev: &OrderPayload) {
    if !matches!(ev.lifecycle, OrderLifecycle::Cancellation) {
        return;
    }
    let before = sess.open_orders.len();
    sess.open_orders.retain(|o| o.id != ev.order_id);
    if sess.open_orders.len() < before {
        ipc::log_line(
            &sess.bot_id.to_string(),
            format!("WS cancel id={} (pruned)", ev.order_id),
        );
    }
}

fn on_market_resolved(sess: &MarketSession, pool: &SqlitePool, ev: &MarketResolvedPayload) {
    let bot_id = sess.bot_id;
    let asset_part = ev
        .winning_asset_id
        .as_deref()
        .map(|a| format!(" | winning_asset_id={a}"))
        .unwrap_or_default();
    ipc::log_line(
        &bot_id.to_string(),
        format!(
            "market_resolved | market={} | winning_outcome={}{} | ts={}",
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
    let market = ev.market.clone();
    let winning_outcome = ev.winning_outcome.clone();
    let winning_asset_id = ev.winning_asset_id.clone();
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

fn maybe_log_book_ready(sess: &mut MarketSession) {
    if sess.book_ready_logged {
        return;
    }
    if sess.up_best_bid > 0.0 && sess.down_best_bid > 0.0 {
        ipc::log_line(
            &sess.bot_id.to_string(),
            format!(
                "Market book ready: up_bid={:.4} up_ask={:.4} down_bid={:.4} down_ask={:.4}",
                sess.up_best_bid, sess.up_best_ask, sess.down_best_bid, sess.down_best_ask
            ),
        );
        sess.book_ready_logged = true;
    }
}

fn run_passive_fills_dryrun(sess: &mut MarketSession, pool: &SqlitePool) {
    let bot_id = sess.bot_id;
    for ex in simulate_passive_fills(sess) {
        let p = &ex.planned;
        super::persist::persist_dryrun_fill(pool, sess, &ex, ex.fill_price, ex.fill_size, "MAKER");
        emit_fill(
            bot_id,
            &ex.order_id,
            p.outcome,
            p.side,
            ex.fill_price,
            ex.fill_size,
            "MATCHED",
        );
    }
}

