//! Bot binary — supervisor tarafından `--bot-id <id>` ile spawn edilir.
//!
//! Tüm iş mantığı `baiter_pro::bot` modülünde; bu dosya yalnız:
//! 1) tracing/rustls init,
//! 2) `bot::run()` çağrısı,
//! 3) hata yakalama + frontend Error event'i.

use std::env;

use baiter_pro::bot;
use baiter_pro::ipc::{self, FrontendEvent};
use baiter_pro::time::now_ms;

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

    if let Err(e) = bot::run().await {
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
