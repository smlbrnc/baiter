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
        effective_score: 5.0,
        zone: MarketZone::NormalTrade,
        now_ms: 1_000_000,
        last_averaging_ms: 0,
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

// Test helper — composite/10 → fiyat doğrudan mapping.
fn dp(score: f64) -> (f64, f64) {
    dual_prices(score, 0.01, 0.05, 0.95)
}

#[test]
fn dual_prices_neutral_is_50_50() {
    // composite=5 → up=0.50, down=0.50 (tam ortada)
    let (up, down) = dp(5.0);
    assert!((up - 0.50).abs() < 1e-9, "up={}", up);
    assert!((down - 0.50).abs() < 1e-9, "down={}", down);
}

#[test]
fn dual_prices_full_up_signal_clamped_to_max() {
    // composite=10 → up_raw=1.00 → clamp(0.95), down_raw=0.00 → clamp(0.05)
    let (up, down) = dp(10.0);
    assert!((up - 0.95).abs() < 1e-9, "up={}", up);
    assert!((down - 0.05).abs() < 1e-9, "down={}", down);
}

#[test]
fn dual_prices_full_down_signal_clamped_to_min() {
    // composite=0 → up_raw=0.00 → clamp(0.05), down_raw=1.00 → clamp(0.95)
    let (up, down) = dp(0.0);
    assert!((up - 0.05).abs() < 1e-9, "up={}", up);
    assert!((down - 0.95).abs() < 1e-9, "down={}", down);
}

#[test]
fn dual_prices_partial_bullish() {
    // composite=8 → up_raw=0.80, down_raw=0.20
    let (up, down) = dp(8.0);
    assert!((up - 0.80).abs() < 1e-9, "up={}", up);
    assert!((down - 0.20).abs() < 1e-9, "down={}", down);
}

#[test]
fn dual_prices_partial_bearish() {
    // composite=3.92 (bot 38 örneği) → up_raw=0.392 → snap 0.39, down_raw=0.608 → snap 0.61
    let (up, down) = dp(3.92);
    assert!((up - 0.39).abs() < 1e-9, "up={}", up);
    assert!((down - 0.61).abs() < 1e-9, "down={}", down);
}

#[test]
fn dual_prices_orderbook_independent() {
    // dual_prices artık orderbook almıyor — aynı composite hep aynı fiyatı verir
    let (up_a, down_a) = dual_prices(7.0, 0.01, 0.05, 0.95);
    let (up_b, down_b) = dual_prices(7.0, 0.01, 0.05, 0.95);
    assert_eq!(up_a, up_b);
    assert_eq!(down_a, down_b);
    assert!((up_a - 0.70).abs() < 1e-9, "up={}", up_a);
    assert!((down_a - 0.30).abs() < 1e-9, "down={}", down_a);
}

#[test]
fn dual_prices_sums_to_one_modulo_clamp() {
    // composite=6 → up=0.60, down=0.40 → toplam=1.00
    let (up, down) = dp(6.0);
    assert!((up + down - 1.0).abs() < 1e-9, "sum={}", up + down);
}

#[test]
fn dual_prices_snap_to_tick() {
    // composite=5.27 → up_raw=0.527 → snap (tick=0.01) 0.53, down_raw=0.473 → snap 0.47
    let (up, down) = dp(5.27);
    assert!((up - 0.53).abs() < 1e-9, "up={}", up);
    assert!((down - 0.47).abs() < 1e-9, "down={}", down);
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
fn open_dual_high_signal_up_aggressive_down_passive() {
    // composite=10 → up=composite/10=1.00 → clamp(0.95) (agresif taker)
    //                down=1−1.00=0.00     → clamp(0.05) (pasif derin)
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
            assert!((up.price - 0.95).abs() < 1e-9, "up={}", up.price);
            assert!((down.price - 0.05).abs() < 1e-9, "down={}", down.price);
        }
        _ => panic!("expected PlaceOrders"),
    }
}

