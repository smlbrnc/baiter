//! bot15 8-market Dutch Book (Elis) backtest testi.
//!
//! Gerçek tick verisi: `exports/bot15-ticks-20260429/` (8 market)
//! Strateji: Dutch Book spread capture (docs/elis.md)
//! Sinyal: KULLANILMAZ — sadece fiyat verileri (up/down ask/bid)
//!
//! Her market için:
//!  1. Tüm tick'ler ElisEngine'den geçirilir
//!  2. PlaceOrders → anında fill simülasyonu (ask fiyatından)
//!  3. CancelOrders → NoOp (fill edildi varsayılır)
//!  4. Arbitraj marjı: 1 − (UP_ask + DOWN_ask) per trade
//!  5. P&L: fill_count × arb_margin × pair_size
//!
//! Sonuçlar `cargo test -- --nocapture` ile görüntülenir.

use std::fs;
use std::path::PathBuf;

use baiter_pro::config::{ElisParams, StrategyParams};
use baiter_pro::strategy::common::{Decision, StrategyContext};
use baiter_pro::strategy::elis::{ElisEngine, ElisState};
use baiter_pro::strategy::metrics::StrategyMetrics;
use baiter_pro::time::MarketZone;
use baiter_pro::types::Outcome;

#[derive(Debug, serde::Deserialize)]
struct Tick {
    up_best_bid: f64,
    up_best_ask: f64,
    down_best_bid: f64,
    down_best_ask: f64,
    ts_ms: u64,
}

struct MarketResult {
    slug: String,
    total_ticks: usize,
    arb_ticks: usize,        // Arb marjı ≥ threshold olan tick sayısı
    batches_placed: usize,   // PlaceOrders kaç kez döndü
    up_filled: f64,          // Simüle edilen toplam UP share
    down_filled: f64,        // Simüle edilen toplam DOWN share
    total_cost: f64,         // Toplam ödenen USDC (up_ask*up_size + down_ask*dn_size)
    total_arb_margin: f64,   // Yakalanan toplam arbitraj marjı ($ cinsinden)
    winner: Option<Outcome>, // Final tick'te kazanan taraf
    theoretical_pnl: f64,   // Teorik P&L (mükemmel fill varsayımıyla)
}

fn exports_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exports")
}

fn load_ticks(folder: &str, slug: &str) -> Vec<Tick> {
    let path = exports_dir()
        .join(folder)
        .join(format!("{}_ticks.json", slug));
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Tick dosyası okunamadı {:?}: {}", path, e));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("JSON parse hatası {:?}: {}", path, e))
}

fn load_bot15_ticks(slug: &str) -> Vec<Tick> {
    load_ticks("bot15-ticks-20260429", slug)
}

/// Son tick'te up_best_bid ≥ 0.95 → UP kazandı, down_best_bid ≥ 0.95 → DOWN kazandı.
fn detect_winner(ticks: &[Tick]) -> Option<Outcome> {
    if let Some(last) = ticks.last() {
        if last.up_best_bid >= 0.95 {
            return Some(Outcome::Up);
        }
        if last.down_best_bid >= 0.95 {
            return Some(Outcome::Down);
        }
    }
    None
}

fn simulate_market(slug: &str) -> MarketResult {
    simulate_market_in(slug, "bot15-ticks-20260429")
}

