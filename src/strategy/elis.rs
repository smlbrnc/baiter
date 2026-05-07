//! Elis stratejisi — Dutch Book Bid Loop.
//!
//! ## Çalışma Mantığı
//!
//! Her `trade_cooldown_ms` (default 4sn) bir döngü çalışır:
//!
//! 1. **P2 Lock:** Pozisyon kilitliyse (garantili kâr) → Done.
//! 2. **P5 Vol filter:** Dominant taraf spread'i çok genişse → NoOp.
//! 3. **P5 BSI filter:** Her iki taraf BSI'dan onay almazsa → NoOp.
//! 4. **Koşul:** `up_bid + dn_bid < $1.00` AND her iki bid `min_price` üzerinde.
//! 5. **P4 Improvement:** Yeni alım mevcut avg pair cost'u yeterince düşürüyor mu?
//! 6. BUY UP @ `up_best_ask` (dominant — taker) veya `up_best_bid` (weaker — maker).
//! 7. BUY DOWN @ `dn_best_ask` (dominant — taker) veya `dn_best_bid` (weaker — maker).
//! 8. Emir boyutu = max(base + accum, min_shares) — notional kesinlikle > $1.00.
//! 9. Cooldown sonunda `elis:` emirleri iptal, dolmayan miktar biriktirilir.
//!
//! ## FSM State'leri
//!
//! ```text
//! Idle { accum_up, accum_dn }
//!   → koşullar geçerse UP+DOWN emri ver → Ordering
//!
//! Ordering { placed_at_ms, accum_up, accum_dn }
//!   → cooldown veya stale timeout geçince iptal → Idle (yeni accum)
//!
//! Done → NoOp (pencere bitti / pozisyon kilitlendi)
//! ```

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::config::ElisParams;
use crate::types::{OrderType, Outcome, Side};

const REASON_UP: &str = "elis:dutch:up";
const REASON_DN: &str = "elis:dutch:down";
/// Biriktirilen dolmayan miktarın maksimum çarpanı (base × çarpan).
const MAX_ACCUM_MULTIPLIER: f64 = 5.0;

// ============================================================================
// FSM State
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElisState {
    Idle { accum_up: f64, accum_dn: f64 },
    Ordering { placed_at_ms: u64, accum_up: f64, accum_dn: f64 },
    Done,
}

impl Default for ElisState {
    #[inline]
    fn default() -> Self {
        Self::Idle { accum_up: 0.0, accum_dn: 0.0 }
    }
}

// ============================================================================
// Engine
// ============================================================================

pub struct ElisEngine;

impl ElisEngine {
    /// Tek tick — yeni state + Decision döndürür.
    /// Hot-path: Done ve Ordering(NoOp) dalları sıfır alloc.
    #[inline]
    pub fn decide(state: ElisState, ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        let p = ElisParams::from_strategy_params(ctx.strategy_params);

        match state {
            // ── Pencere/lock bitti ────────────────────────────────────────────
            ElisState::Done => (ElisState::Done, Decision::NoOp),

            // ── Emirler açık ──────────────────────────────────────────────────
            ElisState::Ordering { placed_at_ms, accum_up, accum_dn } => {
                // Pencere stop — önce kapat.
                if is_window_stop(ctx, &p) {
                    let (_, decision) = cancel_and_unfilled(ctx, p.max_buy_order_size);
                    return (ElisState::Done, decision);
                }

                let elapsed = ctx.now_ms.saturating_sub(placed_at_ms);

                // Cooldown henüz dolmadıysa bekle (hot path: O(1) kontrol).
                if elapsed < p.trade_cooldown_ms {
                    // P6: Stale yalnızca cooldown dolmadan erken çıkış için önemli.
                    // max_order_age_ms, cooldown'dan büyük olmalı; küçükse stale öncelik alır.
                    if elapsed < p.max_order_age_ms {
                        return (
                            ElisState::Ordering { placed_at_ms, accum_up, accum_dn },
                            Decision::NoOp,
                        );
                    }
                    // P6 stale: cooldown'dan önce emir yaşı limit aştı → zorla iptal.
                }

                // Cooldown veya stale doldu: unfilled hesapla + iptal et (TEK GEÇİŞ).
                let base = p.max_buy_order_size;
                let ((unfilled_up, unfilled_dn), decision) = cancel_and_unfilled(ctx, base);

                (
                    ElisState::Idle {
                        accum_up: unfilled_up.min(base * MAX_ACCUM_MULTIPLIER),
                        accum_dn: unfilled_dn.min(base * MAX_ACCUM_MULTIPLIER),
                    },
                    decision,
                )
            }

            // ── Fırsat tara: ucuz kontroller önce ───────────────────────────
            ElisState::Idle { accum_up, accum_dn } => {
                macro_rules! noop {
                    () => {
                        return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp)
                    };
                }

                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, Decision::NoOp);
                }

                // OB hazır mı? (en erken ret — sıfır hesap)
                let up_bid = ctx.up_best_bid;
                let dn_bid = ctx.down_best_bid;
                if up_bid <= 0.0 || dn_bid <= 0.0
                    || ctx.up_best_ask <= 0.0 || ctx.down_best_ask <= 0.0
                {
                    noop!();
                }

