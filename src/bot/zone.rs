//! Periyodik (1 sn) frontend snapshot emit'i: BestBidAsk + SignalUpdate.
//! PnL snapshot'ı `bot/persist.rs` içinde aynı timer'dan çağrılır.

use crate::engine::MarketSession;
use crate::ipc::{self, FrontendEvent};
use crate::slug::SlugInfo;
use crate::time::now_ms;

use super::ctx::Ctx;
use super::signal::SignalSnapshot;

/// 1 sn cadence: book fiyatlarını ve Binance sinyal skorunu frontend'e
/// push'lar. Değişim filtresi yok — frontend her saniye güncel snapshot alır.
///
/// Çağıran (`window.rs::run_trading_loop`) `observed_snapshot`'ı tek seferde
/// hesaplayıp ref geçer; aynı tick'te `persist::snapshot_tick` de aynı snapshot'ı
/// alır — RwLock + composite hesabı tek noktaya indirgenir.
pub fn emit_frontend_snapshot(
    ctx: &Ctx,
    sess: &MarketSession,
    slug: SlugInfo,
    sig: &SignalSnapshot,
) {
    let ts_ms = now_ms();

    ipc::emit(&FrontendEvent::BestBidAsk {
        bot_id: ctx.bot_id,
        yes_best_bid: sess.yes_best_bid,
        yes_best_ask: sess.yes_best_ask,
        no_best_bid: sess.no_best_bid,
        no_best_ask: sess.no_best_ask,
        ts_ms,
    });

    ipc::emit(&FrontendEvent::SignalUpdate {
        bot_id: ctx.bot_id,
        symbol: slug.asset.binance_symbol().to_string(),
        signal_score: sig.composite,
        bsi: sig.bsi,
        ofi: sig.ofi,
        cvd: sig.cvd,
        ts_ms,
    });

    // BBA + signal verilerini tek event'te birleştir; frontend REST polling'i kaldırabilir.
    ipc::emit(&FrontendEvent::TickSnapshot {
        bot_id: ctx.bot_id,
        slug: sess.slug.clone(),
        yes_best_bid: sess.yes_best_bid,
        yes_best_ask: sess.yes_best_ask,
        no_best_bid: sess.no_best_bid,
        no_best_ask: sess.no_best_ask,
        signal_score: sig.composite,
        bsi: sig.bsi,
        ofi: sig.ofi,
        cvd: sig.cvd,
        ts_ms,
    });

    if let Some(rtds_snap) = sig.rtds {
        ipc::emit(&FrontendEvent::RtdsUpdate {
            bot_id: ctx.bot_id,
            current_price: rtds_snap.current_price,
            window_open_price: rtds_snap.window_open_price,
            window_delta_bps: rtds_snap.window_delta_bps,
            ts_ms,
        });
    }
}
