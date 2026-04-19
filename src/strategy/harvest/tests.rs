//! Harvest FSM birim testleri.

use super::single::position_held_with_open;
use super::*;

use crate::config::StrategyParams;
use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::{Decision, OpenOrder};
use crate::time::MarketZone;
use crate::types::{OrderType, Outcome, Side};

const COOLDOWN_THRESHOLD: u64 = 30_000;

fn mk_open(id: &str, outcome: Outcome, reason: &str, placed_at_ms: u64, size: f64) -> OpenOrder {
    OpenOrder {
        id: id.to_string(),
        outcome,
        side: Side::Buy,
        price: 0.50,
        size,
        reason: reason.to_string(),
        placed_at_ms,
    }
}

fn default_ctx<'a>(
    metrics: &'a StrategyMetrics,
    params: &'a StrategyParams,
    open_orders: &'a [OpenOrder],
) -> HarvestContext<'a> {
    HarvestContext {
        params,
        metrics,
        yes_token_id: "yes",
        no_token_id: "no",
        yes_best_bid: 0.50,
        yes_best_ask: 0.52,
        no_best_bid: 0.46,
        no_best_ask: 0.48,
        api_min_order_size: 5.0,
        order_usdc: 5.0,
        signal_weight: 0.0,
        effective_score: 5.0,
        zone: MarketZone::NormalTrade,
        now_ms: 1_000_000,
        last_averaging_ms: 0,
        last_fill_price: 0.0,
        tick_size: 0.01,
        dual_timeout: 5_000,
        open_orders,
        avg_threshold: 0.98,
        max_position_size: 100.0,
        min_price: 0.05,
        max_price: 0.95,
        cooldown_threshold: 30_000,
    }
}

#[test]
fn dual_prices_neutral_returns_50_50() {
    let (up, down) = dual_prices(5.0, 0.01);
    assert!((up - 0.50).abs() < 1e-9);
    assert!((down - 0.50).abs() < 1e-9);
    assert!((up + down - 1.0).abs() < 1e-9);
}

#[test]
fn dual_prices_max_up_signal_returns_75_25() {
    let (up, down) = dual_prices(10.0, 0.01);
    assert!((up - 0.75).abs() < 1e-9);
    assert!((down - 0.25).abs() < 1e-9);
    assert!((up + down - 1.0).abs() < 1e-9);
}

#[test]
fn dual_prices_max_down_signal_returns_25_75() {
    let (up, down) = dual_prices(0.0, 0.01);
    assert!((up - 0.25).abs() < 1e-9);
    assert!((down - 0.75).abs() < 1e-9);
    assert!((up + down - 1.0).abs() < 1e-9);
}

#[test]
fn dual_prices_partial_signal_linear() {
    let (up, down) = dual_prices(8.0, 0.01);
    assert!((up - 0.65).abs() < 1e-9, "up={}", up);
    assert!((down - 0.35).abs() < 1e-9, "down={}", down);
    assert!((up + down - 1.0).abs() < 1e-9);
}

#[test]
fn dual_prices_partial_down_signal_linear() {
    let (up, down) = dual_prices(2.0, 0.01);
    assert!((up - 0.35).abs() < 1e-9, "up={}", up);
    assert!((down - 0.65).abs() < 1e-9, "down={}", down);
}

#[test]
fn open_dual_waits_when_book_missing() {
    let metrics = StrategyMetrics::default();
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.yes_best_bid = 0.0;
    let (state, decision) = decide(HarvestState::Pending, &ctx);
    assert_eq!(state, HarvestState::Pending);
    assert!(matches!(decision, Decision::NoOp));
}

#[test]
fn pending_transitions_to_open_dual_with_two_orders() {
    let metrics = StrategyMetrics::default();
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &params, &opens);
    let (state, decision) = decide(HarvestState::Pending, &ctx);
    match state {
        HarvestState::OpenDual { deadline_ms } => {
            assert_eq!(deadline_ms, ctx.now_ms + ctx.dual_timeout);
        }
        _ => panic!("expected OpenDual{{deadline_ms}}"),
    }
    match decision {
        Decision::PlaceOrders(orders) => assert_eq!(orders.len(), 2),
        _ => panic!("expected PlaceOrders"),
    }
}

#[test]
fn open_dual_high_signal_produces_075_025() {
    let metrics = StrategyMetrics::default();
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.effective_score = 10.0;
    let (_state, decision) = decide(HarvestState::Pending, &ctx);
    match decision {
        Decision::PlaceOrders(orders) => {
            let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
            let down = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
            assert!((up.price - 0.75).abs() < 1e-9);
            assert!((down.price - 0.25).abs() < 1e-9);
        }
        _ => panic!("expected PlaceOrders"),
    }
}

