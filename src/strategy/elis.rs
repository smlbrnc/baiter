//! Elis stratejisi — Dutch Book Bid Loop.
//!
//! ## Çalışma Mantığı
//!
//! Her **2 saniyede** bir döngü çalışır:
//!
//! 1. **P2 Lock:** Pozisyon kilitliyse (garantili kâr) → Done.
//! 2. **P5 Vol filter:** Spread çok genişse (OB ince) → NoOp.
//! 3. **P5 BSI filter:** Aşırı tek yönlü akış varsa karşı tarafı engelle.
//! 4. **Koşul:** `up_bid + dn_bid < 1.00` AND her iki bid `min_price` üzerinde.
//! 5. **P4 Improvement:** Yeni alım mevcut avg pair cost'u yeterince düşürüyor mu?
//! 6. BUY UP  @ `up_best_ask`  (dominant taraf — taker) veya `up_best_bid` (maker).
//! 7. BUY DOWN @ `dn_best_ask` (dominant taraf — taker) veya `dn_best_bid` (maker).
//! 8. Emir boyutu = `base_size + accum` (önceki loop'ta dolmayan birikmiş miktar).
//! 9. 2sn sonra tüm açık `elis:` emirleri iptal edilir.
//! 10. Dolmayan miktar hesaplanır → bir sonraki loop'ta base'e eklenir.
//!
//! ## FSM State'leri
//!
//! ```text
//! Idle { accum_up, accum_dn }
//!   → lock/filter/improvement kontrollerinden geçerse UP+DOWN emir ver → Ordering
//!
//! Ordering { placed_at_ms, accum_up, accum_dn }
//!   → 2sn veya stale timeout geçince açık emirlerdeki dolmayan miktarı al → iptal → Idle
//!
//! Done
//!   → pencere sona erdi veya pozisyon kilitlendi, artık işlem yok
//! ```
//!
//! ## Pattern Referansları (docs/gabagool.md)
//!
//! - **P2** Hedged Lock Condition — `avg_sum < lock_threshold AND pair_count > cost_basis`
//! - **P4** Improvement-Based Decision — `current_pair - projected_pair ≥ min_improvement`
//! - **P5** Microstructure Filters — vol filter (spread) + BSI filter
//! - **P6** Stale Order Cleanup — `max_order_age_ms` aşan emirleri zorla iptal

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::config::ElisParams;
use crate::types::{OrderType, Outcome, Side};

const REASON_UP: &str = "elis:dutch:up";
const REASON_DN: &str = "elis:dutch:down";
/// Biriktirilen dolmayan miktarın maksimum çarpanı (base × çarpan).
const MAX_ACCUM_MULTIPLIER: f64 = 5.0;

// ============================================================================
// FSM State
// ============================================================================

