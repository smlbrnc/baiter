//! Sinyal snapshot helper — composite skor + RTDS bileşenlerini tek noktadan
//! hesaplar. `tick`, `zone`, `persist` üçü de aynı formülü tekrar etmesin.
//!
//! İki varyant:
//! - [`decision_composite`]: opener karar gecikmesi için lookahead'lı projeksiyon
//!   (`recent_velocity_bps_per_sec * lookahead_secs`) + `signal_ready` flag'i.
//! - [`observed_snapshot`]: anlık (lookahead'sız) — chart/log/IPC push'ları için.

use crate::engine::MarketSession;
use crate::rtds;

use super::ctx::Ctx;

/// Her iki snapshot için ortak ham alanlar (composite + RTDS alt sinyalleri).
#[derive(Debug, Clone, Copy)]
pub struct SignalSnapshot {
    pub composite: f64,
    pub bsi: f64,
    pub ofi: f64,
    pub cvd: f64,
}

/// `tick.rs` için: lookahead'lı opener composite skoru + RTDS hazır mı flag'i.
/// Yalnız composite kullanıldığı için struct değil iki tuple döner.
pub async fn decision_composite(ctx: &Ctx, sess: &MarketSession) -> (f64, bool) {
    let binance_score = ctx.signal_state.read().await.signal_score;
    let rtds_enabled = ctx.cfg.strategy_params.rtds_enabled_or_default();
    let (window_score, signal_ready) = if rtds_enabled {
        let rtds_snap = ctx.rtds_state.read().await;
        let ready = rtds_snap.window_open_price.is_some();
        let interval_secs = sess.end_ts.saturating_sub(sess.start_ts);
        let lookahead = ctx.cfg.strategy_params.signal_lookahead_secs_or_default();
        let projected_bps =
            rtds_snap.window_delta_bps + rtds_snap.recent_velocity_bps_per_sec * lookahead;
        let score = rtds::window_delta_score(projected_bps, rtds::interval_scale(interval_secs));
        (score, ready)
    } else {
        (5.0, true)
    };
    let composite = rtds::composite_score(
        window_score,
        binance_score,
        ctx.cfg.strategy_params.window_delta_weight_or_default(),
    );
    (composite, signal_ready)
}

/// `zone.rs` ve `persist.rs` için: lookahead'sız anlık composite + alt sinyaller.
pub async fn observed_snapshot(ctx: &Ctx, sess: &MarketSession) -> SignalSnapshot {
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
    let composite = rtds::composite_score(
        window_score,
        binance_score,
        ctx.cfg.strategy_params.window_delta_weight_or_default(),
    );
    SignalSnapshot {
        composite,
        bsi,
        ofi,
        cvd,
    }
}
