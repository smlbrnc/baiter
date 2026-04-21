//! Harvest v3 birim testleri — profit-lock + cost-balanced hedge + atomic
//! avg-down/pyramid + hedge re-place senaryoları.

use super::state::{
    AVG_DOWN_REASON_PREFIX, HEDGE_REASON_PREFIX, OPEN_REASON_PREFIX, PYRAMID_REASON_PREFIX,
};
use super::*;

use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::{Decision, OpenOrder, MIN_NOTIONAL_USD};
use crate::time::MarketZone;
use crate::types::{Outcome, Side};

const COOLDOWN: u64 = 30_000;

fn mk_order(
    id: &str,
    outcome: Outcome,
    reason: &str,
    price: f64,
    size: f64,
    placed_at_ms: u64,
) -> OpenOrder {
    OpenOrder {
        id: id.into(),
        outcome,
        side: Side::Buy,
        price,
        size,
        reason: reason.into(),
        placed_at_ms,
        size_matched: 0.0,
    }
}

fn default_ctx<'a>(
    metrics: &'a StrategyMetrics,
    open_orders: &'a [OpenOrder],
) -> HarvestContext<'a> {
    HarvestContext {
        metrics,
        yes_token_id: "yes",
        no_token_id: "no",
        yes_best_bid: 0.50,
        yes_best_ask: 0.52,
        no_best_bid: 0.46,
        no_best_ask: 0.48,
        api_min_order_size: 5.0,
        order_usdc: 5.0,
        effective_score: 5.0,
        zone: MarketZone::NormalTrade,
        now_ms: 1_000_000,
        last_averaging_ms: 0,
        tick_size: 0.01,
        open_orders,
        avg_threshold: 0.98,
        min_price: 0.05,
        max_price: 0.95,
        cooldown_threshold: COOLDOWN,
        signal_ready: true,
    }
}

#[test]
fn pending_noop_when_book_missing() {
    let metrics = StrategyMetrics::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.yes_best_bid = 0.0;
    let (state, dec) = decide(HarvestState::Pending, &ctx);
    assert_eq!(state, HarvestState::Pending);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn pending_waits_when_signal_not_ready() {
    let metrics = StrategyMetrics::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.signal_ready = false;
    let (state, dec) = decide(HarvestState::Pending, &ctx);
    assert_eq!(state, HarvestState::Pending);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn pending_opens_pair_neutral() {
    let metrics = StrategyMetrics::default();
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(HarvestState::Pending, &ctx);
    assert_eq!(state, HarvestState::OpenPair);
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        _ => panic!("expected PlaceOrders"),
    };
    assert_eq!(orders.len(), 2);
    let open = orders
        .iter()
        .find(|o| o.reason.starts_with(OPEN_REASON_PREFIX))
        .expect("opener");
    let hedge = orders
        .iter()
        .find(|o| o.reason.starts_with(HEDGE_REASON_PREFIX))
        .expect("hedge");
    assert_eq!(open.outcome, Outcome::Up);
    assert!((open.price - 0.50).abs() < 1e-9, "open={}", open.price);
    assert_eq!(hedge.outcome, Outcome::Down);
    assert!((hedge.price - 0.48).abs() < 1e-9, "hedge={}", hedge.price);
}

#[test]
fn pending_opens_pair_high_signal() {
    let metrics = StrategyMetrics::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.effective_score = 8.0;
    let (state, dec) = decide(HarvestState::Pending, &ctx);
    assert_eq!(state, HarvestState::OpenPair);
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        _ => panic!("expected PlaceOrders"),
    };
    let open = orders
        .iter()
        .find(|o| o.reason.starts_with(OPEN_REASON_PREFIX))
        .unwrap();
    let hedge = orders
        .iter()
        .find(|o| o.reason.starts_with(HEDGE_REASON_PREFIX))
        .unwrap();
    assert_eq!(open.outcome, Outcome::Up);
    // delta = (8-5)/5 × (0.52-0.50) = 0.012 → open = snap(0.532) = 0.53
    assert!((open.price - 0.53).abs() < 1e-9, "open={}", open.price);
    assert_eq!(hedge.outcome, Outcome::Down);
    // hedge = snap(0.98 - 0.53) = 0.45
    assert!((hedge.price - 0.45).abs() < 1e-9, "hedge={}", hedge.price);
}

