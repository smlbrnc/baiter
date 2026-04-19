//! SQLite katmanı — alt modüllere bölünmüş.
//!
//! WAL mode init'te açılır. `sqlx::query!` compile-time doğrulama yerine
//! runtime `query_as` / `query` kullanılır (basitlik için).
//!
//! Referans: [docs/bot-platform-mimari.md §6-12 §9a §17](../../../docs/bot-platform-mimari.md).

use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::AppError;

pub mod bots;
pub mod credentials;
pub mod logs;
pub mod markets;
pub mod orders;
pub mod pnl;
pub mod sessions;
pub mod trades;

pub use bots::{delete_bot, get_bot, insert_bot, list_bots, set_bot_state, BotRow};
pub use credentials::{get_credentials, upsert_credentials};
pub use logs::{insert_log, recent_logs, LogRow};
pub use markets::upsert_market_resolved;
pub use orders::{upsert_order, OrderRecord};
pub use pnl::{insert_pnl_snapshot, latest_pnl_for_bot, PnlSnapshot};
pub use sessions::{
    latest_session_for_bot, update_market_session_meta, upsert_market_session, MarketSessionRow,
    SessionSummary,
};
pub use trades::{upsert_trade, TradeRecord};

/// Veritabanını aç (dosya yoksa oluştur) ve WAL mode etkinleştir.
pub async fn open(db_path: &str) -> Result<SqlitePool, AppError> {
    if let Some(parent) = Path::new(db_path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let opts = SqliteConnectOptions::from_str(db_path)
        .map_err(|e| AppError::Config(format!("sqlite connect options: {e}")))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    Ok(pool)
}

/// `migrations/` klasöründeki SQL dosyalarını sırasıyla çalıştır.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Fire-and-forget DB yazımı için tek noktadan helper (§⚡ Kural 4).
///
/// Verilen futureı `tokio::spawn` ile arkaplana atar; hata olursa `label`
/// etiketiyle `tracing::warn` basar. WS/event yolunda bloklamadan kalmak
/// için kullanılır.
pub fn spawn_db<F, T, E>(label: &'static str, fut: F)
where
    F: std::future::Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = fut.await {
            tracing::warn!(error=%e, "{label} failed");
        }
    });
}