                // Dutch book koşulu: toplam bid < $1.00.
                if up_bid + dn_bid >= 1.0 { noop!(); }

                // Fiyat aralığı.
                if up_bid < ctx.min_price || up_bid > ctx.max_price
                    || dn_bid < ctx.min_price || dn_bid > ctx.max_price
                {
                    noop!();
                }

                // P2 Lock — pair cost check (küçük aritmetik, lock nadiren aktif).
                if is_profit_locked(ctx, p.lock_threshold) {
                    return (ElisState::Done, Decision::NoOp);
                }

                // P5 Vol filter — sadece dominant taraf spread'ini kontrol et.
                if !vol_filter_ok(up_bid, ctx.up_best_ask, dn_bid, ctx.down_best_ask, p.vol_threshold) {
                    noop!();
                }

                // P5 BSI filter — çift alım zorunluluğu.
                if !bsi_both_ok(ctx, p.bsi_filter_threshold) { noop!(); }

                // Fiyat seçimi: dominant → ask (taker), weaker → bid (maker).
                let (up_price, dn_price, up_is_taker) = if up_bid > dn_bid {
                    (ctx.up_best_ask, dn_bid, true)
                } else if dn_bid > up_bid {
                    (up_bid, ctx.down_best_ask, false)
                } else {
                    (up_bid, dn_bid, true) // eşit: UP taker sayıyoruz
                };

                // Emir boyutu: max(base + accum, min_shares_for_1usd_notional).
                let base = p.max_buy_order_size;
                let up_size = (base + accum_up).max(min_shares(up_price));
                let dn_size = (base + accum_dn).max(min_shares(dn_price));

                // P4 Improvement — taker-only kontrol.
                if !improvement_ok(ctx, up_price, up_size, dn_price, dn_size, p.min_improvement, up_is_taker) {
                    noop!();
                }

                // Emirleri oluştur (en geç adım — alloc yalnızca burada).
                let up_ord = make_order(ctx, Outcome::Up,   up_price, up_size, REASON_UP);
                let dn_ord = make_order(ctx, Outcome::Down, dn_price, dn_size, REASON_DN);