/// Share-balanced opening: opener_size == hedge_size. Hedge dolarsa
/// shares_yes == shares_no covered pair oluşur.
#[test]
fn pending_opens_pair_low_signal_share_balanced_hedge() {
    let metrics = StrategyMetrics::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.effective_score = 2.0;
    let (state, dec) = decide(HarvestState::Pending, &ctx);
    assert_eq!(state, HarvestState::OpenPair);
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        _ => panic!("expected PlaceOrders"),
    };
    let open = orders
        .iter()
        .find(|o| o.reason.starts_with(OPEN_REASON_PREFIX))
        .unwrap();
    let hedge = orders
        .iter()
        .find(|o| o.reason.starts_with(HEDGE_REASON_PREFIX))
        .unwrap();
    assert_eq!(open.outcome, Outcome::Down);
    // delta(Down) = (2-5)/5 × (0.48-0.46) = -0.012 → open = snap(0.492) = 0.49
    assert!((open.price - 0.49).abs() < 1e-9, "open={}", open.price);
    assert_eq!(hedge.outcome, Outcome::Up);
    // hedge = snap(0.98 - 0.49) = 0.49
    assert!((hedge.price - 0.49).abs() < 1e-9, "hedge={}", hedge.price);
    // Share balance: open_size == hedge_size.
    assert!(
        (open.size - hedge.size).abs() < 1e-9,
        "share balanced: open_size={} hedge_size={}",
        open.size,
        hedge.size
    );
}

/// Share-balanced hedge: hedge_size == open_size (fiyatlardan bağımsız).
/// Hedge dolarsa shares_yes == shares_no covered pair oluşur.
#[test]
fn open_pair_hedge_size_matches_open_size() {
    let metrics = StrategyMetrics::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &opens);
    // Bullish piyasa: opener UP @ 0.55, hedge DOWN @ 0.43.
    ctx.yes_best_bid = 0.55;
    ctx.yes_best_ask = 0.55;
    ctx.no_best_bid = 0.43;
    ctx.no_best_ask = 0.45;
    ctx.order_usdc = 10.0;
    let (_state, dec) = decide(HarvestState::Pending, &ctx);
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        _ => panic!("expected PlaceOrders"),
    };
    let open = orders
        .iter()
        .find(|o| o.reason.starts_with(OPEN_REASON_PREFIX))
        .unwrap();
    let hedge = orders
        .iter()
        .find(|o| o.reason.starts_with(HEDGE_REASON_PREFIX))
        .unwrap();
    // open = snap(0.55) = 0.55, hedge = snap(0.98 - 0.55) = 0.43
    assert!((open.price - 0.55).abs() < 1e-9);
    assert!((hedge.price - 0.43).abs() < 1e-9);
    // open_size = ceil(10/0.55) = 19, hedge_size == open_size (share-balanced)
    assert!(open.size >= 19.0 && open.size <= 19.5, "open_size={}", open.size);
    assert!(
        (hedge.size - open.size).abs() < 1e-9,
        "hedge_size == open_size beklenir, open={} hedge={}",
        open.size,
        hedge.size
    );
}

#[test]
fn open_pair_single_leg_fill_to_position_open() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.53, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.45,
        10.0,
        0,
    )];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(HarvestState::OpenPair, &ctx);
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn open_pair_both_filled_to_pair_complete() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.48, 10.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(HarvestState::OpenPair, &ctx);
    assert_eq!(state, HarvestState::PairComplete);
    assert!(matches!(dec, Decision::NoOp));
}

