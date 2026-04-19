//! Tek market penceresinin yönetimi.
//!
//! - **T-15 ön hazırlığı (doc §4):** [`wait_for_t_minus_15`] pencere
//!   başlangıcından 15 sn önce uyandırır; Gamma fetch + book ön-fetch + Market
//!   WS abonelik kurulumu burada yapılır.
//! - **T=0:** gerçek trading loop ([`tick`]) başlar.

use std::time::Duration;

use tokio::signal::unix::Signal;
use tokio::sync::mpsc;
use tokio::time::{interval as tokio_interval, sleep};

use crate::db;
use crate::engine::{Executor, MarketSession};
use crate::error::AppError;
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::gamma::GammaMarket;
use crate::polymarket::ws::{run_market_ws, run_user_ws, PolymarketEvent};
use crate::slug::SlugInfo;
use crate::time::{now_secs, t_minus_15};

use super::ctx::Ctx;
use super::{event, persist, shutdown, tick, zone};

/// Tek bir market penceresini yönetir.
///
/// - `Ok(None)` → pencere normal bitti, bir sonrakine geç.
/// - `Ok(Some(reason))` → SIGTERM/SIGINT, graceful shutdown iste.
/// - `Err(_)` → fatal (üst katman bot'u yeniden başlatır).
pub async fn run_window(
    ctx: &Ctx,
    slug: SlugInfo,
    sigterm: &mut Signal,
    sigint: &mut Signal,
) -> Result<Option<&'static str>, AppError> {
    let label = ctx.bot_id.to_string();
    let slug_str = slug.to_slug();
    ipc::log_line(&label, format!("Target market: {slug_str}"));

    if let Some(reason) = wait_for_t_minus_15(slug.ts, sigterm, sigint, &label).await {
        return Ok(Some(reason));
    }

    ipc::log_line(&label, format!("📡 Fetching market: {slug_str}"));
    let market = ctx.gamma.get_market_by_slug(&slug_str).await?;
    let (yes_id, no_id) = market.parse_token_ids()?;
    let condition_id = market.condition_id.clone().unwrap_or_default();
    let (start_ts, end_ts) = (slug.ts, slug.end_ts());

    if let Some(q) = market.question.as_deref() {
        ipc::log_line(&label, format!("✅ Found market: {q}"));
    }
    ipc::log_line(
        &label,
        format!("Window: {} UTC - {} UTC", fmt_utc(start_ts), fmt_utc(end_ts)),
    );
    ipc::log_line(&label, format!("    UP:   {yes_id}"));
    ipc::log_line(&label, format!("    DOWN: {no_id}"));

    let session_id = db::sessions::upsert_market_session(
        &ctx.pool,
        ctx.bot_id,
        &slug_str,
        start_ts as i64,
        end_ts as i64,
    )
    .await?;

    db::sessions::update_market_session_meta(
        &ctx.pool,
        session_id,
        &condition_id,
        &yes_id,
        &no_id,
        market.tick_size.unwrap_or(0.01),
        market.minimum_order_size.unwrap_or(5.0),
    )
    .await?;

    let mut sess = build_session(
        ctx,
        slug,
        &market,
        &yes_id,
        &no_id,
        &condition_id,
        start_ts,
        end_ts,
        session_id,
    );

    ipc::emit(&FrontendEvent::SessionOpened {
        bot_id: ctx.bot_id,
        slug: slug_str.clone(),
        start_ts,
        end_ts,
        yes_token_id: yes_id.clone(),
        no_token_id: no_id.clone(),
    });

    ipc::log_line(&label, "🔌 Connecting to Market WebSocket...");
    let (ev_tx, mut ev_rx) = mpsc::channel::<PolymarketEvent>(512);
    let market_ws = tokio::spawn(run_market_ws(
        ctx.env_.clob_ws_base.clone(),
        vec![yes_id, no_id],
        ev_tx.clone(),
    ));
    let user_ws = ctx.creds.as_ref().map(|c| {
        ipc::log_line(&label, "🔌 Connecting to User WebSocket...");
        tokio::spawn(run_user_ws(
            ctx.env_.clob_ws_base.clone(),
            c.clone(),
            vec![condition_id],
            ev_tx,
        ))
    });

    // T=0'a kadar bekle (T-15 ön hazırlığında biriken WS event'leri loop
    // başladığında işlenir).
    let now = now_secs();
    if now < start_ts {
        sleep(Duration::from_secs(start_ts - now)).await;
    }

    ipc::log_line(
        &label,
        format!(
            "🚀 Starting trading loop (strategy: {:?}, mode: {:?})",
            ctx.cfg.strategy, ctx.cfg.run_mode
        ),
    );

    let mut tick_timer = tokio_interval(Duration::from_millis(500));
    let mut zone_timer = tokio_interval(Duration::from_secs(5));
    let mut pnl_timer = tokio_interval(Duration::from_secs(5));
    let mut last_zone: Option<String> = None;
    let mut last_book_snapshot: Option<(f64, f64, f64, f64)> = None;

    let result: Option<&'static str> = loop {
        tokio::select! {
            _ = sigterm.recv() => break Some("sigterm"),
            _ = sigint.recv()  => break Some("sigint"),
            Some(ev) = ev_rx.recv() => {
                event::handle_event(&mut sess, &ctx.pool, ctx.bot_id, ctx.cfg.run_mode, ev).await;
            }
            _ = tick_timer.tick() => tick::tick(ctx, &mut sess).await,
            _ = zone_timer.tick() => {
                zone::emit_zone_signal(ctx, &sess, slug, &mut last_zone, &mut last_book_snapshot).await;
                if now_secs() >= sess.end_ts {
                    break None;
                }
            }
            _ = pnl_timer.tick() => persist::snapshot_pnl(&ctx.pool, &sess),
        }
    };

    market_ws.abort();
    if let Some(h) = user_ws {
        h.abort();
    }
    if matches!(ctx.executor, Executor::Live(_)) {
        shutdown::cancel_all_open(ctx, "window boundary").await;
    }
    if result.is_none() {
        ipc::log_line(
            &label,
            "🏁 Market window complete, transitioning to next market...",
        );
    }
    Ok(result)
}

