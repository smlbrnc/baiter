//! Polymarket WS event handler dispatch.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::db;
use crate::engine::{
    absorb_trade_matched, outcome_from_asset_id, simulate_passive_fills, update_best,
    MarketSession,
};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::PolymarketEvent;
use crate::time::now_ms;
use crate::types::RunMode;

/// WS event'ini ilgili sub-handler'a yönlendir.
///
/// Sync — kritik yol kuralı (Rule 1) gereği WS event işleme bloklamamalı; tüm
/// alt handler'lar zaten sync ve DB yazımları `db::*::persist_*` üzerinden
/// `spawn_db` ile arka plana atılır.
pub fn handle_event(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    bot_id: i64,
    run_mode: RunMode,
    ev: PolymarketEvent,
) {
    match ev {
        PolymarketEvent::BestBidAsk {
            asset_id,
            best_bid,
            best_ask,
            ..
        } => on_best_bid_ask(sess, bot_id, run_mode, &asset_id, best_bid, best_ask),
        PolymarketEvent::Book {
            asset_id,
            bids,
            asks,
            ..
        } => on_book_snapshot(sess, bot_id, run_mode, &asset_id, &bids, &asks),
        PolymarketEvent::PriceChange { changes, .. } => {
            on_price_change(sess, bot_id, run_mode, &changes)
        }
        PolymarketEvent::Trade {
            trade_id,
            market,
            asset_id,
            side,
            outcome: outcome_str,
            size,
            price,
            status,
            fee_rate_bps,
            timestamp_ms,
            raw,
        } => {
            on_trade(
                sess,
                pool,
                bot_id,
                trade_id,
                market,
                &asset_id,
                side,
                outcome_str,
                size,
                price,
                status,
                fee_rate_bps,
                timestamp_ms,
                raw,
            )
        }
        PolymarketEvent::Order {
            order_id,
            market,
            asset_id,
            side,
            outcome: outcome_str,
            original_size,
            size_matched,
            price,
            order_type,
            status,
            lifecycle_type,
            timestamp_ms,
            raw,
        } => on_order(
            sess,
            pool,
            bot_id,
            order_id,
            market,
            asset_id,
            side,
            outcome_str,
            original_size,
            size_matched,
            price,
            order_type,
            status,
            lifecycle_type,
            timestamp_ms,
            raw,
        ),
        PolymarketEvent::MarketResolved {
            market,
            winning_outcome,
            winning_asset_id,
            timestamp_ms,
        } => on_market_resolved(
            pool,
            sess,
            bot_id,
            market,
            winning_outcome,
            winning_asset_id,
            timestamp_ms,
        ),
    }
}

/// WS `best_bid_ask` event'i — sadece in-memory book'u güncelle ve strateji
/// pipeline'ını (`after_book_update`) tetikle. Frontend BestBidAsk emit'i
/// `bot/zone.rs` içinden 1 sn cadence ile yapılır.
fn on_best_bid_ask(
    sess: &mut MarketSession,
    bot_id: i64,
    run_mode: RunMode,
    asset_id: &str,
    best_bid: f64,
    best_ask: f64,
) {
    update_best(sess, asset_id, best_bid, best_ask);
    after_book_update(sess, bot_id, run_mode);
}

fn on_book_snapshot(
    sess: &mut MarketSession,
    bot_id: i64,
    run_mode: RunMode,
    asset_id: &str,
    bids: &[f64],
    asks: &[f64],
) {
    // Polymarket WS'in array sıralamasına güvenmeden best_bid = max(bids),
    // best_ask = min(asks). Sparse NO orderbook'larında (ör. tek seviye
    // 0.01/0.99) bile doğru sonuç.
    let best_bid = bids.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let best_ask = asks.iter().copied().fold(f64::INFINITY, f64::min);
    if best_bid.is_finite() && best_ask.is_finite() {
        update_best(sess, asset_id, best_bid, best_ask);
        after_book_update(sess, bot_id, run_mode);
    }
}

/// `price_change` delta'sı best_bid/best_ask field'larını taşır; book snapshot
/// sonrası tek güncelleme kanalıdır. Field eksikse o seviye atlanır.
fn on_price_change(
    sess: &mut MarketSession,
    bot_id: i64,
    run_mode: RunMode,
    changes: &[crate::polymarket::PriceChangeLevel],
) {
    let mut any_update = false;
    for ch in changes {
        if let (Some(bb), Some(ba)) = (ch.best_bid, ch.best_ask) {
            update_best(sess, &ch.asset_id, bb, ba);
            any_update = true;
        }
    }
    if any_update {
        after_book_update(sess, bot_id, run_mode);
    }
}

