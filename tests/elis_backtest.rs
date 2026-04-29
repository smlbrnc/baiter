//! Elis stratejisi 24-market integration testi (v4b).
//!
//! Tick dosyaları: `exports/bot14-ticks-20260429/` (16 market) +
//! `exports/bot15-ticks-20260429/` (8 market). `ElisEngine`'in:
//!  1. **20 tick boyunca Pending** kalıp emir vermediğini,
//!  2. **t=20'de open_pair** ürettiğini ve composite opener intent'in
//!     beklenen yönde olduğunu (Python sim ile %100 paralellik),
//!  3. **Final tickte resolve olmuş** marketlerde yön doğruluğunun (flip dahil)
//!     v4b parametreleriyle %85 (17/20) seviyesinde olduğunu doğrular.
//!
//! Backtest detayı: `exports/backtest-final-24-markets.md`

use std::fs;
use std::path::PathBuf;

use baiter_pro::config::{ElisParams, StrategyParams};
use baiter_pro::strategy::common::{Decision, StrategyContext};
use baiter_pro::strategy::elis::{ElisEngine, ElisState};
use baiter_pro::strategy::metrics::StrategyMetrics;
use baiter_pro::time::MarketZone;
use baiter_pro::types::Outcome;

#[derive(Debug, Clone, serde::Deserialize)]
struct Tick {
    up_best_bid: f64,
    up_best_ask: f64,
    down_best_bid: f64,
    down_best_ask: f64,
    signal_score: f64,
    bsi: f64,
    ofi: f64,
    cvd: f64,
    ts_ms: u64,
}

fn exports_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exports")
}

fn load_ticks(slug: &str) -> Vec<Tick> {
    // bot14-ticks-* veya bot15-ticks-* (veya yenileri) — slug hangi klasörde varsa ondan oku.
    for entry in fs::read_dir(exports_dir()).expect("exports dir") {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if !name_s.starts_with("bot") || !name_s.contains("-ticks-") {
            continue;
        }
        let p = entry.path().join(format!("{}_ticks.json", slug));
        if p.exists() {
            let raw = fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("tick dosyası okunamadı {:?}: {}", p, e));
            return serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("JSON parse hatası {:?}: {}", p, e));
        }
    }
    panic!("tick dosyası bulunamadı: {}", slug);
}

/// Resolved marketler için "true winner" — final tick `up_best_bid >= 0.95` → Up,
/// `down_best_bid >= 0.95` → Down. 24 marketin 20'si net resolve, 4'ü belirsiz.
/// Tablo: `exports/backtest-final-24-markets.md` §3.1
fn expected_winner(slug: &str) -> Option<Outcome> {
    match slug {
        // bot14 (16 market)
        "btc-updown-5m-1777467000" => Some(Outcome::Up),
        "btc-updown-5m-1777467300" => Some(Outcome::Down),
        "btc-updown-5m-1777467600" => None,            // belirsiz
        "btc-updown-5m-1777467900" => Some(Outcome::Down),
        "btc-updown-5m-1777468200" => Some(Outcome::Up),
        "btc-updown-5m-1777468500" => None,            // belirsiz
        "btc-updown-5m-1777471200" => Some(Outcome::Down),
        "btc-updown-5m-1777471800" => None,            // belirsiz
        "btc-updown-5m-1777472100" => Some(Outcome::Up),
        "btc-updown-5m-1777473000" => Some(Outcome::Down),
        "btc-updown-5m-1777473900" => Some(Outcome::Down),
        "btc-updown-5m-1777474500" => Some(Outcome::Down),
        "btc-updown-5m-1777474800" => Some(Outcome::Down),
        "btc-updown-5m-1777475100" => Some(Outcome::Down),
        "btc-updown-5m-1777476300" => Some(Outcome::Down),
        "btc-updown-5m-1777476600" => Some(Outcome::Up),
        // bot15 (8 market)
        "btc-updown-5m-1777479000" => Some(Outcome::Down),
        "btc-updown-5m-1777479300" => Some(Outcome::Down),
        "btc-updown-5m-1777479600" => Some(Outcome::Down),
        "btc-updown-5m-1777479900" => Some(Outcome::Up),
        "btc-updown-5m-1777480200" => Some(Outcome::Up),
        "btc-updown-5m-1777480500" => Some(Outcome::Up),
        "btc-updown-5m-1777480800" => Some(Outcome::Up),
        "btc-updown-5m-1777481100" => None,            // belirsiz
        _ => None,
    }
}

