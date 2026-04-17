//! Bot binary — supervisor tarafından `--bot-id <id>` ile spawn edilir.
//!
//! Yapı:
//! - Bir kez: DB/creds/clob/executor/sinyal/heartbeat task'ları kurulur.
//! - Her market penceresi: `run_window` çağrılır; pencere bitince bir sonraki
//!   pencereye geçilir. SIGTERM/SIGINT sırasında `graceful_shutdown` çalışır.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use baiter_pro::binance::{self, new_shared_state, SharedSignalState};
use baiter_pro::config::{BotConfig, Credentials, RuntimeEnv};
use baiter_pro::db;
use baiter_pro::engine::{
    absorb_trade_matched, execute, outcome_from_asset_id, update_best, Executor, MarketSession,
    Simulator,
};
use baiter_pro::error::AppError;
use baiter_pro::ipc::{self, FrontendEvent};
use baiter_pro::polymarket::clob::{shared_http_client, ClobClient};
use baiter_pro::polymarket::gamma::{GammaClient, GammaMarket};
use baiter_pro::polymarket::ws::{run_market_ws, run_user_ws, PolymarketEvent};
use baiter_pro::slug::{parse_slug, Interval, SlugInfo};
use baiter_pro::strategy::Decision;
use baiter_pro::time::{now_ms, now_secs};
use baiter_pro::types::RunMode;
use sqlx::SqlitePool;
use tokio::fs;
use tokio::signal::unix::{signal, Signal, SignalKind};
use tokio::sync::mpsc;
use tokio::time::interval as tokio_interval;

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    if let Err(e) = run().await {
        let bot_id = env::var("BAITER_BOT_ID")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(-1);
        ipc::emit(&FrontendEvent::Error {
            bot_id,
            message: format!("{e}"),
            ts_ms: now_ms(),
        });
        tracing::error!(error=%e, "bot exited with error");
        std::process::exit(1);
    }
}

fn parse_bot_id() -> Result<i64, AppError> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bot-id" => {
                let v = args
                    .next()
                    .ok_or_else(|| AppError::Config("--bot-id value missing".into()))?;
                return v
                    .parse()
                    .map_err(|_| AppError::Config(format!("invalid --bot-id '{v}'")));
            }
            a if a.starts_with("--bot-id=") => {
                let v = &a["--bot-id=".len()..];
                return v
                    .parse()
                    .map_err(|_| AppError::Config(format!("invalid --bot-id '{v}'")));
            }
            _ => {}
        }
    }
    env::var("BAITER_BOT_ID")
        .map_err(|_| AppError::Config("--bot-id or BAITER_BOT_ID required".into()))?
        .parse()
        .map_err(|_| AppError::Config("BAITER_BOT_ID must be integer".into()))
}

/// Bot ömrü boyunca sabit kalan bağlam — pencereler arası paylaşılır.
struct Ctx {
    bot_id: i64,
    cfg: BotConfig,
    env_: RuntimeEnv,
    pool: SqlitePool,
    gamma: GammaClient,
    creds: Option<Credentials>,
    executor: Executor,
    signal_state: SharedSignalState,
}

