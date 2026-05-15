//! Gravie stratejisi için DB-replay backtest aracı.
//!
//! `market_ticks` tablosundaki kayıtlı 1-Hz BBA snapshot'larını seçilen
//! `bot_id`'nin oturumlarından okuyup Gravie engine'inden geçirir. Her tick'te
//! `Simulator` ile FAK fill simüle edilir. Session sonunda kazanan tarafı
//! son tick'in bid'inden ($\geq$ 0.95) belirler ve PnL hesaplar.
//!
//! Kullanım:
//!
//! ```bash
//! cargo run --release --bin backtest-gravie -- \
//!   --db data/baiter_remote.db --bot 119 --limit 200
//! ```
//!
//! Argümanlar:
//! - `--db <path>`        DB dosyası (default `data/baiter_remote.db`)
//! - `--bot <id>`         Tick verisi okunacak bot (default 119 = Bonereaper v5)
//! - `--limit <n>`        Max session sayısı (default 200)
//! - `--order-usdc <x>`   Base order USDC (default 10)
//! - `--api-min <x>`      API min order USDC (default 5)

use std::sync::Arc;

use baiter_pro::config::{BotConfig, StrategyParams};
use baiter_pro::engine::{executor::Simulator, update_top_of_book, MarketSession};
use baiter_pro::strategy::Decision;
use baiter_pro::types::{Outcome, RunMode, Strategy};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
struct CliArgs {
    db: String,
    bot_id: i64,
    limit: i64,
    order_usdc: f64,
    api_min_order_size: f64,
}

fn parse_args() -> CliArgs {
    let mut args = CliArgs {
        db: "data/baiter_remote.db".to_string(),
        bot_id: 119,
        limit: 200,
        order_usdc: 10.0,
        api_min_order_size: 5.0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--db" => args.db = it.next().expect("--db value"),
            "--bot" => args.bot_id = it.next().expect("--bot value").parse().unwrap(),
            "--limit" => args.limit = it.next().expect("--limit value").parse().unwrap(),
            "--order-usdc" => args.order_usdc = it.next().expect("--order-usdc value").parse().unwrap(),
            "--api-min" => args.api_min_order_size = it.next().expect("--api-min value").parse().unwrap(),
            "-h" | "--help" => {
                eprintln!("backtest-gravie [--db PATH] [--bot ID] [--limit N] [--order-usdc X] [--api-min X]");
                std::process::exit(0);
            }
            other => {
                eprintln!("bilinmeyen argüman: {other}");
                std::process::exit(2);
            }
        }
    }
    args
}

#[derive(Debug, Clone)]
struct SessionRow {
    id: i64,
    slug: String,
    start_ts: i64,
    end_ts: i64,
    asset_id_up: String,
    asset_id_down: String,
}

#[derive(Debug, Clone)]
struct TickRow {
    up_bid: f64,
    up_ask: f64,
    dn_bid: f64,
    dn_ask: f64,
    ts_ms: i64,
}