fn simulate_market_in(slug: &str, folder: &str) -> MarketResult {
    let ticks = load_ticks(folder, slug);
    let ep = ElisParams::default();
    let params = StrategyParams::default();

    // Market penceresi: slug'daki timestamp'ten hesapla.
    let slug_ts: u64 = slug
        .rsplit('-')
        .next()
        .unwrap()
        .parse()
        .expect("slug son bölümü sayı olmalı");
    let market_end_ms = (slug_ts + 300) * 1000;

    let mut state = ElisState::Idle;
    let mut metrics = StrategyMetrics::default();

    let mut batches_placed = 0usize;
    let mut arb_ticks = 0usize;
    let mut total_cost = 0.0f64;
    let mut total_arb_margin = 0.0f64;

    // Son yerleştirilen batch'in fiyatlarını izle (P&L için).
    let mut last_up_bid = 0.0f64;
    let mut last_dn_bid = 0.0f64;
    let mut last_up_size = 0.0f64;
    let mut last_dn_size = 0.0f64;

    for tick in &ticks {
        let remaining_secs = (market_end_ms.saturating_sub(tick.ts_ms)) as f64 / 1000.0;
        let up_spread = tick.up_best_ask - tick.up_best_bid;
        let dn_spread = tick.down_best_ask - tick.down_best_bid;
        if up_spread >= ep.spread_threshold && dn_spread >= ep.spread_threshold {
            arb_ticks += 1;
        }

        // Sinyal kullanma: bsi/ofi/cvd = None.
        let ctx = StrategyContext {
            metrics: &metrics,
            up_token_id: "UP",
            down_token_id: "DOWN",
            up_best_bid: tick.up_best_bid,
            up_best_ask: tick.up_best_ask,
            down_best_bid: tick.down_best_bid,
            down_best_ask: tick.down_best_ask,
            api_min_order_size: 1.0,
            order_usdc: 20.0,
            effective_score: 0.0, // sinyal yok
            zone: MarketZone::DeepTrade,
            now_ms: tick.ts_ms,
            last_averaging_ms: 0,
            tick_size: 0.01,
            open_orders: &[],    // fill simüle edildiğinden açık emir yok
            min_price: 0.15,
            max_price: 0.89,
            cooldown_threshold: 0,
            avg_threshold: 0.98,
            signal_ready: false, // sinyal devre dışı
            strategy_params: &params,
            bsi: None,
            ofi: None,
            cvd: None,
            market_remaining_secs: Some(remaining_secs),
        };

        let (next_state, decision) = ElisEngine::decide(state, &ctx);

        match decision {
            Decision::PlaceOrders(orders) => {
                batches_placed += 1;
                for o in &orders {
                    let cost = o.price * o.size;
                    total_cost += cost;
                    if o.outcome == Outcome::Up {
                        last_up_bid = o.price;
                        last_up_size = o.size;
                    } else {
                        last_dn_bid = o.price;
                        last_dn_size = o.size;
                    }
                }
                // Maker limit: fill olursa UP_bid + DOWN_bid < $1.00 → kârlı.
                let pair = last_up_size.min(last_dn_size);
                let pair_cost = last_up_bid + last_dn_bid;
                total_arb_margin += pair * (1.0 - pair_cost);
            }
            Decision::CancelOrders(_) => {
                // DryRun: fill simüle et (önceki batch anında dolduruldu varsay).
                metrics.up_filled += last_up_size;
                metrics.down_filled += last_dn_size;
            }
            _ => {}
        }

        state = next_state;
    }

    let winner = detect_winner(&ticks);
    // Teorik P&L: kazanan taraf pay başına $1.00 öder.
    // Her batch'te hem UP hem DOWN aldık; pay başına kâr = arb_margin.
    // Pozisyon dengeli olduğundan P&L ≈ total_arb_margin − fee.
    let theoretical_pnl = total_arb_margin;

    MarketResult {
        slug: slug.to_string(),
        total_ticks: ticks.len(),
        arb_ticks,
        batches_placed,
        up_filled: metrics.up_filled,
        down_filled: metrics.down_filled,
        total_cost,
        total_arb_margin,
        winner,
        theoretical_pnl,
    }
}

