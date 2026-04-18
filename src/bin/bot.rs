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
    absorb_trade_matched, execute, outcome_from_asset_id, simulate_passive_fills, update_best,
    Executor, MarketSession, Simulator,
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
    // Mimari §5.1: tracing satırları da supervisor → SQLite logs tablosuna gider.
    // Stdout'a (ANSI'sız, compact, timestamp'siz) yazıyoruz; supervisor stdout
    // pipe'ında `[[EVENT]]` prefix'i olmayan satırları logs'a yazar ve seviyeyi
    // satır başındaki `INFO`/`WARN`/`ERROR` token'ından çözer.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,hyper=warn,sqlx=warn,tungstenite=warn,reqwest=warn",
                )
            }),
        )
        .with_target(false)
        .without_time()
        .with_ansi(false)
        .with_writer(std::io::stdout)
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
    ipc::log_line(
        &bot_id.to_string(),
        format!(
            "Bot started — strategy={:?} mode={:?} order_usdc={} signal_weight={}",
            cfg.strategy, cfg.run_mode, cfg.order_usdc, cfg.signal_weight,
        ),
    );
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
        bot_id,
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
    let label = ctx.bot_id.to_string();
    let slug_str = slug.to_slug();
    ipc::log_line(&label, format!("Target market: {slug_str}"));
    ipc::log_line(&label, format!("📡 Fetching market: {slug_str}"));
    let market = ctx.gamma.get_market_by_slug(&slug_str).await?;
    let (yes_id, no_id) = market.parse_token_ids()?;
    let condition_id = market.condition_id.clone().unwrap_or_default();
    let (start_ts, end_ts) = (slug.ts, slug.end_ts());

    if let Some(q) = market.question.as_deref() {
        ipc::log_line(&label, format!("✅ Found market: {q}"));
    }
    ipc::log_line(
        &label,
        format!("Window: {} UTC - {} UTC", fmt_utc(start_ts), fmt_utc(end_ts)),
    );
    ipc::log_line(&label, format!("    UP:   {yes_id}"));
    ipc::log_line(&label, format!("    DOWN: {no_id}"));

    db::upsert_market_session(
        &ctx.pool,
        ctx.bot_id,
        &slug_str,
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
        slug: slug_str.clone(),
        start_ts,
        end_ts,
        yes_token_id: yes_id.clone(),
        no_token_id: no_id.clone(),
    });

    ipc::log_line(&label, "🔌 Connecting to Market WebSocket...");
    let (ev_tx, mut ev_rx) = mpsc::channel::<PolymarketEvent>(512);
    let market_ws = tokio::spawn(run_market_ws(
        ctx.env_.clob_ws_base.clone(),
        vec![yes_id, no_id],
        ev_tx.clone(),
    ));
    let user_ws = ctx.creds.as_ref().map(|c| {
        ipc::log_line(&label, "🔌 Connecting to User WebSocket...");
        tokio::spawn(run_user_ws(
            ctx.env_.clob_ws_base.clone(),
            c.clone(),
            vec![condition_id],
            ev_tx,
        ))
    });
    ipc::log_line(
        &label,
        format!(
            "🚀 Starting trading loop (strategy: {:?}, mode: {:?})",
            ctx.cfg.strategy, ctx.cfg.run_mode
        ),
    );

    let mut tick_timer = tokio_interval(Duration::from_millis(500));
    let mut zone_timer = tokio_interval(Duration::from_secs(5));
    let mut last_zone: Option<String> = None;
    let mut last_book_snapshot: Option<(f64, f64, f64, f64)> = None;

    let result: Option<&'static str> = loop {
        tokio::select! {
            _ = sigterm.recv() => break Some("sigterm"),
            _ = sigint.recv()  => break Some("sigint"),
            Some(ev) = ev_rx.recv() => {
                handle_event(&mut sess, &ctx.pool, ctx.bot_id, ctx.cfg.run_mode, ev).await;
            }
            _ = tick_timer.tick() => tick(ctx, &mut sess).await,
            _ = zone_timer.tick() => {
                emit_zone_signal(ctx, &sess, slug, &mut last_zone, &mut last_book_snapshot).await;
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
        ipc::log_line(&label, "🚫 cancel_all");
        match cl.cancel_all().await {
            Ok(resp) => ipc::log_line(
                &label,
                format!(
                    "    canceled={:?} not_canceled={}",
                    resp.canceled, resp.not_canceled
                ),
            ),
            Err(e) => {
                ipc::log_line(&label, format!("    cancel_all error: {e}"));
                tracing::warn!(error=%e, "cancel_all failed at window boundary");
            }
        }
    }
    if result.is_none() {
        ipc::log_line(
            &label,
            "🏁 Market window complete, transitioning to next market...",
        );
    }
    Ok(result)
}

