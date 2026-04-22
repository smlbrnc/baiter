//! Polymarket WS event handler dispatch.
//!
//! Sync (Rule 1). DB I/O `db::*::persist_*` → `spawn_db` ile fire-and-forget;
//! strateji-kritik in-memory state her zaman önce, audit persist sonra.
//!
//! Spec referansı: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.

use sqlx::SqlitePool;

use crate::db;
use crate::engine::{
    absorb_trade_matched, simulate_passive_fills, update_best, MarketSession,
};
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::{
    fee_for_role, MarketResolvedPayload, OrderLifecycle, OrderPayload, PolymarketEvent,
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
        PolymarketEvent::BestBidAsk { asset_id, best_bid, best_ask } => {
            on_best_bid_ask(sess, pool, run_mode, &asset_id, best_bid, best_ask)
        }
        PolymarketEvent::Book { asset_id, bids, asks } => {
            on_book_snapshot(sess, pool, run_mode, &asset_id, &bids, &asks)
        }
        PolymarketEvent::PriceChange { changes } => {
            on_price_change(sess, pool, run_mode, &changes)
        }
        PolymarketEvent::Trade(t) => on_trade(sess, pool, &t),
        PolymarketEvent::Order(o) => on_order(sess, &o),
        PolymarketEvent::MarketResolved(r) => on_market_resolved(sess, pool, &r),
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

/// Bizim bir trade event içindeki tek fill'imizin rolü.
///
/// - `Maker { open_order_id }`: `maker_orders[]` entry'mizin `order_id`'si
///   bizim `open_orders`'ımızda → fill `open_order_id`'a attribute edilir.
/// - `Taker { marker_order_id }`: top-level event bizim tarafımızdan tetiklendi.
///   Live mod REST `POST /order` `status=matched` yanıtında `LiveExecutor::place`
///   aynı id'yi `open_orders`'a marker olarak push eder; o id `taker_order_id`
///   alanında geri gelirse `marker_order_id` set edilir ve marker prune'lanır.
#[derive(Debug, Clone)]
enum FillRole {
    Maker { open_order_id: String },
    Taker { marker_order_id: Option<String> },
}

impl FillRole {
    fn is_taker(&self) -> bool {
        matches!(self, Self::Taker { .. })
    }

    /// `record_fill_and_prune_if_full` için open_order id'si.
    fn open_order_id(&self) -> Option<&str> {
        match self {
            Self::Maker { open_order_id } => Some(open_order_id.as_str()),
            Self::Taker { marker_order_id } => marker_order_id.as_deref(),
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
    /// Maker = 0, Taker = concave Polymarket fee. `bps` `MarketSession.fee_rate_bps`
    /// (CLOB `GET /fee-rate`) tek otoritedir.
    fn fee(&self, bps: u32) -> f64 {
        fee_for_role(self.price, self.size, bps, self.role.is_taker())
    }
}

/// DB persist için tek-satır view; per-fill aggregation veya top-level fallback.
///
/// `asset_id` ve `side` daima set (`TradePayload.side: Side` zorunlu).
/// `outcome` doğrudan WS payload'undan okunur — payload'da yoksa `None`.
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

    /// `fills.len() >= 1` garanti — caller sadece non-empty list'le çağırır.
    /// Polymarket bir trade event'i tek `asset_id` etrafında oluşur; aggregate
    /// her zaman tek-outcome'dur.
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

/// User Channel `trade` event handler.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.
/// Fill attribution **sadece** ilk `MATCHED`'de; sonraki `MINED/CONFIRMED` aynı
/// `trade_id`'yi upsert edip status/ts/raw günceller — outcome/price/size FREEZE.
fn on_trade(sess: &mut MarketSession, pool: &SqlitePool, ev: &TradePayload) {
    let bot_id = sess.bot_id;
    let label = bot_id.to_string();

    log_ws_trade_line(&label, ev);

    let fills: Vec<Fill> = if ev.status.is_initial_match() {
        extract_fills(sess, ev)
    } else {
        Vec::new()
    };

    let bps = sess.fee_rate_bps;
    let fees: Vec<f64> = fills.iter().map(|f| f.fee(bps)).collect();
    let total_fee: f64 = fees.iter().sum();

    for (f, &fee) in fills.iter().zip(&fees) {
        apply_fill(sess, &label, bot_id, &ev.trade_id, ev.status, f, fee);
    }

    let view = if fills.is_empty() {
        PersistedTrade::from_top_level(ev)
    } else {
        PersistedTrade::from_fills(&fills)
    };
    persist_trade(pool, sess, ev, &view, total_fee);
}

/// Trade event'inden bizim fill'lerimizi çıkar.
///
/// AsyncAPI spec: <https://docs.polymarket.com/api-reference/wss/user>.
/// Top-level `owner` REQUIRED alanı = "API key of the taker". Rolümüz bu
/// alanın bizim API key UUID'mize eşit olup olmamasıyla belirlenir:
///
/// - `ev.owner == sess.owner_uuid` → biz **TAKER**; top-level alanlar bizim
///   fill'imizi taşır; `taker_order_id` bizim emir id'mizdir.
/// - aksi → biz **MAKER**; `maker_orders[]` içinde
///   `owner == sess.owner_uuid` olan entry'ler bizim fill'lerimizdir.
///
/// `trader_side` alanı spec'te OPTIONAL → dispatch için kullanılmaz.
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
            marker_order_id: ev.taker_order_id.clone(),
        },
        outcome,
        asset_id: ev.asset_id.clone(),
        side: ev.side,
        price: ev.price,
        size: ev.size,
    })
}

