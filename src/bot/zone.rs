//! Periyodik zone + signal + book snapshot logu/IPC.

use crate::engine::MarketSession;
use crate::ipc::{self, FrontendEvent};
use crate::slug::SlugInfo;
use crate::time::{now_ms, now_secs, zone_pct};

use super::ctx::Ctx;

/// 5 sn cadence: zone değişimini IPC'ye gönder, signal güncellemesini
/// frontend'e push'la, book snapshot'ı (değiştiyse) logla.
pub async fn emit_zone_signal(
    ctx: &Ctx,
    sess: &MarketSession,
    slug: SlugInfo,
    last_zone: &mut Option<String>,
    last_book_snapshot: &mut Option<(f64, f64, f64, f64)>,
) {
    let zone_str = format!("{:?}", sess.current_zone(now_secs()));
    if last_zone.as_deref() != Some(zone_str.as_str()) {
        *last_zone = Some(zone_str.clone());
        ipc::emit(&FrontendEvent::ZoneChanged {
            bot_id: ctx.bot_id,
            zone: zone_str,
            zone_pct: zone_pct(sess.start_ts, sess.end_ts, now_secs()),
            ts_ms: now_ms(),
        });
    }
    let snap = ctx.signal_state.read().await;
    ipc::emit(&FrontendEvent::SignalUpdate {
        bot_id: ctx.bot_id,
        symbol: slug.asset.binance_symbol().to_string(),
        signal_score: snap.signal_score,
        bsi: snap.bsi,
        ofi: snap.ofi,
        cvd: snap.cvd,
        ts_ms: now_ms(),
    });

    // §5.4 book snapshot: 5s cadence'inde, sadece değişiklik varsa logla.
    let current = (
        sess.yes_best_bid,
        sess.yes_best_ask,
        sess.no_best_bid,
        sess.no_best_ask,
    );
    if current.0 > 0.0 && current.2 > 0.0 && last_book_snapshot.as_ref() != Some(&current) {
        *last_book_snapshot = Some(current);
        ipc::log_line(
            &ctx.bot_id.to_string(),
            format!(
                "📚 Book snapshot: yes_bid={:.4} yes_ask={:.4} no_bid={:.4} no_ask={:.4} | yes_spread={:.4} no_spread={:.4}",
                current.0,
                current.1,
                current.2,
                current.3,
                (current.1 - current.0).max(0.0),
                (current.3 - current.2).max(0.0),
            ),
        );
    }
}
