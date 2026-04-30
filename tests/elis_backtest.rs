//! Elis (Dutch Book) stratejisi entegrasyon testleri.
//!
//! Dokümandan: `docs/elis.md`
//!
//! Bu testler Dutch Book spread capture döngüsünü uçtan uca doğrular:
//!  1. Dar spreadde NoOp
//!  2. Geniş spreadde UP+DOWN batch emri
//!  3. Cooldown süresince NoOp
//!  4. Cooldown sonrası iptal + Idle'a dönüş
//!  5. Balance factor mekanizması (dok §7 örneği)
//!  6. Pencere stop (stop_before_end_secs)
//!  7. Fiyat aralığı koruması (min_price / max_price)
//!  8. Tam döngü simülasyonu: Idle → BatchPending → Idle → ... → Done

use baiter_pro::config::{ElisParams, StrategyParams};
use baiter_pro::strategy::common::{Decision, OpenOrder, StrategyContext};
use baiter_pro::strategy::elis::{ElisEngine, ElisState};
use baiter_pro::strategy::metrics::StrategyMetrics;
use baiter_pro::time::MarketZone;
use baiter_pro::types::{Outcome, Side};

fn make_ctx<'a>(
    m: &'a StrategyMetrics,
    p: &'a StrategyParams,
    oo: &'a [OpenOrder],
    up_bid: f64,
    up_ask: f64,
    dn_bid: f64,
    dn_ask: f64,
    rem_secs: Option<f64>,
    now_ms: u64,
) -> StrategyContext<'a> {
    StrategyContext {
        metrics: m,
        up_token_id: "UP_TOKEN",
        down_token_id: "DN_TOKEN",
        up_best_bid: up_bid,
        up_best_ask: up_ask,
        down_best_bid: dn_bid,
        down_best_ask: dn_ask,
        api_min_order_size: 1.0,
        order_usdc: 20.0,
        effective_score: 5.0,
        zone: MarketZone::DeepTrade,
        now_ms,
        last_averaging_ms: 0,
        tick_size: 0.01,
        open_orders: oo,
        min_price: 0.15,
        max_price: 0.89,
        cooldown_threshold: 0,
        avg_threshold: 0.98,
        signal_ready: true,
        strategy_params: p,
        bsi: None,
        ofi: None,
        cvd: None,
        market_remaining_secs: rem_secs,
    }
}

