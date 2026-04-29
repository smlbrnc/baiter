//! Composite skor + RTDS bileşenlerini tek noktadan hesaplar.
//! `decision_composite`: opener kararı için lookahead'lı (`signal_ready` döner).
//! `observed_snapshot`: anlık (lookahead'sız) — chart/log/IPC için.

use crate::engine::MarketSession;
use crate::rtds::{self, RtdsState};

use super::ctx::Ctx;

/// Composite + opsiyonel RTDS alt sinyalleri (frontend RtdsUpdate için).
#[derive(Debug, Clone, Copy)]
pub struct SignalSnapshot {
    pub composite: f64,
    pub bsi: f64,
    pub ofi: f64,
    pub cvd: f64,
    pub rtds: Option<RtdsSnapshot>,
}

#[derive(Debug, Clone, Copy)]
pub struct RtdsSnapshot {
    pub current_price: f64,
    pub window_open_price: Option<f64>,
    pub window_delta_bps: f64,
}

const NEUTRAL_WINDOW_SCORE: f64 = 5.0;

/// Lookahead'lı opener composite skoru + RTDS pencere açılışı yakalandı mı.
/// `bsi/ofi/cvd` Elis composite opener kuralları için döndürülür.
pub async fn decision_composite(
    ctx: &Ctx,
    sess: &MarketSession,
) -> (f64, bool, Option<f64>, Option<f64>, Option<f64>) {
    let (binance_score, bsi, ofi, cvd) = {
        let snap = ctx.signal_state.read().await;
        (snap.signal_score, snap.bsi, snap.ofi, snap.cvd)
    };
    let rtds_enabled = ctx.cfg.strategy_params.rtds_enabled_or_default();
    let (window_score, signal_ready) = if rtds_enabled {
        let rtds_snap = ctx.rtds_state.read().await;
        let ready = rtds_snap.window_open_price.is_some();
        let lookahead = ctx.cfg.strategy_params.signal_lookahead_secs_or_default();
        let interval_secs = sess.end_ts.saturating_sub(sess.start_ts);
        (rtds_window_score(&rtds_snap, interval_secs, lookahead), ready)
    } else {
        (NEUTRAL_WINDOW_SCORE, true)
    };
    let composite = rtds::composite_score(
        window_score,
        binance_score,
        ctx.cfg.strategy_params.window_delta_weight_or_default(),
    );
    (composite, signal_ready, Some(bsi), Some(ofi), Some(cvd))
}

/// Lookahead'sız anlık composite + alt sinyaller. Tek RwLock turu.
pub async fn observed_snapshot(ctx: &Ctx, sess: &MarketSession) -> SignalSnapshot {
    let (binance_score, bsi, ofi, cvd) = {
        let snap = ctx.signal_state.read().await;
        (snap.signal_score, snap.bsi, snap.ofi, snap.cvd)
    };
    let (window_score, rtds) = if ctx.cfg.strategy_params.rtds_enabled_or_default() {
        let rtds_snap = ctx.rtds_state.read().await;
        let interval_secs = sess.end_ts.saturating_sub(sess.start_ts);
        let score = rtds_window_score(&rtds_snap, interval_secs, 0.0);
        let snap = RtdsSnapshot {
            current_price: rtds_snap.current_price,
            window_open_price: rtds_snap.window_open_price,
            window_delta_bps: rtds_snap.window_delta_bps,
        };
        (score, Some(snap))
    } else {
        (NEUTRAL_WINDOW_SCORE, None)
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
        rtds,
    }
}

/// `lookahead_secs > 0` → projeksiyon (`delta + velocity * lookahead`); `= 0` → anlık.
fn rtds_window_score(rtds_snap: &RtdsState, interval_secs: u64, lookahead_secs: f64) -> f64 {
    let projected_bps =
        rtds_snap.window_delta_bps + rtds_snap.recent_velocity_bps_per_sec * lookahead_secs;
    rtds::window_delta_score(projected_bps, rtds::interval_scale(interval_secs))
}