async fn run() -> Result<(), AppError> {
    let bot_id = parse_bot_id()?;
    env::set_var("BAITER_BOT_ID", bot_id.to_string());

    let env_ = RuntimeEnv::from_env()?;
    let pool = db::open(&env_.db_path).await?;

    let cfg = db::get_bot(&pool, bot_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("bot id {bot_id} bulunamadı")))?
        .to_config()?;

    // Live modda credential zorunlu; DryRun'da gerek yok.
    let creds = match cfg.run_mode {
        RunMode::Live => Some(
            db::get_credentials(&pool, bot_id)
                .await?
                .ok_or_else(|| AppError::Config("Live mod için credential yok".into()))?,
        ),
        RunMode::Dryrun => None,
    };

    db::set_bot_state(&pool, bot_id, "RUNNING").await?;

    let slug = parse_slug(&cfg.slug_pattern).or_else(|_| prefix_slug(&cfg.slug_pattern))?;
    ipc::emit(&FrontendEvent::BotStarted {
        bot_id,
        name: cfg.name.clone(),
        slug: slug.to_slug(),
        ts_ms: now_ms(),
    });

    let http = shared_http_client();
    let gamma = GammaClient::new(http.clone(), env_.gamma_base_url.clone());
    let clob = creds.as_ref().map(|c| {
        Arc::new(ClobClient::new(
            http.clone(),
            env_.clob_base_url.clone(),
            Some(c.clone()),
        ))
    });
    let executor = match clob.as_ref() {
        Some(cl) => Executor::Live(cl.clone()),
        None => Executor::DryRun(Simulator),
    };

    let signal_state = new_shared_state();
    tokio::spawn(binance_task(
        slug.asset.binance_symbol().to_string(),
        slug.interval,
        signal_state.clone(),
    ));
    tokio::spawn(heartbeat_task(heartbeat_path(&env_.heartbeat_dir, bot_id)));
    if let Some(cl) = clob.as_ref() {
        tokio::spawn(clob_heartbeat_task(cl.clone()));
    }

    let mut sigterm =
        signal(SignalKind::terminate()).map_err(|e| AppError::Config(format!("sigterm: {e}")))?;
    let mut sigint =
        signal(SignalKind::interrupt()).map_err(|e| AppError::Config(format!("sigint: {e}")))?;

    let ctx = Ctx {
        bot_id,
        cfg,
        env_,
        pool,
        gamma,
        creds,
        executor,
        signal_state,
    };

    let mut slug = slug;
    loop {
        match run_window(&ctx, slug, &mut sigterm, &mut sigint).await? {
            Some(reason) => {
                graceful_shutdown(&ctx, reason).await;
                return Ok(());
            }
            None => slug = next_window(slug),
        }
    }
}

/// Pencereyi bir interval ileri kaydır; şimdi geride kalmışsak güncel sınıra snap.
fn next_window(mut slug: SlugInfo) -> SlugInfo {
    let secs = slug.interval.seconds();
    slug.ts += secs;
    let snap = (now_secs() / secs) * secs;
    if slug.ts < snap {
        slug.ts = snap;
    }
    slug
}