/// Avg-down tetiklenince ATOMIC: cancel(eski hedge) + place(avg-down + yeni hedge).
/// Yeni hedge avg-down'un tam dolacağı varsayımı ile projekte edilir.
#[test]
fn position_open_normal_trade_avg_down_atomic_replaces_hedge() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.48,
        10.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.now_ms = COOLDOWN + 1;
    ctx.last_averaging_ms = 0;
    ctx.yes_best_bid = 0.47;
    ctx.yes_best_ask = 0.48;
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    let (cancels, places) = match dec {
        Decision::CancelAndPlace { cancels, places } => (cancels, places),
        other => panic!("expected CancelAndPlace, got {:?}", other),
    };
    assert_eq!(cancels, vec!["hedge".to_string()]);
    assert_eq!(places.len(), 2);
    let avg_order = places
        .iter()
        .find(|o| o.reason.starts_with(AVG_DOWN_REASON_PREFIX))
        .expect("avg_down");
    let new_hedge = places
        .iter()
        .find(|o| o.reason.starts_with(HEDGE_REASON_PREFIX))
        .expect("hedge");
    assert_eq!(avg_order.outcome, Outcome::Up);
    assert!((avg_order.price - 0.47).abs() < 1e-9);
    assert_eq!(new_hedge.outcome, Outcome::Down);
    // Projekte avg_yes = (0.50*10 + 0.47*11) / 21 ≈ 0.4843 → target = 0.98 - 0.484 = 0.4957 → snap=0.50
    assert!(
        (new_hedge.price - 0.50).abs() < 1e-9,
        "hedge price={}",
        new_hedge.price
    );
    // Share-balanced: projekte shares_yes=21, shares_no=0 → hedge_size = 21
    assert!(
        (new_hedge.size - 21.0).abs() < 1e-9,
        "hedge size={} expected=21 (shares_yes after fill)",
        new_hedge.size
    );
}

#[test]
fn position_open_normal_trade_avg_down_skipped_when_ask_above_avg() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        // Cost-balanced: hedge price=0.48, target_notional=5.0, size=5/0.48=10.42
        0.48,
        10.42,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.now_ms = COOLDOWN + 1;
    ctx.yes_best_ask = 0.55;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert!(matches!(dec, Decision::NoOp), "got {:?}", dec);
}

/// AggTrade pyramid tetikleyicisi: rising == filled_side iken `ask > avg_filled`
/// (eski: `ask > last_fill`). Pyramid de avg-down gibi atomic CancelAndPlace
/// (eski hedge cancel + pyramid + yeni hedge).
#[test]
fn position_open_agg_trade_pyramid_uses_avg_filled_threshold_not_last_fill() {
    let mut metrics = StrategyMetrics::default();
    // İki avg birden: avg_yes = 0.55, son fill 0.60. Eski test 'last_fill > ask'
    // koşuluyla pyramid'i tetikliyordu; yeni koşul: `ask > avg_filled(0.55)`.
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.60, 10.0, 0.0);
    // Share-balanced hedge: shares_yes=20 → hedge_size=20, price=0.98-0.55=0.43
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.43,
        20.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::AggTrade;
    ctx.now_ms = COOLDOWN + 1;
    ctx.yes_best_bid = 0.60;
    ctx.yes_best_ask = 0.62;
    ctx.effective_score = 8.0;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    let (_cancels, places) = match dec {
        Decision::CancelAndPlace { cancels, places } => (cancels, places),
        other => panic!("expected CancelAndPlace (atomic pyramid+hedge), got {:?}", other),
    };
    assert!(places.iter().any(|o| o.reason.starts_with(PYRAMID_REASON_PREFIX)
        && o.outcome == Outcome::Up));
    assert!(places.iter().any(|o| o.reason.starts_with(HEDGE_REASON_PREFIX)));
}

/// Pyramid `last_fill > ask` koşuluna ARTIK takılmıyor — sadece `ask > avg_filled`
/// önemli. Eski testte last_fill_price_yes = 0.55 + ask = 0.62 → tetiklenir.
/// Yeni testte avg = 0.65 + ask = 0.62 → tetiklenmez (ask < avg).
#[test]
fn position_open_agg_trade_pyramid_skipped_when_ask_below_avg() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.65, 10.0, 0.0);
    // Share-balanced hedge: shares_yes=10 → hedge_size=10
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.33,
        10.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::AggTrade;
    ctx.now_ms = COOLDOWN + 1;
    ctx.yes_best_bid = 0.60;
    ctx.yes_best_ask = 0.62; // ask 0.62 < avg 0.65 → pyramid YOK
    ctx.effective_score = 8.0;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert!(
        matches!(dec, Decision::NoOp),
        "ask < avg → pyramid skip, got {:?}",
        dec
    );
}

