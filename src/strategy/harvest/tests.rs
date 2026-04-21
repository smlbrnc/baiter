//! Harvest v2 birim testleri — doc §4–§13 akış senaryoları.

use super::state::{
    AVG_DOWN_REASON_PREFIX, HEDGE_REASON_PREFIX, OPEN_REASON_PREFIX, PYRAMID_REASON_PREFIX,
};
use super::*;

use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::{Decision, OpenOrder};
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
    // doc §3, §5: RTDS aktif iken window_open_price yakalanana kadar opener
    // basılmaz. Pending NoOp döner; bir sonraki tick'te (RTDS event'i geldikten
    // sonra) tekrar denenir.
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

#[test]
fn pending_opens_pair_low_signal() {
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
    // doc §5: hedge_size == open_size (balanced pair).
    assert!(
        (hedge.size - open.size).abs() < 1e-9,
        "hedge_size={} open_size={}",
        hedge.size,
        open.size
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

#[test]
fn position_open_normal_trade_avg_down_triggers() {
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
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        _ => panic!("expected PlaceOrders, got {:?}", dec),
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].outcome, Outcome::Up);
    assert!(orders[0].reason.starts_with(AVG_DOWN_REASON_PREFIX));
    assert!((orders[0].price - 0.47).abs() < 1e-9);
}

#[test]
fn position_open_normal_trade_avg_down_skipped_when_ask_above_avg() {
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
    ctx.yes_best_ask = 0.55;
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn position_open_agg_trade_pyramid_same_side() {
    // rising=Up (yes_bid=0.60), filled=Up, last_fill=0.55, ask=0.62 > last_fill → pyramid.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.55, 10.0, 0.0);
    let opens = vec![mk_order(
        "hedge",
        Outcome::Down,
        "harvest_v2:hedge:down",
        0.43,
        10.0,
        0,
    )];
    let mut ctx = default_ctx(&metrics, &opens);
    ctx.zone = MarketZone::AggTrade;
    ctx.now_ms = COOLDOWN + 1;
    ctx.yes_best_bid = 0.60;
    ctx.yes_best_ask = 0.62;
    ctx.effective_score = 8.0;
    // hedge @ 0.43 doğru çünkü avg_threshold − avg_filled = 0.98 − 0.55 = 0.43
    let (_state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        _ => panic!("expected PlaceOrders, got {:?}", dec),
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].outcome, Outcome::Up);
    assert!(orders[0].reason.starts_with(PYRAMID_REASON_PREFIX));
}

#[test]
fn position_open_agg_trade_pyramid_opposite_skips_trend_gate() {
    // filled=Up, yes_bid=0.40 → rising=Down. Trend gate atlanır.
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
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        _ => panic!("expected PlaceOrders, got {:?}", dec),
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].outcome, Outcome::Down);
    assert!(orders[0].reason.starts_with(PYRAMID_REASON_PREFIX));
}

#[test]
fn position_open_hedge_drift_triggers_cancel() {
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
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(
        HarvestState::PositionOpen {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::HedgeUpdating {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelOrders(ids) => assert_eq!(ids, vec!["hedge1".to_string()]),
        other => panic!("expected CancelOrders, got {:?}", other),
    }
}

#[test]
fn hedge_updating_cancel_ok_reprices() {
    // Hedge gitti (open_orders=[]), shares_yes=20, shares_no=0 → imbalance=20 > api_min.
    // target = 0.98 − avg_yes.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 20.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(
        HarvestState::HedgeUpdating {
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
    assert_eq!(orders[0].outcome, Outcome::Down);
    assert!(orders[0].reason.starts_with(HEDGE_REASON_PREFIX));
    assert!((orders[0].price - 0.48).abs() < 1e-9);
    assert!((orders[0].size - 20.0).abs() < 1e-9);
}

#[test]
fn hedge_updating_cancel_race_to_pair_complete() {
    // Hedge fill oldu (shares_no > 0), imbalance ≈ 0 < api_min → PairComplete.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, Side::Buy, 0.48, 10.0, 0.0);
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(
        HarvestState::HedgeUpdating {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(state, HarvestState::PairComplete);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn hedge_updating_cancel_pending_returns_noop() {
    // Hedge hâlâ kitapta → cancel response bekleniyor → NoOp.
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
    let ctx = default_ctx(&metrics, &opens);
    let (state, dec) = decide(
        HarvestState::HedgeUpdating {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::HedgeUpdating {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
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
        10.0,
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
            10.0,
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
fn position_open_hedge_passive_fill_completes_pair() {
    // Hedge kayboldu + shares(opposite) > 0 → passive fill oldu → PairComplete.
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
    assert_eq!(state, HarvestState::PairComplete);
    assert!(matches!(dec, Decision::NoOp));
}

/// Bot 2 (`btc-updown-5m-1776766500`) regresyonu: hedge cancel race / API
/// hata sonrası `open_orders`'tan düştü ve `shares(opposite)==0` kaldı. Eski
/// kod `position_open` içinde sessiz NoOp dönüyordu → bot avg-down yığarken
/// profit-lock'u kaçırıyordu. Yeni davranış: `hedge_update::handle`'a delege
/// → DOWN @ (0.98 − avg_yes) re-place edilir.
#[test]
fn position_open_missing_hedge_replaces_via_hedge_update() {
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
        other => panic!("expected hedge re-place, got {:?}", other),
    };
    assert_eq!(orders.len(), 1, "tek hedge order beklenir");
    let h = &orders[0];
    assert_eq!(h.outcome, Outcome::Down, "hedge karşı tarafa konur");
    let expected_target = 0.98
        - ((0.45 * 11.0 + 0.30 * 17.0 + 0.17 * 30.0) / (11.0 + 17.0 + 30.0));
    assert!(
        (h.price - expected_target).abs() < 0.02,
        "hedge price={} expected≈{}",
        h.price,
        expected_target
    );
    assert!(
        (h.size - metrics.shares_yes).abs() < 1e-9,
        "hedge size = imbalance ({}) , got {}",
        metrics.shares_yes,
        h.size
    );
}