#[test]
fn open_dual_low_signal_produces_025_075() {
    let metrics = StrategyMetrics::default();
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.effective_score = 0.0;
    let (_state, decision) = decide(HarvestState::Pending, &ctx);
    match decision {
        Decision::PlaceOrders(orders) => {
            let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
            let down = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
            assert!((up.price - 0.25).abs() < 1e-9);
            assert!((down.price - 0.75).abs() < 1e-9);
        }
        _ => panic!("expected PlaceOrders"),
    }
}

#[test]
fn open_dual_both_filled_transitions_to_single_leg_by_signal() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens = vec![
        mk_open("o1", Outcome::Up, "harvest:open_dual:yes", 0, 10.0),
        mk_open("o2", Outcome::Down, "harvest:open_dual:no", 0, 10.0),
    ];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.effective_score = 7.0;
    let (state, dec) = decide(
        HarvestState::OpenDual {
            deadline_ms: ctx.now_ms + 1_000,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelOrders(c) => assert_eq!(c.len(), 2),
        _ => panic!("expected CancelOrders"),
    }
}

#[test]
fn open_dual_timeout_no_fill_returns_to_pending() {
    let metrics = StrategyMetrics::default();
    let params = StrategyParams::default();
    let opens = vec![
        mk_open("o1", Outcome::Up, "harvest:open_dual:yes", 0, 10.0),
        mk_open("o2", Outcome::Down, "harvest:open_dual:no", 0, 10.0),
    ];
    let ctx = default_ctx(&metrics, &params, &opens);
    let (state, dec) = decide(
        HarvestState::OpenDual {
            deadline_ms: ctx.now_ms.saturating_sub(1),
        },
        &ctx,
    );
    assert_eq!(state, HarvestState::Pending);
    match dec {
        Decision::CancelOrders(c) => assert_eq!(c.len(), 2),
        _ => panic!("expected CancelOrders"),
    }
}

#[test]
fn open_dual_timeout_one_fill_cancels_other_to_single_leg() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens = vec![mk_open(
        "no_open",
        Outcome::Down,
        "harvest:open_dual:no",
        0,
        10.0,
    )];
    let ctx = default_ctx(&metrics, &params, &opens);
    let (state, dec) = decide(
        HarvestState::OpenDual {
            deadline_ms: ctx.now_ms.saturating_sub(1),
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelOrders(c) => assert_eq!(c, vec!["no_open".to_string()]),
        _ => panic!("expected CancelOrders"),
    }
}

#[test]
fn single_leg_profit_lock_triggered_when_sum_under_threshold() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.48, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.no_best_ask = 0.49;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(state, HarvestState::ProfitLock);
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].order_type, OrderType::Fak);
            assert_eq!(orders[0].outcome, Outcome::Down);
        }
        _ => panic!("expected FAK order"),
    }
}

#[test]
fn stop_trade_zone_blocks_averaging() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.48, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.zone = MarketZone::StopTrade;
    ctx.no_best_ask = 0.80;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn averaging_when_price_falls_and_cooldown_passed() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.last_fill_price = 0.50;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].order_type, OrderType::Gtc);
            assert_eq!(orders[0].outcome, Outcome::Up);
        }
        _ => panic!("expected averaging GTC"),
    }
}

#[test]
fn single_leg_skips_averaging_while_open_avg_within_cooldown() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let now = COOLDOWN_THRESHOLD + 10_000;
    let opens = vec![mk_open(
        "avg1",
        Outcome::Up,
        "harvest:averaging:Up",
        now - 5_000,
        10.0,
    )];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = now;
    ctx.last_fill_price = 0.50;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    assert_eq!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn single_leg_cancels_open_avg_after_cooldown_threshold() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let now = COOLDOWN_THRESHOLD * 3;
    let opens = vec![mk_open(
        "avg1",
        Outcome::Up,
        "harvest:averaging:Up",
        now - COOLDOWN_THRESHOLD - 1_000,
        10.0,
    )];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = now;
    ctx.last_fill_price = 0.50;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    let (_state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    match dec {
        Decision::CancelOrders(ids) => assert_eq!(ids, vec!["avg1".to_string()]),
        _ => panic!("expected CancelOrders for stale averaging"),
    }
}

#[test]
fn single_leg_emits_new_averaging_after_cancel_in_next_tick() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD * 3;
    ctx.last_averaging_ms = ctx.now_ms - COOLDOWN_THRESHOLD - 1;
    ctx.last_fill_price = 0.50;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    let (_state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
        },
        &ctx,
    );
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].outcome, Outcome::Up);
            assert_eq!(orders[0].order_type, OrderType::Gtc);
        }
        _ => panic!("expected new averaging GTC"),
    }
}

#[test]
fn pos_held_includes_open_averaging_size() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens = vec![mk_open(
        "avg1",
        Outcome::Up,
        "harvest:averaging:Up",
        0,
        7.0,
    )];
    let ctx = default_ctx(&metrics, &params, &opens);
    let pos = position_held_with_open(&ctx, Outcome::Up);
    assert!((pos - 17.0).abs() < 1e-9);
}