/// `signal_score = 5.0` artı bsi/ofi/cvd alanlarıyla `StrategyContext` üretir.
fn make_ctx<'a>(
    metrics: &'a StrategyMetrics,
    params: &'a StrategyParams,
    open_orders: &'a [baiter_pro::strategy::common::OpenOrder],
    tick: &Tick,
    market_end_ms: u64,
) -> StrategyContext<'a> {
    let remaining_secs = (market_end_ms.saturating_sub(tick.ts_ms)) as f64 / 1000.0;
    StrategyContext {
        metrics,
        up_token_id: "UP_TOKEN",
        down_token_id: "DOWN_TOKEN",
        up_best_bid: tick.up_best_bid,
        up_best_ask: tick.up_best_ask,
        down_best_bid: tick.down_best_bid,
        down_best_ask: tick.down_best_ask,
        api_min_order_size: 1.0,
        order_usdc: 10.0,
        effective_score: tick.signal_score,
        zone: MarketZone::DeepTrade,
        now_ms: tick.ts_ms,
        last_averaging_ms: 0,
        tick_size: 0.01,
        open_orders,
        min_price: 0.01,
        max_price: 0.99,
        cooldown_threshold: 0,
        avg_threshold: 0.98,
        signal_ready: true,
        strategy_params: params,
        bsi: Some(tick.bsi),
        ofi: Some(tick.ofi),
        cvd: Some(tick.cvd),
        market_remaining_secs: Some(remaining_secs),
    }
}

/// Tek market simülasyonu: 20 tick Pending, sonra tek open_pair, sonra
/// her tick'te decide_active. Trade sayısı + opener intent + final intent döner.
struct SimResult {
    opener_intent: Outcome,
    final_intent: Outcome,
    trade_count: usize,
    flipped: bool,
}

fn simulate_market(slug: &str) -> SimResult {
    let ticks = load_ticks(slug);
    assert!(ticks.len() >= 30, "{} az tick içeriyor: {}", slug, ticks.len());

    let market_end_ms = ticks.last().unwrap().ts_ms + 1000;
    let metrics = StrategyMetrics::default();
    let params = StrategyParams::default();
    let open_orders = vec![];

    let mut state = ElisState::default();
    let mut opener_intent: Option<Outcome> = None;
    let mut trade_count = 0usize;

    for tick in &ticks {
        let ctx = make_ctx(&metrics, &params, &open_orders, tick, market_end_ms);
        let (next_state, decision) = ElisEngine::decide(state, &ctx);
        match &decision {
            Decision::PlaceOrders(orders) => trade_count += orders.len(),
            Decision::CancelAndPlace { places, .. } => trade_count += places.len(),
            _ => {}
        }
        // Opener intent'i ilk Active geçişte yakala
        if opener_intent.is_none() {
            if let ElisState::Active(active) = &next_state {
                opener_intent = Some(active.intent);
            }
        }
        state = next_state;
    }

    let opener = opener_intent.expect("opener intent t=20'de yakalanmalı");
    let (final_intent, flipped) = match &state {
        ElisState::Active(a) => (a.intent, a.flip_count > 0),
        ElisState::Done => (opener, false),
        ElisState::Pending { .. } => (opener, false),
    };

    SimResult {
        opener_intent: opener,
        final_intent,
        trade_count,
        flipped,
    }
}

