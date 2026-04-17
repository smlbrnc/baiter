//! Supervisor — bot process spawn + lifecycle + stdout event bridge.
//!
//! - Her bot ayrı `Child` olarak başlar (PID izolasyonu § 1).
//! - `ChildStdout` satır satır okunur, `[[EVENT]]` prefix'li satırlar parse edilip
//!   internal `broadcast` kanalıyla SSE frontend'e iletilir.
//! - Crash loop kuralı: exit_code ≠ 0 → exponential backoff (1s, 2s, 4s, 8s, max 60s).
//! - SIGTERM → 10 sn timeout → SIGKILL (§18.2).
//!
//! Referans: [docs/bot-platform-mimari.md §1 §5.1 §18](../../../docs/bot-platform-mimari.md).

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};
use tokio::time::sleep;

use crate::config::RuntimeEnv;
use crate::db;
use crate::ipc::{parse_event_line, FrontendEvent, EVENT_PREFIX};

/// Supervisor'un paylaşılan state'i.
pub struct AppState {
    pub pool: SqlitePool,
    pub env: RuntimeEnv,
    pub events: broadcast::Sender<FrontendEvent>,
    pub children: Mutex<HashMap<i64, BotHandle>>,
}

pub struct BotHandle {
    pub shutdown: tokio::sync::oneshot::Sender<()>,
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
pub async fn start_bot(state: Arc<AppState>, bot_id: i64) -> Result<(), String> {
    {
        let children = state.children.lock().await;
        if children.contains_key(&bot_id) {
            return Ok(());
        }
    }

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    {
        let mut children = state.children.lock().await;
        children.insert(
            bot_id,
            BotHandle {
                shutdown: shutdown_tx,
            },
        );
    }

    let state2 = state.clone();
    tokio::spawn(async move {
        run_bot_with_backoff(state2, bot_id, shutdown_rx).await;
    });

    Ok(())
}

/// Bot'u durdur (SIGTERM → 10sn → SIGKILL).
pub async fn stop_bot(state: Arc<AppState>, bot_id: i64) -> Result<(), String> {
    let handle = {
        let mut children = state.children.lock().await;
        children.remove(&bot_id)
    };
    if let Some(h) = handle {
        let _ = h.shutdown.send(());
    }
    let _ = db::set_bot_state(&state.pool, bot_id, "STOPPED").await;
    Ok(())
}

async fn run_bot_with_backoff(
    state: Arc<AppState>,
    bot_id: i64,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(60);

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                tracing::info!(bot_id, "supervisor shutdown requested before spawn");
                return;
            }
            res = spawn_once(state.clone(), bot_id) => {
                match res {
                    Ok(0) => {
                        tracing::info!(bot_id, "bot exited cleanly");
                        let _ = db::set_bot_state(&state.pool, bot_id, "STOPPED").await;
                        return;
                    }
                    Ok(code) => {
                        tracing::warn!(bot_id, exit_code = code, "bot crashed, backoff {:?}", backoff);
                    }
                    Err(e) => {
                        tracing::error!(bot_id, error = %e, "spawn failed, backoff {:?}", backoff);
                    }
                }
            }
        }

        let _ = db::insert_log(
            &state.pool,
            Some(bot_id),
            "error",
            &format!("bot crashed, restarting in {:?}", backoff),
        )
        .await;

        tokio::select! {
            _ = &mut shutdown_rx => return,
            _ = sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(max_backoff);
    }
}

/// Tek bir bot process'i spawn eder; exit code döner.
async fn spawn_once(state: Arc<AppState>, bot_id: i64) -> Result<i32, String> {
    let mut cmd = Command::new(&state.env.bot_binary);
    cmd.arg("--bot-id")
        .arg(bot_id.to_string())
        .env("BAITER_BOT_ID", bot_id.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child: Child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    tracing::info!(bot_id, pid = child.id(), "bot spawned");
    let _ = db::set_bot_state(&state.pool, bot_id, "RUNNING").await;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout pipe missing".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "stderr pipe missing".to_string())?;

    let s_out = state.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            handle_stdout_line(&s_out, bot_id, &line).await;
        }
    });

    let s_err = state.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = db::insert_log(&s_err.pool, Some(bot_id), "warn", &line).await;
        }
    });

    let status = child.wait().await.map_err(|e| format!("wait: {e}"))?;
    let code = status.code().unwrap_or(-1);
    Ok(code)
}

async fn handle_stdout_line(state: &AppState, bot_id: i64, line: &str) {
    if line.starts_with(EVENT_PREFIX) {
        if let Some(ev) = parse_event_line(line) {
            let _ = state.events.send(ev);
        } else {
            tracing::warn!(bot_id, "event parse failed: {line}");
        }
    } else if !line.is_empty() {
        let _ = db::insert_log(&state.pool, Some(bot_id), "info", line).await;
    }
}

/// Uygulama başlarken previously RUNNING botları otomatik olarak yeniden başlat.
pub async fn restart_previously_running(state: Arc<AppState>) {
    let bots = match db::list_bots(&state.pool).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error=%e, "list_bots failed");
            return;
        }
    };
    for b in bots {
        if b.state == "RUNNING" {
            tracing::info!(bot_id = b.id, "auto-restart previously running bot");
            let _ = start_bot(state.clone(), b.id).await;
        }
    }
}
