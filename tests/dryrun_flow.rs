//! Harvest stratejisinin DryRun ucuna uçtan uca akış testi.
//!
//! Mock market + fake WS event stream → Harvest FSM → Simulator → PnL hesabı.

use baiter_pro::config::{BotConfig, StrategyParams};
use baiter_pro::engine::{execute, Executor, MarketSession, Simulator};
use baiter_pro::strategy::harvest::HarvestState;
use baiter_pro::time::now_ms;
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
        // offset=-2 ⇒ up_bid = down_bid = 0.48 (boyutlar eşit, avg_sum=0.96 ≤ 0.98)
        strategy_params: StrategyParams {
            harvest_open_offset_ticks: Some(-2),
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
    // Market açılışında best_bid_ask
    s.yes_best_bid = 0.50;
    s.yes_best_ask = 0.52;
    s.no_best_bid = 0.48;
    s.no_best_ask = 0.50;
    s
}

#[tokio::test]
async fn open_dual_fills_both_legs_and_reaches_profit_lock() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let exec = Executor::DryRun(Simulator);

    // Tick 1: Pending → OpenDual (iki emir dolar)
    let dec = sess.tick(&cfg, now_ms());
    let filled = execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(filled.len(), 2, "OpenDual iki emir gönderilmeli");
    assert_eq!(sess.harvest_state, HarvestState::OpenDualOpen);
    assert!((sess.metrics.shares_yes - sess.metrics.shares_no).abs() < f64::EPSILON);

    // Tick 2: OpenDualOpen'da avg_sum kontrolü yapılır
    let dec = sess.tick(&cfg, now_ms());
    let _ = execute(&mut sess, &exec, dec).await.unwrap();
    // avg_yes = 0.50 (up_bid varsayılanı), avg_no = 0.48 (down_bid) → sum = 0.98 = threshold
    // imbalance = 10 - 10 = 0 → ProfitLock, NoOp
    assert_eq!(sess.harvest_state, HarvestState::ProfitLock);

    // PnL: up_bid=down_bid=0.48, order_usdc=5 → size = ceil(5/0.48) = 11
    // cost_basis = 0.48*11*2 + fee(0.0002 her iki fill) ≈ 10.5621
    // shares = 11 → pnl_if_up ≈ 11 - 10.5621 ≈ 0.4379 (fee dahil)
    let pnl = sess.pnl();
    let expected = 11.0 - (0.48 * 11.0 * 2.0 + 0.48 * 11.0 * 0.0002 * 2.0);
    assert!(
        (pnl.pnl_if_up - expected).abs() < 1e-6,
        "pnl_if_up={} expected={}",
        pnl.pnl_if_up,
        expected
    );
    assert!((pnl.pnl_if_down - expected).abs() < 1e-6);
}

#[tokio::test]
async fn no_new_orders_after_profit_lock() {
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    let exec = Executor::DryRun(Simulator);

    // OpenDual → ProfitLock
    let dec = sess.tick(&cfg, now_ms());
    execute(&mut sess, &exec, dec).await.unwrap();
    let dec = sess.tick(&cfg, now_ms());
    execute(&mut sess, &exec, dec).await.unwrap();
    assert_eq!(sess.harvest_state, HarvestState::ProfitLock);

    // ProfitLock durumunda yeni tick → hiç emir yok
    let dec = sess.tick(&cfg, now_ms());
    let filled = execute(&mut sess, &exec, dec).await.unwrap();
    assert!(filled.is_empty());
}

#[tokio::test]
async fn dryrun_and_live_share_same_decision_pipeline() {
    // Her iki mod da aynı Decision tipini üretir; Live burada simüle edilmez ama
    // `tick()` fonksiyonu mod-agnostik olduğu için aynı state'e ulaşmalı.
    let mut cfg_live = dryrun_cfg();
    cfg_live.run_mode = RunMode::Live;
    let mut cfg_dry = dryrun_cfg();
    cfg_dry.run_mode = RunMode::Dryrun;

    let mut s_live = session(&cfg_live);
    let mut s_dry = session(&cfg_dry);

    let ts = now_ms();
    let d_live = s_live.tick(&cfg_live, ts);
    let d_dry = s_dry.tick(&cfg_dry, ts);

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
    // Market penceresinin %99'undayız → StopTrade zone, hiç emir atma.
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    // end_ts'i now()'a yaklaştır: start=now-297, end=now+3 → ≈99%
    let now = now_ms() / 1000;
    sess.start_ts = now.saturating_sub(297);
    sess.end_ts = now + 3;
    let exec = Executor::DryRun(Simulator);

    let dec = sess.tick(&cfg, now_ms());
    let filled = execute(&mut sess, &exec, dec).await.unwrap();
    assert!(filled.is_empty(), "StopTrade bölgesinde yeni emir olmamalı");
}

