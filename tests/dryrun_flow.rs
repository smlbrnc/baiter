//! Harvest stratejisinin DryRun ucuna uçtan uca akış testi.
//!
//! Mock market + fake WS event stream → Harvest FSM → Simulator → PnL hesabı.

use baiter_pro::config::{BotConfig, StrategyParams};
use baiter_pro::engine::{execute, Executor, MarketSession, Simulator};
use baiter_pro::strategy::harvest::HarvestState;
use baiter_pro::strategy::OpenOrder;
use baiter_pro::time::now_ms;

const COOLDOWN_THRESHOLD: u64 = 30_000;
use baiter_pro::types::{Outcome, RunMode, Strategy};

fn dryrun_cfg() -> BotConfig {
    BotConfig {
        id: 42,
        name: "dryrun-test".into(),
        slug_pattern: "btc-updown-5m-1776420900".into(),
        strategy: Strategy::Harvest,
        run_mode: RunMode::Dryrun,
        order_usdc: 5.0,
        signal_weight: 0.0,
        min_price: 0.05,
        max_price: 0.95,
        cooldown_threshold: 30_000,
        start_offset: 0,
        strategy_params: StrategyParams {
            harvest_dual_timeout: Some(5_000),
            ..Default::default()
        },
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
    s.no_best_bid = 0.50;
    s.no_best_ask = 0.50;
    s
}

#[tokio::test]
async fn open_dual_fills_both_legs_transitions_to_single_leg() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms(), 5.0);
    let filled = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(filled.placed.len(), 2, "OpenDual iki emir gönderilmeli");
    assert!(matches!(sess.harvest_state, HarvestState::OpenDual { .. }));
    assert!(filled.placed.iter().all(|e| e.filled));
    assert!(sess.metrics.shares_yes > 0.0 && sess.metrics.shares_no > 0.0);

    let dec = sess.tick(&cfg, now_ms(), 5.0);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(
        sess.harvest_state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );

    let pnl = sess.pnl();
    let expected = 10.0 - (0.50 * 10.0 * 2.0 + 0.50 * 10.0 * 0.0002 * 2.0);
    assert!(
        (pnl.pnl_if_up - expected).abs() < 1e-6,
        "pnl_if_up={} expected={}",
        pnl.pnl_if_up,
        expected
    );
    assert!((pnl.pnl_if_down - expected).abs() < 1e-6);
}

#[tokio::test]
async fn single_leg_no_averaging_until_price_falls() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms(), 5.0);
    execute(&mut sess, &exec, dec).await.unwrap();
    let dec = sess.tick(&cfg, now_ms(), 5.0);
    execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(
        sess.harvest_state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );

    sess.no_best_ask = 0.55;
    sess.yes_best_bid = 0.50;
    let dec = sess.tick(&cfg, now_ms(), 5.0);
    let filled = execute(&mut sess, &exec, dec).await.unwrap();
    assert!(filled.placed.is_empty(), "averaging tetiklememeli");
    assert_eq!(
        sess.harvest_state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        }
    );
}

#[tokio::test]
async fn dryrun_and_live_share_same_decision_pipeline() {
    let mut cfg_live = dryrun_cfg();
    cfg_live.run_mode = RunMode::Live;
    let mut cfg_dry = dryrun_cfg();
    cfg_dry.run_mode = RunMode::Dryrun;

    let mut s_live = session(&cfg_live);
    let mut s_dry = session(&cfg_dry);

    let ts = now_ms();
    let d_live = s_live.tick(&cfg_live, ts, 5.0);
    let d_dry = s_dry.tick(&cfg_dry, ts, 5.0);

    match (d_live, d_dry) {
        (
            baiter_pro::strategy::Decision::PlaceOrders(a),
            baiter_pro::strategy::Decision::PlaceOrders(b),
        ) => {
            assert_eq!(a.len(), b.len());
            for (oa, ob) in a.iter().zip(b.iter()) {
                assert_eq!(oa.outcome, ob.outcome);
                assert!((oa.price - ob.price).abs() < 1e-9);
                assert!((oa.size - ob.size).abs() < 1e-9);
            }
        }
        _ => panic!("both modes should produce PlaceOrders at T=0"),
    }
}

#[tokio::test]
async fn stop_trade_zone_blocks_new_orders() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let now = now_ms() / 1000;
    sess.start_ts = now.saturating_sub(297);
    sess.end_ts = now + 3;
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms(), 5.0);
    let filled = execute(&mut sess, &exec, dec).await.unwrap();
    assert!(filled.placed.is_empty(), "StopTrade bölgesinde yeni emir olmamalı");
}

