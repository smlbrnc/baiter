//! Supervisor binary — HTTP API + bot process orchestration.
//!
//! Çalıştırma:
//! ```bash
//! cargo run --bin supervisor
//! ```
//!
//! Referans: [docs/bot-platform-mimari.md §1 §2](../../../docs/bot-platform-mimari.md).

use std::net::SocketAddr;

use baiter_pro::api;
use baiter_pro::config::RuntimeEnv;
use baiter_pro::db;
use baiter_pro::supervisor::{restart_previously_running, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,hyper=warn,sqlx=warn")),
        )
        .with_target(false)
        .init();

    let env = RuntimeEnv::from_env()?;
    tracing::info!(port = env.port, db = %env.db_path, "supervisor starting");

    let pool = db::open(&env.db_path).await?;
    db::run_migrations(&pool).await?;

    let state = AppState::new(pool, env.clone());

    restart_previously_running(state.clone()).await;

    let router = api::router(state.clone());
    let addr: SocketAddr = format!("0.0.0.0:{}", env.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "http listener bound");

    axum::serve(listener, router).await?;
    Ok(())
}
