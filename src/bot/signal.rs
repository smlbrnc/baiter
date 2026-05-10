//! Sinyal motoru — Window Delta (dominant) + Binance CVD + OKX momentum.
//!
//! signal-report.md Section 5.1:
//!   "Window delta hâlâ kral — 5-15 dk'lık binary piyasalarda tek başına
//!    diğer tüm TA indikatörlerinden 5-7 kat daha güçlü."
//!
//! Yeni formül:
//!   - Window Delta: %70 ağırlık (en güçlü sinyal, %82 accuracy)
//!   - Binance CVD + OKX Momentum: %30 ağırlık (destekleyici)
//!
//! `effective_score ∈ [0, 10]` (5.0 = nötr); >5 = UP, <5 = DOWN.

use super::ctx::Ctx;
use crate::rtds;

/// Window delta ağırlığı (%70 = 0.70). Signal-report: "5-7x daha güçlü".
const W_WINDOW_DELTA: f64 = 0.70;
/// Binance+OKX ağırlığı (%30).
const W_BINANCE: f64 = 0.30;
/// CVD ağırlığı (Binance içinde).
const W_CVD: f64 = 0.6;
/// Momentum ağırlığı (Binance içinde).
const W_MOM: f64 = 0.4;
/// Momentum bps cap.
const MOM_CAP: f64 = 5.0;

/// Tek cadence'da okunacak sinyal anlık görüntüsü.
#[derive(Debug, Clone, Copy)]
pub struct SignalSnapshot {
    /// Composite score ∈ [0, 10]; 5.0 = nötr.
    pub composite: f64,
    /// Binance CVD imbalance ∈ [−1, +1].
    pub imbalance: f64,
    /// OKX momentum (bps).
    pub momentum_bps: f64,
    /// Window delta (bps).
    pub window_delta_bps: f64,
    /// Birleşik sinyal skoru ∈ [−1, +1]; + = UP, − = DOWN.
    pub skor: f64,
}

/// Binance sinyalini [0, 10] skalasına dönüştür.
#[inline]
fn binance_score_10(imbalance: f64, momentum_bps: f64) -> f64 {
    let mom_norm = (momentum_bps / MOM_CAP).clamp(-1.0, 1.0);
    let skor = (imbalance * W_CVD + mom_norm * W_MOM).clamp(-1.0, 1.0);
    (skor * 5.0 + 5.0).clamp(0.0, 10.0)
}

/// Strateji kararı için kullanılan sinyal.
///
/// Dönüş: `(composite, signal_ready, cvd_opt, bsi_opt, ofi_opt)`.
/// - `composite`: Window Delta (%70) + Binance (%30) birleşik skor [0, 10].
/// - `signal_ready`: RTDS window_open alındıysa VE Binance/OKX warmup bittiyse true.
pub async fn decision_composite(ctx: &Ctx) -> (f64, bool, Option<f64>, Option<f64>, Option<f64>) {
    // Binance CVD
    let (imbalance, warmup_b) = {
        let s = ctx.signal_state.read().await;
        (s.imbalance, s.warmup)
    };
    // OKX momentum
    let (momentum_bps, warmup_o) = {
        let s = ctx.okx_state.read().await;
        (s.momentum_bps, s.warmup)
    };
    // RTDS window delta
    let (window_delta_bps, has_window_open, interval_secs) = {
        let s = ctx.rtds_state.read().await;
        let has_open = s.window_open_price.is_some();
        // interval_secs: session süresi (5m=300, 15m=900, vb.)
        let interval = if s.window_start_ts_ms > 0 {
            // Varsayılan 5m (300s); gerçek değer session'dan alınmalı
            300_u64
        } else {
            300
        };
        (s.window_delta_bps, has_open, interval)
    };

    // Signal ready: Tüm kaynaklar hazır
    let signal_ready = !warmup_b && !warmup_o && has_window_open;

    // Window delta score (signal-report Section 5.1 tier sistemi)
    let interval_scale = rtds::interval_scale(interval_secs);
    let wd_score = rtds::window_delta_score(window_delta_bps, interval_scale);

    // Binance score (CVD + momentum)
    let bn_score = binance_score_10(imbalance, momentum_bps);

    // Composite: Window Delta %70 + Binance %30
    // Signal-report: "window delta 5-7x daha güçlü"
    let composite = rtds::composite_score(wd_score, bn_score, W_WINDOW_DELTA);

    // cvd = imbalance
    (composite, signal_ready, Some(imbalance), None, None)
}

/// Lookahead'sız anlık snapshot — chart/log/IPC cadence için.
pub async fn observed_snapshot(ctx: &Ctx) -> SignalSnapshot {
    // Binance CVD
    let (imbalance, _warmup_b) = {
        let s = ctx.signal_state.read().await;
        (s.imbalance, s.warmup)
    };
    // OKX momentum
    let (momentum_bps, _warmup_o) = {
        let s = ctx.okx_state.read().await;
        (s.momentum_bps, s.warmup)
    };
    // RTDS window delta
    let (window_delta_bps, interval_secs) = {
        let s = ctx.rtds_state.read().await;
        let interval = if s.window_start_ts_ms > 0 { 300_u64 } else { 300 };
        (s.window_delta_bps, interval)
    };

    // Window delta score
    let interval_scale = rtds::interval_scale(interval_secs);
    let wd_score = rtds::window_delta_score(window_delta_bps, interval_scale);

    // Binance score
    let bn_score = binance_score_10(imbalance, momentum_bps);

    // Composite
    let composite = rtds::composite_score(wd_score, bn_score, W_WINDOW_DELTA);

    // Skor [-1, +1] için: (composite - 5) / 5
    let skor = (composite - 5.0) / 5.0;

    SignalSnapshot {
        composite,
        imbalance,
        momentum_bps,
        window_delta_bps,
        skor,
    }
}
