//! Bot binary çekirdeği — alt modüllere dağıtılmış.
//!
//! `src/bin/bot.rs` artık sadece `bot::run()` çağıran ince entry point;
//! sorumluluklar bu modüller arasında bölünmüştür:
//! - [`ctx`]      — `Ctx` (paylaşılan state), CLI parse, env/DB load.
//! - [`window`]   — `run_window`, T-15 ön hazırlığı, build_session, next_window.
//! - [`tick`]     — strateji tick + state transition logu + place/cancel logu.
//! - [`event`]    — Polymarket WS event handler dispatch.
//! - [`zone`]     — periyodik zone + signal + book snapshot logu/IPC.
//! - [`tasks`]    — heartbeat, CLOB heartbeat, Binance signal arka plan task'ları.
//! - [`shutdown`] — graceful shutdown + ortak `cancel_all_open` yardımcısı.
//!
//! Referans: [docs/bot-platform-mimari.md §4 §5 §13](../../../docs/bot-platform-mimari.md).

pub mod ctx;
pub mod event;
pub mod persist;
pub mod shutdown;
pub mod tasks;
pub mod tick;
pub mod window;
pub mod zone;

use crate::error::AppError;
use crate::ipc::{self, FrontendEvent};
use crate::time::now_ms;

pub use ctx::{parse_bot_id, Ctx};

/// Bot binary entry point — `cargo run --bin bot -- --bot-id N`.
pub async fn run() -> Result<(), AppError> {
    let bot_id = parse_bot_id()?;
    std::env::set_var("BAITER_BOT_ID", bot_id.to_string());

    let (ctx, mut slug, mut sigterm, mut sigint) = ctx::load(bot_id).await?;
    ipc::log_line(
        &bot_id.to_string(),
        format!(
            "Bot started — strategy={:?} mode={:?} order_usdc={} signal_weight={}",
            ctx.cfg.strategy, ctx.cfg.run_mode, ctx.cfg.order_usdc, ctx.cfg.signal_weight,
        ),
    );
    ipc::emit(&FrontendEvent::BotStarted {
        bot_id,
        name: ctx.cfg.name.clone(),
        slug: slug.to_slug(),
        ts_ms: now_ms(),
    });

    loop {
        match window::run_window(&ctx, slug, &mut sigterm, &mut sigint).await? {
            Some(reason) => {
                shutdown::graceful_shutdown(&ctx, reason).await;
                return Ok(());
            }
            None => slug = window::next_window(slug),
        }
    }
}
