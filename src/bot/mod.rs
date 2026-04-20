//! Bot binary çekirdeği — `src/bin/bot.rs` ince bir `bot::run()` entry point'tir;
//! sorumluluklar alt modüller arasında bölünmüştür.

pub(crate) mod ctx;
pub(crate) mod event;
pub(crate) mod persist;
pub(crate) mod shutdown;
pub(crate) mod signal;
pub(crate) mod tasks;
pub(crate) mod tick;
pub(crate) mod window;
pub(crate) mod zone;

use crate::error::AppError;
use crate::ipc::{self, FrontendEvent};
use crate::time::now_ms;

/// Bot binary entry point — `cargo run --bin bot -- --bot-id N`.
pub async fn run() -> Result<(), AppError> {
    let bot_id = ctx::parse_bot_id()?;
    std::env::set_var("BAITER_BOT_ID", bot_id.to_string());

    let (ctx, mut slug, mut sigterm, mut sigint) = ctx::load(bot_id).await?;
    ipc::log_line(
        &bot_id.to_string(),
        format!(
            "Bot started — strategy={:?} mode={:?} order_usdc={}",
            ctx.cfg.strategy, ctx.cfg.run_mode, ctx.cfg.order_usdc,
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