/// Tek fill'in tüm side-effect'leri tek noktada: metrics absorb + open_order
/// prune + console log + frontend event emit.
fn apply_fill(
    sess: &mut MarketSession,
    label: &str,
    bot_id: i64,
    trade_id: &str,
    status: TradeStatus,
    fill: &Fill,
    fee: f64,
) {
    absorb_trade_matched(sess, fill.outcome, fill.side, fill.price, fill.size, fee);
    record_fill_and_prune_if_full(sess, fill.role.open_order_id(), fill.size, label);
    log_fill_and_position(label, sess, fill.outcome, fill.size, fill.price);
    ipc::emit(&FrontendEvent::Fill {
        bot_id,
        trade_id: trade_id.to_string(),
        outcome: fill.outcome,
        side: fill.side.as_str().to_string(),
        price: fill.price,
        size: fill.size,
        status: status.as_str().to_string(),
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
        // `Vec<MakerOrder>` serializasyonu deterministik ve infallible.
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

/// Tam-fill veya gerçek dust olduğunda prune edilecek tolerans (share).
///
/// Polymarket `min_order_size` (≈5.0) **yeni emir POST için** geçerli;
/// book'taki kısmi emir herhangi bir küçük boyutta dolmaya devam eder.
/// Eşiği çok yüksek tutmak, remaining < threshold sınırını aşan kısmi
/// fill'lerin erken prune'a yol açar ve sonraki fill'lerin yanlış TAKER
/// fallback'e düşmesine neden olur (bkz. ab551d33 phantom fill bug).
const FILL_DUST_THRESHOLD: f64 = 0.01;

/// Maker fill'i `OpenOrder.size_matched`'e ekle; kalan miktar
/// `FILL_DUST_THRESHOLD`'un altına düşerse emri `open_orders`'tan düşür →
/// strateji FSM (Alis/Elis/Aras) `PairComplete` benzeri kapanışları doğru tetikler.
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
            ipc::log_line(
                label,
                format!(
                    "open_order effectively filled — pruning id={id} size={} matched={} remaining={remaining} (< {FILL_DUST_THRESHOLD})",
                    o.size, o.size_matched
                ),
            );
        }
    }
    if fully_filled {
        sess.open_orders.retain(|o| o.id != id);
    }
}

fn log_ws_trade_line(label: &str, ev: &TradePayload) {
    ipc::log_line(
        label,
        format!(
            "WS trade | id={} status={} side={} size={} price={}",
            ev.trade_id,
            ev.status.as_str(),
            ev.side.as_str(),
            ev.size,
            ev.price,
        ),
    );
}

fn log_fill_and_position(label: &str, sess: &MarketSession, outcome: Outcome, size: f64, price: f64) {
    ipc::log_line(
        label,
        format!("fill_summary outcome={} size={size} price={price}", outcome.as_str()),
    );
    let imb = sess.metrics.imbalance();
    ipc::log_line(
        label,
        format!(
            "[{}] Position: UP={}, DOWN={} (imbalance: {imb:+})",
            sess.state.label(),
            sess.metrics.up_filled,
            sess.metrics.down_filled
        ),
    );
}

/// User Channel `order` event handler — in-memory state günceller.
///
/// `UPDATE` lifecycle'da full-fill prune `on_trade`'de yapılır: WS UPDATE
/// ilgili trade `MATCHED`'ten önce gelirse `extract_fills` order_id'i
/// bulamaz ve maker fill yanlış attribute olur.
fn on_order(sess: &mut MarketSession, ev: &OrderPayload) {
    let label = sess.bot_id.to_string();
    match ev.lifecycle {
        OrderLifecycle::Placement => {
            let mut parts = vec!["type=PLACEMENT".to_string()];
            if let Some(ot) = ev.order_type {
                parts.push(format!("order_type={}", ot.as_str()));
            }
            parts.push(format!("status={}", ev.status));
            parts.push(format!("id={}", ev.order_id));
            ipc::log_line(&label, format!("WS order {}", parts.join(" ")));
        }
        OrderLifecycle::Update => {
            let mut parts = vec!["type=UPDATE".to_string(), format!("id={}", ev.order_id)];
            if let Some(sm) = ev.size_matched {
                parts.push(format!("size_matched={sm}"));
            }
            ipc::log_line(&label, format!("WS order {}", parts.join(" ")));
        }
        OrderLifecycle::Cancellation => {
            ipc::log_line(&label, format!("WS order type=CANCELLATION id={}", ev.order_id));
            let before = sess.open_orders.len();
            sess.open_orders.retain(|o| o.id != ev.order_id);
            if sess.open_orders.len() < before {
                ipc::log_line(
                    &label,
                    format!(
                        "open_order canceled — pruning id={} (WS CANCELLATION)",
                        ev.order_id
                    ),
                );
            }
        }
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
    let label = bot_id.to_string();
    for ex in simulate_passive_fills(sess) {
        let p = &ex.planned;
        let fp = ex.fill_price;
        let fs = ex.fill_size;
        ipc::log_line(
            &label,
            format!(
                "passive_fill side={} outcome={} size={fs} price={fp:.4} reason={}",
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
            side: p.side.as_str().to_string(),
            price: fp,
            size: fs,
            status: "MATCHED".to_string(),
            ts_ms: now_ms(),
        });
    }
}