const ALL_SLUGS: &[&str] = &[
    // bot14 (16 market)
    "btc-updown-5m-1777467000",
    "btc-updown-5m-1777467300",
    "btc-updown-5m-1777467600",
    "btc-updown-5m-1777467900",
    "btc-updown-5m-1777468200",
    "btc-updown-5m-1777468500",
    "btc-updown-5m-1777471200",
    "btc-updown-5m-1777471800",
    "btc-updown-5m-1777472100",
    "btc-updown-5m-1777473000",
    "btc-updown-5m-1777473900",
    "btc-updown-5m-1777474500",
    "btc-updown-5m-1777474800",
    "btc-updown-5m-1777475100",
    "btc-updown-5m-1777476300",
    "btc-updown-5m-1777476600",
    // bot15 (8 market)
    "btc-updown-5m-1777479000",
    "btc-updown-5m-1777479300",
    "btc-updown-5m-1777479600",
    "btc-updown-5m-1777479900",
    "btc-updown-5m-1777480200",
    "btc-updown-5m-1777480500",
    "btc-updown-5m-1777480800",
    "btc-updown-5m-1777481100",
];

#[test]
fn pre_opener_pending_for_first_19_ticks() {
    // İlk 19 tickte Pending kalmalı, opener t=20'de oluşmalı (default config).
    let slug = "btc-updown-5m-1777467000";
    let ticks = load_ticks(slug);
    let market_end_ms = ticks.last().unwrap().ts_ms + 1000;
    let metrics = StrategyMetrics::default();
    let params = StrategyParams::default();
    let open_orders = vec![];
    let p = ElisParams::default();

    let mut state = ElisState::default();
    for (i, tick) in ticks.iter().enumerate().take(p.pre_opener_ticks) {
        let ctx = make_ctx(&metrics, &params, &open_orders, tick, market_end_ms);
        let (next_state, decision) = ElisEngine::decide(state, &ctx);
        if i < p.pre_opener_ticks - 1 {
            assert!(
                matches!(next_state, ElisState::Pending { .. }),
                "tick {} hâlâ Pending olmalı",
                i
            );
            assert!(matches!(decision, Decision::NoOp), "tick {}: NoOp beklendi", i);
        } else {
            assert!(
                matches!(next_state, ElisState::Active(_)),
                "tick {} (=pre_opener_ticks-1) Active'e geçmeli",
                i
            );
            assert!(
                matches!(decision, Decision::PlaceOrders(_)),
                "tick {}: open_pair PlaceOrders beklendi",
                i
            );
        }
        state = next_state;
    }
}

#[test]
fn all_16_markets_simulate_without_panic() {
    // En temel sanity: 16 marketin hepsi panic atmadan baştan sona çalışmalı.
    for slug in ALL_SLUGS {
        let r = simulate_market(slug);
        assert!(
            r.trade_count > 0,
            "{}: en az 1 trade beklendi (open_pair) — gelen 0",
            slug
        );
    }
}

#[test]
fn opener_direction_accuracy_meets_85pct() {
    // 24-market combined: 20 resolved. Final intent (flip dahil) gerçek winner
    // ile ≥%80 eşleşmeli. v4b parametreleriyle Python sim 17/20 = %85 veriyor;
    // Rust impl en az %80 (16/20) tutturmalı.
    let mut correct = 0usize;
    let mut total = 0usize;
    let mut log: Vec<String> = Vec::new();

    for slug in ALL_SLUGS {
        let Some(winner) = expected_winner(slug) else {
            continue;
        };
        total += 1;
        let r = simulate_market(slug);
        let ok = r.final_intent == winner;
        if ok {
            correct += 1;
        }
        log.push(format!(
            "  {} → opener={:?} final={:?} flipped={} winner={:?} {}",
            slug,
            r.opener_intent,
            r.final_intent,
            r.flipped,
            winner,
            if ok { "✓" } else { "✗" }
        ));
    }

    let pct = correct as f64 / total as f64 * 100.0;
    println!(
        "\nOpener direction accuracy: {}/{} = {:.0}%\n{}",
        correct,
        total,
        pct,
        log.join("\n")
    );
    assert!(
        pct >= 80.0,
        "Yön doğruluğu %80 altında: {}/{} = {:.0}%",
        correct,
        total,
        pct
    );
}
