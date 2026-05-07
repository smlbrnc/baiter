//! Tek market penceresinin yönetimi: T-15 ön hazırlık (Gamma + WS), T=0 trading loop.

use std::time::Duration;

use tokio::signal::unix::Signal;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};

use crate::db;
use crate::engine::{Executor, MarketSession};
use crate::error::AppError;
use crate::ipc::{self, FrontendEvent};
use crate::polymarket::{run_market_ws, run_user_ws, PolymarketEvent, WsChannels};
use crate::rtds;
use crate::slug::SlugInfo;
use crate::time::{now_secs, t_minus_15};

use super::ctx::Ctx;
use super::signal::observed_snapshot;
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

    let result = run_trading_loop(
        ctx,
        session,
        streams.event_rx,
        streams.book_rx,
        sigterm,
        sigint,
    )
    .await;

    cleanup_window(ctx, streams.market_ws, streams.user_ws, &label, result).await;
    Ok(result)
}

struct MarketMeta {
    up_id: String,
    down_id: String,
    condition_id: String,
    tick_size: f64,
    api_min_order_size: f64,
    neg_risk: bool,
    start_ts: u64,
    end_ts: u64,
}

/// T-15 ön hazırlığı: Gamma → DB session → fee rate → `MarketSession`.
async fn prepare_window(
    ctx: &Ctx,
    slug: SlugInfo,
    slug_str: &str,
    label: &str,
) -> Result<MarketSession, AppError> {
    let meta = fetch_market_meta(ctx, slug, slug_str, label).await?;
    let session_id = persist_session_setup(ctx, slug_str, &meta).await?;
    let fee_rate = resolve_fee_rate(ctx, &meta.condition_id, label).await?;
    let owner_uuid = ctx.creds.as_ref().map(|c| c.poly_api_key.clone());

    Ok(MarketSession {
        up_token_id: meta.up_id,
        down_token_id: meta.down_id,
        condition_id: meta.condition_id,
        tick_size: meta.tick_size,
        api_min_order_size: meta.api_min_order_size,
        neg_risk: meta.neg_risk,
        start_ts: meta.start_ts,
        end_ts: meta.end_ts,
        market_session_id: session_id,
        fee_rate,
        owner_uuid,
        ..MarketSession::new(ctx.bot_id, ctx.bot_label.clone(), slug.to_slug(), &ctx.cfg)
    })
}

async fn fetch_market_meta(
    ctx: &Ctx,
    slug: SlugInfo,
    slug_str: &str,
    label: &str,
) -> Result<MarketMeta, AppError> {
    ipc::log_line(label, format!("📡 Fetching market: {slug_str}"));
    let market = ctx.gamma.get_market_by_slug(slug_str).await?;
    let (up_id, down_id) = market.parse_token_ids()?;
    let condition_id = market
        .condition_id
        .clone()
        .ok_or_else(|| AppError::Gamma(format!("conditionId eksik (slug={slug_str})")))?;
    let tick_size = market
        .tick_size
        .ok_or_else(|| AppError::Gamma(format!("orderPriceMinTickSize eksik (slug={slug_str})")))?;
    let api_min_order_size = market
        .min_order_size
        .ok_or_else(|| AppError::Gamma(format!("orderMinSize eksik (slug={slug_str})")))?;
    let neg_risk = market.neg_risk.unwrap_or(false);
    let (start_ts, end_ts) = (slug.ts, slug.end_ts());

    if let Some(q) = market.question.as_deref() {
        ipc::log_line(label, format!("✅ Found market: {q}"));
    }
    ipc::log_line(
        label,
        format!("Window: {} UTC - {} UTC", fmt_utc(start_ts), fmt_utc(end_ts)),
    );
    ipc::log_line(label, format!("    UP:   {up_id}"));
    ipc::log_line(label, format!("    DOWN: {down_id}"));

    Ok(MarketMeta {
        up_id,
        down_id,
        condition_id,
        tick_size,
        api_min_order_size,
        neg_risk,
        start_ts,
        end_ts,
    })
}

async fn persist_session_setup(
    ctx: &Ctx,
    slug_str: &str,
    meta: &MarketMeta,
) -> Result<i64, AppError> {
    let session_id = db::sessions::upsert_market_session(
        &ctx.pool,
        ctx.bot_id,
        slug_str,
        meta.start_ts as i64,
        meta.end_ts as i64,
    )
    .await?;

    rtds::reset_window(&ctx.rtds_state, meta.start_ts * 1000).await;

    db::sessions::update_market_session_meta(
        &ctx.pool,
        session_id,
        &meta.condition_id,
        &meta.up_id,
        &meta.down_id,
        meta.tick_size,
        meta.api_min_order_size,
    )
    .await?;

    ipc::emit(&FrontendEvent::SessionOpened {
        bot_id: ctx.bot_id,
        slug: slug_str.to_string(),
        start_ts: meta.start_ts,
        end_ts: meta.end_ts,
        up_token_id: meta.up_id.clone(),
        down_token_id: meta.down_id.clone(),
    });

    Ok(session_id)
}

