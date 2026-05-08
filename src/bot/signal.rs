//! Sinyal motoru — Binance CVD imbalance + OKX EMA momentum → `skor ∈ [−1, +1]`.
//!
//! signal.md Katman 3 formülü:
//!   skor = (imbalance × 0.6) + (clip(momentum_bps, −5, +5) / 5 × 0.4)
//!   skor > 0 → UP, skor ≤ 0 → DOWN
//!
//! `effective_score ∈ [0, 10]` haritalama (strateji katmanıyla uyumluluk):
//!   effective_score = skor × 5 + 5

use super::ctx::Ctx;

const W_CVD: f64 = 0.6;
const W_MOM: f64 = 0.4;
const MOM_CAP: f64 = 5.0;

/// Tek cadence'da okunacak sinyal anlık görüntüsü.
#[derive(Debug, Clone, Copy)]
pub struct SignalSnapshot {
    /// `skor × 5 + 5 ∈ [0, 10]`; 5.0 = nötr. Strateji katmanının beklediği skala.
    pub composite: f64,
    /// Binance CVD imbalance ∈ [−1, +1].
    pub imbalance: f64,
    /// OKX momentum (bps); kırpılmadan ham değer.
    pub momentum_bps: f64,
    /// Birleşik sinyal skoru ∈ [−1, +1]; + = UP, − = DOWN.
    pub skor: f64,
}

/// `skor ∈ [−1, +1]` → `[0, 10]` (5.0 = nötr).
#[inline]
fn skor_to_composite(skor: f64) -> f64 {
    (skor * 5.0 + 5.0).clamp(0.0, 10.0)
}

/// sinyal.md formülü: `skor = (imbalance × W_CVD) + (clip(mom, −CAP, +CAP) / CAP × W_MOM)`.
#[inline]
fn compute_skor(imbalance: f64, momentum_bps: f64) -> f64 {
    let mom_norm = (momentum_bps / MOM_CAP).clamp(-1.0, 1.0);
    (imbalance * W_CVD + mom_norm * W_MOM).clamp(-1.0, 1.0)
}

/// Strateji kararı için kullanılan sinyal; `(composite, signal_ready, cvd_opt, bsi_opt, ofi_opt)`.
/// `signal_ready`: Binance VE OKX warmup'ı tamamlandıysa `true`.
pub async fn decision_composite(ctx: &Ctx) -> (f64, bool, Option<f64>, Option<f64>, Option<f64>) {
    let (imbalance, warmup_b) = {
        let s = ctx.signal_state.read().await;
        (s.imbalance, s.warmup)
    };
    let (momentum_bps, warmup_o) = {
        let s = ctx.okx_state.read().await;
        (s.momentum_bps, s.warmup)
    };

    let signal_ready = !warmup_b && !warmup_o;
    let skor = compute_skor(imbalance, momentum_bps);
    let composite = skor_to_composite(skor);

    // cvd = imbalance (Binance), bsi/ofi = None (eski alanlar kaldırıldı)
    (composite, signal_ready, Some(imbalance), None, None)
}

/// Lookahead'sız anlık snapshot — chart/log/IPC cadence için.
pub async fn observed_snapshot(ctx: &Ctx) -> SignalSnapshot {
    let (imbalance, _warmup_b) = {
        let s = ctx.signal_state.read().await;
        (s.imbalance, s.warmup)
    };
    let (momentum_bps, _warmup_o) = {
        let s = ctx.okx_state.read().await;
        (s.momentum_bps, s.warmup)
    };

    let skor = compute_skor(imbalance, momentum_bps);
    let composite = skor_to_composite(skor);

    SignalSnapshot {
        composite,
        imbalance,
        momentum_bps,
        skor,
    }
}
