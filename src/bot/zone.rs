//! Periyodik (1 sn) frontend snapshot emit'i: BestBidAsk + SignalUpdate.
//! PnL snapshot'ı `bot/persist.rs` içinde aynı timer'dan çağrılır.

use crate::engine::MarketSession;
use crate::ipc::{self, FrontendEvent};
use crate::rtds;
use crate::slug::SlugInfo;
use crate::time::now_ms;

use super::ctx::Ctx;

/// 1 sn cadence: book fiyatlarını ve Binance sinyal skorunu frontend'e
/// push'lar. Değişim filtresi yok — frontend her saniye güncel snapshot alır.
pub async fn emit_frontend_snapshot(ctx: &Ctx, sess: &MarketSession, slug: SlugInfo) {
    let ts_ms = now_ms();

    ipc::emit(&FrontendEvent::BestBidAsk {
        bot_id: ctx.bot_id,
        yes_best_bid: sess.yes_best_bid,
        yes_best_ask: sess.yes_best_ask,
        no_best_bid: sess.no_best_bid,
        no_best_ask: sess.no_best_ask,
        ts_ms,
    });

    let (binance_score, bsi, ofi, cvd) = {
        let snap = ctx.signal_state.read().await;
        (snap.signal_score, snap.bsi, snap.ofi, snap.cvd)
    };
    let window_score = if ctx.cfg.strategy_params.rtds_enabled_or_default() {
        let rtds_snap = ctx.rtds_state.read().await;
        rtds::window_delta_score(
            rtds_snap.window_delta_bps,
            rtds::interval_scale(sess.end_ts.saturating_sub(sess.start_ts)),
        )
    } else {
        5.0
    };
    let signal_score = rtds::composite_score(
        window_score,
        binance_score,
        ctx.cfg.strategy_params.window_delta_weight_or_default(),
    );
    ipc::emit(&FrontendEvent::SignalUpdate {
        bot_id: ctx.bot_id,
        symbol: slug.asset.binance_symbol().to_string(),
        signal_score,
        bsi,
        ofi,
        cvd,
        ts_ms,
    });

    if ctx.cfg.strategy_params.rtds_enabled_or_default() {
        let (current_price, window_open_price, window_delta_bps) = {
            let snap = ctx.rtds_state.read().await;
            (snap.current_price, snap.window_open_price, snap.window_delta_bps)
        };
        ipc::emit(&FrontendEvent::RtdsUpdate {
            bot_id: ctx.bot_id,
            current_price,
            window_open_price,
            window_delta_bps,
            ts_ms,
        });
    }
}
