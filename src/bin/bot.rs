//! Bot binary — supervisor tarafından `--bot-id <id>` ile spawn edilir.
//!
//! Sorumluluklar:
//! - DB'den `BotConfig` + `Credentials` yükle (§9a).
//! - Gamma ile aktif/gelecek market penceresini belirle (§0, §11).
//! - `MarketSession` kur, Market WS + User WS + Binance sinyal task'larını başlat.
//! - CLOB REST heartbeat döngüsü (§4.1).
//! - Heartbeat dosyasını periyodik güncelle (§1 "crash loop" kuralı).
//! - Stdout'a `[[EVENT]] …` JSON satırları emit et (§5.1).
//! - SIGTERM: açık GTC emirleri iptal, WS kapat, exit 0 (§18.2).
//!
//! Referans: [docs/bot-platform-mimari.md §1 §4.1 §5 §18](../../../docs/bot-platform-mimari.md).

use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use baiter_pro::binance::{self, new_shared_state, SharedSignalState};
use baiter_pro::config::RuntimeEnv;
use baiter_pro::db;
use baiter_pro::engine::{
    absorb_trade_matched, execute, outcome_from_asset_id, update_best, Executor, MarketSession,
    Simulator,
};
use baiter_pro::error::AppError;
use baiter_pro::ipc::{self, FrontendEvent};
use baiter_pro::polymarket::clob::{shared_http_client, ClobClient};
use baiter_pro::polymarket::gamma::GammaClient;
use baiter_pro::polymarket::ws::{run_market_ws, run_user_ws, PolymarketEvent};
use baiter_pro::slug::{parse_slug, Interval, SlugInfo};
use baiter_pro::strategy::Decision;
use baiter_pro::time::{now_ms, now_secs};
use baiter_pro::types::RunMode;
use sqlx::SqlitePool;
use tokio::fs;
use tokio::signal::unix::{signal, SignalKind};
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