#[test]
fn open_dual_low_signal_up_passive_down_aggressive() {
    // composite=0 → up=0.00 → clamp(0.05) (pasif derin), down=1.00 → clamp(0.95) (agresif taker)
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
            assert!((up.price - 0.05).abs() < 1e-9, "up={}", up.price);
            assert!((down.price - 0.95).abs() < 1e-9, "down={}", down.price);
        }
        _ => panic!("expected PlaceOrders"),
    }
}

#[test]
fn open_dual_both_filled_transitions_to_double_leg() {
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
    assert_eq!(state, HarvestState::DoubleLeg);
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
    assert!(matches!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            ..
        }
    ));
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
    // entered_at_ms=0 → warmup uzun süre önce geçti (now_ms=1_000_000).
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: 0,
        },
        &ctx,
    );
    assert_eq!(state, HarvestState::Done);
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
fn single_leg_profit_lock_warmup_blocks_first_tick() {
    // entered_at_ms = now_ms → warmup henüz geçmedi → ProfitLock pas geçilir.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.48, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.no_best_ask = 0.49;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: ctx.now_ms,
        },
        &ctx,
    );
    assert!(matches!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            ..
        }
    ));
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn single_leg_profit_lock_after_warmup_triggers_fak() {
    // entered_at_ms = now_ms - cooldown_threshold - 1 → warmup geçti → FAK + Done.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.48, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.no_best_ask = 0.49;
    let entered = ctx.now_ms - ctx.cooldown_threshold - 1;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: entered,
        },
        &ctx,
    );
    assert_eq!(state, HarvestState::Done);
    assert!(matches!(dec, Decision::PlaceOrders(ref o) if o.len() == 1 && o[0].order_type == OrderType::Fak));
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
            entered_at_ms: 0,
        },
        &ctx,
    );
    assert!(matches!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            ..
        }
    ));
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn averaging_when_price_falls_and_cooldown_passed() {
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0); // last_fill_price_yes=0.50
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: 0,
        },
        &ctx,
    );
    assert!(matches!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            ..
        }
    ));
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
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    let (state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: 0,
        },
        &ctx,
    );
    assert!(matches!(
        state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            ..
        }
    ));
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
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    let (_state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: 0,
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
    ctx.yes_best_bid = 0.48;
    ctx.no_best_ask = 0.55;
    let (_state, dec) = decide(
        HarvestState::SingleLeg {
            filled_side: Outcome::Up,
            entered_at_ms: 0,
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

// ───────── DoubleLeg birim testleri ─────────

#[test]
fn double_leg_profit_lock_when_avg_sum_under_threshold_and_balanced() {
    // avg_yes=0.49 + avg_no=0.48 = 0.97 ≤ 0.98, shares_yes=shares_no=10 →
    // |imbalance|=0 < api_min → Done + NoOp.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.49, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.48, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &params, &opens);
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::Done);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn double_leg_avg_sum_ok_but_imbalance_routes_close_gtc() {
    // avg_sum=0.90 ≤ 0.98, shares_yes=20, shares_no=10 → imbalance=+10 (NO eksik).
    // last_averaging_ms=0, now_ms=COOLDOWN+1 → cooldown_ok.
    // Eksik tarafa (Down) size=|imbalance|=10 GTC, price=no_best_bid.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.45, 20.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.45, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1, "tek emir eksik tarafa");
            assert_eq!(orders[0].outcome, Outcome::Down);
            assert!((orders[0].size - 10.0).abs() < 1e-9, "size={}", orders[0].size);
            assert!((orders[0].price - 0.46).abs() < 1e-9);
            assert_eq!(orders[0].reason, "harvest:averaging:Down");
        }
        _ => panic!("expected single PlaceOrders, got {:?}", dec),
    }
}

#[test]
fn double_leg_avg_sum_ok_imbalance_negative_routes_yes_side() {
    // imbalance=-10 → eksik=Up.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.45, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.45, 20.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].outcome, Outcome::Up);
            assert!((orders[0].size - 10.0).abs() < 1e-9);
            assert!((orders[0].price - 0.50).abs() < 1e-9);
        }
        _ => panic!("expected YES PlaceOrders, got {:?}", dec),
    }
}