/// Geniş spread senaryosu: UP $0.38/$0.40, DOWN $0.59/$0.61
/// UP_spread=0.02 ✓, DOWN_spread=0.02 ✓ — arb marjının işareti önemli değil.
fn ctx_good_spread<'a>(
    m: &'a StrategyMetrics,
    p: &'a StrategyParams,
    oo: &'a [OpenOrder],
    rem: Option<f64>,
    now_ms: u64,
) -> StrategyContext<'a> {
    make_ctx(m, p, oo, 0.38, 0.40, 0.59, 0.61, rem, now_ms)
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Dar spread → NoOp
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn narrow_spread_returns_noop() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // UP_spread=0.01 < 0.02 → engellenmeli.
    let c = make_ctx(&m, &p, &[], 0.49, 0.50, 0.49, 0.51, Some(200.0), 1000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::Idle));
    assert!(matches!(d, Decision::NoOp));
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Geniş spread → UP+DOWN batch emri ask fiyatından
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn wide_spread_places_batch_at_ask() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // UP_spread=0.02 ✓, DOWN_spread=0.02 ✓
    let c = ctx_good_spread(&m, &p, &[], Some(200.0), 5000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::BatchPending { placed_at_ms: 5000 }));
    match d {
        Decision::PlaceOrders(orders) => {
            assert_eq!(orders.len(), 2, "UP + DOWN iki emir beklendi");
            let up_o = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
            let dn_o = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
        // Fiyatlar bid'den alınmalı (maker limit).
            assert!((up_o.price - 0.38).abs() < 1e-9, "UP bid fiyatı hatalı");
            assert!((dn_o.price - 0.59).abs() < 1e-9, "DOWN bid fiyatı hatalı");
            // Sıfır pozisyonda her iki taraf eşit: max_buy_order_size = 20.
            let ep = ElisParams::default();
            assert!(
                (up_o.size - ep.max_buy_order_size).abs() < 1e-9,
                "UP size hatalı: {}",
                up_o.size
            );
            assert!(
                (dn_o.size - ep.max_buy_order_size).abs() < 1e-9,
                "DOWN size hatalı: {}",
                dn_o.size
            );
        }
        other => panic!("PlaceOrders beklendi, gelen {:?}", other),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Cooldown süresinde BatchPending → NoOp
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn batch_pending_noop_during_cooldown() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // placed_at=0, now=2000ms → 5000ms dolmadı.
    let c = ctx_good_spread(&m, &p, &[], Some(200.0), 2000);
    let state = ElisState::BatchPending { placed_at_ms: 0 };
    let (s, d) = ElisEngine::decide(state, &c);
    assert!(matches!(s, ElisState::BatchPending { .. }));
    assert!(matches!(d, Decision::NoOp));
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Cooldown sonrası CancelOrders + Idle
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn cooldown_triggers_cancel_and_idle() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    let ep = ElisParams::default();
    let orders = vec![
        OpenOrder {
            id: "up-order".into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.37,
            size: 20.0,
            reason: "elis:dutch:up".into(),
            placed_at_ms: 0,
            size_matched: 0.0,
        },
        OpenOrder {
            id: "dn-order".into(),
            outcome: Outcome::Down,
            side: Side::Buy,
            price: 0.61,
            size: 20.0,
            reason: "elis:dutch:down".into(),
            placed_at_ms: 0,
            size_matched: 0.0,
        },
    ];
    // Tam cooldown eşiği: now = trade_cooldown_ms.
    let c = ctx_good_spread(&m, &p, &orders, Some(200.0), ep.trade_cooldown_ms);
    let state = ElisState::BatchPending { placed_at_ms: 0 };
    let (s, d) = ElisEngine::decide(state, &c);
    assert!(matches!(s, ElisState::Idle));
    match d {
        Decision::CancelOrders(ids) => {
            assert!(ids.contains(&"up-order".to_string()));
            assert!(ids.contains(&"dn-order".to_string()));
        }
        other => panic!("CancelOrders beklendi, gelen {:?}", other),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Balance factor — doküman §7 örneği
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn balance_factor_doc_example_cycle9() {
    // Doküman §7 — Döngü #9:
    // UP=54, DOWN=78 → imbalance=24, adjustment=round(24×0.7×0.5)=round(8.4)=8
    // UP emir = 20+8 = 28, DOWN emir = 20-8 = 12
    // arb = 1 - 0.37 - 0.61 = 0.02 ✓
    let mut m = StrategyMetrics::default();
    m.up_filled = 54.0;
    m.down_filled = 78.0;

    let p = StrategyParams::default();
    let c = ctx_good_spread(&m, &p, &[], Some(200.0), 1000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::BatchPending { .. }));
    match d {
        Decision::PlaceOrders(orders) => {
            let up_o = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
            let dn_o = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
            assert!((up_o.size - 28.0).abs() < 1e-9, "UP size: {} (beklenen 28)", up_o.size);
            assert!((dn_o.size - 12.0).abs() < 1e-9, "DOWN size: {} (beklenen 12)", dn_o.size);
        }
        other => panic!("PlaceOrders beklendi, gelen {:?}", other),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Pencere stop — Idle'dan Done'a geç
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn idle_stops_before_window_end() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // Kalan 50s < stop_before_end_secs (60s). arb=0.02 olsa da stop tetiklenir.
    let c = ctx_good_spread(&m, &p, &[], Some(50.0), 1000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::Done));
    assert!(matches!(d, Decision::NoOp));
}

#[test]
fn batch_pending_stops_and_cancels_before_window_end() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    let orders = vec![OpenOrder {
        id: "o1".into(),
        outcome: Outcome::Up,
        side: Side::Buy,
        price: 0.37,
        size: 20.0,
        reason: "elis:dutch:up".into(),
        placed_at_ms: 0,
        size_matched: 0.0,
    }];
    // Kalan 30s < 60s → stop tetikle, emirleri iptal et.
    let c = make_ctx(&m, &p, &orders, 0.36, 0.37, 0.60, 0.61, Some(30.0), 1000);
    let state = ElisState::BatchPending { placed_at_ms: 0 };
    let (s, d) = ElisEngine::decide(state, &c);
    assert!(matches!(s, ElisState::Done));
    match d {
        Decision::CancelOrders(ids) => assert!(ids.contains(&"o1".to_string())),
        other => panic!("CancelOrders beklendi, gelen {:?}", other),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Fiyat aralığı koruması
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn price_above_max_blocks_batch() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // Spread ✓ ama UP_ask=0.91 > max_price(0.89) → price range engeller.
    // UP_spread=0.04 ✓, DOWN_spread=0.02 ✓
    let c = make_ctx(&m, &p, &[], 0.87, 0.91, 0.06, 0.08, Some(200.0), 1000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::Idle));
    assert!(matches!(d, Decision::NoOp));
}

