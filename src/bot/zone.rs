//! Periyodik (1 sn) frontend snapshot emit'i: TickSnapshot (book + sinyal) +
//! opsiyonel RtdsUpdate. PnL snapshot'ı `bot/persist.rs` içinde aynı timer'dan
//! çağrılır.

use crate::engine::MarketSession;
use crate::ipc::{self, FrontendEvent};
use crate::time::now_ms;

use super::ctx::Ctx;
use super::signal::SignalSnapshot;

/// 1 sn cadence: book fiyatlarını ve composite sinyal skorunu tek
/// `TickSnapshot` event'inde frontend'e push'lar. Değişim filtresi yok —
/// frontend her saniye güncel snapshot alır.
///
/// Çağıran (`window.rs::run_trading_loop`) `observed_snapshot`'ı tek seferde
/// hesaplayıp ref geçer; aynı tick'te `persist::snapshot_tick` de aynı snapshot'ı
/// alır — RwLock + composite hesabı tek noktaya indirgenir.
pub fn emit_frontend_snapshot(ctx: &Ctx, sess: &MarketSession, sig: &SignalSnapshot) {
    let ts_ms = now_ms();

    ipc::emit(&FrontendEvent::TickSnapshot {
        bot_id: ctx.bot_id,
        slug: sess.slug.clone(),
        up_best_bid: sess.up_best_bid,
        up_best_ask: sess.up_best_ask,
        down_best_bid: sess.down_best_bid,
        down_best_ask: sess.down_best_ask,
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