/// Book güncellendikten sonra ortak kuyruk: book-ready logu + dryrun passive fill.
fn after_book_update(sess: &mut MarketSession, bot_id: i64, run_mode: RunMode) {
    maybe_log_book_ready(sess, bot_id);
    run_passive_fills_if_dryrun(sess, bot_id, run_mode);
}

#[allow(clippy::too_many_arguments)]
fn on_trade(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    bot_id: i64,
    trade_id: String,
    market: String,
    asset_id: &str,
    side: Option<String>,
    outcome_str: Option<String>,
    size: f64,
    price: f64,
    status: String,
    fee_rate_bps: Option<f64>,
    timestamp_ms: u64,
    raw: Arc<serde_json::Value>,
) {
    let status_upper = status.to_ascii_uppercase();
    let label = bot_id.to_string();

    log_ws_trade_line(&label, &trade_id, &status_upper, outcome_str.as_deref(), size, price, &raw);

    let fee = fee_rate_bps
        .map(|bps| price * size * bps / 10_000.0)
        .unwrap_or(0.0);
    // trade_id ve status_upper'a aşağıda Fill emit'i için tekrar erişiliyor →
    // sadece bu ikisini clone et; market/side/outcome_str kullanılmadığı için move.
    let trade_record = db::trades::TradeRecord::from_user_ws(db::trades::WsTradeInput {
        bot_id,
        market_session_id: sess.market_session_id,
        trade_id: trade_id.clone(),
        market,
        asset_id: asset_id.to_string(),
        side,
        outcome: outcome_str,
        size,
        price,
        status: status_upper.clone(),
        fee,
        ts_ms: timestamp_ms as i64,
        raw: &raw,
    });
    db::trades::persist_trade(pool, trade_record, "user_ws upsert_trade");

    if status_upper != "MATCHED" {
        return;
    }
    let Some(outcome) = outcome_from_asset_id(sess, asset_id) else {
        return;
    };
    absorb_trade_matched(sess, outcome, price, size, fee);
    log_fill_and_position(&label, sess, outcome, size, price);

    ipc::emit(&FrontendEvent::Fill {
        bot_id,
        trade_id,
        outcome,
        price,
        size,
        status: status_upper,
        ts_ms: now_ms(),
    });
}

/// §5.3 satır formatı — tüm statuslar için tek WS trade logu.
fn log_ws_trade_line(
    label: &str,
    trade_id: &str,
    status_upper: &str,
    outcome_str: Option<&str>,
    size: f64,
    price: f64,
    raw: &serde_json::Value,
) {
    let mut parts = vec![
        format!("id={trade_id}"),
        format!("status={status_upper}"),
    ];
    if let Some(o) = outcome_str {
        parts.push(format!("outcome={o}"));
    }
    parts.push(format!("size={size}"));
    parts.push(format!("price={price}"));
    if let Some(s) = raw.get("taker_order_id").and_then(|v| v.as_str()) {
        parts.push(format!("taker_order_id={s}"));
    }
    if let Some(s) = raw.get("trader_side").and_then(|v| v.as_str()) {
        parts.push(format!("trader_side={s}"));
    }
    ipc::log_line(label, format!("📬 WS trade | {}", parts.join(" ")));
}

/// MATCHED sonrası: fill summary + pozisyon/imbalance log satırları.
fn log_fill_and_position(
    label: &str,
    sess: &MarketSession,
    outcome: crate::types::Outcome,
    size: f64,
    price: f64,
) {
    ipc::log_line(
        label,
        format!(
            "✅ fill_summary outcome={} size={size} price={price}",
            outcome.as_str()
        ),
    );
    let imb = sess.metrics.imbalance;
    let imb_sign = if imb >= 0.0 {
        format!("+{imb}")
    } else {
        imb.to_string()
    };
    ipc::log_line(
        label,
        format!(
            "📊 [{:?}] Position: UP={}, DOWN={} (imbalance: {})",
            sess.strategy, sess.metrics.shares_yes, sess.metrics.shares_no, imb_sign
        ),
    );
}