/// Unix saniyesini `YYYY-MM-DD HH:MM:SS` (UTC) string'ine çevirir — log için.
fn fmt_utc(ts: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
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
    let snap = ctx.signal_state.read().await;
    let es = baiter_pro::binance::effective_score(snap.signal_score, ctx.cfg.signal_weight);
    let prev_state = sess.harvest_state;
    let decision = sess.tick(&ctx.cfg, now_ms(), es);
    let label = ctx.bot_id.to_string();

    // §5.2: OpenDual giriş/çıkış logları (state geçişlerini görsel hale getir).
    match (prev_state, sess.harvest_state) {
        (
            baiter_pro::strategy::harvest::HarvestState::Pending,
            baiter_pro::strategy::harvest::HarvestState::OpenDual { deadline_ms },
        ) => {
            if let Decision::PlaceOrders(orders) = &decision {
                let up = orders
                    .iter()
                    .find(|o| matches!(o.outcome, baiter_pro::types::Outcome::Up))
                    .map(|o| o.price)
                    .unwrap_or(0.0);
                let down = orders
                    .iter()
                    .find(|o| matches!(o.outcome, baiter_pro::types::Outcome::Down))
                    .map(|o| o.price)
                    .unwrap_or(0.0);
                ipc::log_line(
                    &label,
                    format!(
                        "🎯 OpenDual signal_score={:.2} effective_score={:.2} → up_bid={:.2} down_bid={:.2} deadline={}ms",
                        snap.signal_score, es, up, down, deadline_ms
                    ),
                );
            }
        }
        (
            baiter_pro::strategy::harvest::HarvestState::OpenDual { .. },
            baiter_pro::strategy::harvest::HarvestState::SingleLeg { filled_side },
        ) => {
            let yes_filled = sess.metrics.shares_yes > 0.0;
            let no_filled = sess.metrics.shares_no > 0.0;
            if yes_filled && no_filled {
                ipc::log_line(
                    &label,
                    format!(
                        "🔀 OpenDual both filled → SingleLeg{{by_signal={}}}",
                        filled_side.as_str()
                    ),
                );
            } else {
                ipc::log_line(
                    &label,
                    format!(
                        "⏰ OpenDual timeout (one_fill={}) → cancelling counter side",
                        filled_side.as_str()
                    ),
                );
            }
        }
        (
            baiter_pro::strategy::harvest::HarvestState::OpenDual { .. },
            baiter_pro::strategy::harvest::HarvestState::Pending,
        ) => {
            ipc::log_line(
                &label,
                "⏰ OpenDual timeout (no_fill) → cancelling 2 orders, reopening".to_string(),
            );
        }
        (
            baiter_pro::strategy::harvest::HarvestState::SingleLeg { filled_side },
            baiter_pro::strategy::harvest::HarvestState::SingleLeg { .. },
        ) => {
            if let Decision::CancelOrders(ids) = &decision {
                ipc::log_line(
                    &label,
                    format!(
                        "🔁 Averaging timeout (side={}) → cancelling {} order(s), will retry",
                        filled_side.as_str(),
                        ids.len()
                    ),
                );
            }
        }
        (
            baiter_pro::strategy::harvest::HarvestState::SingleLeg { filled_side },
            baiter_pro::strategy::harvest::HarvestState::ProfitLock,
        ) => {
            // §5.2: ProfitLock tetiklendi — first_leg + hedge_leg ≤ avg_threshold.
            // avg_threshold engine.rs::tick() ile aynı formülden türetilir.
            let avg_threshold = ctx
                .cfg
                .strategy_params
                .harvest_profit_lock_pct
                .map(|p| 1.0 - p.abs())
                .unwrap_or(0.98);
            let first_leg = match filled_side {
                baiter_pro::types::Outcome::Up => sess.metrics.avg_yes,
                baiter_pro::types::Outcome::Down => sess.metrics.avg_no,
            };
            let hedge_leg = match filled_side {
                baiter_pro::types::Outcome::Up => sess.no_best_ask,
                baiter_pro::types::Outcome::Down => sess.yes_best_ask,
            };
            ipc::log_line(
                &label,
                format!(
                    "🔒 ProfitLock triggered: first_leg({})={:.4} + hedge_leg({})={:.4} = {:.4} ≤ threshold({:.2}) → FAK",
                    filled_side.as_str(),
                    first_leg,
                    match filled_side {
                        baiter_pro::types::Outcome::Up => "DOWN",
                        baiter_pro::types::Outcome::Down => "UP",
                    },
                    hedge_leg,
                    first_leg + hedge_leg,
                    avg_threshold,
                ),
            );
        }
        _ => {}
    }

    if matches!(decision, Decision::NoOp) {
        return;
    }

    // §5.5: cancel önce log'lansın (DELETE /order ({n} ids) ids=[..]).
    let cancel_ids: Vec<String> = match &decision {
        Decision::CancelOrders(ids) => ids.clone(),
        Decision::Batch { cancel, .. } => cancel.clone(),
        _ => Vec::new(),
    };
    if !cancel_ids.is_empty() {
        ipc::log_line(
            &label,
            format!(
                "🚫 DELETE /order ({} ids) ids={:?}",
                cancel_ids.len(),
                cancel_ids
            ),
        );
    }

    let Ok(out) = execute(sess, &ctx.executor, decision).await else {
        return;
    };

    // §5.5: cancel sonucu — gerçek CancelResponse alanları.
    for c in &out.canceled {
        ipc::log_line(
            &label,
            format!(
                "    canceled={:?} not_canceled={}",
                c.canceled, c.not_canceled
            ),
        );
    }

    // §5.2/§5.5: order placement — status=matched|live.
    // signal=X.XX(eff X.XX) emir bağlamını taşır (periyodik Binance loguna gerek
    // kalmaması için sadece emir anında basılır).
    for ex in &out.placed {
        let status = if ex.filled { "matched" } else { "live" };
        ipc::log_line(
            &label,
            format!(
                "✅ orderType={} side={} outcome={} size={} price={} | status={} | reason={} | signal={:.2}(eff {:.2})",
                ex.planned.order_type.as_str(),
                ex.planned.side.as_str(),
                ex.planned.outcome.as_str(),
                ex.planned.size,
                ex.planned.price,
                status,
                ex.planned.reason,
                snap.signal_score,
                es,
            ),
        );
    }

    for ex in out.placed.into_iter().filter(|e| e.filled) {
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
    last_book_snapshot: &mut Option<(f64, f64, f64, f64)>,
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

    // §5.4 book snapshot: 5s cadence'inde, sadece değişiklik varsa logla.
    // Binance signal context'i artık periyodik basılmaz; emir gönderilirken
    // `✅ orderType=... | signal=...(eff ...)` satırına dahil edilir.
    let current = (
        sess.yes_best_bid,
        sess.yes_best_ask,
        sess.no_best_bid,
        sess.no_best_ask,
    );
    if current.0 > 0.0
        && current.2 > 0.0
        && last_book_snapshot.as_ref() != Some(&current)
    {
        *last_book_snapshot = Some(current);
        ipc::log_line(
            &ctx.bot_id.to_string(),
            format!(
                "📚 Book snapshot: yes_bid={:.4} yes_ask={:.4} no_bid={:.4} no_ask={:.4} | yes_spread={:.4} no_spread={:.4}",
                current.0,
                current.1,
                current.2,
                current.3,
                (current.1 - current.0).max(0.0),
                (current.3 - current.2).max(0.0),
            ),
        );
    }
}

/// İlk kez her iki taraf book'u dolduğunda tek seferlik bilgi logu.
fn maybe_log_book_ready(sess: &mut MarketSession, bot_id: i64) {
    if sess.book_ready_logged {
        return;
    }
    if sess.yes_best_bid > 0.0 && sess.no_best_bid > 0.0 {
        ipc::log_line(
            &bot_id.to_string(),
            format!(
                "📚 Market book ready: yes_bid={:.4} yes_ask={:.4} no_bid={:.4} no_ask={:.4}",
                sess.yes_best_bid, sess.yes_best_ask, sess.no_best_bid, sess.no_best_ask
            ),
        );
        sess.book_ready_logged = true;
    }
}

/// DryRun ise market book güncellemesinden sonra açık emirleri yeni quote'larla
/// karşılaştırıp passive (maker) fill'leri uygula.
fn run_passive_fills_if_dryrun(sess: &mut MarketSession, bot_id: i64, run_mode: RunMode) {
    if run_mode != RunMode::Dryrun {
        return;
    }
    let label = bot_id.to_string();
    for ex in simulate_passive_fills(sess) {
        let p = &ex.planned;
        let fp = ex.fill_price.unwrap_or(p.price);
        let fs = ex.fill_size.unwrap_or(p.size);
        ipc::log_line(
            &label,
            format!(
                "📥 passive_fill side={} outcome={} size={} price={:.4} reason={}",
                p.side.as_str(),
                p.outcome.as_str(),
                fs,
                fp,
                p.reason
            ),
        );
        ipc::emit(&FrontendEvent::Fill {
            bot_id,
            trade_id: ex.order_id.clone(),
            outcome: p.outcome,
            price: fp,
            size: fs,
            status: "MATCHED".to_string(),
            ts_ms: now_ms(),
        });
    }
}

async fn handle_event(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    bot_id: i64,
    run_mode: RunMode,
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
            maybe_log_book_ready(sess, bot_id);
            run_passive_fills_if_dryrun(sess, bot_id, run_mode);
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
                maybe_log_book_ready(sess, bot_id);
                run_passive_fills_if_dryrun(sess, bot_id, run_mode);
            }
        }
        PolymarketEvent::Trade {
            asset_id,
            size,
            price,
            status,
            fee_rate_bps,
            trade_id,
            outcome: outcome_str,
            raw,
            ..
        } => {
            let status_upper = status.to_ascii_uppercase();
            let label = bot_id.to_string();

            // §5.3: WS trade — tüm statuslar için tek satır.
            let mut parts = vec![
                format!("id={trade_id}"),
                format!("status={status_upper}"),
            ];
            if let Some(o) = outcome_str.as_deref() {
                parts.push(format!("outcome={o}"));
            }
            parts.push(format!("size={size}"));
            parts.push(format!("price={price}"));
            if let Some(s) = raw.get("taker_order_id").and_then(|v| v.as_str()) {
                parts.push(format!("taker_order_id={s}"));
            }
            if let Some(s) = raw.get("trader_side").and_then(|v| v.as_str()) {
                parts.push(format!("trader_side={s}"));
            }
            ipc::log_line(&label, format!("📬 WS trade | {}", parts.join(" ")));

            if status_upper == "MATCHED" {
                if let Some(outcome) = outcome_from_asset_id(sess, &asset_id) {
                    let fee = fee_rate_bps
                        .map(|bps| price * size * bps / 10_000.0)
                        .unwrap_or(0.0);
                    absorb_trade_matched(sess, outcome, price, size, fee);

                    // §5.3: fill_summary + Position.
                    ipc::log_line(
                        &label,
                        format!(
                            "✅ fill_summary outcome={} size={size} price={price}",
                            outcome.as_str()
                        ),
                    );
                    let imb = sess.metrics.imbalance;
                    let imb_sign = if imb >= 0.0 {
                        format!("+{imb}")
                    } else {
                        imb.to_string()
                    };
                    ipc::log_line(
                        &label,
                        format!(
                            "📊 [{:?}] Position: UP={}, DOWN={} (imbalance: {})",
                            sess.strategy,
                            sess.metrics.shares_yes,
                            sess.metrics.shares_no,
                            imb_sign
                        ),
                    );

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
        PolymarketEvent::Order {
            order_id,
            lifecycle_type,
            order_type,
            status,
            size_matched,
            raw,
            ..
        } => {
            let label = bot_id.to_string();
            match lifecycle_type.as_str() {
                "PLACEMENT" => {
                    let mut parts = vec![
                        "type=PLACEMENT".to_string(),
                    ];
                    if let Some(ot) = order_type.as_deref().filter(|s| !s.is_empty()) {
                        parts.push(format!("order_type={ot}"));
                    }
                    if !status.is_empty() {
                        parts.push(format!("status={status}"));
                    }
                    parts.push(format!("id={order_id}"));
                    ipc::log_line(&label, format!("📬 WS order {}", parts.join(" ")));
                }
                "UPDATE" => {
                    let mut parts = vec![
                        "type=UPDATE".to_string(),
                        format!("id={order_id}"),
                    ];
                    if let Some(sm) = size_matched {
                        parts.push(format!("size_matched={sm}"));
                    }
                    if let Some(at) = raw.get("associate_trades") {
                        parts.push(format!("associate_trades={at}"));
                    }
                    ipc::log_line(&label, format!("📬 WS order {}", parts.join(" ")));
                }
                "CANCELLATION" => {
                    ipc::log_line(
                        &label,
                        format!("📬 WS order type=CANCELLATION id={order_id}"),
                    );
                }
                _ => {}
            }
        }
        PolymarketEvent::MarketResolved {
            market,
            winning_outcome,
            winning_asset_id,
            timestamp_ms,
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

            let label = bot_id.to_string();
            let mut parts = vec![
                format!("market={market}"),
                format!("winning_outcome={winning_outcome}"),
            ];
            if let Some(a) = winning_asset_id.as_deref() {
                parts.push(format!("winning_asset_id={a}"));
            }
            parts.push(format!("ts={timestamp_ms}"));
            ipc::log_line(&label, format!("🏆 market_resolved | {}", parts.join(" | ")));

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
    let label = ctx.bot_id.to_string();
    if let Executor::Live(clob) = &ctx.executor {
        ipc::log_line(&label, "🚫 cancel_all");
        match clob.cancel_all().await {
            Ok(resp) => ipc::log_line(
                &label,
                format!(
                    "    canceled={:?} not_canceled={}",
                    resp.canceled, resp.not_canceled
                ),
            ),
            Err(e) => {
                ipc::log_line(&label, format!("    cancel_all error: {e}"));
                tracing::warn!(error=%e, "cancel_all failed");
            }
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

async fn binance_task(symbol: String, interval: Interval, state: SharedSignalState, bot_id: i64) {
    binance::run_binance_signal(&symbol, interval, state, bot_id).await;
}