/// Karşı taraf pyramid: trend gate (`ask > avg_filled`) sadece pyramid yönü
/// `filled_side` ile aynı olduğunda uygulanır. Ek olarak RİSK 3 gate'i:
/// projected `pair_avg_sum ≤ avg_threshold`.
#[test]
fn position_open_agg_trade_pyramid_opposite_skips_trend_gate() {
    // filled=Up @ 0.20 (düşük cost), yes_bid=0.40 → rising=Down (ε=0.05 dış).
    // Pyramid Down @ 0.59 → projected_avg_no = 0.59, projected_sum = 0.79 ≤ 0.98 → izin.
    // Share-balanced hedge: shares_yes=30 → hedge_size=30.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.20, 30.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.78,
        30.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::AggTrade;
    ctx.now_ms = COOLDOWN + 1;
    ctx.yes_best_bid = 0.40;
    ctx.yes_best_ask = 0.42;
    ctx.no_best_bid = 0.56;
    ctx.no_best_ask = 0.58;
    ctx.effective_score = 2.0;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    let (_cancels, places) = match dec {
        Decision::CancelAndPlace { cancels, places } => (cancels, places),
        other => panic!("expected CancelAndPlace, got {:?}", other),
    };
    let pyr = places
        .iter()
        .find(|o| o.reason.starts_with(PYRAMID_REASON_PREFIX))
        .expect("pyramid order");
    assert_eq!(pyr.outcome, Outcome::Down);
}

/// RİSK 5: `yes_bid` 0.5 ± ε(0.05) dead zone'da rising_side belirsiz → pyramid skip.
#[test]
fn position_open_pyramid_skipped_in_rising_dead_zone() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.48,
        10.42,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::AggTrade;
    ctx.now_ms = COOLDOWN + 1;
    // yes_bid = 0.52 → |0.52 - 0.5| = 0.02 < ε(0.05) → rising None.
    ctx.yes_best_bid = 0.52;
    ctx.yes_best_ask = 0.54;
    ctx.no_best_bid = 0.46;
    ctx.no_best_ask = 0.48;
    ctx.effective_score = 8.0;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert!(
        matches!(dec, Decision::NoOp),
        "rising dead zone → NoOp, got {:?}",
        dec
    );
}

/// RİSK 1: pozisyon imbalance flip ettiğinde hedge tarafı dinamik olarak
/// güncellenir. Eski (yanlış taraftaki) hedge cancel + yeni majority'nin
/// tersine hedge place atomic gönderilir.
#[test]
fn position_open_hedge_side_flips_when_majority_changes() {
    // Pyramid Down dolduktan sonra majority Down → hedge Up'a flip etmeli.
    // avg_sum = 0.40+0.65 = 1.05 > 0.98 → profit_locked tetiklenmesin.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.40, 10.0, 0.0); // cost_yes = 4.0
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.65, 30.0, 0.0); // cost_no = 19.5 → majority Down
    // Eski hedge hâlâ Down tarafında kalmış (filled_side=Up'ın hedge'i).
    let opens = vec![mk_order(
        "stale_hedge_down",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.58,
        5.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::DeepTrade;
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    let (cancels, places) = match dec {
        Decision::CancelAndPlace { cancels, places } => (cancels, places),
        other => panic!("expected CancelAndPlace (hedge flip), got {:?}", other),
    };
    assert_eq!(cancels, vec!["stale_hedge_down".to_string()]);
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].outcome, Outcome::Up, "hedge tarafı Up'a flip");
    assert!(places[0].reason.starts_with(HEDGE_REASON_PREFIX));
    // hedge price = avg_threshold - avg_filled(Down) = 0.98 - 0.65 = 0.33
    assert!((places[0].price - 0.33).abs() < 1e-9, "got {}", places[0].price);
}

/// ProfitLock: shares dengeli (`|diff| < api_min_order_size`) ve her iki tarafta
/// fill var → ProfitLocked (HOLD). Share-balanced hedge sayesinde
/// `avg_sum ≤ avg_threshold` invariant'ı otomatik garanti edilir.
#[test]
fn position_open_profit_locks_when_shares_balanced_within_api_min() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.45, 10.0, 0.0);
    // shares_no = 12 → diff = 2 < api_min(5) → balanced.
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.55, 12.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::ProfitLocked {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
}

