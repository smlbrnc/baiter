//! Bot ömrü boyunca sabit bağlam — pencereler arası paylaşılır.

use std::env;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::signal::unix::{signal, Signal, SignalKind};

use crate::binance::{self, new_shared_state, SharedSignalState};
use crate::config::{BotConfig, Credentials, RuntimeEnv};
use crate::db;
use crate::engine::{Executor, LiveExecutor, Simulator};
use crate::error::AppError;
use crate::polymarket::{shared_http_client, ClobClient, GammaClient};
use crate::rtds::{self, SharedRtdsState};
use crate::slug::{parse_slug_or_prefix, SlugInfo};
use crate::types::RunMode;

use super::tasks;

/// Paylaşılan bot bağlamı.
pub struct Ctx {
    pub bot_id: i64,
    pub cfg: BotConfig,
    pub env_: RuntimeEnv,
    pub pool: SqlitePool,
    pub gamma: GammaClient,
    pub creds: Option<Credentials>,
    pub executor: Executor,
    pub signal_state: SharedSignalState,
    pub rtds_state: SharedRtdsState,
}

/// CLI veya `BAITER_BOT_ID` env'inden bot id parse et.
pub fn parse_bot_id() -> Result<i64, AppError> {
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

/// `Ctx` + ilk `SlugInfo` + SIGTERM/SIGINT handler'larını kur.
pub async fn load(bot_id: i64) -> Result<(Ctx, SlugInfo, Signal, Signal), AppError> {
    let env_ = RuntimeEnv::from_env()?;
    let pool = db::open(&env_.db_path).await?;

    let cfg = load_and_validate_cfg(&pool, bot_id).await?;
    let creds = load_validated_creds(&pool, bot_id, cfg.run_mode).await?;

    db::set_bot_state(&pool, bot_id, "RUNNING").await?;
    let slug = parse_slug_or_prefix(&cfg.slug_pattern, cfg.start_offset)?;

    let http = shared_http_client();
    let gamma = GammaClient::new(http.clone(), env_.gamma_base_url.clone());
    let (executor, clob) = build_executor(&http, &env_, &cfg, creds.as_ref());

    let signal_state = new_shared_state();
    let rtds_state = rtds::new_shared_state();
    spawn_background_tasks(BackgroundTasksArgs {
        bot_id,
        slug,
        heartbeat_dir: env_.heartbeat_dir.clone(),
        signal_state: signal_state.clone(),
        rtds_state: rtds_state.clone(),
        clob: clob.as_ref(),
        cfg: &cfg,
        env_: &env_,
    });

    let (sigterm, sigint) = register_signals()?;

    Ok((
        Ctx {
            bot_id,
            cfg,
            env_,
            pool,
            gamma,
            creds,
            executor,
            signal_state,
            rtds_state,
        },
        slug,
        sigterm,
        sigint,
    ))
}

async fn load_and_validate_cfg(pool: &SqlitePool, bot_id: i64) -> Result<BotConfig, AppError> {
    db::get_bot(pool, bot_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("bot id {bot_id} bulunamadı")))?
        .to_config()
}

/// Live modda creds zorunlu. Çözümleme sırası: `bot_credentials` → `global_credentials`.
async fn load_validated_creds(
    pool: &SqlitePool,
    bot_id: i64,
    run_mode: RunMode,
) -> Result<Option<Credentials>, AppError> {
    if run_mode != RunMode::Live {
        return Ok(None);
    }
    let c = match db::get_credentials(pool, bot_id).await? {
        Some(c) => c,
        None => db::get_global_credentials(pool)
            .await?
            .ok_or(AppError::MissingCredentials { bot_id })?
            .into(),
    };
    validate_signature_type(bot_id, &c)?;
    Ok(Some(c))
}

/// EIP-712 signature_type: 0 (EOA), 1 (POLY_PROXY), 2 (POLY_GNOSIS_SAFE); 1|2 için `funder` zorunlu.
fn validate_signature_type(bot_id: i64, c: &Credentials) -> Result<(), AppError> {
    if !matches!(c.signature_type, 0..=2) {
        return Err(AppError::Config(format!(
            "bot {bot_id}: signature_type {} geçersiz (0|1|2 olmalı)",
            c.signature_type
        )));
    }
    if matches!(c.signature_type, 1..=2) && c.funder.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::Config(format!(
            "bot {bot_id}: signature_type {} için 'funder' adresi zorunlu",
            c.signature_type
        )));
    }
    Ok(())
}

/// Live'da `LiveExecutor` + paylaşılan `ClobClient`; DryRun'da `Simulator` + `None`.
fn build_executor(
    http: &reqwest::Client,
    env_: &RuntimeEnv,
    cfg: &BotConfig,
    creds: Option<&Credentials>,
) -> (Executor, Option<Arc<ClobClient>>) {
    let Some(c) = creds else {
        return (Executor::DryRun(Simulator), None);
    };
    let clob = Arc::new(ClobClient::new(
        http.clone(),
        env_.clob_base_url.clone(),
        Some(c.clone()),
    ));
    let exec = Executor::Live(Box::new(LiveExecutor {
        client: clob.clone(),
        creds: c.clone(),
        chain_id: env_.polygon_chain_id,
        // GTD timeout = cooldown_threshold (ms→s); V2 protocol +60s buffer
        // `expiration_for` içinde uygulanır.
        gtd_timeout_secs: cfg.cooldown_threshold / 1000,
        builder_code: c.builder_code.clone(),
    }));
    (exec, Some(clob))
}

struct BackgroundTasksArgs<'a> {
    bot_id: i64,
    slug: SlugInfo,
    heartbeat_dir: String,
    signal_state: SharedSignalState,
    rtds_state: SharedRtdsState,
    clob: Option<&'a Arc<ClobClient>>,
    cfg: &'a BotConfig,
    env_: &'a RuntimeEnv,
}

/// Binance signal + (opsiyonel) RTDS + heartbeat + (Live'da) CLOB ping task'larını spawn eder.
fn spawn_background_tasks(args: BackgroundTasksArgs<'_>) {
    let BackgroundTasksArgs {
        bot_id,
        slug,
        heartbeat_dir,
        signal_state,
        rtds_state,
        clob,
        cfg,
        env_,
    } = args;
    let symbol = slug.asset.binance_symbol().to_string();
    tokio::spawn(async move {
        binance::run_binance_signal(&symbol, slug.interval, signal_state, bot_id).await;
    });
    if cfg.strategy_params.rtds_enabled_or_default() {
        let rtds_symbol = slug.asset.rtds_symbol().to_string();
        let ws_url = env_.rtds_ws_url.clone();
        let stale_ms = env_.rtds_stale_threshold_ms;
        let max_backoff_ms = env_.rtds_reconnect_max_backoff_ms;
        tokio::spawn(async move {
            rtds::run_rtds_task(
                ws_url,
                rtds_symbol,
                stale_ms,
                max_backoff_ms,
                rtds_state,
                bot_id,
            )
            .await;
        });
    }
    tokio::spawn(tasks::heartbeat_task(tasks::heartbeat_path(
        &heartbeat_dir,
        bot_id,
    )));
    if let Some(cl) = clob {
        tokio::spawn(tasks::clob_heartbeat_task(cl.clone()));
    }
}

fn register_signals() -> Result<(Signal, Signal), AppError> {
    let sigterm =
        signal(SignalKind::terminate()).map_err(|e| AppError::Config(format!("sigterm: {e}")))?;
    let sigint =
        signal(SignalKind::interrupt()).map_err(|e| AppError::Config(format!("sigint: {e}")))?;
    Ok((sigterm, sigint))
}
