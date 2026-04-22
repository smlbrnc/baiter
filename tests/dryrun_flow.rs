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
    // Opportunistic taker hedge'in burada tetiklenmemesi için DOWN ask'ı
    // pair_cost ≥ 1.0 olacak şekilde ayarla (0.50 + 0.55 = 1.05).
    sess.no_best_bid = 0.53;
    sess.no_best_ask = 0.55;
    sess.last_averaging_ms = 0;

    let t = now_ms() + COOLDOWN + 1_000;
    let dec = sess.tick(&cfg, t, 5.0, true);
    // Yeni davranış: avg-down + hedge re-place atomic CancelAndPlace.
    let (cancels, places) = match dec {
        Decision::CancelAndPlace { cancels, places } => (cancels, places),
        other => panic!("expected CancelAndPlace (atomic avg-down+hedge), got {:?}", other),
    };
    assert_eq!(cancels, vec!["hedge-1".to_string()]);
    let avg = places
        .iter()
        .find(|o| o.reason.starts_with("harvest_v2:avg_down:"))
        .expect("avg_down");
    let hedge = places
        .iter()
        .find(|o| o.reason.starts_with("harvest_v2:hedge:"))
        .expect("new hedge");
    assert_eq!(avg.outcome, Outcome::Up);
    assert!((avg.price - 0.46).abs() < 1e-9);
    assert_eq!(hedge.outcome, Outcome::Down);
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
    // Hedge cost-balanced: cost_filled = 0.55*10 = 5.5, size = 5.5/0.43 ≈ 12.79.
    sess.open_orders.push(OpenOrder {
        id: "hedge-1".into(),
        outcome: Outcome::Down,
        side: Side::Buy,
        price: 0.43,
        size: 12.79,
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
    // Yeni davranış: pyramid + hedge re-place atomic CancelAndPlace.
    let (cancels, places) = match dec {
        Decision::CancelAndPlace { cancels, places } => (cancels, places),
        other => panic!("expected CancelAndPlace (atomic pyramid+hedge), got {:?}", other),
    };
    assert_eq!(cancels, vec!["hedge-1".to_string()]);
    let pyr = places
        .iter()
        .find(|o| o.reason.starts_with("harvest_v2:pyramid:"))
        .expect("pyramid order");
    assert_eq!(pyr.outcome, Outcome::Up);
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
    // Opportunistic taker hedge'in burada tetiklenmemesi için DOWN ask'ı
    // pair_cost ≥ 1.0 yap (0.475 + 0.55 = 1.025).
    sess.no_best_bid = 0.53;
    sess.no_best_ask = 0.55;

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    // Atomic re-price: state PositionOpen'da kalır (HedgeUpdating ara state'i
    // atlanır), Decision::CancelAndPlace ile aynı tick'te eski hedge cancel
    // + yeni hedge `0.98 − 0.475 = 0.505` fiyatıyla place edilir.
    assert_eq!(
        sess.harvest_state,
        HarvestState::PositionOpen {
            filled_side: Outcome::Up
        }
    );
    match dec {
        Decision::CancelAndPlace { cancels, places } => {
            assert_eq!(cancels, vec!["hedge-old".to_string()]);
            assert_eq!(places.len(), 1);
            assert_eq!(places[0].outcome, Outcome::Down);
            // 0.98 - 0.475 = 0.505, tick=0.01 → snap'e göre 0.50 veya 0.51.
            assert!(
                (places[0].price - 0.505).abs() <= 0.01,
                "hedge price {} should be ~0.50/0.51",
                places[0].price
            );
        }
        other => panic!("expected CancelAndPlace, got {:?}", other),
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

/// Bot 6 (`btc-updown-5m-1776776400`) regresyonu — REST `POST /order`
/// `status=matched` artık `metrics`'i kendisi ingest ETMİYOR. Tek kaynak
/// User WS `trade MATCHED` event'idir (gerçek fill price `planned.price`'tan
/// farklı olabilir → eski kod planned ile ingest edip VWAP'ı bozuyordu).
/// `LiveExecutor::place`'in yaptığı atomic adımlar:
///   - `open_orders.push(size_matched = planned.size)` → marker;
///   - `last_averaging_ms = now_ms()` (averaging-like reason'larda).
/// WS event geldiğinde `extract_fills` → `metrics.ingest_fill` (gerçek
/// price) + `record_fill_and_prune_if_full` marker'ı düşürür.
#[tokio::test]
async fn live_matched_response_does_not_ingest_metrics_directly() {
    use baiter_pro::strategy::OpenOrder;
    use baiter_pro::time::now_ms;

    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.fee_rate_bps = 30;

    // 1) REST `POST /order` `status=matched` simülasyonu (LiveExecutor::place
    //    davranışı): marker push + cooldown. `metrics` değişmez.
    let order_id = "0xMATCHED_REST".to_string();
    let planned_price = 0.43;
    let planned_size = 10.0;
    sess.open_orders.push(OpenOrder {
        id: order_id.clone(),
        outcome: Outcome::Down,
        side: Side::Buy,
        price: planned_price,
        size: planned_size,
        reason: "harvest_v2:hedge:down".into(),
        placed_at_ms: now_ms(),
        size_matched: planned_size,
    });
    let shares_after_rest = sess.metrics.shares_no;
    assert!(
        (shares_after_rest - 0.0).abs() < 1e-9,
        "REST matched metrics'i değiştirmemeli, got shares_no={shares_after_rest}"
    );
    assert_eq!(sess.open_orders.len(), 1);

    // 2) WS `trade MATCHED` gerçek fill price ile gelir (örn. 0.39, planned
    //    0.43'ten daha iyi). Test pipeline simulation: doğrudan absorb +
    //    record helpers (event.rs::on_trade ile aynı sıra).
    let actual_price = 0.39;
    baiter_pro::engine::absorb_trade_matched(
        &mut sess,
        Outcome::Down,
        Side::Buy,
        actual_price,
        planned_size,
        0.0,
    );
    assert!((sess.metrics.shares_no - planned_size).abs() < 1e-9);
    assert!(
        (sess.metrics.avg_no - actual_price).abs() < 1e-9,
        "VWAP gerçek fill price'ı yansıtmalı (planned 0.43 değil), got {}",
        sess.metrics.avg_no
    );
}

/// Bot 4 regresyonu — opener fill `is_averaging_like` kapsamında olduğu için
/// `place_batch` `last_averaging_ms`'i ileri alır. Bu, session reset / FSM
/// Pending'e dönüş senaryosunda peş peşe avg-down/pyramid spam'ini engeller.
#[tokio::test]
async fn opener_fill_sets_averaging_cooldown_dryrun() {
    use baiter_pro::time::now_ms;

    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let exec = Executor::DryRun(Simulator);

    let before = sess.last_averaging_ms;
    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let _ = execute(&mut sess, &exec, dec).await.unwrap();

    assert!(
        sess.last_averaging_ms > before,
        "opener fill sonrası last_averaging_ms ileri alınmalı (last={}, before={})",
        sess.last_averaging_ms,
        before
    );
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

/// P5: pair_cost = avg_majority + best_ask(hedge) < 1.0 → FAK BUY hedge_side.
/// Senaryo: shares_yes=10 @ 0.55 (avg=0.55), no_best_ask=0.43 → pair=0.98 lock.
#[tokio::test]
async fn harvest_v2_opportunistic_taker_hedge_fires_on_lock_window() {
    use baiter_pro::types::OrderType;

    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.55, 10.0, 0.0);
    sess.harvest_state = HarvestState::PositionOpen {
        filled_side: Outcome::Up,
    };
    sess.no_best_ask = 0.43;
    sess.no_best_bid = 0.41;

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    let places = match dec {
        Decision::PlaceOrders(o) => o,
        other => panic!("expected PlaceOrders (taker hedge), got {:?}", other),
    };
    assert_eq!(places.len(), 1);
    let taker = &places[0];
    assert_eq!(taker.outcome, Outcome::Down);
    assert!(taker.reason.starts_with("harvest_v2:taker_hedge:"));
    assert_eq!(taker.order_type, OrderType::Fak);
    assert!((taker.price - 0.43).abs() < 1e-9);
    assert!((taker.size - 10.0).abs() < 1e-9);
}

/// P5: pair_cost ≥ 1.0 → trigger ETMEZ; pasif hedge yoksa GTC place edilir.
#[tokio::test]
async fn harvest_v2_opportunistic_taker_hedge_skipped_when_no_lock() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.55, 10.0, 0.0);
    sess.harvest_state = HarvestState::PositionOpen {
        filled_side: Outcome::Up,
    };
    // pair_cost = 0.55 + 0.46 = 1.01 → lock yok.
    sess.no_best_ask = 0.46;
    sess.no_best_bid = 0.44;

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    if let Decision::PlaceOrders(orders) | Decision::CancelAndPlace { places: orders, .. } = &dec {
        assert!(
            orders
                .iter()
                .all(|o| !o.reason.starts_with("harvest_v2:taker_hedge:")),
            "taker_hedge fire ETMEMELİ; got: {:?}",
            orders
        );
    }
}

/// P5: lock_min_profit_pct margin'i tetiklemeyi bloke eder.
#[tokio::test]
async fn harvest_v2_opportunistic_taker_hedge_respects_min_profit_margin() {
    let mut cfg = dryrun_cfg();
    cfg.strategy_params.lock_min_profit_pct = Some(0.05);
    let mut sess = session(&cfg);
    sess.metrics
        .ingest_fill(Outcome::Up, Side::Buy, 0.55, 10.0, 0.0);
    sess.harvest_state = HarvestState::PositionOpen {
        filled_side: Outcome::Up,
    };
    // pair_cost = 0.98 ama margin 0.05 → max_pair_cost = 0.95 → tetiklenmez.
    sess.no_best_ask = 0.43;
    sess.no_best_bid = 0.41;

    let dec = sess.tick(&cfg, now_ms(), 5.0, true);
    if let Decision::PlaceOrders(orders) | Decision::CancelAndPlace { places: orders, .. } = &dec {
        assert!(
            orders
                .iter()
                .all(|o| !o.reason.starts_with("harvest_v2:taker_hedge:")),
            "margin altında taker_hedge fire ETMEMELİ; got: {:?}",
            orders
        );
    }
}