/// Elis Dutch Book Bid Loop FSM state'i.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElisState {
    /// Fırsat bekleniyor; birikmiş dolmayan miktarlar taşınır.
    Idle {
        /// Önceki loop'ta UP tarafta dolmayan toplam share.
        accum_up: f64,
        /// Önceki loop'ta DOWN tarafta dolmayan toplam share.
        accum_dn: f64,
    },
    /// Emirler verildi; `trade_cooldown_ms` geçince iptal → yeni loop.
    Ordering {
        placed_at_ms: u64,
        accum_up: f64,
        accum_dn: f64,
    },
    /// Pencere bitti veya pozisyon kilitlendi — sadece NoOp.
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
    #[inline]
    pub fn decide(state: ElisState, ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        let p = ElisParams::from_strategy_params(ctx.strategy_params);

        match state {
            // ── Pencere/lock bitti ────────────────────────────────────────────
            ElisState::Done => (ElisState::Done, Decision::NoOp),

            // ── Emirler açık: timer veya stale kontrolü ───────────────────────
            ElisState::Ordering { placed_at_ms, accum_up, accum_dn } => {
                // Pencere stop — önce kapat.
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, cancel_all_elis(ctx));
                }

                // P6: Stale order cleanup — MAX_ORDER_AGE_MS'den eski emir varsa
                // normal timer beklemeden zorla iptal et ve Idle'a dön.
                if has_stale_orders(ctx, p.max_order_age_ms) {
                    let base = p.max_buy_order_size;
                    let (unfilled_up, unfilled_dn) = compute_unfilled(ctx);
                    let new_accum_up = unfilled_up.min(base * MAX_ACCUM_MULTIPLIER);
                    let new_accum_dn = unfilled_dn.min(base * MAX_ACCUM_MULTIPLIER);
                    return (
                        ElisState::Idle { accum_up: new_accum_up, accum_dn: new_accum_dn },
                        cancel_all_elis(ctx),
                    );
                }

                // Normal cooldown dolmadıysa bekle.
                if ctx.now_ms.saturating_sub(placed_at_ms) < p.trade_cooldown_ms {
                    return (
                        ElisState::Ordering { placed_at_ms, accum_up, accum_dn },
                        Decision::NoOp,
                    );
                }

                // Süre doldu: dolmayan miktarı al, cap uygula, iptal et.
                let base = p.max_buy_order_size;
                let (unfilled_up, unfilled_dn) = compute_unfilled(ctx);
                let new_accum_up = unfilled_up.min(base * MAX_ACCUM_MULTIPLIER);
                let new_accum_dn = unfilled_dn.min(base * MAX_ACCUM_MULTIPLIER);

                (
                    ElisState::Idle { accum_up: new_accum_up, accum_dn: new_accum_dn },
                    cancel_all_elis(ctx),
                )
            }

            // ── Fırsat tara: filter → improvement → emir ─────────────────────
            ElisState::Idle { accum_up, accum_dn } => {
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, Decision::NoOp);
                }

                // ── P2: Hedged Lock — pozisyon kilitliyse artık emir verme ───
                if is_profit_locked(ctx, p.lock_threshold) {
                    return (ElisState::Done, Decision::NoOp);
                }

                let up_bid = ctx.up_best_bid;
                let dn_bid = ctx.down_best_bid;

                // OB henüz dolu değil.
                if up_bid <= 0.0 || dn_bid <= 0.0
                    || ctx.up_best_ask <= 0.0 || ctx.down_best_ask <= 0.0
                {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // Dutch book koşulu: toplam bid < $1.00.
                if up_bid + dn_bid >= 1.0 {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // Fiyat aralığı kontrolü.
                if up_bid < ctx.min_price
                    || up_bid > ctx.max_price
                    || dn_bid < ctx.min_price
                    || dn_bid > ctx.max_price
                {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // ── P5: Volatility filter — spread çok genişse OB ince ───────
                if !vol_filter_ok(ctx, p.vol_threshold) {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // ── P5: BSI filter — aşırı tek yönlü akış kontrolü ───────────
                let (up_allowed, dn_allowed) = bsi_filter(ctx, p.bsi_filter_threshold);

                // Dominant taraf ask, weaker taraf bid'den emir.
                let (up_price, dn_price) = if up_bid > dn_bid {
                    (ctx.up_best_ask, dn_bid)
                } else if dn_bid > up_bid {
                    (up_bid, ctx.down_best_ask)
                } else {
                    (up_bid, dn_bid)
                };

                // BSI filter: izin verilmeyen tarafı None yap.
                let base = p.max_buy_order_size;
                let up_size = if up_allowed { base + accum_up } else { 0.0 };
                let dn_size = if dn_allowed { base + accum_dn } else { 0.0 };

                // Her iki taraf da engelleniyorsa NoOp.
                if up_size <= 0.0 && dn_size <= 0.0 {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // ── P4: Improvement-based decision ───────────────────────────
                // Mevcut fill varsa yeni alımın avg pair cost'u yeterince
                // düşürüp düşürmediğini kontrol et.
                if !improvement_ok(ctx, up_price, up_size, dn_price, dn_size, p.min_improvement) {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // Emirleri oluştur.
                let up_ord = if up_size > 0.0 {
                    make_order(ctx, Outcome::Up, up_price, up_size, REASON_UP)
                } else {
                    None
                };
                let dn_ord = if dn_size > 0.0 {
                    make_order(ctx, Outcome::Down, dn_price, dn_size, REASON_DN)
                } else {
                    None
                };

                let orders: Vec<PlannedOrder> = [up_ord, dn_ord]
                    .into_iter()
                    .flatten()
                    .collect();

                if orders.is_empty() {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                (
                    ElisState::Ordering { placed_at_ms: ctx.now_ms, accum_up, accum_dn },
                    Decision::PlaceOrders(orders),
                )
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

/// P2 — Hedged Lock Condition (docs/gabagool.md §2).
///
/// İki koşul birden:
/// 1. `avg_up + avg_down < lock_threshold`  (pair cost yeterince düşük)
/// 2. `min(up_filled, dn_filled) > cost_basis`  (hedged qty > toplam maliyet)
#[inline]
fn is_profit_locked(ctx: &StrategyContext<'_>, lock_threshold: f64) -> bool {
    let m = ctx.metrics;
    let pair_count = m.pair_count();
    if pair_count == 0.0 {
        return false;
    }
    let avg_ok = m.avg_sum() < lock_threshold;
    let qty_ok = pair_count > m.cost_basis();
    avg_ok && qty_ok
}

/// P4 — Improvement-Based Decision (docs/gabagool.md §4).
///
/// Yalnızca **her iki tarafta da fill** varsa iyileştirme kontrolü yapılır.
/// Tek taraflı fill durumunda `avg_sum()` yanlış bir baseline oluşturur
/// (ör. DOWN filled=0.52, UP filled=0 → avg_sum=0.52; UP almak projected=1.30
/// yapar → improvement=-0.78 → yanlış blok). Bu yüzden her iki taraf doluysa
/// kontrol et, aksi halde her zaman izin ver (pozisyon inşa aşaması).
#[inline]
fn improvement_ok(
    ctx: &StrategyContext<'_>,
    up_price: f64,
    up_size: f64,
    dn_price: f64,
    dn_size: f64,
    min_improvement: f64,
) -> bool {
    let m = ctx.metrics;
    // Herhangi bir tarafta fill yoksa → pozisyon inşa aşaması, izin ver.
    if m.up_filled == 0.0 || m.down_filled == 0.0 {
        return true;
    }
    // Her iki tarafta da fill var → pair cost anlamlı, iyileştirme kontrol et.
    let current_pair = m.avg_sum();
    let new_avg_up = weighted_avg(m.avg_up, m.up_filled, up_price, up_size);
    let new_avg_dn = weighted_avg(m.avg_down, m.down_filled, dn_price, dn_size);
    let projected_pair = new_avg_up + new_avg_dn;
    current_pair - projected_pair >= min_improvement
}

/// Ağırlıklı ortalama hesabı (VWAP benzeri).
#[inline]
fn weighted_avg(current_avg: f64, current_qty: f64, new_price: f64, new_qty: f64) -> f64 {
    let total = current_qty + new_qty;
    if total <= 0.0 {
        return new_price;
    }
    (current_avg * current_qty + new_price * new_qty) / total
}

/// P5 — Volatility filter: bid-ask spread çok genişse OB güvenilmez.
///
/// Dominant taraf (taker) ask'tan alınacağı için dominant tarafın spreadi
/// kritik; weaker taraf (maker) bid'den beklendiği için sadece dominant
/// tarafın spreadi kontrol edilir. Eşit fiyat durumunda her ikisi kontrol.
#[inline]
fn vol_filter_ok(ctx: &StrategyContext<'_>, threshold: f64) -> bool {
    let up_spread = ctx.up_best_ask - ctx.up_best_bid;
    let dn_spread = ctx.down_best_ask - ctx.down_best_bid;
    let up_bid = ctx.up_best_bid;
    let dn_bid = ctx.down_best_bid;
    // Dominant taraf = daha yüksek bid'e sahip taraf
    if up_bid > dn_bid {
        up_spread <= threshold  // UP dominant (taker) → sadece UP spread kontrol
    } else if dn_bid > up_bid {
        dn_spread <= threshold  // DOWN dominant (taker) → sadece DOWN spread kontrol
    } else {
        up_spread <= threshold && dn_spread <= threshold
    }
}

/// P5 — BSI filter: aşırı tek yönlü akış varsa karşı tarafı engelle.
///
/// Returns `(up_allowed, dn_allowed)`.
/// - `bsi > +threshold` → UP baskısı → DOWN almayı engelle
/// - `bsi < -threshold` → DOWN baskısı → UP almayı engelle
/// - `None` veya nötr → her iki taraf serbest
#[inline]
fn bsi_filter(ctx: &StrategyContext<'_>, threshold: f64) -> (bool, bool) {
    match ctx.bsi {
        Some(bsi) if bsi > threshold  => (true, false),
        Some(bsi) if bsi < -threshold => (false, true),
        _ => (true, true),
    }
}

/// P6 — Stale order tespiti: herhangi bir elis emri `max_age_ms`'den eskiyse true.
#[inline]
fn has_stale_orders(ctx: &StrategyContext<'_>, max_age_ms: u64) -> bool {
    ctx.open_orders
        .iter()
        .filter(|o| o.reason.starts_with("elis:"))
        .any(|o| o.age_ms(ctx.now_ms) > max_age_ms)
}

/// `elis:` prefix'li tüm açık emirleri iptal et.
#[inline]
fn cancel_all_elis(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("elis:"))
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(ids)
    }
}

/// Açık elis emirlerinden dolmayan (remaining) share miktarını hesapla.
/// Returns `(up_unfilled, dn_unfilled)`.
#[inline]
fn compute_unfilled(ctx: &StrategyContext<'_>) -> (f64, f64) {
    let mut up = 0.0_f64;
    let mut dn = 0.0_f64;
    for o in ctx.open_orders.iter().filter(|o| o.reason.starts_with("elis:")) {
        let remaining = (o.size - o.size_matched).max(0.0);
        match o.outcome {
            Outcome::Up => up += remaining,
            Outcome::Down => dn += remaining,
        }
    }
    (up, dn)
}

/// Tek taraf için BUY limit emri oluştur.
#[inline]
fn make_order(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    size: f64,
    reason: &'static str,
) -> Option<PlannedOrder> {
    if price <= 0.0 || size <= 0.0 || size * price < ctx.api_min_order_size {
        return None;
    }
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