async fn load_sessions(pool: &SqlitePool, bot_id: i64, limit: i64) -> Vec<SessionRow> {
    sqlx::query(
        "SELECT id, slug, start_ts, end_ts, asset_id_up, asset_id_down \
         FROM market_sessions \
         WHERE bot_id = ? AND asset_id_up IS NOT NULL AND asset_id_down IS NOT NULL \
         ORDER BY start_ts ASC LIMIT ?",
    )
    .bind(bot_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .expect("market_sessions query")
    .into_iter()
    .map(|r| SessionRow {
        id: r.get("id"),
        slug: r.get("slug"),
        start_ts: r.get("start_ts"),
        end_ts: r.get("end_ts"),
        asset_id_up: r.get("asset_id_up"),
        asset_id_down: r.get("asset_id_down"),
    })
    .collect()
}

async fn load_ticks(pool: &SqlitePool, session_id: i64) -> Vec<TickRow> {
    sqlx::query(
        "SELECT up_best_bid, up_best_ask, down_best_bid, down_best_ask, ts_ms \
         FROM market_ticks WHERE market_session_id = ? ORDER BY ts_ms ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .expect("market_ticks query")
    .into_iter()
    .map(|r| TickRow {
        up_bid: r.get("up_best_bid"),
        up_ask: r.get("up_best_ask"),
        dn_bid: r.get("down_best_bid"),
        dn_ask: r.get("down_best_ask"),
        ts_ms: r.get("ts_ms"),
    })
    .collect()
}

/// Son tick'e bakıp kazanan tarafı bul. Hiçbiri 0.95 üstünde değilse `None`.
fn detect_winner(ticks: &[TickRow]) -> Option<Outcome> {
    let last = ticks.last()?;
    if last.up_bid >= 0.95 {
        Some(Outcome::Up)
    } else if last.dn_bid >= 0.95 {
        Some(Outcome::Down)
    } else if last.up_bid > last.dn_bid + 0.30 {
        Some(Outcome::Up)
    } else if last.dn_bid > last.up_bid + 0.30 {
        Some(Outcome::Down)
    } else {
        None
    }
}

fn build_gravie_cfg(order_usdc: f64) -> BotConfig {
    BotConfig {
        id: 9999,
        name: "backtest-gravie".to_string(),
        slug_pattern: "*".to_string(),
        strategy: Strategy::Gravie,
        run_mode: RunMode::Dryrun,
        order_usdc,
        min_price: 0.01,
        max_price: 0.99,
        cooldown_threshold: 0,
        start_offset: 0,
        strategy_params: StrategyParams::default(),
    }
}

#[derive(Debug, Default, Clone)]
struct SessionResult {
    slug: String,
    up_filled: f64,
    down_filled: f64,
    avg_up: f64,
    avg_down: f64,
    cost_basis: f64,
    fee_total: f64,
    n_trades: usize,
    winner: Option<Outcome>,
    pnl: f64,
    pair_count: f64,
    avg_sum: f64,
}

fn simulate_session(
    args: &CliArgs,
    cfg: &BotConfig,
    session: &SessionRow,
    ticks: &[TickRow],
) -> SessionResult {
    let bot_label: Arc<str> = Arc::from("backtest");
    let mut s = MarketSession::new(cfg.id, bot_label, session.slug.clone(), cfg);
    s.market_session_id = session.id;
    s.up_token_id = session.asset_id_up.clone();
    s.down_token_id = session.asset_id_down.clone();
    s.tick_size = 0.01;
    s.api_min_order_size = args.api_min_order_size;
    s.start_ts = session.start_ts as u64;
    s.end_ts = session.end_ts as u64;

    let sim = Simulator;
    let mut n_trades = 0usize;

    for t in ticks {
        update_top_of_book(&mut s, &session.asset_id_up, t.up_bid, t.up_ask);
        update_top_of_book(&mut s, &session.asset_id_down, t.dn_bid, t.dn_ask);

        let decision = s.tick(cfg, t.ts_ms as u64, 5.0, true, None, None, None);
        let orders = match decision {
            Decision::PlaceOrders(o) => o,
            _ => continue,
        };
        for o in orders {
            if o.price < cfg.min_price || o.price > cfg.max_price {
                continue;
            }
            let ex = sim.fill(&mut s, &o);
            if ex.filled {
                n_trades += 1;
            }
        }
    }

    let winner = detect_winner(ticks);
    let m = s.metrics;
    let pair = m.up_filled.min(m.down_filled);
    let imb_up = m.up_filled - pair;
    let imb_dn = m.down_filled - pair;

    let last = ticks.last();
    let (up_mtm_price, dn_mtm_price) = match (winner, last) {
        (Some(Outcome::Up), _) => (1.0, 0.0),
        (Some(Outcome::Down), _) => (0.0, 1.0),
        (None, Some(l)) => (l.up_bid, l.dn_bid),
        (None, None) => (0.0, 0.0),
    };
    let pnl = pair + imb_up * up_mtm_price + imb_dn * dn_mtm_price - m.cost_basis() - m.fee_total;

    SessionResult {
        slug: session.slug.clone(),
        up_filled: m.up_filled,
        down_filled: m.down_filled,
        avg_up: m.avg_up,
        avg_down: m.avg_down,
        cost_basis: m.cost_basis(),
        fee_total: m.fee_total,
        n_trades,
        winner,
        pnl,
        pair_count: pair,
        avg_sum: m.avg_sum(),
    }
}

fn print_session(r: &SessionResult) {
    let w = match r.winner {
        Some(Outcome::Up) => "UP",
        Some(Outcome::Down) => "DOWN",
        None => "?",
    };
    println!(
        "{:42}  trades={:>3}  up={:>6.1}@{:.3}  dn={:>6.1}@{:.3}  sum={:.3}  pair={:>5.1}  W={:<4}  pnl={:>+7.2}",
        r.slug,
        r.n_trades,
        r.up_filled,
        r.avg_up,
        r.down_filled,
        r.avg_down,
        r.avg_sum,
        r.pair_count,
        w,
        r.pnl,
    );
}

fn print_summary(results: &[SessionResult]) {
    let n = results.len() as f64;
    if n == 0.0 {
        println!("\nhiç session yok.");
        return;
    }
    let total_pnl: f64 = results.iter().map(|r| r.pnl).sum();
    let total_cost: f64 = results.iter().map(|r| r.cost_basis).sum();
    let total_trades: usize = results.iter().map(|r| r.n_trades).sum();
    let total_fee: f64 = results.iter().map(|r| r.fee_total).sum();
    let wins = results.iter().filter(|r| r.pnl > 0.0).count();
    let losses = results.iter().filter(|r| r.pnl < 0.0).count();
    let zeros = results.iter().filter(|r| r.pnl == 0.0).count();
    let dual = results.iter().filter(|r| r.pair_count > 0.0).count();
    let avg_sum_filled: Vec<f64> = results
        .iter()
        .filter(|r| r.up_filled > 0.0 && r.down_filled > 0.0)
        .map(|r| r.avg_sum)
        .collect();
    let mean_avg_sum = if !avg_sum_filled.is_empty() {
        avg_sum_filled.iter().sum::<f64>() / avg_sum_filled.len() as f64
    } else {
        0.0
    };
    let pct_avg_sum_lt1 = avg_sum_filled.iter().filter(|x| **x < 1.0).count();
    let total_pair: f64 = results.iter().map(|r| r.pair_count).sum();

    let traded = results.iter().filter(|r| r.n_trades > 0).count();
    let worst = results.iter().map(|r| r.pnl).fold(f64::INFINITY, f64::min);
    let best = results.iter().map(|r| r.pnl).fold(f64::NEG_INFINITY, f64::max);

    println!("\n============ ÖZET ============");
    println!("Session     : {:.0}", n);
    println!("Trade'li    : {} ({:.1}%)", traded, 100.0 * traded as f64 / n);
    println!("Dual (>0/>0): {} ({:.1}%)", dual, 100.0 * dual as f64 / n);
    println!("Toplam trade: {}", total_trades);
    println!("Toplam cost : ${:.2}", total_cost);
    println!("Toplam fee  : ${:.2}", total_fee);
    println!("Toplam PnL  : ${:+.2}", total_pnl);
    if total_cost > 0.0 {
        println!("ROI         : {:+.2}%", 100.0 * total_pnl / total_cost);
    }
    println!(
        "Win / Loss  : {} ({:.1}%) / {} ({:.1}%)  | flat: {}",
        wins,
        100.0 * wins as f64 / n,
        losses,
        100.0 * losses as f64 / n,
        zeros
    );
    println!("Best / Worst: ${:+.2} / ${:+.2}", best, worst);
    println!("Toplam pair : {:.1} share", total_pair);
    if !avg_sum_filled.is_empty() {
        println!(
            "avg_sum     : mean={:.3}  |  <1.0: {} / {} ({:.1}%)",
            mean_avg_sum,
            pct_avg_sum_lt1,
            avg_sum_filled.len(),
            100.0 * pct_avg_sum_lt1 as f64 / avg_sum_filled.len() as f64
        );
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = parse_args();
    let url = format!("sqlite://{}?mode=ro", args.db);
    let pool = SqlitePool::connect(&url).await.expect("db connect");
    let sessions = load_sessions(&pool, args.bot_id, args.limit).await;
    println!(
        "Backtest: db={} bot={} sessions={} order_usdc=${:.2}",
        args.db,
        args.bot_id,
        sessions.len(),
        args.order_usdc
    );
    let cfg = build_gravie_cfg(args.order_usdc);

    let mut results = Vec::with_capacity(sessions.len());
    for (i, sess) in sessions.iter().enumerate() {
        let ticks = load_ticks(&pool, sess.id).await;
        if ticks.is_empty() {
            continue;
        }
        let r = simulate_session(&args, &cfg, sess, &ticks);
        if i < 30 || r.n_trades > 0 {
            print_session(&r);
        }
        results.push(r);
    }

    print_summary(&results);
}