const BOT15_SLUGS: &[&str] = &[
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
fn bot15_dutch_book_price_only_backtest() {
    let ep = ElisParams::default();
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║     Dutch Book Backtest — bot15 8 Market (Fiyat Bazlı, Sinyal Yok)      ║");
    println!("║     Parametre: spread_threshold={:.2}  max_size={}  cooldown={}ms         ║",
        ep.spread_threshold, ep.max_buy_order_size as u32, ep.trade_cooldown_ms);
    println!("╠══════════════════════════════════════════════════════════════════════════╣");
    println!("║ {:<32} {:>4} {:>5} {:>6} {:>7} {:>7} {:>6} ║",
        "Slug (son 10)", "Tck", "Spr✓", "Batch", "Cost$", "PnL$", "Kazanan");
    println!("╠══════════════════════════════════════════════════════════════════════════╣");

    let mut total_batches = 0usize;
    let mut total_arb = 0.0f64;
    let mut total_cost = 0.0f64;
    let mut total_arb_ticks = 0usize;

    let mut results = Vec::new();
    for slug in BOT15_SLUGS {
        let r = simulate_market(slug);
        results.push(r);
    }

    for r in &results {
        let slug_short = &r.slug[r.slug.len().saturating_sub(10)..];
        let winner_str = match r.winner {
            Some(Outcome::Up) => "UP   ",
            Some(Outcome::Down) => "DOWN ",
            None => "?    ",
        };
        println!("║ {:<32} {:>4} {:>5} {:>6} {:>7.2} {:>7.4} {:>6} ║",
            slug_short,
            r.total_ticks,
            r.arb_ticks,
            r.batches_placed,
            r.total_cost,
            r.total_arb_margin,
            winner_str,
        );
        total_batches += r.batches_placed;
        total_arb += r.total_arb_margin;
        total_cost += r.total_cost;
        total_arb_ticks += r.arb_ticks;
    }

    println!("╠══════════════════════════════════════════════════════════════════════════╣");
    println!("║ {:<32} {:>4} {:>5} {:>6} {:>7.2} {:>7.4} {:>6} ║",
        "TOPLAM",
        results.iter().map(|r| r.total_ticks).sum::<usize>(),
        total_arb_ticks,
        total_batches,
        total_cost,
        total_arb,
        "",
    );
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Açıklama:");
    println!("  Tck     = toplam tick sayısı");
    println!("  Arb✓    = her iki taraf bid-ask spread ≥ {:.2} olan tick sayısı", ep.spread_threshold);
    println!("  Batch   = PlaceOrders kararı sayısı (her batch = UP+DOWN çifti)");
    println!("  Cost$   = simüle edilen toplam alım maliyeti (ask×size)");
    println!("  PnL$    = teorik net kâr/zarar: $1.00 − (up_bid + dn_bid) × size (fill varsayımıyla)");
    println!("  Kazanan = final tick bid ≥ 0.95 ise kazanan taraf");
    println!();

    // Temel geçerliliği doğrula: simülasyon panik olmadan çalıştı.
    for r in &results {
        assert!(
            r.total_ticks > 0,
            "{}: tick yüklenmedi",
            r.slug
        );
        // Eğer batch var ise maliyet pozitif olmalı.
        if r.batches_placed > 0 {
            assert!(r.total_cost > 0.0, "{}: batch var ama maliyet sıfır", r.slug);
        }
    }
}

/// bot15 marketlerinde bireysel bid-ask spread fırsatlarını belgele.
#[test]
fn bot15_arb_opportunity_analysis() {
    let ep = ElisParams::default();
    let mut markets_with_spread = 0usize;
    let mut markets_with_inwindow_spread = 0usize;

    println!();
    println!("  ── Bot15 Spread Fırsat Analizi ─────────────────────────────────────────");
    println!("  {:<12}  {:>10}  {:>11}  {:>11}  {:>10}  {:>8}",
        "Market", "SpreadTick", "InRangeSpr", "InWindowSpr", "Kazanan", "SonKalan");

    for slug in BOT15_SLUGS {
        let ticks = load_bot15_ticks(slug);
        let slug_ts: u64 = slug.rsplit('-').next().unwrap().parse().unwrap();
        let market_end_ms = (slug_ts + 300) * 1000;

        // Tüm tick'lerde her iki spread ≥ threshold olanların sayısı
        let spread_tick_count = ticks
            .iter()
            .filter(|t| {
                let up_s = t.up_best_ask - t.up_best_bid;
                let dn_s = t.down_best_ask - t.down_best_bid;
                up_s >= ep.spread_threshold && dn_s >= ep.spread_threshold
            })
            .count();

        // Fiyat aralığında (0.15 ≤ ask ≤ 0.89) spread tick sayısı
        let in_range_spread_count = ticks
            .iter()
            .filter(|t| {
                let up_s = t.up_best_ask - t.up_best_bid;
                let dn_s = t.down_best_ask - t.down_best_bid;
                up_s >= ep.spread_threshold && dn_s >= ep.spread_threshold
                    && t.up_best_ask >= 0.15 && t.up_best_ask <= 0.89
                    && t.down_best_ask >= 0.15 && t.down_best_ask <= 0.89
            })
            .count();

        // Aktif pencerede (remaining > stop_before_end_secs) VE fiyat aralığında spread tick sayısı
        let in_window_spread_count = ticks
            .iter()
            .filter(|t| {
                let rem = (market_end_ms.saturating_sub(t.ts_ms)) as f64 / 1000.0;
                let up_s = t.up_best_ask - t.up_best_bid;
                let dn_s = t.down_best_ask - t.down_best_bid;
                rem > ep.stop_before_end_secs
                    && up_s >= ep.spread_threshold && dn_s >= ep.spread_threshold
                    && t.up_best_ask >= 0.15 && t.up_best_ask <= 0.89
                    && t.down_best_ask >= 0.15 && t.down_best_ask <= 0.89
            })
            .count();

        let winner = detect_winner(&ticks);
        let last_remaining = ticks.last()
            .map(|t| (market_end_ms.saturating_sub(t.ts_ms)) as f64 / 1000.0)
            .unwrap_or(0.0);

        if spread_tick_count > 0 {
            markets_with_spread += 1;
        }
        if in_window_spread_count > 0 {
            markets_with_inwindow_spread += 1;
        }

        let winner_str = match winner {
            Some(Outcome::Up) => "UP",
            Some(Outcome::Down) => "DOWN",
            None => "?",
        };

        println!(
            "  {:<12}  {:>10}  {:>11}  {:>11}  {:>10}  {:>8.1}s  {}",
            &slug[slug.len().saturating_sub(10)..],
            spread_tick_count,
            in_range_spread_count,
            in_window_spread_count,
            winner_str,
            last_remaining,
            if in_window_spread_count > 0 { "✓ TRADE FIRSATI" } else { "✗ yok" }
        );
    }

    println!();
    println!("  Spread tick'i olan market: {} / {}", markets_with_spread, BOT15_SLUGS.len());
    println!("  Aktif pencerede spread:    {} / {} market", markets_with_inwindow_spread, BOT15_SLUGS.len());
    println!("  (Koşul: UP_spread ≥ {:.2} AND DOWN_spread ≥ {:.2})", ep.spread_threshold, ep.spread_threshold);
    println!("  (Aktif pencere = remaining > {}s AND 0.15 ≤ ask ≤ 0.89)", ep.stop_before_end_secs);
    println!();
}

// ──────────────────────────────────────────────────────────────────────────────
// BOT14
// ──────────────────────────────────────────────────────────────────────────────

const BOT14_SLUGS: &[&str] = &[
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
];

fn run_backtest(label: &str, slugs: &[&str], folder: &str) {
    let ep = ElisParams::default();
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  Dutch Book Backtest — {:<50} ║", format!("{} ({} market)", label, slugs.len()));
    println!("║  Parametre: spread_threshold={:.2}  max_size={}  cooldown={}ms           ║",
        ep.spread_threshold, ep.max_buy_order_size as u32, ep.trade_cooldown_ms);
    println!("╠══════════════════════════════════════════════════════════════════════════╣");
    println!("║ {:<32} {:>4} {:>5} {:>6} {:>7} {:>7} {:>6} ║",
        "Slug (son 10)", "Tck", "Spr✓", "Batch", "Cost$", "PnL$", "Kazanan");
    println!("╠══════════════════════════════════════════════════════════════════════════╣");

    let mut total_batches = 0usize;
    let mut total_pnl = 0.0f64;
    let mut total_cost = 0.0f64;
    let mut total_spr_ticks = 0usize;

    let mut results = Vec::new();
    for slug in slugs {
        results.push(simulate_market_in(slug, folder));
    }

    for r in &results {
        let slug_short = &r.slug[r.slug.len().saturating_sub(10)..];
        let winner_str = match r.winner {
            Some(Outcome::Up) => "UP   ",
            Some(Outcome::Down) => "DOWN ",
            None => "?    ",
        };
        println!("║ {:<32} {:>4} {:>5} {:>6} {:>7.2} {:>7.4} {:>6} ║",
            slug_short, r.total_ticks, r.arb_ticks, r.batches_placed,
            r.total_cost, r.total_arb_margin, winner_str);
        total_batches += r.batches_placed;
        total_pnl += r.total_arb_margin;
        total_cost += r.total_cost;
        total_spr_ticks += r.arb_ticks;
    }

    println!("╠══════════════════════════════════════════════════════════════════════════╣");
    println!("║ {:<32} {:>4} {:>5} {:>6} {:>7.2} {:>7.4} {:>6} ║",
        "TOPLAM",
        results.iter().map(|r| r.total_ticks).sum::<usize>(),
        total_spr_ticks, total_batches, total_cost, total_pnl, "");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Tck=tick  Spr✓=her iki spread≥{:.2}  Batch=UP+DOWN çifti  Cost$=alım  PnL$=teorik kâr(bid×fill)",
        ep.spread_threshold);
    println!();

    for r in &results {
        if r.batches_placed > 0 {
            assert!(r.total_cost > 0.0, "{}: batch var ama maliyet sıfır", r.slug);
        }
    }
}

#[test]
fn bot14_dutch_book_price_only_backtest() {
    run_backtest("bot14", BOT14_SLUGS, "bot14-ticks-20260429");
}
