//! Polymarket WS event handler dispatch.

use std::collections::{HashMap, HashSet};
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
        PolymarketEvent::BestBidAsk { asset_id, best_bid, best_ask } => {
            on_best_bid_ask(sess, pool, run_mode, &asset_id, best_bid, best_ask)
        }
        PolymarketEvent::Book { asset_id, bids, asks } => {
            on_book_snapshot(sess, pool, run_mode, &asset_id, &bids, &asks)
        }
        PolymarketEvent::PriceChange { changes } => {
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

/// Bizim trade'imizdeki tek bir fill — pozisyon math + DB persist için yeter.
#[derive(Debug, Clone)]
struct OurFill {
    outcome: Outcome,
    asset_id: String,
    /// Maker entry'nin `side`'ı; taker fallback yolunda top-level `side` (yoksa "").
    side: String,
    price: f64,
    size: f64,
}

fn on_trade(sess: &mut MarketSession, pool: &SqlitePool, ev: TradeMsg) {
    let bot_id = sess.bot_id;
    let label = bot_id.to_string();
    let status_upper = ev.status.to_ascii_uppercase();

    log_ws_trade_line(&label, &ev, &status_upper);

    let trade_id = ev.trade_id;
    let raw = ev.raw;
    let fee_rate_bps = ev.fee_rate_bps;

    // MATCHED dışındaki status update'lerde (MINED/CONFIRMED) extract gerekmez:
    // `upsert_trade` ON CONFLICT yalnızca status/ts_ms/raw_payload günceller,
    // outcome/price/size FREEZE kalır → ilk MATCHED'de yazdığımız OUR view
    // korunur.
    let our_fills: Vec<OurFill> = if status_upper == "MATCHED" {
        extract_our_fills(sess, &raw, &ev.asset_id, ev.side.as_deref(), ev.price, ev.size)
    } else {
        Vec::new()
    };

    // Persist view: tek satır/trade_id. our_fills doluysa bizim aggregate'i
    // yaz; boşsa (bilinmeyen asset veya MATCHED-dışı) top-level'ı yaz (audit).
    let (p_outcome, p_asset, p_side, p_price, p_size) = if !our_fills.is_empty() {
        aggregate_for_persist(&label, &our_fills, ev.side.as_deref())
    } else {
        (
            ev.outcome.clone(),
            Some(ev.asset_id.clone()),
            ev.side.clone(),
            ev.price,
            ev.size,
        )
    };
    let persist_fee = match fee_rate_bps {
        Some(bps) => p_price * p_size * bps / 10_000.0,
        None => {
            ipc::log_line(
                &label,
                format!("⚠️ trade {} missing fee_rate_bps; persist fee=0", trade_id),
            );
            0.0
        }
    };
    let record = db::trades::TradeRecord::from_user_ws(db::trades::WsTradeInput {
        bot_id,
        market_session_id: sess.market_session_id,
        trade_id: trade_id.clone(),
        market: ev.market,
        asset_id: p_asset.unwrap_or_default(),
        side: p_side,
        outcome: p_outcome,
        size: p_size,
        price: p_price,
        status: status_upper.clone(),
        fee: persist_fee,
        ts_ms: ev.timestamp_ms as i64,
        raw: &raw,
    });
    db::trades::persist_trade(pool, record, "user_ws upsert_trade");

    if status_upper != "MATCHED" {
        return;
    }

    // Pozisyon math: her bireysel fill ayrı `absorb_trade_matched` (mixed-outcome
    // edge case'inde her iki tarafın pozisyonu da doğru güncellenir).
    for f in our_fills {
        let fee_per_fill = fee_rate_bps
            .map(|bps| f.price * f.size * bps / 10_000.0)
            .unwrap_or(0.0); // warn yukarıda persist_fee match'inde tek sefer atıldı.
        absorb_trade_matched(sess, f.outcome, f.price, f.size, fee_per_fill);
        log_fill_and_position(&label, sess, f.outcome, f.size, f.price);
        ipc::emit(&FrontendEvent::Fill {
            bot_id,
            trade_id: trade_id.clone(),
            outcome: f.outcome,
            price: f.price,
            size: f.size,
            status: status_upper.clone(),
            ts_ms: now_ms(),
        });
    }
}

/// Trade event'inden bizim fill'lerimizi çıkar (User Channel garantisi: bu
/// event'e biz dahiliz; maker_orders'ta id'miz yoksa biz taker'ız).
///
/// 1. **Maker yolu**: `maker_orders[]`'ta `open_orders.id` ile eşleşen entry'ler
///    (her biri kendi `asset_id`/`side`/`price`/`matched_amount`'ı taşır;
///    NEG_RISK'te asset top-level'dan farklı outcome'da olabilir).
/// 2. **Taker fallback**: top-level `(asset_id, side, price, size)` —
///    `maker_orders[]`'ta id'miz yoksa biz taker'ız.
///
/// Bilinmeyen `asset_id` (UP/DOWN ikilisi dışı) sessizce atlanır — partial
/// match'lerde bile diğer geçerli fill'ler kaybolmaz.
fn extract_our_fills(
    sess: &MarketSession,
    raw: &serde_json::Value,
    top_asset_id: &str,
    top_side: Option<&str>,
    top_price: f64,
    top_size: f64,
) -> Vec<OurFill> {
    let our_ids: HashSet<&str> = sess.open_orders.iter().map(|o| o.id.as_str()).collect();

    let maker: Vec<OurFill> = raw
        .get("maker_orders")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("order_id")?.as_str()?;
                    if !our_ids.contains(id) {
                        return None;
                    }
                    let asset = m.get("asset_id")?.as_str()?;
                    let outcome = outcome_from_asset_id(sess, asset)?;
                    let amount: f64 = m.get("matched_amount")?.as_str()?.parse().ok()?;
                    let price: f64 = m.get("price")?.as_str()?.parse().ok()?;
                    let side = m
                        .get("side")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    Some(OurFill {
                        outcome,
                        asset_id: asset.to_string(),
                        side,
                        price,
                        size: amount,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !maker.is_empty() {
        return maker;
    }

    outcome_from_asset_id(sess, top_asset_id)
        .map(|outcome| {
            vec![OurFill {
                outcome,
                asset_id: top_asset_id.to_string(),
                side: top_side.unwrap_or_default().to_string(),
                price: top_price,
                size: top_size,
            }]
        })
        .unwrap_or_default()
}

/// `our_fills`'i tek bir DB satırına indirgemek için aggregate.
///
/// - Tek outcome'da birden fazla fill: `size = sum`, `price = sum(p*s)/sum(s)`.
/// - Birden fazla outcome (NEG_RISK edge case): toplam size'ı en yüksek olan
///   outcome'u dominant kabul edip onu yaz, WARN logla. Diğer outcome'un
///   pozisyonu yine `absorb_trade_matched` döngüsünde doğru güncellenir.
fn aggregate_for_persist(
    label: &str,
    fills: &[OurFill],
    fallback_side: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>, f64, f64) {
    // outcome -> (sum_size, sum_price_x_size, asset_id, side)
    let mut by_outcome: HashMap<Outcome, (f64, f64, String, String)> = HashMap::new();
    for f in fills {
        let entry = by_outcome
            .entry(f.outcome)
            .or_insert_with(|| (0.0, 0.0, f.asset_id.clone(), f.side.clone()));
        entry.0 += f.size;
        entry.1 += f.price * f.size;
    }
    if by_outcome.len() > 1 {
        ipc::log_line(
            label,
            format!(
                "⚠️ trade contains fills across {} outcomes — persisting dominant only",
                by_outcome.len()
            ),
        );
    }
    let (out, (sz, pxsz, asset, side)) = by_outcome
        .into_iter()
        .max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap_or(std::cmp::Ordering::Equal))
        .expect("aggregate_for_persist requires non-empty fills");
    let avg_price = pxsz / sz.max(f64::EPSILON);
    let side_out = if side.is_empty() {
        fallback_side.map(str::to_string)
    } else {
        Some(side)
    };
    (
        Some(out.as_str().to_string()),
        Some(asset),
        side_out,
        avg_price,
        sz,
    )
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
        other => {
            ipc::log_line(
                &label,
                format!("⚠️ unknown ws order lifecycle '{other}' id={}", ev.order_id),
            );
        }
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
        let fp = ex.fill_price;
        let fs = ex.fill_size;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BotConfig, StrategyParams};
    use crate::strategy::OpenOrder;
    use crate::types::{RunMode, Side, Strategy};
    use serde_json::json;

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
        }
    }

    /// Maker rolü: `maker_orders[]` içinde 3 entry, biri bizim → tek OurFill
    /// dönmeli ve değerler maker entry'den (top-level değil).
    #[test]
    fn extract_our_fills_maker_picks_matching_order_id() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xMINE"));

        let raw = json!({
            "maker_orders": [
                {
                    "order_id": "0xOTHER1",
                    "matched_amount": "5",
                    "price": "0.32",
                    "asset_id": "UP_TOKEN",
                    "side": "BUY"
                },
                {
                    "order_id": "0xMINE",
                    "matched_amount": "9.33",
                    "price": "0.33",
                    "asset_id": "UP_TOKEN",
                    "side": "BUY"
                },
                {
                    "order_id": "0xOTHER2",
                    "matched_amount": "2",
                    "price": "0.34",
                    "asset_id": "UP_TOKEN",
                    "side": "BUY"
                }
            ]
        });

        // Top-level taker view (DOWN/97/0.67) — maker eşleşince yok sayılmalı.
        let fills = extract_our_fills(&sess, &raw, "DOWN_TOKEN", Some("SELL"), 0.67, 97.0);
        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert_eq!(f.outcome, Outcome::Up);
        assert_eq!(f.asset_id, "UP_TOKEN");
        assert_eq!(f.side, "BUY");
        assert!((f.price - 0.33).abs() < 1e-9);
        assert!((f.size - 9.33).abs() < 1e-9);
    }

    /// Taker rolü: `maker_orders[]` ya boş ya da bizim id yok → top-level
    /// (asset_id, side, price, size) ile OurFill üret.
    #[test]
    fn extract_our_fills_taker_returns_top_level_ourfill() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xMINE"));

        let raw = json!({
            "maker_orders": [
                {
                    "order_id": "0xSOMEONE_ELSE",
                    "matched_amount": "9",
                    "price": "0.57",
                    "asset_id": "UP_TOKEN",
                    "side": "SELL"
                }
            ]
        });

        let fills = extract_our_fills(&sess, &raw, "UP_TOKEN", Some("BUY"), 0.57, 9.0);
        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert_eq!(f.outcome, Outcome::Up);
        assert_eq!(f.asset_id, "UP_TOKEN");
        assert_eq!(f.side, "BUY");
        assert!((f.price - 0.57).abs() < 1e-9);
        assert!((f.size - 9.0).abs() < 1e-9);
    }

    /// `maker_orders` alanı hiç yoksa ve top-level asset bilinmiyorsa → boş.
    /// Panik atmamalı.
    #[test]
    fn extract_our_fills_unknown_asset_returns_empty() {
        let sess = make_session();
        let raw = json!({});
        let fills = extract_our_fills(&sess, &raw, "UNKNOWN_ASSET", Some("BUY"), 0.5, 1.0);
        assert!(fills.is_empty());
    }

    /// Bir trade içinde bizim BİRDEN FAZLA maker emrimiz match olabilir
    /// (örn. UP ve DOWN tarafta açık iki emir; trade her ikisini de tetikler).
    #[test]
    fn extract_our_fills_maker_collects_all_our_orders() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xUP"));
        sess.open_orders.push({
            let mut o = open("0xDOWN");
            o.outcome = Outcome::Down;
            o
        });

        let raw = json!({
            "maker_orders": [
                {"order_id": "0xUP",   "matched_amount": "3", "price": "0.40", "asset_id": "UP_TOKEN",   "side": "BUY"},
                {"order_id": "0xDOWN", "matched_amount": "7", "price": "0.55", "asset_id": "DOWN_TOKEN", "side": "BUY"}
            ]
        });

        let fills = extract_our_fills(&sess, &raw, "UP_TOKEN", Some("SELL"), 0.6, 10.0);
        assert_eq!(fills.len(), 2);
        assert!(fills.iter().any(|f| f.outcome == Outcome::Up));
        assert!(fills.iter().any(|f| f.outcome == Outcome::Down));
    }

    /// NEG_RISK edge case: top-level taker token (DOWN_TOKEN/0.42) ile maker
    /// entry token (UP_TOKEN/0.58) farklı outcome. extract bizim entry'mizden
    /// (UP/0.58) okumalı, top-level'a uymamalı.
    #[test]
    fn extract_our_fills_maker_negrisk_different_asset() {
        let mut sess = make_session();
        sess.open_orders.push(open("0xMINE"));

        let raw = json!({
            "maker_orders": [
                {
                    "order_id": "0xMINE",
                    "matched_amount": "9",
                    "price": "0.58",
                    "asset_id": "UP_TOKEN",
                    "side": "BUY"
                }
            ]
        });

        let fills = extract_our_fills(&sess, &raw, "DOWN_TOKEN", Some("BUY"), 0.42, 50.50);
        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert_eq!(f.outcome, Outcome::Up);
        assert_eq!(f.asset_id, "UP_TOKEN");
        assert!((f.price - 0.58).abs() < 1e-9);
        assert!((f.size - 9.0).abs() < 1e-9);
    }

    /// Aynı outcome'da iki fill aggregate edilince size toplanır, price
    /// weighted average olur.
    #[test]
    fn aggregate_for_persist_sums_same_outcome() {
        let fills = vec![
            OurFill {
                outcome: Outcome::Up,
                asset_id: "UP_TOKEN".into(),
                side: "BUY".into(),
                price: 0.32,
                size: 5.0,
            },
            OurFill {
                outcome: Outcome::Up,
                asset_id: "UP_TOKEN".into(),
                side: "BUY".into(),
                price: 0.34,
                size: 10.0,
            },
        ];
        let (outcome, asset, side, price, size) = aggregate_for_persist("test", &fills, None);
        assert_eq!(outcome.as_deref(), Some("UP"));
        assert_eq!(asset.as_deref(), Some("UP_TOKEN"));
        assert_eq!(side.as_deref(), Some("BUY"));
        assert!((size - 15.0).abs() < 1e-9);
        // weighted: (0.32*5 + 0.34*10) / 15 = (1.6 + 3.4) / 15 ≈ 0.3333
        assert!((price - (1.6 + 3.4) / 15.0).abs() < 1e-9);
    }

    /// Mixed-outcome (NEG_RISK rare): UP size 9 + DOWN size 4 → outcome=UP
    /// (dominant), size=9.
    #[test]
    fn aggregate_for_persist_picks_dominant_when_mixed() {
        let fills = vec![
            OurFill {
                outcome: Outcome::Up,
                asset_id: "UP_TOKEN".into(),
                side: "BUY".into(),
                price: 0.58,
                size: 9.0,
            },
            OurFill {
                outcome: Outcome::Down,
                asset_id: "DOWN_TOKEN".into(),
                side: "BUY".into(),
                price: 0.40,
                size: 4.0,
            },
        ];
        let (outcome, asset, _side, price, size) = aggregate_for_persist("test", &fills, None);
        assert_eq!(outcome.as_deref(), Some("UP"));
        assert_eq!(asset.as_deref(), Some("UP_TOKEN"));
        assert!((size - 9.0).abs() < 1e-9);
        assert!((price - 0.58).abs() < 1e-9);
    }
}
