//! `DB_PATH` altındaki SQLite dosyasını sil ve `migrations/` şemasını sıfırdan uygula.
//!
//! Kullanım: repo kökünden `.env` ile `cargo run --bin reset-db`

use baiter_pro::config::RuntimeEnv;
use baiter_pro::db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,sqlx=warn")
            }),
        )
        .with_target(false)
        .init();

    let env = RuntimeEnv::from_env()?;
    let path = env.db_path.as_str();

    for suffix in ["", "-wal", "-shm"] {
        let p = format!("{path}{suffix}");
        match std::fs::remove_file(&p) {
            Ok(()) => tracing::info!(file = %p, "removed"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("remove {p}: {e}").into()),
        }
    }

    let pool = db::open(path).await?;
    db::run_migrations(&pool).await?;
    tracing::info!(db = %path, "migrations applied");
    Ok(())
}
