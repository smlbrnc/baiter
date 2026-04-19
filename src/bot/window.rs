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
use crate::engine::MarketSession;
use crate::error::AppError;
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::{run_market_ws, run_user_ws, PolymarketEvent};
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

    let session = prepare_window(ctx, slug, &slug_str, &label).await?;
    let streams = connect_streams(ctx, &session, &label);

    wait_for_t_zero(session.start_ts).await;
    log_loop_start(ctx, &label);

    let result = run_trading_loop(ctx, slug, session, streams.ev_rx, sigterm, sigint).await;

    cleanup_window(ctx, streams.market_ws, streams.user_ws, &label, result).await;
    Ok(result)
}

/// T-15 ön hazırlığı: Gamma fetch → DB session upsert → MarketSession build →
/// SessionOpened IPC. WS bağlantısı [`connect_streams`] içinde kurulur.
async fn prepare_window(
    ctx: &Ctx,
    slug: SlugInfo,
    slug_str: &str,
    label: &str,
) -> Result<MarketSession, AppError> {
    ipc::log_line(label, format!("📡 Fetching market: {slug_str}"));
    let market = ctx.gamma.get_market_by_slug(slug_str).await?;
    let (yes_id, no_id) = market.parse_token_ids()?;
    let condition_id = market.condition_id.clone().unwrap_or_default();
    let (start_ts, end_ts) = (slug.ts, slug.end_ts());

    if let Some(q) = market.question.as_deref() {
        ipc::log_line(label, format!("✅ Found market: {q}"));
    }
    ipc::log_line(
        label,
        format!("Window: {} UTC - {} UTC", fmt_utc(start_ts), fmt_utc(end_ts)),
    );
    ipc::log_line(label, format!("    UP:   {yes_id}"));
    ipc::log_line(label, format!("    DOWN: {no_id}"));

    let session_id = db::sessions::upsert_market_session(
        &ctx.pool,
        ctx.bot_id,
        slug_str,
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

    ipc::emit(&FrontendEvent::SessionOpened {
        bot_id: ctx.bot_id,
        slug: slug_str.to_string(),
        start_ts,
        end_ts,
        yes_token_id: yes_id.clone(),
        no_token_id: no_id.clone(),
    });

    Ok(MarketSession {
        yes_token_id: yes_id,
        no_token_id: no_id,
        condition_id,
        tick_size: market.tick_size.unwrap_or(0.01),
        api_min_order_size: market.minimum_order_size.unwrap_or(5.0),
        neg_risk: market.neg_risk.unwrap_or(false),
        start_ts,
        end_ts,
        market_session_id: session_id,
        ..MarketSession::new(ctx.bot_id, slug.to_slug(), &ctx.cfg)
    })
}

/// Aktif WS bağlantıları + event channel.
struct WindowStreams {
    ev_rx: mpsc::Receiver<PolymarketEvent>,
    market_ws: tokio::task::JoinHandle<()>,
    user_ws: Option<tokio::task::JoinHandle<()>>,
}

/// Market WS + (varsa) User WS task'larını başlatır, mpsc channel kurar.
fn connect_streams(ctx: &Ctx, session: &MarketSession, label: &str) -> WindowStreams {
    ipc::log_line(label, "🔌 Connecting to Market WebSocket...");
    // Buffer 2048: yoğun book + user event burst'lerinde drop oranını düşürür;
    // PolymarketEvent ~200B → ~400KB worst case bellek (ihmal edilebilir).
    let (ev_tx, ev_rx) = mpsc::channel::<PolymarketEvent>(2048);
    let market_ws = tokio::spawn(run_market_ws(
        ctx.env_.clob_ws_base.clone(),
        vec![session.yes_token_id.clone(), session.no_token_id.clone()],
        ev_tx.clone(),
    ));
    let user_ws = ctx.creds.as_ref().map(|c| {
        ipc::log_line(label, "🔌 Connecting to User WebSocket...");
        tokio::spawn(run_user_ws(
            ctx.env_.clob_ws_base.clone(),
            c.clone(),
            vec![session.condition_id.clone()],
            ev_tx,
        ))
    });
    WindowStreams {
        ev_rx,
        market_ws,
        user_ws,
    }
}

/// T=0'a kadar bekle. T-15 hazırlığında biriken WS event'leri trading loop
/// başladığında işlenir.
async fn wait_for_t_zero(start_ts: u64) {
    let now = now_secs();
    if now < start_ts {
        sleep(Duration::from_secs(start_ts - now)).await;
    }
}

fn log_loop_start(ctx: &Ctx, label: &str) {
    ipc::log_line(
        label,
        format!(
            "🚀 Starting trading loop (strategy: {:?}, mode: {:?})",
            ctx.cfg.strategy, ctx.cfg.run_mode
        ),
    );
}

/// Asıl trading döngüsü — `select!` ile WS event / tick / zone / pnl / sinyal.
///
/// Dönüş: `None` ⇒ pencere doğal sonu (sonraki window'a geç),
/// `Some(reason)` ⇒ sigterm/sigint (graceful shutdown).
async fn run_trading_loop(
    ctx: &Ctx,
    slug: SlugInfo,
    mut sess: MarketSession,
    mut ev_rx: mpsc::Receiver<PolymarketEvent>,
    sigterm: &mut Signal,
    sigint: &mut Signal,
) -> Option<&'static str> {
    // ⚡ Kural 1 — Critical Path Zero Block:
    //   [WS event] → handle_event(update_best) → tick::tick(decide+execute)
    // tek select! arm'ında zincirlenir; aralarında bekleme yok.
    // tick_timer (1 sn) safety net: WS event akışı yokken Binance signal
    // değişimleri için periyodik fallback.
    let mut tick_timer = tokio_interval(Duration::from_secs(1));
    let mut frontend_timer = tokio_interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = sigterm.recv() => return Some("sigterm"),
            _ = sigint.recv()  => return Some("sigint"),
            Some(ev) = ev_rx.recv() => {
                event::handle_event(&mut sess, &ctx.pool, ctx.cfg.run_mode, ev);
                tick::tick(ctx, &mut sess).await;
            }
            _ = tick_timer.tick() => tick::tick(ctx, &mut sess).await,
            _ = frontend_timer.tick() => {
                zone::emit_frontend_snapshot(ctx, &sess, slug).await;
                persist::snapshot_pnl(&ctx.pool, &sess);
                persist::snapshot_tick(ctx, &sess).await;
                if now_secs() >= sess.end_ts {
                    return None;
                }
            }
        }
    }
}

/// Pencere bitiminde WS task'larını abort + (Live ise) açık emirleri iptal et.
async fn cleanup_window(
    ctx: &Ctx,
    market_ws: tokio::task::JoinHandle<()>,
    user_ws: Option<tokio::task::JoinHandle<()>>,
    label: &str,
    result: Option<&'static str>,
) {
    market_ws.abort();
    if let Some(h) = user_ws {
        h.abort();
    }
    shutdown::cancel_all_open(ctx, "window boundary").await;
    if result.is_none() {
        ipc::log_line(
            label,
            "🏁 Market window complete, transitioning to next market...",
        );
    }
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

/// Unix saniyesini `YYYY-MM-DD HH:MM:SS` (UTC) string'ine çevirir.
fn fmt_utc(ts: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}
