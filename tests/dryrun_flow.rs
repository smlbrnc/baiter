//! Harvest v2 DryRun entegrasyon testleri.
//!
//! Mock market + Simulator fill pipeline → Harvest FSM state transitions.

use baiter_pro::config::{BotConfig, StrategyParams};
use baiter_pro::engine::{execute, simulate_passive_fills, Executor, MarketSession, Simulator};
use baiter_pro::strategy::harvest::HarvestState;
use baiter_pro::strategy::{Decision, OpenOrder};
use baiter_pro::time::now_ms;
use baiter_pro::types::{Outcome, RunMode, Side, Strategy};

const COOLDOWN: u64 = 30_000;

fn dryrun_cfg() -> BotConfig {
    BotConfig {
        id: 42,
        name: "dryrun-test".into(),
        slug_pattern: "btc-updown-5m-1776420900".into(),
        strategy: Strategy::Harvest,
        run_mode: RunMode::Dryrun,
        order_usdc: 5.0,
        min_price: 0.05,
        max_price: 0.95,
        cooldown_threshold: COOLDOWN,
        start_offset: 0,
        strategy_params: StrategyParams::default(),
    }
}

fn session(cfg: &BotConfig) -> MarketSession {
    let mut s = MarketSession::new(42, "btc-updown-5m-1776420900".into(), cfg);
    s.yes_token_id = "yes".into();
    s.no_token_id = "no".into();
    s.tick_size = 0.01;
    s.api_min_order_size = 5.0;
    s.start_ts = now_ms() / 1000;
    s.end_ts = s.start_ts + 300;
    s.yes_best_bid = 0.50;
    s.yes_best_ask = 0.50;
    s.no_best_bid = 0.48;
    s.no_best_ask = 0.48;
    s
}

#[tokio::test]
async fn harvest_v2_open_pair_places_opener_and_hedge() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let out = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(out.placed.len(), 2, "OpenPair opener + hedge");
    assert_eq!(sess.harvest_state, HarvestState::OpenPair);
    assert!(
        out.placed
            .iter()
            .any(|e| e.planned.reason.starts_with("harvest_v2:open:"))
    );
    assert!(
        out.placed
            .iter()
            .any(|e| e.planned.reason.starts_with("harvest_v2:hedge:"))
    );
}

#[tokio::test]
async fn harvest_v2_open_pair_both_fill_transitions_to_pair_complete() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    assert!(sess.metrics.shares_yes > 0.0 && sess.metrics.shares_no > 0.0);

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(sess.harvest_state, HarvestState::PairComplete);

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(sess.harvest_state, HarvestState::Done);
}

#[tokio::test]
async fn harvest_v2_open_pair_single_leg_fill_becomes_position_open() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    // yes_ask=0.50 → Up taker fill. no_ask yüksek → hedge live (maker) kalır.
    sess.no_best_ask = 0.95;
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let out = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(out.placed.len(), 2);
    let filled: Vec<_> = out.placed.iter().filter(|e| e.filled).collect();
    assert_eq!(filled.len(), 1);
    assert_eq!(filled[0].planned.outcome, Outcome::Up);
    assert_eq!(sess.open_orders.len(), 1, "hedge live kalmalı");
    assert_eq!(sess.harvest_state, HarvestState::OpenPair);

    let dec = sess.tick(&cfg, now_ms() + 1, 5.0, true);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(
        sess.harvest_state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
}

#[tokio::test]
async fn harvest_v2_stop_trade_cancels_all_open_orders() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    // Elle PositionOpen senaryosu kur: shares_yes fill + hedge kitapta.
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    sess.harvest_state = HarvestState::PositionOpen {
        filled_side: Outcome::Up,
    };
    sess.open_orders.push(OpenOrder {
        id: "hedge-1".into(),
        outcome: Outcome::Down,
        side: Side::Buy,
        price: 0.48,
        size: 10.0,
        reason: "harvest_v2:hedge:down".into(),
        size_matched: 0.0,
        placed_at_ms: now_ms(),
    });
    // StopTrade zone'a sok.
    let now = now_ms() / 1000;
    sess.start_ts = now.saturating_sub(297);
    sess.end_ts = now + 3;

    let exec = Executor::DryRun(Simulator);
    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let out = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(sess.harvest_state, HarvestState::Done);
    assert_eq!(out.canceled.len(), 1);
    assert!(sess.open_orders.is_empty());
}

#[tokio::test]
async fn harvest_v2_normal_trade_avg_down_places_bid_when_ask_below_avg() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    // Manuel PositionOpen: shares_yes=10 @ 0.50, hedge kitapta @ 0.48.
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    sess.harvest_state = HarvestState::PositionOpen {
        filled_side: Outcome::Up,
    };
    sess.open_orders.push(OpenOrder {
        id: "hedge-1".into(),
        outcome: Outcome::Down,
        side: Side::Buy,
        price: 0.48,
        size: 10.0,
        reason: "harvest_v2:hedge:down".into(),
        size_matched: 0.0,
        placed_at_ms: 0,
    });
    sess.yes_best_bid = 0.46;
    sess.yes_best_ask = 0.47;
    sess.last_averaging_ms = 0;

    let t = now_ms() + COOLDOWN + 1_000;
    let dec = sess.tick(&cfg, t, 5.0, true);
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        other => panic!("expected PlaceOrders, got {:?}", other),
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].outcome, Outcome::Up);
    assert!(orders[0].reason.starts_with("harvest_v2:avg_down:"));
    assert!((orders[0].price - 0.46).abs() < 1e-9);
}

