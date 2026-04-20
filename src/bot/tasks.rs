//! Arka plan task'ları — heartbeat, CLOB ping, Binance signal.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::fs;
use tokio::time::interval;

use crate::polymarket::ClobClient;
use crate::time::now_ms;

/// `<heartbeat_dir>/<bot_id>.heartbeat` yolu.
pub fn heartbeat_path(dir: &str, bot_id: i64) -> PathBuf {
    let mut p = PathBuf::from(dir);
    p.push(format!("{bot_id}.heartbeat"));
    p
}

/// Her 5 sn dosyaya unix-ms yazar (supervisor sağlık takibi için).
pub async fn heartbeat_task(path: PathBuf) {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            tracing::warn!(path=%parent.display(), error=%e, "heartbeat dir create failed");
        }
    }
    let mut tick = interval(Duration::from_secs(5));
    loop {
        tick.tick().await;
        if let Err(e) = fs::write(&path, now_ms().to_string().as_bytes()).await {
            tracing::warn!(path=%path.display(), error=%e, "heartbeat write failed");
        }
    }
}

/// CLOB session'ını canlı tutmak için 5 sn'lik ping (yalnız Live mod).
pub async fn clob_heartbeat_task(clob: Arc<ClobClient>) {
    let mut tick = interval(Duration::from_secs(5));
    loop {
        tick.tick().await;
        if let Err(e) = clob.heartbeat_once().await {
            tracing::warn!(error=%e, "clob heartbeat failed");
        }
    }
}