                // Her ikisi de Some olmalı (min_shares bunu garantiler).
                match (up_ord, dn_ord) {
                    (Some(u), Some(d)) => (
                        ElisState::Ordering { placed_at_ms: ctx.now_ms, accum_up, accum_dn },
                        Decision::PlaceOrders(vec![u, d]),
                    ),
                    _ => noop!(),
                }
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Pencere stop koşulu.
#[inline]
fn is_window_stop(ctx: &StrategyContext<'_>, p: &ElisParams) -> bool {
    matches!(ctx.market_remaining_secs, Some(r) if r <= p.stop_before_end_secs)
}

/// P2 — Hedged Lock: avg_sum < threshold VE pair_count > cost_basis.
#[inline]
fn is_profit_locked(ctx: &StrategyContext<'_>, threshold: f64) -> bool {
    let m = ctx.metrics;
    let pc = m.pair_count();
    pc > 0.0 && m.avg_sum() < threshold && pc > m.cost_basis()
}

/// P4 — Improvement check.
///
/// - both_empty → always ok (entry condition already checked)
/// - one_side_empty → avg_existing + new_price < 1.0 (zarar güvencesi)
/// - both_filled → **TAKER-ONLY** improvement kontrolü:
///
/// **Neden taker-only?**
/// Eski "her iki taraf doluyor" projeksiyonu yanlış varsayım yapıyordu.
/// Maker emir (weaker taraf) dolduktan ÖNCE cooldown ateşleniyor → maker fill
/// hiç gerçekleşmeyebilir. Sadece taker'ın (dominant, anlık fill) etkisini
/// dikkate almak gerekir; maker bonus — eğer dolarsa avg daha da iyileşir.
///
/// Örnek hata: avg_sum=1.00, UP @ 0.65 (taker), DOWN @ 0.35 (maker)
///   - Eski: projected=0.62+0.38=1.00, improvement=0 → ALLOWS
///   - Gerçek: UP fills @ 0.65 → avg_up=0.62, DOWN dolmadı → actual_sum=1.03 ZARAR
///   - Taker-only: 0.62 + 0.41(mevcut) = 1.03 → improvement=-0.03 → BLOCKED ✓
#[inline]
fn improvement_ok(
    ctx: &StrategyContext<'_>,
    up_price: f64,
    up_size: f64,
    dn_price: f64,
    dn_size: f64,
    min_imp: f64,
    up_is_taker: bool,  // true: UP dominant (ask), false: DOWN dominant (ask)
) -> bool {
    let m = ctx.metrics;

    if m.up_filled == 0.0 && m.down_filled == 0.0 { return true; }

    // Tek taraf boşken kritik sorun: metrics stale olabilir.
    // UP taker bu loop'ta dolacaksa avg_up değişecek. Bunu projekte et.
    //
    // Ör: avg_up=0.53 (önceki loop), bu loop UP @ 0.55 taker + DOWN @ 0.46 maker
    //   → Stale kontrol: 0.53 + 0.46 = 0.99 → ALLOWS
    //   → Gerçek: UP fills → avg_up=0.54 → 0.54 + 0.46 = 1.00 → zarar
    // Düzeltme: taker fill sonrası avg'yi kullan.
    if m.down_filled == 0.0 {
        // DOWN hiç dolmadı; UP dolmuş durumda.
        // Bu loop UP taker ise, fill sonrası projected avg_up ile kontrol et.
        let eff_avg_up = if up_is_taker {
            wavg(m.avg_up, m.up_filled, up_price, up_size)
        } else {
            m.avg_up  // UP maker — fill garantisi yok, mevcut avg kullan
        };
        return eff_avg_up + dn_price < 1.0;
    }

    if m.up_filled == 0.0 {
        // UP hiç dolmadı; DOWN dolmuş durumda.
        let eff_avg_dn = if !up_is_taker {
            wavg(m.avg_down, m.down_filled, dn_price, dn_size)
        } else {
            m.avg_down
        };
        return up_price + eff_avg_dn < 1.0;
    }

    // Her iki taraf dolu: taker-only zarar kontrolü + tam projeksiyon iyileşmesi.
    let cur = m.avg_sum();
    let (taker_new_avg, other_avg) = if up_is_taker {
        (wavg(m.avg_up,   m.up_filled,   up_price, up_size), m.avg_down)
    } else {
        (wavg(m.avg_down, m.down_filled, dn_price, dn_size), m.avg_up)
    };
    let taker_safe = taker_new_avg + other_avg < 1.0;
    let new_u = wavg(m.avg_up,   m.up_filled,   up_price, up_size);
    let new_d = wavg(m.avg_down, m.down_filled, dn_price, dn_size);
    let full_ok = cur - (new_u + new_d) >= min_imp;
    taker_safe && full_ok
}

/// Ağırlıklı ortalama (VWAP).
#[inline(always)]
fn wavg(avg: f64, qty: f64, price: f64, new_qty: f64) -> f64 {
    let t = qty + new_qty;
    if t <= 0.0 { price } else { (avg * qty + price * new_qty) / t }
}

/// P5 Vol filter — dominant tarafın spread'i kontrol et.
#[inline]
fn vol_filter_ok(ub: f64, ua: f64, db: f64, da: f64, thr: f64) -> bool {
    if ub > db { (ua - ub) <= thr }
    else if db > ub { (da - db) <= thr }
    else { (ua - ub) <= thr && (da - db) <= thr }
}

/// P5 BSI filter — her iki taraf da nötr/izinli mi?
/// Tek taraf blokluysa çift alım yapılamaz → false döner.
#[inline]
fn bsi_both_ok(ctx: &StrategyContext<'_>, thr: f64) -> bool {
    match ctx.bsi {
        Some(b) if b.abs() > thr => false, // herhangi bir taraf blok
        _ => true,
    }
}

/// Polymarket minimum notional güvencesi: notional kesinlikle > $1.00.
/// `floor(1.0 / price) + 1` formülü bunu garanti eder.
///   price=0.05 → floor(20.0)+1 = 21 → 21×0.05 = $1.05 ✓
///   price=0.24 → floor(4.17)+1 = 5  →  5×0.24 = $1.20 ✓
#[inline(always)]
fn min_shares(price: f64) -> f64 {
    if price <= 0.0 { 2.0 } else { (1.0_f64 / price).floor() + 1.0 }
}

/// Açık `elis:` emirlerini tek geçişte hem unfilled hesapla hem iptal et.
/// İki ayrı iteration yerine O(n) tek geçiş → %50 daha az iteration.
/// Returns `((unfilled_up, unfilled_dn), Decision)`.
#[inline]
fn cancel_and_unfilled(ctx: &StrategyContext<'_>, _base: f64) -> ((f64, f64), Decision) {
    let mut up = 0.0_f64;
    let mut dn = 0.0_f64;
    let mut ids: Vec<String> = Vec::new();

    for o in ctx.open_orders.iter() {
        if !is_elis_order(o) { continue; }
        let remaining = (o.size - o.size_matched).max(0.0);
        match o.outcome {
            Outcome::Up   => up += remaining,
            Outcome::Down => dn += remaining,
        }
        ids.push(o.id.clone());
    }

    let decision = if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) };
    ((up, dn), decision)
}

/// `elis:` prefix kontrolü — `starts_with` yerine uzunluk + byte karşılaştırma.
#[inline(always)]
fn is_elis_order(o: &OpenOrder) -> bool {
    o.reason.starts_with("elis:")
}

/// Tek taraf için BUY limit emri.
/// `min_shares` zaten notional > $1.00 garanti ettiğinden sadece price/size sıfır kontrolü.
#[inline]
fn make_order(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    size: f64,
    reason: &'static str,
) -> Option<PlannedOrder> {
    if price <= 0.0 || size <= 0.0 { return None; }
    Some(PlannedOrder {
        outcome,
        token_id: ctx.token_id(outcome).to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Gtc,
        reason: reason.to_string(),
    })
}