/// Live → `get_taker_fee`; DryRun → `0.0` (`DRYRUN_FEE_RATE` persist tarafında).
async fn resolve_fee_rate(ctx: &Ctx, condition_id: &str, label: &str) -> Result<f64, AppError> {
    match &ctx.executor {
        Executor::Live(live) => {
            let fee = live.client.get_taker_fee(condition_id).await?;
            ipc::log_line(
                label,
                format!(
                    "CLOB taker fee rate={:.4} taker_only={}",
                    fee.rate, fee.taker_only
                ),
            );
            Ok(fee.rate)
        }
        Executor::DryRun(_) => Ok(0.0),
    }
}

struct WindowStreams {
    event_rx: mpsc::Receiver<PolymarketEvent>,
    book_rx: mpsc::Receiver<PolymarketEvent>,
    market_ws: tokio::task::JoinHandle<()>,
    user_ws: Option<tokio::task::JoinHandle<()>>,
}

fn connect_streams(ctx: &Ctx, session: &MarketSession, label: &str) -> WindowStreams {
    ipc::log_line(label, "🔌 Connecting to Market WebSocket...");
    let (event_tx, event_rx) = mpsc::channel::<PolymarketEvent>(4096);
    let (book_tx, book_rx) = mpsc::channel::<PolymarketEvent>(2048);
    let chans = WsChannels { book_tx, event_tx };
    let ws_base = ctx.env_.clob_ws_base.clone();
    let market_ws = tokio::spawn(run_market_ws(
        ws_base.clone(),
        vec![session.up_token_id.clone(), session.down_token_id.clone()],
        chans.clone(),
    ));
    let user_ws = ctx.creds.as_ref().map(|c| {
        ipc::log_line(label, "🔌 Connecting to User WebSocket...");
        tokio::spawn(run_user_ws(
            ws_base,
            c.clone(),
            vec![session.condition_id.clone()],
            chans,
        ))
    });
    WindowStreams {
        event_rx,
        book_rx,
        market_ws,
        user_ws,
    }
}

/// T=0'a kadar bekle; T-15 hazırlığında biriken WS event'leri loop başlayınca işlenir.
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

/// Trading döngüsü. `None` = pencere bitti, `Some(reason)` = sigterm/sigint.
async fn run_trading_loop(
    ctx: &Ctx,
    mut sess: MarketSession,
    mut event_rx: mpsc::Receiver<PolymarketEvent>,
    mut book_rx: mpsc::Receiver<PolymarketEvent>,
    sigterm: &mut Signal,
    sigint: &mut Signal,
) -> Option<&'static str> {
    let mut cadence = interval(Duration::from_secs(1));
    let mut rtds_open_persisted = false;

    loop {
        tokio::select! {
            biased;
            _ = sigterm.recv() => return Some("sigterm"),
            _ = sigint.recv()  => return Some("sigint"),
            Some(ev) = event_rx.recv() => {
                event::handle_event(&mut sess, &ctx.pool, ctx.cfg.run_mode, ev);
                tick::tick(ctx, &mut sess).await;
            }
            Some(ev) = book_rx.recv() => {
                let mut bba_changed = event::handle_event(&mut sess, &ctx.pool, ctx.cfg.run_mode, ev);
                while let Ok(more) = book_rx.try_recv() {
                    if event::handle_event(&mut sess, &ctx.pool, ctx.cfg.run_mode, more) {
                        bba_changed = true;
                    }
                }
                if bba_changed {
                    tick::tick(ctx, &mut sess).await;
                }
            }
            _ = cadence.tick() => {
                if ctx.cfg.run_mode == crate::types::RunMode::Dryrun {
                    event::run_passive_fills_dryrun(&mut sess, &ctx.pool);
                }
                let sig = observed_snapshot(ctx).await;
                zone::emit_frontend_snapshot(ctx, &sess, &sig);
                persist::snapshot_pnl(&ctx.pool, &sess);
                persist::snapshot_tick(ctx, &sess, &sig);
                if !rtds_open_persisted && sess.market_session_id > 0 {
                    rtds_open_persisted =
                        persist::maybe_persist_rtds_window_open(ctx, &sess).await;
                }
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

/// Pencere başlangıcından 15 sn önce uyanır (doc §4 T-15 hazırlığı).
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

fn fmt_utc(ts: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}