async fn run() -> Result<(), AppError> {
    let bot_id = parse_bot_id()?;
    env::set_var("BAITER_BOT_ID", bot_id.to_string());

    let env_ = RuntimeEnv::from_env()?;
    let pool = db::open(&env_.db_path).await?;

    let row = db::get_bot(&pool, bot_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("bot id {bot_id} bulunamadı")))?;
    let cfg = row.to_config()?;

    // Live modda credential zorunlu; DryRun'da gerek yok (simülatör kullanılır).
    let creds = match cfg.run_mode {
        RunMode::Live => Some(
            db::get_credentials(&pool, bot_id)
                .await?
                .or_else(|| env_.fallback_creds.clone())
                .ok_or_else(|| AppError::Config("Live mod için credential yok".into()))?,
        ),
        RunMode::Dryrun => None,
    };

    db::set_bot_state(&pool, bot_id, "RUNNING").await?;

    let slug_info = parse_slug(&cfg.slug_pattern).or_else(|_| find_prefix_slug(&cfg.slug_pattern))?;

    ipc::emit(&FrontendEvent::BotStarted {
        bot_id,
        name: cfg.name.clone(),
        slug: slug_info.to_slug(),
        ts_ms: now_ms(),
    });

    let http = shared_http_client();
    let gamma = GammaClient::new(http.clone(), env_.gamma_base_url.clone());
    // Clob client yalnız Live modda gerekir (cancel_all, heartbeat, order post).
    let clob = creds.as_ref().map(|c| {
        Arc::new(ClobClient::new(
            http.clone(),
            env_.clob_base_url.clone(),
            Some(c.clone()),
        ))
    });

    let market = resolve_market(&gamma, slug_info).await?;
    let (yes_id, no_id) = market.parse_token_ids()?;
    let condition_id = market.condition_id.clone().unwrap_or_default();
    let tick_size = market.tick_size.unwrap_or(0.01);
    let min_size = market.minimum_order_size.unwrap_or(5.0);

    let session_id = db::upsert_market_session(
        &pool,
        bot_id,
        &slug_info.to_slug(),
        slug_info.ts as i64,
        slug_info.end_ts() as i64,
    )
    .await?;
    let _ = session_id;

    let session = MarketSession {
        yes_token_id: yes_id.clone(),
        no_token_id: no_id.clone(),
        condition_id: condition_id.clone(),
        tick_size,
        api_min_order_size: min_size,
        start_ts: slug_info.ts,
        end_ts: slug_info.end_ts(),
        ..MarketSession::new(bot_id, slug_info.to_slug(), &cfg)
    };

    ipc::emit(&FrontendEvent::SessionOpened {
        bot_id,
        slug: slug_info.to_slug(),
        start_ts: slug_info.ts,
        end_ts: slug_info.end_ts(),
        yes_token_id: yes_id.clone(),
        no_token_id: no_id.clone(),
    });

    let signal_state = new_shared_state();

    let (ev_tx, mut ev_rx) = mpsc::channel::<PolymarketEvent>(512);

    tokio::spawn(run_market_ws(
        env_.clob_ws_base.clone(),
        vec![yes_id.clone(), no_id.clone()],
        ev_tx.clone(),
    ));

    if let (Some(c), Some(cl)) = (creds.as_ref(), clob.as_ref()) {
        tokio::spawn(run_user_ws(
            env_.clob_ws_base.clone(),
            c.clone(),
            vec![condition_id.clone()],
            ev_tx.clone(),
        ));
        tokio::spawn(clob_heartbeat_task(cl.clone()));
    }

    tokio::spawn(run_binance_task(
        slug_info.asset.binance_symbol().to_string(),
        slug_info.interval,
        signal_state.clone(),
    ));

    let heartbeat_path = heartbeat_file_path(&env_.heartbeat_dir, bot_id);
    tokio::spawn(heartbeat_file_task(heartbeat_path));

    let executor = match clob.as_ref() {
        Some(cl) => Executor::Live(cl.clone()),
        None => Executor::DryRun(Simulator),
    };

    let mut sigterm =
        signal(SignalKind::terminate()).map_err(|e| AppError::Config(format!("sigterm: {e}")))?;
    let mut sigint =
        signal(SignalKind::interrupt()).map_err(|e| AppError::Config(format!("sigint: {e}")))?;

    let mut tick_timer = tokio_interval(Duration::from_millis(500));
    let mut zone_timer = tokio_interval(Duration::from_secs(5));

    let mut sess = session;
    let mut last_zone: Option<String> = None;

    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received, graceful shutdown");
                graceful_shutdown(bot_id, &pool, &executor, "sigterm").await;
                break;
            }
            _ = sigint.recv() => {
                tracing::info!("SIGINT received, graceful shutdown");
                graceful_shutdown(bot_id, &pool, &executor, "sigint").await;
                break;
            }
            Some(ev) = ev_rx.recv() => {
                handle_event(&mut sess, &pool, &signal_state, bot_id, ev).await;
            }
            _ = tick_timer.tick() => {
                let decision = sess.tick(&cfg, now_ms());
                if !matches!(decision, Decision::NoOp) {
                    let outcomes = execute(&mut sess, &executor, decision).await;
                    if let Ok(list) = outcomes {
                        for ex in list {
                            if ex.filled {
                                ipc::emit(&FrontendEvent::OrderPlaced {
                                    bot_id,
                                    order_id: ex.order_id.clone(),
                                    outcome: ex.planned.outcome,
                                    side: ex.planned.side,
                                    price: ex.planned.price,
                                    size: ex.planned.size,
                                    order_type: format!("{:?}", ex.planned.order_type),
                                    ts_ms: now_ms(),
                                });
                            }
                        }
                    }
                }
            }
            _ = zone_timer.tick() => {
                let zone = sess.current_zone(now_secs());
                let zone_str = format!("{zone:?}");
                if last_zone.as_deref() != Some(zone_str.as_str()) {
                    last_zone = Some(zone_str.clone());
                    let pct = baiter_pro::time::zone_pct(sess.start_ts, sess.end_ts, now_secs());
                    ipc::emit(&FrontendEvent::ZoneChanged {
                        bot_id,
                        zone: zone_str,
                        zone_pct: pct,
                        ts_ms: now_ms(),
                    });
                }
                let snap = signal_state.read().await;
                ipc::emit(&FrontendEvent::SignalUpdate {
                    bot_id,
                    symbol: slug_info.asset.binance_symbol().to_string(),
                    signal_score: snap.signal_score,
                    bsi: snap.bsi,
                    ofi: snap.ofi,
                    cvd: snap.cvd,
                    ts_ms: now_ms(),
                });
                drop(snap);
                if now_secs() >= sess.end_ts {
                    ipc::emit(&FrontendEvent::BotStopped {
                        bot_id,
                        ts_ms: now_ms(),
                        reason: "window-ended".into(),
                    });
                    graceful_shutdown(bot_id, &pool, &executor, "window-ended").await;
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn handle_event(
    sess: &mut MarketSession,
    pool: &SqlitePool,
    _signal_state: &SharedSignalState,
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
        PolymarketEvent::Order { .. }
        | PolymarketEvent::PriceChange { .. }
        | PolymarketEvent::LastTradePrice { .. }
        | PolymarketEvent::TickSizeChange { .. }
        | PolymarketEvent::Disconnected { .. }
        | PolymarketEvent::Reconnected => {}
    }
}

async fn graceful_shutdown(
    bot_id: i64,
    pool: &SqlitePool,
    executor: &Executor,
    reason: &str,
) {
    if let Executor::Live(clob) = executor {
        match clob.cancel_all().await {
            Ok(_) => tracing::info!("open orders canceled"),
            Err(e) => tracing::warn!(error=%e, "cancel_all failed"),
        }
    }
    let _ = db::set_bot_state(pool, bot_id, "STOPPED").await;
    ipc::emit(&FrontendEvent::BotStopped {
        bot_id,
        ts_ms: now_ms(),
        reason: reason.into(),
    });
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

async fn resolve_market(
    gamma: &GammaClient,
    slug: SlugInfo,
) -> Result<baiter_pro::polymarket::gamma::GammaMarket, AppError> {
    let exact = slug.to_slug();
    if let Ok(m) = gamma.get_market_by_slug(&exact).await {
        return Ok(m);
    }
    let prefix = baiter_pro::slug::SlugInfo::prefix(slug.asset, slug.interval);
    let list = gamma.list_active_by_prefix(&prefix).await?;
    list.into_iter()
        .next()
        .ok_or_else(|| AppError::Gamma(format!("aktif market bulunamadı: {prefix}")))
}

/// Kullanıcı ts'siz slug öneki (`btc-updown-5m-`) girdiyse şu andaki
/// aktif pencereyi hesapla; `parse_slug`'a tam slug olarak ver.
fn find_prefix_slug(pattern: &str) -> Result<SlugInfo, AppError> {
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

fn heartbeat_file_path(dir: &str, bot_id: i64) -> PathBuf {
    let mut p = PathBuf::from(dir);
    p.push(format!("{bot_id}.heartbeat"));
    p
}

async fn heartbeat_file_task(path: PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    let mut tick = tokio_interval(Duration::from_secs(5));
    loop {
        tick.tick().await;
        let ts = now_ms().to_string();
        if let Err(e) = fs::write(&path, ts.as_bytes()).await {
            tracing::warn!(error=%e, "heartbeat write failed");
        }
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

async fn run_binance_task(symbol: String, interval: Interval, state: SharedSignalState) {
    binance::run_binance_signal(&symbol, interval, state).await;
}