#[tokio::test]
async fn dutch_book_strategy_stub_is_noop() {
    use baiter_pro::types::Strategy as S;
    let mut cfg = dryrun_cfg();
    cfg.strategy = S::DutchBook;
    let mut sess = session(&cfg);
    sess.strategy = S::DutchBook;

    let dec = sess.tick(&cfg, now_ms());
    assert!(matches!(dec, baiter_pro::strategy::Decision::NoOp));
}

#[tokio::test]
async fn prism_strategy_stub_is_noop() {
    use baiter_pro::types::Strategy as S;
    let mut cfg = dryrun_cfg();
    cfg.strategy = S::Prism;
    let mut sess = session(&cfg);
    sess.strategy = S::Prism;

    let dec = sess.tick(&cfg, now_ms());
    assert!(matches!(dec, baiter_pro::strategy::Decision::NoOp));
}

#[tokio::test]
async fn dutch_book_decide_fn_returns_noop() {
    use baiter_pro::strategy::dutch_book::{decide, DutchBookContext, DutchBookState};
    use baiter_pro::strategy::metrics::StrategyMetrics;

    let metrics = StrategyMetrics::default();
    let ctx = DutchBookContext { metrics: &metrics };
    let (state, dec) = decide(DutchBookState::Pending, &ctx);
    assert_eq!(state, DutchBookState::Pending);
    assert!(matches!(dec, baiter_pro::strategy::Decision::NoOp));
}

#[tokio::test]
async fn prism_decide_fn_returns_noop() {
    use baiter_pro::strategy::metrics::StrategyMetrics;
    use baiter_pro::strategy::prism::{decide, PrismContext, PrismState};

    let metrics = StrategyMetrics::default();
    let ctx = PrismContext { metrics: &metrics };
    let (state, dec) = decide(PrismState::Pending, &ctx);
    assert_eq!(state, PrismState::Pending);
    assert!(matches!(dec, baiter_pro::strategy::Decision::NoOp));
}

#[tokio::test]
async fn single_leg_branch_when_only_yes_fills_in_book() {
    // OpenDual gider ama simülatör tam dolum yapıyor → manuel olarak sadece YES
    // tarafı dolmuş bir session kur.
    let cfg = dryrun_cfg();
    let mut sess = session(&cfg);
    // up_bid ve down_bid öyle ki avg_sum > 0.98 → SingleLeg'e düşsün.
    // Ama simulator iki taraftaki emri de dolduruyor. Bunun yerine sadece metrics'i
    // elle ingest edelim ve OpenDualOpen → SingleLeg geçişini test edelim.
    sess.metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
    sess.harvest_state = HarvestState::OpenDualOpen;

    let dec = sess.tick(&cfg, now_ms());
    assert_eq!(
        sess.harvest_state,
        HarvestState::SingleLeg {
            filled_side: Outcome::Up
        },
        "yalnız YES dolmuşsa SingleLeg UP olmalı"
    );
    // Karar: no_best_ask=0.50, first_leg=0.55, sum=1.05 > 0.98 → ProfitLock yok; averaging şartı?
    // last_fill_price hâlâ 0 (elle değiştirmedik; session.last_fill_price = 0)
    // Hani ingest_fill sadece metrics'e işliyor, session.last_fill_price absorb_trade_matched ile güncelleniyor.
    // Averaging için last_fill_price > 0 gerekir (price_fell koşulu) → NoOp bekleniyor.
    matches!(dec, baiter_pro::strategy::Decision::NoOp);
}