#[tokio::test]
async fn harvest_v2_agg_trade_pyramid_same_side_with_signal() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.55, 10.0, 0.0);
    sess.harvest_state = HarvestState::PositionOpen {
        filled_side: Outcome::Up,
    };
    sess.open_orders.push(OpenOrder {
        id: "hedge-1".into(),
        outcome: Outcome::Down,
        side: Side::Buy,
        price: 0.43,
        size: 10.0,
        reason: "harvest_v2:hedge:down".into(),
        size_matched: 0.0,
        placed_at_ms: 0,
    });
    sess.yes_best_bid = 0.60;
    sess.yes_best_ask = 0.62;
    // AggTrade zone: ~%50-90 pencere.
    let now_s = now_ms() / 1000;
    sess.start_ts = now_s.saturating_sub(150);
    sess.end_ts = now_s + 150;
    sess.last_averaging_ms = 0;

    let t = now_ms() + COOLDOWN + 1_000;
    let dec = sess.tick(&cfg, t, 8.0, true);
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        other => panic!("expected PlaceOrders, got {:?}", other),
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].outcome, Outcome::Up);
    assert!(orders[0].reason.starts_with("harvest_v2:pyramid:"));
}

#[tokio::test]
async fn harvest_v2_avg_down_match_triggers_hedge_reprice() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    // shares_yes=10 @ 0.50 + avg-down 10 @ 0.45 → avg_yes=0.475
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.45, 10.0, 0.0);
    sess.harvest_state = HarvestState::PositionOpen {
        filled_side: Outcome::Up,
    };
    // Hedge eski fiyatıyla kitapta (0.48): target = 0.98-0.475 = 0.505 → drift.
    sess.open_orders.push(OpenOrder {
        id: "hedge-old".into(),
        outcome: Outcome::Down,
        side: Side::Buy,
        price: 0.48,
        size: 10.0,
        reason: "harvest_v2:hedge:down".into(),
        size_matched: 0.0,
        placed_at_ms: 0,
    });

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    assert_eq!(
        sess.harvest_state,
        HarvestState::HedgeUpdating {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelOrders(ids) => assert_eq!(ids, vec!["hedge-old".to_string()]),
        other => panic!("expected CancelOrders, got {:?}", other),
    }
}

#[tokio::test]
async fn harvest_v2_hedge_passive_fill_completes_pair() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    // OpenPair'den tek leg filled, hedge live.
    sess.no_best_ask = 0.95;
    let exec = Executor::DryRun(Simulator);
    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(sess.open_orders.len(), 1, "hedge live");

    // monitor → PositionOpen{Up}
    let dec = sess.tick(&cfg, now_ms() + 1, 5.0, true);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(
        sess.harvest_state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );

    // Book hedge'i crossle: no_ask 0.48'e indi → passive fill.
    sess.no_best_ask = 0.48;
    let filled = simulate_passive_fills(&mut sess);
    assert_eq!(filled.len(), 1);
    assert_eq!(filled[0].planned.outcome, Outcome::Down);
    assert!(sess.open_orders.is_empty());

    // Sonraki tick → PairComplete.
    let dec = sess.tick(&cfg, now_ms() + 2, 5.0, true);
    assert_eq!(sess.harvest_state, HarvestState::PairComplete);
    assert!(matches!(dec, Decision::NoOp));
}

#[tokio::test]
async fn harvest_v2_pending_blocks_opener_when_signal_not_ready() {
    // doc §3, §5: RTDS aktif iken window_open_price daha yakalanmadıysa
    // (signal_ready = false), Pending NoOp döner ve harvest_state Pending'de
    // kalır. Bot bir sonraki tick'te (RTDS event'i geldikten sonra) tekrar dener.
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);

    let dec = sess.tick(&cfg, now_ms(), 5.0, false);
    assert_eq!(sess.harvest_state, HarvestState::Pending);
    assert!(matches!(dec, Decision::NoOp));
    assert!(sess.open_orders.is_empty());

    let dec = sess.tick(&cfg, now_ms() + 1, 5.0, true);
    assert_eq!(sess.harvest_state, HarvestState::OpenPair);
    assert!(matches!(dec, Decision::PlaceOrders(_)));
}

#[tokio::test]
async fn harvest_v2_composite_low_signal_opens_down_taker() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let dec = sess.tick(&cfg, now_ms(), 0.0, true);
    let orders = match dec {
        Decision::PlaceOrders(o) => o,
        other => panic!("expected PlaceOrders, got {:?}", other),
    };
    let open = orders
        .iter()
        .find(|o| o.reason.starts_with("harvest_v2:open:"))
        .unwrap();
    let hedge = orders
        .iter()
        .find(|o| o.reason.starts_with("harvest_v2:hedge:"))
        .unwrap();
    assert_eq!(open.outcome, Outcome::Down);
    assert_eq!(hedge.outcome, Outcome::Up);
}
