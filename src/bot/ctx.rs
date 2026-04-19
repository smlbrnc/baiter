//! Bot ömrü boyunca sabit bağlam — pencereler arası paylaşılır.

use std::env;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::signal::unix::{signal, Signal, SignalKind};

use crate::binance::{new_shared_state, SharedSignalState};
use crate::config::{BotConfig, Credentials, RuntimeEnv};
use crate::db;
use crate::engine::{Executor, LiveExecutor, Simulator};
use crate::error::AppError;
use crate::polymarket::clob::{shared_http_client, ClobClient};
use crate::polymarket::gamma::GammaClient;
use crate::slug::{parse_slug_or_prefix, SlugInfo};
use crate::types::{RunMode, Strategy};

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

/// `Ctx` + ilk slug + signal handler'ları kur.
///
/// Doc §11 sözleşmesi: yalnız `Strategy::Harvest` aktif strateji; diğer
/// stratejiler için bot başlatma reddedilir.
pub async fn load(bot_id: i64) -> Result<(Ctx, SlugInfo, Signal, Signal), AppError> {
    let env_ = RuntimeEnv::from_env()?;
    let pool = db::open(&env_.db_path).await?;

    let cfg = db::get_bot(&pool, bot_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("bot id {bot_id} bulunamadı")))?
        .to_config()?;

    if cfg.strategy != Strategy::Harvest {
        return Err(AppError::Config(format!(
            "strategy {:?} aktif değil; doc §11 yalnız 'harvest' destekler",
            cfg.strategy
        )));
    }

    let creds = match cfg.run_mode {
        RunMode::Live => {
            let c = db::get_credentials(&pool, bot_id)
                .await?
                .ok_or(AppError::MissingCredentials { bot_id })?;
            // Polymarket EIP-712: yalnızca 0 (EOA), 1 (POLY_PROXY), 2 (POLY_GNOSIS_SAFE).
            if !matches!(c.signature_type, 0..=2) {
                return Err(AppError::Config(format!(
                    "bot {bot_id}: signature_type {} geçersiz (0|1|2 olmalı)",
                    c.signature_type
                )));
            }
            // type 1/2 için funder zorunlu.
            if matches!(c.signature_type, 1..=2) && c.funder.as_deref().unwrap_or("").is_empty() {
                return Err(AppError::Config(format!(
                    "bot {bot_id}: signature_type {} için 'funder' adresi zorunlu",
                    c.signature_type
                )));
            }
            Some(c)
        }
        RunMode::Dryrun => None,
    };

    db::set_bot_state(&pool, bot_id, "RUNNING").await?;

    let slug = parse_slug_or_prefix(&cfg.slug_pattern)?;

    let http = shared_http_client();
    let gamma = GammaClient::new(http.clone(), env_.gamma_base_url.clone());
    let clob = creds.as_ref().map(|c| {
        Arc::new(ClobClient::new(
            http.clone(),
            env_.clob_base_url.clone(),
            Some(c.clone()),
        ))
    });
    let executor = match (clob.as_ref(), creds.as_ref()) {
        (Some(cl), Some(c)) => Executor::Live(LiveExecutor {
            client: cl.clone(),
            creds: c.clone(),
            chain_id: env_.polygon_chain_id,
            // ms → s; cooldown_threshold averaging GTC max yaşı için kullanıldığından
            // GTD timeout'u olarak da onu kullanıyoruz (doc §13).
            gtd_timeout_secs: cfg.cooldown_threshold / 1000,
            pool: pool.clone(),
        }),
        _ => Executor::DryRun(Simulator),
    };

    let signal_state = new_shared_state();
    tokio::spawn(tasks::binance_task(
        slug.asset.binance_symbol().to_string(),
        slug.interval,
        signal_state.clone(),
        bot_id,
    ));
    tokio::spawn(tasks::heartbeat_task(tasks::heartbeat_path(
        &env_.heartbeat_dir,
        bot_id,
    )));
    if let Some(cl) = clob.as_ref() {
        tokio::spawn(tasks::clob_heartbeat_task(cl.clone()));
    }

    let sigterm =
        signal(SignalKind::terminate()).map_err(|e| AppError::Config(format!("sigterm: {e}")))?;
    let sigint =
        signal(SignalKind::interrupt()).map_err(|e| AppError::Config(format!("sigint: {e}")))?;

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
        },
        slug,
        sigterm,
        sigint,
    ))
}
