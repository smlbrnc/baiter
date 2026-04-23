//! Supervisor — bot süreç spawn + lifecycle + stdout köprüsü (§1, §5.1, §18).
//!
//! Her bot ayrı `Child` (PID izolasyonu); stdout satırları `[[EVENT]]`
//! prefix'ine göre SSE kanalı veya logs tablosuna yönlendirilir. Crash'te
//! exponential backoff (1s..60s); stop'ta `BotHandle::shutdown` oneshot
//! ile `kill_on_drop` SIGKILL.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::time::sleep;

use crate::config::RuntimeEnv;
use crate::db;
use crate::error::AppError;
use crate::ipc::{parse_event_line, FrontendEvent, EVENT_PREFIX};

/// Supervisor'un paylaşılan state'i (axum router + bot süreçleri).
pub struct AppState {
    pub pool: SqlitePool,
    pub env: RuntimeEnv,
    pub events: broadcast::Sender<FrontendEvent>,
    pub children: Mutex<HashMap<i64, BotHandle>>,
}

pub struct BotHandle {
    pub shutdown: oneshot::Sender<()>,
}

impl AppState {
    pub fn new(pool: SqlitePool, env: RuntimeEnv) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(1024);
        Arc::new(Self {
            pool,
            env,
            events: tx,
            children: Mutex::new(HashMap::new()),
        })
    }
}

/// Bir bot'u başlat — zaten çalışıyorsa no-op.
pub async fn start_bot(state: Arc<AppState>, bot_id: i64) -> Result<(), AppError> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    {
        let mut children = state.children.lock().await;
        if children.contains_key(&bot_id) {
            return Ok(());
        }
        children.insert(bot_id, BotHandle { shutdown: shutdown_tx });
    }

    let st = state.clone();
    tokio::spawn(async move { run_bot_with_backoff(st, bot_id, shutdown_rx).await });
    Ok(())
}

/// Bot'u durdur — child SIGKILL ile sonlandırılır, state STOPPED'e set edilir.
pub async fn stop_bot(state: Arc<AppState>, bot_id: i64) -> Result<(), AppError> {
    let handle = state.children.lock().await.remove(&bot_id);
    if let Some(h) = handle {
        let _ = h.shutdown.send(());
    }
    let _ = db::set_bot_state(&state.pool, bot_id, "STOPPED").await;
    Ok(())
}

async fn run_bot_with_backoff(
    state: Arc<AppState>,
    bot_id: i64,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    const MAX_BACKOFF: Duration = Duration::from_secs(60);
    let mut backoff = Duration::from_secs(1);

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                tracing::info!(bot_id, "supervisor shutdown requested");
                return;
            }
            res = spawn_once(state.clone(), bot_id) => match res {
                Ok(0) => {
                    tracing::info!(bot_id, "bot exited cleanly");
                    let _ = db::set_bot_state(&state.pool, bot_id, "STOPPED").await;
                    return;
                }
                Ok(code) => {
                    tracing::warn!(bot_id, exit_code = code, ?backoff, "bot crashed");
                }
                Err(e) => {
                    tracing::error!(bot_id, error = %e, ?backoff, "spawn failed");
                }
            }
        }

        let _ = db::insert_log(
            &state.pool,
            Some(bot_id),
            "error",
            &format!("bot crashed, restarting in {backoff:?}"),
        )
        .await;

        tokio::select! {
            _ = &mut shutdown_rx => return,
            _ = sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// Tek bir bot process'i spawn eder; exit code döner.
async fn spawn_once(state: Arc<AppState>, bot_id: i64) -> Result<i32, AppError> {
    let mut child = Command::new(&state.env.bot_binary)
        .arg("--bot-id")
        .arg(bot_id.to_string())
        .env("BAITER_BOT_ID", bot_id.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    tracing::info!(bot_id, pid = child.id(), "bot spawned");
    let _ = db::set_bot_state(&state.pool, bot_id, "RUNNING").await;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Config("stdout pipe missing".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Config("stderr pipe missing".into()))?;

    let s_out = state.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            handle_stdout_line(&s_out, bot_id, &line).await;
        }
    });

    let s_err = state.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = db::insert_log(&s_err.pool, Some(bot_id), "error", &line).await;
        }
    });

    let status = child.wait().await?;
    Ok(status.code().unwrap_or(-1))
}

async fn handle_stdout_line(state: &AppState, bot_id: i64, line: &str) {
    if let Some(rest) = line.strip_prefix(EVENT_PREFIX) {
        match parse_event_line(line) {
            Some(ev) => {
                let _ = state.events.send(ev);
            }
            None => tracing::warn!(bot_id, payload = rest, "event parse failed"),
        }
        return;
    }
    if line.is_empty() {
        return;
    }
    let level = detect_log_level(line);
    let _ = db::insert_log(&state.pool, Some(bot_id), level, line).await;
}

/// Tracing compact formatı satır başına `INFO`/`WARN`/`ERROR` token koyar
/// (örn. `WARN ws error...`); diğer her şey `info`.
fn detect_log_level(line: &str) -> &'static str {
    match line.split_whitespace().next().unwrap_or("") {
        "ERROR" => "error",
        "WARN" => "warn",
        _ => "info",
    }
}

/// Açılışta previously RUNNING botları otomatik yeniden başlatır.
pub async fn restart_previously_running(state: Arc<AppState>) {
    let bots = match db::list_bots(&state.pool).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error=%e, "list_bots failed");
            return;
        }
    };
    for b in bots.into_iter().filter(|b| b.state == "RUNNING") {
        tracing::info!(bot_id = b.id, "auto-restart previously running bot");
        let _ = start_bot(state.clone(), b.id).await;
    }
}