#[test]
fn double_leg_avg_sum_ok_imbalance_close_bypasses_price_fell() {
    // last_fill_price_yes/no eşit best_bid'lere (price_fell=false). Normal
    // averaging tetiklenmezdi; ama avg_sum ≤ threshold + imbalance>0 yolu
    // BYPASS yaparak eksik tarafa GTC açar.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.50, 20.0, 0.0); // last_fill_yes=0.50
    metrics.ingest_fill(Outcome::Down, 0.45, 10.0, 0.0); // last_fill_no=0.45
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    ctx.no_best_bid = 0.45; // == last_fill_no → price_fell=false (normal yol atlardı)
    // avg_sum = 0.50 + 0.45 = 0.95 ≤ 0.98, imbalance = +10
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].outcome, Outcome::Down);
            assert!((orders[0].size - 10.0).abs() < 1e-9);
        }
        _ => panic!("price_fell BYPASS başarısız, got {:?}", dec),
    }
}

#[test]
fn double_leg_avg_sum_ok_imbalance_close_clipped_by_max_position() {
    // max=10, shares_yes=20 (avg=0.45), shares_no=5 (avg=0.45) → avg_sum=0.90,
    // imbalance=+15. Eksik=Down, pos_held=5, cap=10-5=5, size=min(15,5)=5
    // → kırpılmış GTC (not 15).
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.45, 20.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.45, 5.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    ctx.max_position_size = 10.0;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].outcome, Outcome::Down);
            assert!(
                (orders[0].size - 5.0).abs() < 1e-9,
                "size={} (cap kirpmadi)",
                orders[0].size
            );
        }
        _ => panic!("expected clipped PlaceOrders, got {:?}", dec),
    }
}

#[test]
fn double_leg_avg_sum_ok_imbalance_close_no_cap_room_returns_noop() {
    // max=10, shares_yes=20 (avg=0.45), shares_no=8 (avg=0.45) → imbalance=+12.
    // Eksik=Down, pos_held=8, cap=10-8=2 < api_min=5 → NoOp.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.45, 20.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.45, 8.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    ctx.max_position_size = 10.0;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn double_leg_avg_sum_ok_imbalance_within_tolerance_transitions_done() {
    // |imbalance|=3 < api_min=5 → balanced → Done.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.45, 13.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.45, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let ctx = default_ctx(&metrics, &params, &opens);
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::Done);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn double_leg_avg_sum_ok_imbalance_close_batches_with_stale_open_avg() {
    // avg_sum=0.90, imbalance=+10. Eksik=Down. Kitapta stale Down averaging GTC →
    // cancel + yeni size=10 GTC birlikte Batch.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.45, 20.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.45, 10.0, 0.0);
    let params = StrategyParams::default();
    let now = COOLDOWN_THRESHOLD * 3;
    let opens = vec![mk_open(
        "stale_down",
        Outcome::Down,
        "harvest:averaging:Down",
        now - COOLDOWN_THRESHOLD - 1_000,
        7.0,
    )];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = now;
    match decide(HarvestState::DoubleLeg, &ctx) {
        (HarvestState::DoubleLeg, Decision::Batch { cancel, place }) => {
            assert_eq!(cancel, vec!["stale_down".to_string()]);
            assert_eq!(place.len(), 1);
            assert_eq!(place[0].outcome, Outcome::Down);
            // pos_held(Down) = filled(10) + open(7) = 17, cap = 100-17 = 83,
            // size = min(10, 83) = 10
            assert!((place[0].size - 10.0).abs() < 1e-9);
        }
        other => panic!("expected DoubleLeg + Batch, got {:?}", other),
    }
}

#[test]
fn double_leg_avg_sum_ok_imbalance_close_waits_for_fresh_open_avg() {
    // Açık Down averaging fresh (cooldown içinde) → handle NoOp döner →
    // imbalance_close_decision yeni emir basmaz, NoOp.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.45, 20.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.45, 10.0, 0.0);
    let params = StrategyParams::default();
    let now = COOLDOWN_THRESHOLD + 5_000;
    let opens = vec![mk_open(
        "fresh_down",
        Outcome::Down,
        "harvest:averaging:Down",
        now - 1_000,
        7.0,
    )];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = now;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn double_leg_independent_averaging_yes_only() {
    // last_fill_price_yes=0.55, yes_best_bid düştü (0.48); NO tarafı sabit
    // (no_best_bid=0.50 == last_fill_price_no=0.50 → price_fell=false).
    // avg_sum=1.05 > 0.98 → ProfitLock tetiklenmez.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_bid = 0.50;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].outcome, Outcome::Up);
            assert!((orders[0].price - 0.48).abs() < 1e-9);
        }
        _ => panic!("expected PlaceOrders for YES only"),
    }
}