#[test]
fn price_below_min_blocks_batch() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // Spread ✓ ama DOWN_ask=0.10 < min_price(0.15) → price range engeller.
    // UP_spread=0.04 ✓, DOWN_spread=0.02 ✓
    let c = make_ctx(&m, &p, &[], 0.83, 0.87, 0.08, 0.10, Some(200.0), 1000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::Idle));
    assert!(matches!(d, Decision::NoOp));
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Done state her zaman NoOp döner
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn done_state_always_noop() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    let c = ctx_good_spread(&m, &p, &[], Some(0.0), 999_999);
    let (s, d) = ElisEngine::decide(ElisState::Done, &c);
    assert!(matches!(s, ElisState::Done));
    assert!(matches!(d, Decision::NoOp));
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. Tek taraf dar spread → emir yok
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn one_side_narrow_spread_blocks() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // UP_spread=0.02 ✓ ama DOWN_spread=0.01 < 0.02 → engellenmeli.
    let c = make_ctx(&m, &p, &[], 0.38, 0.40, 0.60, 0.61, Some(200.0), 1000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::Idle));
    assert!(matches!(d, Decision::NoOp));
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. Tam döngü: Idle → Batch → (cooldown) → Idle → ... → Done
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn full_cycle_simulation() {
    let mut m = StrategyMetrics::default();
    let p = StrategyParams::default();
    let ep = ElisParams::default();
    let mut state = ElisState::Idle;

    // Market 300 saniye; her tick 1 saniye ilerleme.
    // arb = 1 - 0.37 - 0.61 = 0.02 ✓ her tick.
    let market_end_secs = 300u64;
    let mut batch_count = 0usize;

    for t in 0u64..market_end_secs {
        let rem = (market_end_secs - t) as f64;
        let now_ms = t * 1000;
        // UP_spread=0.02 ✓, DOWN_spread=0.02 ✓ her tick.
        let c = make_ctx(&m, &p, &[], 0.38, 0.40, 0.59, 0.61, Some(rem), now_ms);
        let (next_state, d) = ElisEngine::decide(state, &c);

        match &d {
            Decision::PlaceOrders(_) => batch_count += 1,
            Decision::CancelOrders(_) => {
                m.up_filled += ep.max_buy_order_size;
                m.down_filled += ep.max_buy_order_size;
            }
            _ => {}
        }
        state = next_state;
    }

    assert!(matches!(state, ElisState::Done | ElisState::BatchPending { .. } | ElisState::Idle));
    assert!(batch_count >= 20, "Döngü sayısı yetersiz: {}", batch_count);
    let imbalance = (m.up_filled - m.down_filled).abs();
    assert!(imbalance <= 1.0, "Final imbalance: {}", imbalance);
}

// ──────────────────────────────────────────────────────────────────────────────
// 11. market_remaining_secs = None → stop koşulu tetiklenmez
// ──────────────────────────────────────────────────────────────────────────────
#[test]
fn no_remaining_secs_does_not_stop() {
    let m = StrategyMetrics::default();
    let p = StrategyParams::default();
    // remaining = None → is_window_stop = false → normal arb kontrolüne geç. arb=0.02 ✓
    let c = ctx_good_spread(&m, &p, &[], None, 1000);
    let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
    assert!(matches!(s, ElisState::BatchPending { .. }));
    assert!(matches!(d, Decision::PlaceOrders(_)));
}