#[tokio::test]
async fn open_dual_high_signal_skews_by_market_spread() {
    // Market: yes_bid=0.48, yes_ask=0.52 (spread=0.04)
    //         no_bid=0.46,  no_ask=0.50  (spread=0.04)
    // delta=+1 → up_bid = yes_ask + spread = 0.56 (agresif taker)
    //            down_bid = no_ask - spread = 0.46 (no_bid seviyesinde, pasif maker)
    let mut cfg = dryrun_cfg();
    cfg.signal_weight = 10.0;
    let mut sess = session(&cfg);
    sess.yes_best_bid = 0.48;
    sess.yes_best_ask = 0.52;
    sess.no_best_bid = 0.46;
    sess.no_best_ask = 0.50;
    let dec = sess.tick(&cfg, now_ms(), 10.0);
    match dec {
        baiter_pro::strategy::Decision::PlaceOrders(orders) => {
            let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
            let down = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
            assert!((up.price - 0.56).abs() < 1e-9, "up_bid={}", up.price);
            assert!((down.price - 0.46).abs() < 1e-9, "down_bid={}", down.price);
        }
        _ => panic!("expected PlaceOrders"),
    }
}

#[tokio::test]
async fn open_dual_skipped_when_book_quotes_missing() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.yes_best_bid = 0.0;
    sess.no_best_bid = 0.0;
    let dec = sess.tick(&cfg, now_ms(), 5.0);
    assert!(matches!(dec, baiter_pro::strategy::Decision::NoOp));
    assert_eq!(sess.harvest_state, HarvestState::Pending);
}

#[tokio::test]
async fn passive_fills_match_when_book_crosses() {
    // Ask'ı max_price (0.95) üstüne çek → bid'ler max_price'a clamp olur, ikisi de
    // ask'ın altında kalır (maker). Ardından yes_ask düşürülünce UP fill olur.
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.yes_best_ask = 0.99;
    sess.no_best_ask = 0.99;
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms(), 5.0);
    let out = execute(&mut sess, &exec, dec).await.unwrap();
    assert!(out.placed.iter().all(|e| !e.filled));
    assert_eq!(sess.open_orders.len(), 2);

    sess.yes_best_ask = 0.50;
    let filled = baiter_pro::engine::simulate_passive_fills(&mut sess);
    assert_eq!(filled.len(), 1, "yalnız UP tarafı doldu");
    assert_eq!(filled[0].planned.outcome, Outcome::Up);
    assert!((filled[0].fill_price.unwrap() - 0.50).abs() < 1e-9);
    assert_eq!(sess.open_orders.len(), 1, "DOWN tarafı hâlâ live");
    assert!(sess.metrics.shares_yes > 0.0);
}

#[tokio::test]
async fn open_dual_timeout_no_fill_reopens() {
    // Ask'ı max_price üstüne çek → bid'ler clamp ile ask altında kalır → fill olmaz.
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.yes_best_ask = 0.99;
    sess.no_best_ask = 0.99;
    let exec = Executor::DryRun(Simulator);

    let t0 = now_ms();
    let dec = sess.tick(&cfg, t0, 5.0);
    let out = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(out.placed.len(), 2);
    assert!(out.placed.iter().all(|e| !e.filled));
    assert!(matches!(sess.harvest_state, HarvestState::OpenDual { .. }));
    assert_eq!(sess.open_orders.len(), 2);

    let t1 = t0 + 6_000;
    let dec = sess.tick(&cfg, t1, 5.0);
    let out = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(sess.harvest_state, HarvestState::Pending);
    assert_eq!(out.canceled.len(), 2);
    assert_eq!(sess.open_orders.len(), 0, "open_orders temizlenmiş olmalı");
}

#[tokio::test]
async fn single_leg_branch_when_only_yes_fills_in_book() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
    let t0 = now_ms();
    sess.harvest_state = HarvestState::OpenDual { deadline_ms: t0 };
    sess.open_orders.push(OpenOrder {
        id: "no_open".into(),
        outcome: Outcome::Down,
        side: baiter_pro::types::Side::Buy,
        price: 0.50,
        size: 10.0,
        reason: "harvest:open_dual:no".into(),
        placed_at_ms: t0,
    });

    let dec = sess.tick(&cfg, t0 + 1, 5.0);
    assert_eq!(
        sess.harvest_state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        },
        "yalnız YES dolmuş + timeout → SingleLeg{{Up}}"
    );
    matches!(dec, baiter_pro::strategy::Decision::CancelOrders(_));
}

#[tokio::test]
async fn averaging_fires_when_price_falls_after_cooldown() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
    sess.harvest_state = HarvestState::SingleLeg {
        filled_side: Outcome::Up,
    };
    sess.last_fill_price = 0.50;
    sess.last_averaging_ms = 0;
    sess.yes_best_bid = 0.45;
    sess.no_best_ask = 0.55;

    let dec = sess.tick(&cfg, COOLDOWN_THRESHOLD + 1, 5.0);
    match dec {
        baiter_pro::strategy::Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].outcome, Outcome::Up);
            assert!((orders[0].price - 0.45).abs() < 1e-9);
        }
        _ => panic!("expected averaging GTC"),
    }
}
