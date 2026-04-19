//! Periyodik (1 sn) frontend snapshot emit'i: BestBidAsk + ZoneChanged +
//! SignalUpdate. PnL snapshot'ı `bot/persist.rs` içinde aynı timer'dan
//! çağrılır.

use crate::engine::MarketSession;
use crate::ipc::{self, FrontendEvent};
use crate::slug::SlugInfo;
use crate::time::{now_ms, now_secs, zone_pct};

use super::ctx::Ctx;

/// 1 sn cadence: book fiyatlarını, zone'u ve Binance sinyal skorunu
/// frontend'e push'lar. Değişim filtresi yok — frontend her saniye
/// güncel snapshot alır.
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

    let zone = sess.current_zone(now_secs());
    ipc::emit(&FrontendEvent::ZoneChanged {
        bot_id: ctx.bot_id,
        zone: format!("{zone:?}"),
        zone_pct: zone_pct(sess.start_ts, sess.end_ts, now_secs()),
        ts_ms,
    });

    // Guard'ı blok scope'una al; scalar'ları kopyala, blok bitiminde guard düşer
    // ki sync `ipc::emit` (stdout lock + flush) Binance writer'ın `state.write()`
    // talebini bloke etmesin.
    let (signal_score, bsi, ofi, cvd) = {
        let snap = ctx.signal_state.read().await;
        (snap.signal_score, snap.bsi, snap.ofi, snap.cvd)
    };
    ipc::emit(&FrontendEvent::SignalUpdate {
        bot_id: ctx.bot_id,
        symbol: slug.asset.binance_symbol().to_string(),
        signal_score,
        bsi,
        ofi,
        cvd,
        ts_ms,
    });
}