/// Hedge fiyat sapması — atomic CancelAndPlace ile tek hedge re-place
/// (avg-down/pyramid tetiklenmiyorsa places tek elemanlı).
#[test]
fn position_open_hedge_price_drift_triggers_cancel_and_place() {
    // avg_yes = 0.45 (iki fill: 0.55 + 0.35). Hedge kitapta @ 0.48 (eski).
    // target = 0.98 − 0.45 = 0.53, |0.48 − 0.53| = 0.05 > tick/2 → drift.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.55, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.35, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge1",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.48,
        10.0,
        0,
    )];
    // avg_down tetiklenmesin: best_ask >= avg → ask=0.50 (avg 0.45 üstü).
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.yes_best_ask = 0.50;
    ctx.zone = MarketZone::DeepTrade; // pyramid/avg-down by-pass
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelAndPlace { cancels, places } => {
            assert_eq!(cancels, vec!["hedge1".to_string()]);
            assert_eq!(places.len(), 1, "tek hedge re-place beklenir");
            let h = &places[0];
            assert_eq!(h.outcome, Outcome::Down);
            assert!(
                (h.price - 0.53).abs() < 1e-9,
                "hedge target = 0.98 − 0.45 = 0.53, got {}",
                h.price
            );
            // Share-balanced: shares_yes=20, shares_no=0 → hedge_size=20.
            assert!(
                (h.size - 20.0).abs() < 1e-9,
                "hedge size={} expected=20 (share-balanced)",
                h.size,
            );
        }
        other => panic!("expected CancelAndPlace, got {:?}", other),
    }
}

/// Size drift: hedge size share-balanced'ı tutturmuyor → re-place.
/// Bot 1 / `btc-updown-5m-1776791700` kısmi opener fill regresyonu.
#[test]
fn position_open_partial_opener_resizes_hedge_to_share_balanced() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.57, 3.0, 0.0); // kısmi opener fill
    // Hedge orijinal opener_size ile basıldı (3'ten büyük, drift yaratacak).
    // target_size = shares(Up)-shares(Down) = 3, hedge size 9 → drift=6 ≥ api_min(1) → re-place.
    let opens = vec![mk_order(
        "hedge_orig",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.39,
        9.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.api_min_order_size = 1.0;
    ctx.avg_threshold = 0.96;
    ctx.zone = MarketZone::DeepTrade;
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelAndPlace { cancels, places } => {
            assert_eq!(cancels, vec!["hedge_orig".to_string()]);
            assert_eq!(places.len(), 1);
            let h = &places[0];
            assert_eq!(h.outcome, Outcome::Down);
            assert!(
                (h.price - 0.39).abs() < 1e-9,
                "fiyat aynı: 0.96 - 0.57 = 0.39, got {}",
                h.price
            );
            // Share-balanced: shares_yes=3 → hedge_size=3 (api_min=1)
            assert!(
                (h.size - 3.0).abs() < 1e-9,
                "hedge size={} expected=3 (share-balanced)",
                h.size,
            );
        }
        other => panic!("expected CancelAndPlace (size drift), got {:?}", other),
    }
}

/// Hedge boyutu share-balanced doğru ise (size diff < api_min) re-place yok.
#[test]
fn position_open_hedge_size_in_share_tolerance_skips_replace() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.57, 9.0, 0.0); // tam opener fill
    // Share-balanced: target_size = shares_yes(9) - shares_no(0) = 9.
    // Hedge size = 9, drift = 0 < api_min(5) → skip.
    let opens = vec![mk_order(
        "hedge_orig",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.39,
        9.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.avg_threshold = 0.96;
    ctx.zone = MarketZone::DeepTrade;
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    assert!(
        matches!(dec, Decision::NoOp),
        "fiyat & size in tolerance → NoOp, got {:?}",
        dec
    );
}

/// Hedge kısmen dolduğunda: shares(Down) artar, target_size düşer, remaining
/// de düşer → share-balanced korunduğu sürece re-place yok.
#[test]
fn position_open_hedge_partial_fill_in_share_sync_no_replace() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.57, 9.0, 0.0);
    // Şimdi hedge 4 share doldu → shares_no=4. target_size = 9-4 = 5.
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.39, 4.0, 0.0);
    // Hedge orig size 9, size_matched 4 → remaining = 5. drift = 0 < api_min(5) → skip.
    let mut hedge = mk_order(
        "hedge_orig",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.39,
        9.0,
        0,
    );
    hedge.size_matched = 4.0;
    let opens = vec![hedge];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.avg_threshold = 0.96;
    ctx.zone = MarketZone::DeepTrade;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert!(
        matches!(dec, Decision::NoOp),
        "cost balanced → NoOp, got {:?}",
        dec
    );
}