#[allow(clippy::too_many_arguments)]
fn on_order(
    sess: &MarketSession,
    pool: &SqlitePool,
    bot_id: i64,
    order_id: String,
    market: String,
    asset_id: String,
    side: String,
    outcome_str: Option<String>,
    original_size: Option<f64>,
    size_matched: Option<f64>,
    price: Option<f64>,
    order_type: Option<String>,
    status: String,
    lifecycle_type: String,
    timestamp_ms: u64,
    raw: Arc<serde_json::Value>,
) {
    let label = bot_id.to_string();
    match lifecycle_type.as_str() {
        "PLACEMENT" => {
            let mut parts = vec!["type=PLACEMENT".to_string()];
            if let Some(ot) = order_type.as_deref().filter(|s| !s.is_empty()) {
                parts.push(format!("order_type={ot}"));
            }
            if !status.is_empty() {
                parts.push(format!("status={status}"));
            }
            parts.push(format!("id={order_id}"));
            ipc::log_line(&label, format!("📬 WS order {}", parts.join(" ")));
        }
        "UPDATE" => {
            let mut parts = vec!["type=UPDATE".to_string(), format!("id={order_id}")];
            if let Some(sm) = size_matched {
                parts.push(format!("size_matched={sm}"));
            }
            if let Some(at) = raw.get("associate_trades") {
                parts.push(format!("associate_trades={at}"));
            }
            ipc::log_line(&label, format!("📬 WS order {}", parts.join(" ")));
        }
        "CANCELLATION" => {
            ipc::log_line(
                &label,
                format!("📬 WS order type=CANCELLATION id={order_id}"),
            );
        }
        _ => {}
    }

    let record = db::orders::OrderRecord::from_user_ws(db::orders::WsOrderInput {
        bot_id,
        market_session_id: sess.market_session_id,
        order_id,
        market,
        asset_id,
        side,
        outcome: outcome_str,
        original_size,
        size_matched,
        price,
        order_type,
        status,
        lifecycle_type,
        ts_ms: timestamp_ms as i64,
        raw: &raw,
    });
    db::orders::persist_order(pool, record, "user_ws upsert_order");
}

fn on_market_resolved(
    pool: &SqlitePool,
    sess: &MarketSession,
    bot_id: i64,
    market: String,
    winning_outcome: String,
    winning_asset_id: Option<String>,
    timestamp_ms: u64,
) {
    let label = bot_id.to_string();
    let mut parts = vec![
        format!("market={market}"),
        format!("winning_outcome={winning_outcome}"),
    ];
    if let Some(a) = winning_asset_id.as_deref() {
        parts.push(format!("winning_asset_id={a}"));
    }
    parts.push(format!("ts={timestamp_ms}"));
    ipc::log_line(&label, format!("🏆 market_resolved | {}", parts.join(" | ")));

    ipc::emit(&FrontendEvent::SessionResolved {
        bot_id,
        slug: sess.slug.clone(),
        winning_outcome: winning_outcome.clone(),
        ts_ms: now_ms(),
    });

    // Kural 4: WS event consumer DB I/O bekleyemez — fire-and-forget.
    // market / winning_outcome / winning_asset_id artık okunmuyor → move.
    let pool = pool.clone();
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
fn maybe_log_book_ready(sess: &mut MarketSession, bot_id: i64) {
    if sess.book_ready_logged {
        return;
    }
    if sess.yes_best_bid > 0.0 && sess.no_best_bid > 0.0 {
        ipc::log_line(
            &bot_id.to_string(),
            format!(
                "📚 Market book ready: yes_bid={:.4} yes_ask={:.4} no_bid={:.4} no_ask={:.4}",
                sess.yes_best_bid, sess.yes_best_ask, sess.no_best_bid, sess.no_best_ask
            ),
        );
        sess.book_ready_logged = true;
    }
}

/// DryRun ise market book güncellemesinden sonra açık emirleri yeni quote'larla
/// karşılaştırıp passive (maker) fill'leri uygula.
fn run_passive_fills_if_dryrun(sess: &mut MarketSession, bot_id: i64, run_mode: RunMode) {
    if run_mode != RunMode::Dryrun {
        return;
    }
    let label = bot_id.to_string();
    for ex in simulate_passive_fills(sess) {
        let p = &ex.planned;
        // simulate_passive_fills her zaman `fill_price`/`fill_size`'ı doldurur;
        // None gelmesi yapısal hatadır (panic ile yüzeye çıkar).
        let fp = ex.fill_price.expect("dryrun fill_price always set");
        let fs = ex.fill_size.expect("dryrun fill_size always set");
        ipc::log_line(
            &label,
            format!(
                "📥 passive_fill side={} outcome={} size={} price={:.4} reason={}",
                p.side.as_str(),
                p.outcome.as_str(),
                fs,
                fp,
                p.reason
            ),
        );
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
