//! Polymarket WS event handler dispatch.
//!
//! Sync (Rule 1). DB I/O `db::*::persist_*` → `spawn_db` ile fire-and-forget;
//! strateji-kritik in-memory state her zaman önce, audit persist sonra.
//!
//! Spec referansı: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.

use std::collections::HashSet;

use sqlx::SqlitePool;

use crate::db;
use crate::engine::{
    absorb_trade_matched, outcome_from_asset_id, simulate_passive_fills, update_best,
    MarketSession,
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
/// `asset_id` ve `side` daima set (`TradePayload.side: Side` zorunlu;
/// aggregate bir asset etrafında). `outcome` gerçekten opsiyonel — top-level
/// fallback'te session yes/no mapping'de asset tanınmıyorsa `None`.
struct PersistedTrade {
    outcome: Option<String>,
    asset_id: String,
    side: String,
    price: f64,
    size: f64,
}

impl PersistedTrade {
    fn from_top_level(sess: &MarketSession, ev: &TradePayload) -> Self {
        let outcome = outcome_from_asset_id(sess, &ev.asset_id).map(|o| o.as_str().to_string());
        Self {
            outcome,
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
        PersistedTrade::from_top_level(sess, ev)
    } else {
        PersistedTrade::from_fills(&fills)
    };
    persist_trade(pool, sess, ev, &view, total_fee);
}

/// Trade event'inden bizim fill'lerimizi çıkar.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/websocket/user-channel>.
/// İki olası rol:
/// - **Maker**: `maker_orders[]`'ta `open_orders.id`'lerimizden eşleşen
///   entry'ler (her biri kendi asset/side/price/matched_amount'ı taşır;
///   NEG_RISK'te asset top-level'dan farklı outcome olabilir).
/// - **Taker**: bizim id `maker_orders`'ta yok → top-level (asset_id, side,
///   price, size) bizim view'ımız; bizim id ise `taker_order_id` field'ında
///   olur.
///
/// Hiçbir `raw.get()` çağrısı yok — sadece tipli `TradePayload` alanları.
fn extract_fills(sess: &MarketSession, ev: &TradePayload) -> Vec<Fill> {
    let our_ids: HashSet<&str> = sess.open_orders.iter().map(|o| o.id.as_str()).collect();

    let maker: Vec<Fill> = ev
        .maker_orders
        .iter()
        .filter(|m| our_ids.contains(m.order_id.as_str()))
        .filter_map(|m| {
            let outcome = outcome_from_asset_id(sess, &m.asset_id)?;
            Some(Fill {
                role: FillRole::Maker { open_order_id: m.order_id.clone() },
                outcome,
                asset_id: m.asset_id.clone(),
                side: m.side,
                price: m.price,
                size: m.matched_amount,
            })
        })
        .collect();
    if !maker.is_empty() {
        return maker;
    }

    let outcome = match outcome_from_asset_id(sess, &ev.asset_id) {
        Some(o) => o,
        None => return Vec::new(),
    };
    let marker_order_id = ev
        .taker_order_id
        .as_deref()
        .filter(|id| our_ids.contains(id))
        .map(str::to_string);
    vec![Fill {
        role: FillRole::Taker { marker_order_id },
        outcome,
        asset_id: ev.asset_id.clone(),
        side: ev.side,
        price: ev.price,
        size: ev.size,
    }]
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
/// book'taki kısmi emir herhangi bir küçük boyutta dolmaya devam eder. Bu
/// yüzden dust eşiği POST minimum'undan **çok daha küçük** olmalı: kalan
/// miktar 0.5 share'in altındaysa pratikte counterparty bulamaz.
const FILL_DUST_THRESHOLD: f64 = 0.5;

/// Maker fill'i `OpenOrder.size_matched`'e ekle; kalan miktar
/// `FILL_DUST_THRESHOLD`'un altına düşerse emri `open_orders`'tan düşür →
/// harvest FSM `PairComplete` doğru tetiklenir.
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
    let imb = sess.metrics.imbalance;
    ipc::log_line(
        label,
        format!(
            "[{:?}] Position: UP={}, DOWN={} (imbalance: {imb:+})",
            sess.strategy, sess.metrics.shares_yes, sess.metrics.shares_no
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
    if sess.yes_best_bid > 0.0 && sess.no_best_bid > 0.0 {
        ipc::log_line(
            &sess.bot_id.to_string(),
            format!(
                "Market book ready: yes_bid={:.4} yes_ask={:.4} no_bid={:.4} no_ask={:.4}",
                sess.yes_best_bid, sess.yes_best_ask, sess.no_best_bid, sess.no_best_ask
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BotConfig, StrategyParams};
    use crate::polymarket::{MakerOrder, TradePayload, TradeStatus};
    use crate::strategy::OpenOrder;
    use crate::types::{RunMode, Side, Strategy};

    fn make_session() -> MarketSession {
        let cfg = BotConfig {
            id: 1,
            name: "t".into(),
            slug_pattern: "t".into(),
            strategy: Strategy::Harvest,
            run_mode: RunMode::Live,
            order_usdc: 5.0,
            min_price: 0.05,
            max_price: 0.95,
            cooldown_threshold: 30_000,
            start_offset: 0,
            strategy_params: StrategyParams::default(),
        };
        let mut s = MarketSession::new(1, "slug".into(), &cfg);
        s.yes_token_id = "UP_TOKEN".into();
        s.no_token_id = "DOWN_TOKEN".into();
        s
    }

    fn open(id: &str) -> OpenOrder {
        OpenOrder {
            id: id.into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.33,
            size: 16.0,
            reason: "test".into(),
            placed_at_ms: 0,
            size_matched: 0.0,
        }
    }

    fn trade_payload(
        asset_id: &str,
        side: Side,
        price: f64,
        size: f64,
        taker_order_id: Option<&str>,
        maker_orders: Vec<MakerOrder>,
    ) -> TradePayload {
        TradePayload {
            trade_id: "T".into(),
            market: "M".into(),
            asset_id: asset_id.into(),
            side,
            size,
            price,
            status: TradeStatus::Matched,
            taker_order_id: taker_order_id.map(str::to_string),
            trader_side: None,
            maker_orders,
            timestamp_ms: 0,
        }
    }

    fn maker(order_id: &str, asset_id: &str, side: Side, price: f64, amount: f64) -> MakerOrder {
        MakerOrder {
            order_id: order_id.into(),
            asset_id: asset_id.into(),
            matched_amount: amount,
            price,
            side,
        }
    }

    /// Maker rolü: maker_orders[] içinde 3 entry, biri bizim → tek Fill (Maker).
    #[test]
    fn extract_fills_maker_picks_matching_order_id() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xMINE"));

        let ev = trade_payload(
            "DOWN_TOKEN",
            Side::Sell,
            0.67,
            97.0,
            None,
            vec![
                maker("0xOTHER1", "UP_TOKEN", Side::Buy, 0.32, 5.0),
                maker("0xMINE", "UP_TOKEN", Side::Buy, 0.33, 9.33),
                maker("0xOTHER2", "UP_TOKEN", Side::Buy, 0.34, 2.0),
            ],
        );

        let fills = extract_fills(&sess, &ev);
        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert_eq!(f.outcome, Outcome::Up);
        assert_eq!(f.asset_id, "UP_TOKEN");
        assert_eq!(f.side, Side::Buy);
        assert!((f.price - 0.33).abs() < 1e-9);
        assert!((f.size - 9.33).abs() < 1e-9);
        match &f.role {
            FillRole::Maker { open_order_id } => assert_eq!(open_order_id, "0xMINE"),
            _ => panic!("expected maker role"),
        }
        assert!(!f.role.is_taker(), "maker rolü → fee=0");
    }

    /// Taker rolü: maker_orders'ta bizim id yok → top-level Fill (Taker).
    #[test]
    fn extract_fills_taker_returns_top_level_fill() {
        let sess = make_session();

        let ev = trade_payload(
            "UP_TOKEN",
            Side::Buy,
            0.57,
            9.0,
            None,
            vec![maker("0xSOMEONE_ELSE", "UP_TOKEN", Side::Sell, 0.57, 9.0)],
        );

        let fills = extract_fills(&sess, &ev);
        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert_eq!(f.outcome, Outcome::Up);
        assert_eq!(f.asset_id, "UP_TOKEN");
        assert_eq!(f.side, Side::Buy);
        assert!((f.price - 0.57).abs() < 1e-9);
        assert!((f.size - 9.0).abs() < 1e-9);
        assert!(f.role.is_taker());
        match &f.role {
            FillRole::Taker { marker_order_id } => assert!(marker_order_id.is_none()),
            _ => panic!("expected taker role"),
        }
    }

    /// REST `status=matched` sonrası `LiveExecutor::place` `open_orders`'a
    /// marker ID push eder; aynı id `taker_order_id` olarak WS event'inde
    /// gelirse `marker_order_id` set edilir → `record_fill_and_prune_if_full`
    /// marker'ı düşürür. Bot 6 regresyonu.
    #[test]
    fn extract_fills_taker_marker_pruned_via_taker_order_id() {
        let mut sess = make_session();
        sess.open_orders.push({
            let mut o = open("0xMARKER");
            o.size_matched = o.size;
            o
        });

        let ev = trade_payload(
            "UP_TOKEN",
            Side::Buy,
            0.57,
            9.0,
            Some("0xMARKER"),
            vec![maker("0xSOMEONE", "UP_TOKEN", Side::Sell, 0.57, 9.0)],
        );

        let fills = extract_fills(&sess, &ev);
        assert_eq!(fills.len(), 1);
        match &fills[0].role {
            FillRole::Taker { marker_order_id } => {
                assert_eq!(marker_order_id.as_deref(), Some("0xMARKER"));
            }
            _ => panic!("expected taker role"),
        }
    }

    /// maker_orders yok ve top-level asset bilinmiyor → boş, panik atmamalı.
    #[test]
    fn extract_fills_unknown_asset_returns_empty() {
        let sess = make_session();
        let ev = trade_payload("UNKNOWN_ASSET", Side::Buy, 0.5, 1.0, None, vec![]);
        let fills = extract_fills(&sess, &ev);
        assert!(fills.is_empty());
    }

    /// Birden fazla maker emrimiz aynı trade'de match olabilir (UP+DOWN).
    #[test]
    fn extract_fills_maker_collects_all_our_orders() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xUP"));
        sess.open_orders.push({
            let mut o = open("0xDOWN");
            o.outcome = Outcome::Down;
            o
        });

        let ev = trade_payload(
            "UP_TOKEN",
            Side::Sell,
            0.6,
            10.0,
            None,
            vec![
                maker("0xUP", "UP_TOKEN", Side::Buy, 0.40, 3.0),
                maker("0xDOWN", "DOWN_TOKEN", Side::Buy, 0.55, 7.0),
            ],
        );

        let fills = extract_fills(&sess, &ev);
        assert_eq!(fills.len(), 2);
        assert!(fills.iter().any(|f| f.outcome == Outcome::Up));
        assert!(fills.iter().any(|f| f.outcome == Outcome::Down));
    }

    /// NEG_RISK: top-level taker token (DOWN/0.42) ile maker entry (UP/0.58)
    /// farklı outcome → bizim entry'den oku, top-level'a uyma.
    #[test]
    fn extract_fills_maker_negrisk_different_asset() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xMINE"));

        let ev = trade_payload(
            "DOWN_TOKEN",
            Side::Buy,
            0.42,
            50.50,
            None,
            vec![maker("0xMINE", "UP_TOKEN", Side::Buy, 0.58, 9.0)],
        );

        let fills = extract_fills(&sess, &ev);
        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert_eq!(f.outcome, Outcome::Up);
        assert_eq!(f.asset_id, "UP_TOKEN");
        assert!((f.price - 0.58).abs() < 1e-9);
        assert!((f.size - 9.0).abs() < 1e-9);
        assert!(!f.role.is_taker(), "maker rolü → fee=0");
    }

    /// İki fill: size toplanır, price weighted average.
    #[test]
    fn persisted_trade_from_fills_sums_same_outcome() {
        let fills = vec![
            Fill {
                role: FillRole::Maker { open_order_id: "0xMINE".into() },
                outcome: Outcome::Up,
                asset_id: "UP_TOKEN".into(),
                side: Side::Buy,
                price: 0.32,
                size: 5.0,
            },
            Fill {
                role: FillRole::Maker { open_order_id: "0xMINE".into() },
                outcome: Outcome::Up,
                asset_id: "UP_TOKEN".into(),
                side: Side::Buy,
                price: 0.34,
                size: 10.0,
            },
        ];
        let v = PersistedTrade::from_fills(&fills);
        assert_eq!(v.outcome.as_deref(), Some("UP"));
        assert_eq!(v.asset_id, "UP_TOKEN");
        assert_eq!(v.side, "BUY");
        assert!((v.size - 15.0).abs() < 1e-9);
        assert!((v.price - (1.6 + 3.4) / 15.0).abs() < 1e-9);
    }

    /// SELL fill DB satırı için `side="SELL"` döner.
    #[test]
    fn persisted_trade_from_fills_sell_emits_sell_side() {
        let fills = vec![Fill {
            role: FillRole::Taker { marker_order_id: None },
            outcome: Outcome::Up,
            asset_id: "UP_TOKEN".into(),
            side: Side::Sell,
            price: 0.92,
            size: 98.41,
        }];
        let v = PersistedTrade::from_fills(&fills);
        assert_eq!(v.side, "SELL");
        assert!((v.price - 0.92).abs() < 1e-9);
        assert!((v.size - 98.41).abs() < 1e-9);
    }

    /// Top-level fallback: outcome session mapping'inden çıkarılır.
    #[test]
    fn persisted_trade_from_top_level_uses_session_outcome_mapping() {
        let sess = make_session();
        let ev = trade_payload("UP_TOKEN", Side::Buy, 0.42, 5.0, None, vec![]);
        let v = PersistedTrade::from_top_level(&sess, &ev);
        assert_eq!(v.outcome.as_deref(), Some("UP"));
        assert_eq!(v.asset_id, "UP_TOKEN");
        assert_eq!(v.side, "BUY");
    }

    /// Tek fill emrin tamamını doldurursa prune.
    #[test]
    fn record_fill_full_prunes_order() {
        let mut sess = make_session();
        let mut o = open("0xHEDGE");
        o.size = 10.0;
        sess.open_orders.push(o);

        record_fill_and_prune_if_full(&mut sess, Some("0xHEDGE"), 10.0, "test");
        assert!(sess.open_orders.is_empty());
    }

    /// Partial fill (kalan ≥ FILL_DUST_THRESHOLD) → emir korunur.
    #[test]
    fn record_fill_partial_keeps_order() {
        let mut sess = make_session();
        let mut o = open("0xHEDGE");
        o.size = 10.0;
        sess.open_orders.push(o);

        record_fill_and_prune_if_full(&mut sess, Some("0xHEDGE"), 1.886_791, "test");
        assert_eq!(sess.open_orders.len(), 1);
        assert!((sess.open_orders[0].size_matched - 1.886_791).abs() < 1e-9);
    }

    /// Taker fill (order_id=None) → no-op.
    #[test]
    fn record_fill_taker_is_noop() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xMINE"));

        record_fill_and_prune_if_full(&mut sess, None, 5.0, "test");
        assert_eq!(sess.open_orders[0].size_matched, 0.0);
    }

    /// Bilinmeyen order_id → no-op.
    #[test]
    fn record_fill_unknown_id_is_noop() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xMINE"));

        record_fill_and_prune_if_full(&mut sess, Some("0xOTHER"), 5.0, "test");
        assert_eq!(sess.open_orders[0].size_matched, 0.0);
    }

    /// Bot 54 dust: hedge 8.996/9 → kalan 0.004 < FILL_DUST_THRESHOLD → prune.
    #[test]
    fn record_fill_dust_below_threshold_prunes() {
        let mut sess = make_session();
        let mut o = open("0xHEDGE");
        o.size = 9.0;
        sess.open_orders.push(o);

        record_fill_and_prune_if_full(&mut sess, Some("0xHEDGE"), 8.996, "test");
        assert!(sess.open_orders.is_empty());
    }

    /// Yarı dolma (5/10 → kalan 5 ≥ 0.5) → emir korunur ve sonraki fill'i
    /// bekler. Session-1 (`btc-updown-5m-1776763500`) bug regresyonu.
    #[test]
    fn record_fill_half_filled_keeps_order_for_session1_regression() {
        let mut sess = make_session();
        let mut o = open("0xe515");
        o.size = 10.0;
        sess.open_orders.push(o);

        record_fill_and_prune_if_full(&mut sess, Some("0xe515"), 0.311_606, "test");
        assert_eq!(sess.open_orders.len(), 1);

        record_fill_and_prune_if_full(&mut sess, Some("0xe515"), 5.0, "test");
        assert_eq!(
            sess.open_orders.len(),
            1,
            "remaining 4.69 hâlâ tradeable → emir korunmalı"
        );
        assert!((sess.open_orders[0].size_matched - 5.311_606).abs() < 1e-9);

        record_fill_and_prune_if_full(&mut sess, Some("0xe515"), 4.68, "test");
        assert!(
            sess.open_orders.is_empty(),
            "9.991/10 → remaining 0.009 < 0.5 → prune"
        );
    }

    /// Bot 53 regresyon: 3 fill (1.886+3.26+4.84) → 9.986/10 → prune.
    #[test]
    fn bot53_three_fills_complete_hedge_prunes() {
        let mut sess = make_session();
        let mut hedge = open("0xfb68");
        hedge.size = 10.0;
        hedge.outcome = Outcome::Down;
        sess.open_orders.push(hedge);

        record_fill_and_prune_if_full(&mut sess, Some("0xfb68"), 1.886_791, "test");
        assert_eq!(sess.open_orders.len(), 1);

        record_fill_and_prune_if_full(&mut sess, Some("0xfb68"), 3.26, "test");
        assert_eq!(sess.open_orders.len(), 1, "5.146/10 → remaining 4.85 ≥ 0.5");

        record_fill_and_prune_if_full(&mut sess, Some("0xfb68"), 4.84, "test");
        assert!(sess.open_orders.is_empty(), "9.986/10 → remaining 0.014 < 0.5");
    }

    /// Bot 54 #2 regresyon: 3 fill (5.78+0.89+3.32) → 9.99/10 → prune.
    #[test]
    fn bot54_three_fills_complete_hedge_prunes() {
        let mut sess = make_session();
        let mut hedge = open("0x2049");
        hedge.size = 10.0;
        sess.open_orders.push(hedge);

        record_fill_and_prune_if_full(&mut sess, Some("0x2049"), 5.78, "test");
        assert_eq!(sess.open_orders.len(), 1, "5.78/10 → remaining 4.22 ≥ 0.5");

        record_fill_and_prune_if_full(&mut sess, Some("0x2049"), 0.89, "test");
        assert_eq!(sess.open_orders.len(), 1, "6.67/10 → remaining 3.33 ≥ 0.5");

        record_fill_and_prune_if_full(&mut sess, Some("0x2049"), 3.32, "test");
        assert!(sess.open_orders.is_empty(), "9.99/10 → remaining 0.01 < 0.5");
    }
}