/// Profit-lock: avg_yes + avg_no ≤ avg_threshold → ProfitLocked state, NoOp.
/// Shares dengesi GEREKLİ DEĞİL (avg_sum eşiği yeterli).
#[test]
fn position_open_transitions_to_profit_locked_when_shares_balanced() {
    let mut metrics = StrategyMetrics::default();
    // shares_yes=10, shares_no=10 → diff=0 < api_min(5) → balanced → lock.
    // Share-balanced hedge price formülü ile avg_sum = 0.30 + 0.66 = 0.96 ≤ threshold otomatik.
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.30, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.66, 10.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.avg_threshold = 0.96;
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::ProfitLocked {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
}

/// Shares unbalanced (`|diff| ≥ api_min_order_size`) → lock yok, normal akış.
#[test]
fn position_open_shares_unbalanced_does_not_lock() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.30, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.70, 4.0, 0.0); // diff=6 ≥ api_min(5)
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.70,
        6.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.avg_threshold = 0.96;
    ctx.zone = MarketZone::DeepTrade;
    let (state, _dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        },
        "shares unbalanced → lock yok"
    );
}

/// Hedge hiç dolmadıysa (shares_no = 0) profit-lock TETİKLENMEZ — covered pair
/// kurulmamış demektir.
#[test]
fn position_open_shares_no_zero_does_not_lock() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.30, 10.0, 0.0); // shares_yes=10, shares_no=0
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.66,
        10.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.avg_threshold = 0.96;
    ctx.zone = MarketZone::DeepTrade;
    let (state, _dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        },
        "shares_no = 0 → lock yok"
    );
}

/// ProfitLocked state: HOLD — her ne olursa olsun NoOp.
#[test]
fn profit_locked_state_returns_noop() {
    let metrics = StrategyMetrics::default();
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.50,
        10.0,
        0,
    )];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(
        HarvestState::ProfitLocked {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::ProfitLocked {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
}

/// StopTrade override: ProfitLocked'da bile cancel-all + Done.
#[test]
fn stop_trade_overrides_profit_locked_with_cancel_all() {
    let metrics = StrategyMetrics::default();
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.50,
        10.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::StopTrade;
    let (state, dec) = decide(
        HarvestState::ProfitLocked {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(state, HarvestState::Done);
    match dec {
        Decision::CancelOrders(ids) => assert_eq!(ids, vec!["hedge".to_string()]),
        other => panic!("expected CancelOrders, got {:?}", other),
    }
}

#[test]
fn stop_trade_cancels_all_and_done() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.48,
        10.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::StopTrade;
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(state, HarvestState::Done);
    match dec {
        Decision::CancelOrders(ids) => assert_eq!(ids, vec!["hedge".to_string()]),
        other => panic!("expected CancelOrders, got {:?}", other),
    }
}

#[test]
fn pair_complete_cancels_remaining_and_done() {
    let metrics = StrategyMetrics::default();
    let opens = vec![mk_order(
        "leftover",
        Outcome::Up,
        "harvest_v2:avg_down:up",
        0.45,
        10.0,
        0,
    )];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(HarvestState::PairComplete, &ctx);
    assert_eq!(state, HarvestState::Done);
    match dec {
        Decision::CancelOrders(ids) => assert_eq!(ids, vec!["leftover".to_string()]),
        other => panic!("expected CancelOrders, got {:?}", other),
    }
}

#[test]
fn pair_complete_noop_when_book_empty() {
    let metrics = StrategyMetrics::default();
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(HarvestState::PairComplete, &ctx);
    assert_eq!(state, HarvestState::Done);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn cooldown_blocks_avg_down_within_window() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.48,
        10.42, // cost-balanced
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.now_ms = COOLDOWN + 5_000;
    ctx.last_averaging_ms = ctx.now_ms - 1_000; // cooldown ihlali
    ctx.yes_best_bid = 0.47;
    ctx.yes_best_ask = 0.48;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn stale_avg_order_is_cancelled_after_cooldown() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    let now = COOLDOWN * 4;
    let opens = vec![
        mk_order(
            "hedge",
            Outcome::Down,
            "harvest_v2:hedge:down",
            0.48,
            10.42,
            0,
        ),
        mk_order(
            "stale_avg",
            Outcome::Up,
            "harvest_v2:avg_down:up",
            0.45,
            10.0,
            now - COOLDOWN - 1_000,
        ),
    ];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.now_ms = now;
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelOrders(ids) => assert_eq!(ids, vec!["stale_avg".to_string()]),
        other => panic!("expected CancelOrders, got {:?}", other),
    }
}

#[test]
fn position_open_hedge_passive_fill_locks_profit() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.48, 10.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    // Shares parity → ProfitLock (HOLD).
    assert_eq!(
        state,
        HarvestState::ProfitLocked {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
}

/// Bot 2 regresyonu: hedge yok + shares(opposite)=0 → re-place
/// (PlaceOrders, cancel edilecek hedge yok). Share-balanced size formülü.
#[test]
fn position_open_missing_hedge_replaces_share_balanced() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.45, 11.0, 0.0);
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.30, 17.0, 0.0);
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.17, 30.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);

    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        other => panic!("expected PlaceOrders, got {:?}", other),
    };
    assert_eq!(orders.len(), 1);
    let h = &orders[0];
    assert_eq!(h.outcome, Outcome::Down);
    let cost_filled = 0.45 * 11.0 + 0.30 * 17.0 + 0.17 * 30.0;
    let avg_filled = cost_filled / metrics.shares_yes;
    let target = 0.98 - avg_filled;
    assert!(
        (h.price - (target * 100.0).round() / 100.0).abs() < 1e-9,
        "hedge price={} expected≈{} (snap)",
        h.price,
        target
    );
    // Share-balanced: target_size = shares_yes - shares_no = 58 - 0 = 58.
    assert!(
        (h.size - metrics.shares_yes).abs() < 1e-9,
        "hedge size={} expected={} (share-balanced)",
        h.size,
        metrics.shares_yes,
    );
}