/// Pencereyi bir interval ileri kaydır; şimdi geride kalmışsak güncel sınıra snap.
pub fn next_window(mut slug: SlugInfo) -> SlugInfo {
    let secs = slug.interval.seconds();
    slug.ts += secs;
    let snap = (now_secs() / secs) * secs;
    if slug.ts < snap {
        slug.ts = snap;
    }
    slug
}

/// Pencere başlangıcından **15 sn önce** uyanır (`time::t_minus_15`).
///
/// Doc §4: Gamma + book ön-fetch + Market WS abonelik kurulumu T-15'te
/// başlatılır; gerçek trading loop T=0'da başlar. Bu fonksiyon yalnız
/// `sleep`'i sinyalle birlikte yönetir.
async fn wait_for_t_minus_15(
    market_start_ts: u64,
    sigterm: &mut Signal,
    sigint: &mut Signal,
    label: &str,
) -> Option<&'static str> {
    let target = t_minus_15(market_start_ts);
    let now = now_secs();
    if now >= target {
        return None;
    }
    let wait = Duration::from_secs(target - now);
    ipc::log_line(
        label,
        format!(
            "⏳ T-15 ön hazırlığı için {}s bekleniyor (start_ts={market_start_ts})",
            wait.as_secs()
        ),
    );
    tokio::select! {
        _ = sleep(wait) => None,
        _ = sigterm.recv() => Some("sigterm"),
        _ = sigint.recv()  => Some("sigint"),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_session(
    ctx: &Ctx,
    slug: SlugInfo,
    market: &GammaMarket,
    yes_id: &str,
    no_id: &str,
    condition_id: &str,
    start_ts: u64,
    end_ts: u64,
    market_session_id: i64,
) -> MarketSession {
    MarketSession {
        yes_token_id: yes_id.to_string(),
        no_token_id: no_id.to_string(),
        condition_id: condition_id.to_string(),
        tick_size: market.tick_size.unwrap_or(0.01),
        api_min_order_size: market.minimum_order_size.unwrap_or(5.0),
        neg_risk: market.neg_risk.unwrap_or(false),
        start_ts,
        end_ts,
        market_session_id,
        ..MarketSession::new(ctx.bot_id, slug.to_slug(), &ctx.cfg)
    }
}

/// Unix saniyesini `YYYY-MM-DD HH:MM:SS` (UTC) string'ine çevirir.
fn fmt_utc(ts: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}