#[test]
fn double_leg_independent_averaging_both_sides_batched() {
    // İki tarafta da bid düştü + YES tarafında açık avg cancel-eligible →
    // Decision::Batch (cancel + place birlikte).
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let now = COOLDOWN_THRESHOLD * 3;
    let opens = vec![mk_open(
        "stale_up",
        Outcome::Up,
        "harvest:averaging:Up",
        now - COOLDOWN_THRESHOLD - 1_000,
        10.0,
    )];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = now;
    ctx.yes_best_bid = 0.50;
    ctx.no_best_bid = 0.48; // DOWN düşmüş → no avg place
    match decide(HarvestState::DoubleLeg, &ctx) {
        (HarvestState::DoubleLeg, Decision::Batch { cancel, place }) => {
            assert_eq!(cancel, vec!["stale_up".to_string()]);
            assert_eq!(place.len(), 1);
            assert_eq!(place[0].outcome, Outcome::Down);
            assert!((place[0].price - 0.48).abs() < 1e-9);
        }
        other => panic!("expected DoubleLeg + Batch, got {:?}", other),
    }
}

#[test]
fn double_leg_open_avg_within_cooldown_skips() {
    // Açık YES avg yaşı < cooldown → wait. Yeni emir basılmaz.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let now = COOLDOWN_THRESHOLD + 5_000;
    let opens = vec![mk_open(
        "fresh_up",
        Outcome::Up,
        "harvest:averaging:Up",
        now - 1_000,
        10.0,
    )];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = now;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_bid = 0.50;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    assert!(matches!(dec, Decision::NoOp));
}

#[test]
fn double_leg_one_side_at_max_position_freezes() {
    // YES tarafı max_position_size'a ulaştı → YES'e avg basılmaz; DOWN normal.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.55, 100.0, 0.0); // pos_held = 100 = max
    metrics.ingest_fill(Outcome::Down, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_bid = 0.48;
    let (state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    assert_eq!(state, HarvestState::DoubleLeg);
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 1, "yalnız DOWN avg basılmalı");
            assert_eq!(orders[0].outcome, Outcome::Down);
        }
        _ => panic!("expected DOWN-only PlaceOrders"),
    }
}

#[test]
fn double_leg_no_signal_multiplier() {
    // effective_score=10 → SingleLeg'de DOWN avg multiplier 1.3× olurdu (UP=1.0).
    // DoubleLeg'de iki taraf da 1.0 — sinyal etkisi double-count edilmez.
    let mut metrics = StrategyMetrics::default();
    metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
    metrics.ingest_fill(Outcome::Down, 0.50, 10.0, 0.0);
    let params = StrategyParams::default();
    let opens: Vec<OpenOrder> = vec![];
    let mut ctx = default_ctx(&metrics, &params, &opens);
    ctx.effective_score = 10.0;
    ctx.now_ms = COOLDOWN_THRESHOLD + 1;
    ctx.yes_best_bid = 0.48;
    ctx.no_best_bid = 0.48;
    let (_state, dec) = decide(HarvestState::DoubleLeg, &ctx);
    let base_up = crate::strategy::order_size(5.0, 0.48, 5.0).round();
    let base_down = crate::strategy::order_size(5.0, 0.48, 5.0).round();
    match dec {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 2);
            for o in &orders {
                let expected = match o.outcome {
                    Outcome::Up => base_up,
                    Outcome::Down => base_down,
                };
                assert!(
                    (o.size - expected).abs() < 1e-9,
                    "outcome={:?} size={} expected={} (DoubleLeg multiplier=1.0)",
                    o.outcome,
                    o.size,
                    expected,
                );
            }
        }
        _ => panic!("expected PlaceOrders"),
    }
}