/// Cost farkı çok küçükse (örn. 0.5 USD) hedge size MIN_NOTIONAL_USD/price ile clamp.
#[test]
fn build_hedge_clamps_to_min_notional_1usd() {
    let mut metrics = StrategyMetrics::default();
    // cost_filled = 0.50 * 1.0 = 0.50 (çok küçük opener fill)
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 1.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.api_min_order_size = 0.1; // çok küçük min'i kapatmak için düşür
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        other => panic!("expected PlaceOrders (missing hedge), got {:?}", other),
    };
    let h = &orders[0];
    let notional = h.price * h.size;
    assert!(
        notional >= MIN_NOTIONAL_USD - 1e-6,
        "notional={} < MIN_NOTIONAL_USD={}",
        notional,
        MIN_NOTIONAL_USD
    );
}

/// Bot 4 cooldown spam regresyonu (opener fill'leri de cooldown tetikler).
#[test]
fn is_averaging_like_includes_opener() {
    use crate::strategy::harvest::is_averaging_like;
    assert!(is_averaging_like("harvest_v2:open:up"));
    assert!(is_averaging_like("harvest_v2:open:down"));
    assert!(is_averaging_like("harvest_v2:avg_down:up"));
    assert!(is_averaging_like("harvest_v2:pyramid:down"));
    assert!(
        !is_averaging_like("harvest_v2:hedge:up"),
        "hedge fill'leri averaging penceresini açmaz"
    );
}

/// Bot 6 regresyonu: OpenPair'de hedge taker fill aldı, opener kitapta
/// `harvest_v2:open:*` reason'la live kalıyor. `hedge_order()` opener'ı
/// bulmalı; ikinci hedge basmamalı.
#[test]
fn position_open_treats_open_leg_as_hedge() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.43, 10.0, 0.0);
    let opens = vec![mk_order(
        "opener_up",
        Outcome::Up,
        "harvest_v2:open:up",
        0.53,
        10.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.now_ms = COOLDOWN + 1;

    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Down,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Down,
        },
    );
    // Hedge target = 0.98 − 0.43 = 0.55. Opener @ 0.53, drift = 0.02 > tick/2 → re-place.
    match dec {
        Decision::CancelAndPlace { cancels, places } => {
            assert_eq!(cancels, vec!["opener_up".to_string()]);
            assert_eq!(places.len(), 1);
            assert_eq!(places[0].outcome, Outcome::Up);
            assert!(
                (places[0].price - 0.55).abs() < 1e-9,
                "hedge target = 0.55, got {}",
                places[0].price
            );
        }
        other => panic!("expected CancelAndPlace, got {:?}", other),
    }
}