/// Tek bir market penceresini yönetir.
///
/// - `Ok(None)` → pencere normal bitti, bir sonrakine geç.
/// - `Ok(Some(reason))` → SIGTERM/SIGINT, graceful shutdown iste.
/// - `Err(_)` → fatal (üst katman bot'u yeniden başlatır).
async fn run_window(
    ctx: &Ctx,
    slug: SlugInfo,
    sigterm: &mut Signal,
    sigint: &mut Signal,
) -> Result<Option<&'static str>, AppError> {
    // Gamma'nın `startDate` değeri market **yaratılma** zamanı (5dk oyuna
    // ~24 saat önce), `endDate` ise resolution zamanı. Chart X ekseni için
    // doğru kaynak slug'dan türetilen (ts, ts+interval) penceresidir.
    let market = ctx.gamma.get_market_by_slug(&slug.to_slug()).await?;
    let (yes_id, no_id) = market.parse_token_ids()?;
    let condition_id = market.condition_id.clone().unwrap_or_default();
    let (start_ts, end_ts) = (slug.ts, slug.end_ts());

    db::upsert_market_session(
        &ctx.pool,
        ctx.bot_id,
        &slug.to_slug(),
        start_ts as i64,
        end_ts as i64,
    )
    .await?;

    let mut sess = build_session(
        ctx,
        slug,
        &market,
        &yes_id,
        &no_id,
        &condition_id,
        start_ts,
        end_ts,
    );

    ipc::emit(&FrontendEvent::SessionOpened {
        bot_id: ctx.bot_id,
        slug: slug.to_slug(),
        start_ts,
        end_ts,
        yes_token_id: yes_id.clone(),
        no_token_id: no_id.clone(),
    });

    let (ev_tx, mut ev_rx) = mpsc::channel::<PolymarketEvent>(512);
    let market_ws = tokio::spawn(run_market_ws(
        ctx.env_.clob_ws_base.clone(),
        vec![yes_id, no_id],
        ev_tx.clone(),
    ));
    let user_ws = ctx.creds.as_ref().map(|c| {
        tokio::spawn(run_user_ws(
            ctx.env_.clob_ws_base.clone(),
            c.clone(),
            vec![condition_id],
            ev_tx,
        ))
    });

    let mut tick_timer = tokio_interval(Duration::from_millis(500));
    let mut zone_timer = tokio_interval(Duration::from_secs(5));
    let mut last_zone: Option<String> = None;

    let result: Option<&'static str> = loop {
        tokio::select! {
            _ = sigterm.recv() => break Some("sigterm"),
            _ = sigint.recv()  => break Some("sigint"),
            Some(ev) = ev_rx.recv() => {
                handle_event(&mut sess, &ctx.pool, ctx.bot_id, ev).await;
            }
            _ = tick_timer.tick() => tick(ctx, &mut sess).await,
            _ = zone_timer.tick() => {
                emit_zone_signal(ctx, &sess, slug, &mut last_zone).await;
                if now_secs() >= sess.end_ts {
                    break None;
                }
            }
        }
    };

    market_ws.abort();
    if let Some(h) = user_ws {
        h.abort();
    }
    if let Executor::Live(cl) = &ctx.executor {
        if let Err(e) = cl.cancel_all().await {
            tracing::warn!(error=%e, "cancel_all failed at window boundary");
        }
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn build_session(
    ctx: &Ctx,
    slug: SlugInfo,
    market: &GammaMarket,
    yes_id: &str,
    no_id: &str,
    condition_id: &str,
    start_ts: u64,
    end_ts: u64,
) -> MarketSession {
    MarketSession {
        yes_token_id: yes_id.to_string(),
        no_token_id: no_id.to_string(),
        condition_id: condition_id.to_string(),
        tick_size: market.tick_size.unwrap_or(0.01),
        api_min_order_size: market.minimum_order_size.unwrap_or(5.0),
        start_ts,
        end_ts,
        ..MarketSession::new(ctx.bot_id, slug.to_slug(), &ctx.cfg)
    }
}

async fn tick(ctx: &Ctx, sess: &mut MarketSession) {
    let decision = sess.tick(&ctx.cfg, now_ms());
    if matches!(decision, Decision::NoOp) {
        return;
    }
    let Ok(list) = execute(sess, &ctx.executor, decision).await else {
        return;
    };
    for ex in list.into_iter().filter(|e| e.filled) {
        ipc::emit(&FrontendEvent::OrderPlaced {
            bot_id: ctx.bot_id,
            order_id: ex.order_id,
            outcome: ex.planned.outcome,
            side: ex.planned.side,
            price: ex.planned.price,
            size: ex.planned.size,
            order_type: format!("{:?}", ex.planned.order_type),
            ts_ms: now_ms(),
        });
    }
}

async fn emit_zone_signal(
    ctx: &Ctx,
    sess: &MarketSession,
    slug: SlugInfo,
    last_zone: &mut Option<String>,
) {
    let zone_str = format!("{:?}", sess.current_zone(now_secs()));
    if last_zone.as_deref() != Some(zone_str.as_str()) {
        *last_zone = Some(zone_str.clone());
        ipc::emit(&FrontendEvent::ZoneChanged {
            bot_id: ctx.bot_id,
            zone: zone_str,
            zone_pct: baiter_pro::time::zone_pct(sess.start_ts, sess.end_ts, now_secs()),
            ts_ms: now_ms(),
        });
    }
    let snap = ctx.signal_state.read().await;
    ipc::emit(&FrontendEvent::SignalUpdate {
        bot_id: ctx.bot_id,
        symbol: slug.asset.binance_symbol().to_string(),
        signal_score: snap.signal_score,
        bsi: snap.bsi,
        ofi: snap.ofi,
        cvd: snap.cvd,
        ts_ms: now_ms(),
    });
}

async fn handle_event(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    bot_id: i64,
    ev: PolymarketEvent,
) {
    match ev {
        PolymarketEvent::BestBidAsk {
            asset_id,
            best_bid,
            best_ask,
            ..
        } => {
            update_best(sess, &asset_id, best_bid, best_ask);
            ipc::emit(&FrontendEvent::BestBidAsk {
                bot_id,
                yes_best_bid: sess.yes_best_bid,
                yes_best_ask: sess.yes_best_ask,
                no_best_bid: sess.no_best_bid,
                no_best_ask: sess.no_best_ask,
                ts_ms: now_ms(),
            });
        }
        PolymarketEvent::Book {
            asset_id,
            bids,
            asks,
            ..
        } => {
            if let (Some(bid), Some(ask)) = (
                bids.first().and_then(|b| b.0.parse::<f64>().ok()),
                asks.first().and_then(|a| a.0.parse::<f64>().ok()),
            ) {
                update_best(sess, &asset_id, bid, ask);
            }
        }
        PolymarketEvent::Trade {
            asset_id,
            size,
            price,
            status,
            fee_rate_bps,
            trade_id,
            ..
        } => {
            let status_upper = status.to_ascii_uppercase();
            if status_upper == "MATCHED" {
                if let Some(outcome) = outcome_from_asset_id(sess, &asset_id) {
                    let fee = fee_rate_bps
                        .map(|bps| price * size * bps / 10_000.0)
                        .unwrap_or(0.0);
                    absorb_trade_matched(sess, outcome, price, size, fee);
                    ipc::emit(&FrontendEvent::Fill {
                        bot_id,
                        trade_id,
                        outcome,
                        price,
                        size,
                        status: status_upper,
                        ts_ms: now_ms(),
                    });
                }
            }
        }
        PolymarketEvent::MarketResolved {
            market,
            winning_outcome,
            winning_asset_id,
            ..
        } => {
            let slug = sess.slug.clone();
            let _ = db::upsert_market_resolved(
                pool,
                &market,
                &winning_outcome,
                winning_asset_id.as_deref(),
                now_ms() as i64,
                None,
            )
            .await;
            ipc::emit(&FrontendEvent::SessionResolved {
                bot_id,
                slug,
                winning_outcome,
                ts_ms: now_ms(),
            });
        }
        _ => {}
    }
}

async fn graceful_shutdown(ctx: &Ctx, reason: &str) {
    if let Executor::Live(clob) = &ctx.executor {
        if let Err(e) = clob.cancel_all().await {
            tracing::warn!(error=%e, "cancel_all failed");
        }
    }
    let _ = db::set_bot_state(&ctx.pool, ctx.bot_id, "STOPPED").await;
    ipc::emit(&FrontendEvent::BotStopped {
        bot_id: ctx.bot_id,
        ts_ms: now_ms(),
        reason: reason.into(),
    });
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

/// Kullanıcı ts'siz slug öneki (`btc-updown-5m-`) girdiyse şu andaki
/// aktif pencereyi hesapla; `parse_slug`'a tam slug olarak ver.
fn prefix_slug(pattern: &str) -> Result<SlugInfo, AppError> {
    let parts: Vec<&str> = pattern.trim_end_matches('-').split('-').collect();
    let interval = parts
        .get(2)
        .and_then(|s| Interval::parse(s))
        .ok_or_else(|| AppError::InvalidSlug {
            slug: pattern.into(),
            reason: "interval parse edilemedi".into(),
        })?;
    let secs = interval.seconds();
    let ts = (now_secs() / secs) * secs;
    parse_slug(&format!("{}-{}-{}-{ts}", parts[0], parts[1], parts[2]))
}

fn heartbeat_path(dir: &str, bot_id: i64) -> PathBuf {
    let mut p = PathBuf::from(dir);
    p.push(format!("{bot_id}.heartbeat"));
    p
}

async fn heartbeat_task(path: PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    let mut tick = tokio_interval(Duration::from_secs(5));
    loop {
        tick.tick().await;
        let _ = fs::write(&path, now_ms().to_string().as_bytes()).await;
    }
}

async fn clob_heartbeat_task(clob: Arc<ClobClient>) {
    let mut tick = tokio_interval(Duration::from_secs(5));
    loop {
        tick.tick().await;
        if let Err(e) = clob.heartbeat_once().await {
            tracing::warn!(error=%e, "clob heartbeat failed");
        }
    }
}

async fn binance_task(symbol: String, interval: Interval, state: SharedSignalState) {
    binance::run_binance_signal(&symbol, interval, state).await;
}
