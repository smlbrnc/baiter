//! Elis stratejisi — Dutch Book Bid Loop.
//!
//! ## Çalışma Mantığı
//!
//! Her **2 saniyede** bir döngü çalışır:
//!
//! 1. Koşul: `up_bid + dn_bid < 1.00` AND her iki bid `min_price` üzerinde.
//! 2. BUY UP  @ `up_best_bid`  — maker limit emri.
//! 3. BUY DOWN @ `dn_best_bid` — maker limit emri.
//! 4. Emir boyutu = `base_size + accum` (önceki loop'ta dolmayan birikmiş miktar).
//! 5. 2sn sonra tüm açık `elis:` emirleri iptal edilir.
//! 6. Dolmayan miktar hesaplanır → bir sonraki loop'ta base'e eklenir.
//!
//! ## FSM State'leri
//!
//! ```text
//! Idle { accum_up, accum_dn }
//!   → koşul sağlanırsa UP+DOWN emir ver → Ordering
//!
//! Ordering { placed_at_ms, accum_up, accum_dn }
//!   → 2sn geçince açık emirlerdeki dolmayan miktarı al → iptal → Idle (yeni accum)
//!
//! Done
//!   → pencere sona erdi, artık işlem yok
//! ```
//!
//! ## Reason Etiketleri
//!
//! `elis:dutch:up`   — UP taraf alım emri
//! `elis:dutch:down` — DOWN taraf alım emri

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::config::ElisParams;
use crate::types::{OrderType, Outcome, Side};

const REASON_UP: &str = "elis:dutch:up";
const REASON_DN: &str = "elis:dutch:down";

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
        /// Bu loop'tan önceki birikmiş UP miktarı (iptal sonrası yeniden hesaplanır).
        accum_up: f64,
        /// Bu loop'tan önceki birikmiş DOWN miktarı.
        accum_dn: f64,
    },
    /// Pencere bitti — sadece NoOp.
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
            // ── Pencere bitti ─────────────────────────────────────────────────
            ElisState::Done => (ElisState::Done, Decision::NoOp),

            // ── Emirler açık: 2sn dolunca iptal et ───────────────────────────
            ElisState::Ordering { placed_at_ms, accum_up, accum_dn } => {
                // Pencere stop — önce kapat.
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, cancel_all_elis(ctx));
                }

                // 2sn dolmadıysa bekle.
                if ctx.now_ms.saturating_sub(placed_at_ms) < p.trade_cooldown_ms {
                    return (
                        ElisState::Ordering { placed_at_ms, accum_up, accum_dn },
                        Decision::NoOp,
                    );
                }

                // Süre doldu: açık elis emirlerinden dolmayan miktarı hesapla.
                let (unfilled_up, unfilled_dn) = compute_unfilled(ctx);
                let new_accum_up = accum_up + unfilled_up;
                let new_accum_dn = accum_dn + unfilled_dn;

                let cancel = cancel_all_elis(ctx);
                (
                    ElisState::Idle { accum_up: new_accum_up, accum_dn: new_accum_dn },
                    cancel,
                )
            }

            // ── Fırsat tara ve emir ver ───────────────────────────────────────
            ElisState::Idle { accum_up, accum_dn } => {
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, Decision::NoOp);
                }

                let up_bid = ctx.up_best_bid;
                let dn_bid = ctx.down_best_bid;

                // OB henüz dolu değil.
                if up_bid <= 0.0 || dn_bid <= 0.0 {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // Dutch book koşulu: toplam bid < $1.00.
                if up_bid + dn_bid >= 1.0 {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // Fiyat aralığı kontrolü (her iki bid de kabul edilebilir aralıkta).
                if up_bid < ctx.min_price
                    || up_bid > ctx.max_price
                    || dn_bid < ctx.min_price
                    || dn_bid > ctx.max_price
                {
                    return (ElisState::Idle { accum_up, accum_dn }, Decision::NoOp);
                }

                // Emir boyutu: base + birikmiş dolmayan miktar.
                let base = p.max_buy_order_size;
                let up_size = base + accum_up;
                let dn_size = base + accum_dn;

                // Yükselen (dominant) taraf ask'tan taker emir alır; weaker taraf
                // bid'den maker olarak bekler.
                // up_bid > dn_bid → UP dominant → UP @ ask, DOWN @ bid
                // dn_bid > up_bid → DOWN dominant → DOWN @ ask, UP @ bid
                let (up_price, up_type, dn_price, dn_type) = if up_bid > dn_bid {
                    (ctx.up_best_ask, OrderType::Gtc, dn_bid, OrderType::Gtc)
                } else if dn_bid > up_bid {
                    (up_bid, OrderType::Gtc, ctx.down_best_ask, OrderType::Gtc)
                } else {
                    // Eşit: her iki taraf bid'den maker.
                    (up_bid, OrderType::Gtc, dn_bid, OrderType::Gtc)
                };

                let up_ord = make_order(ctx, Outcome::Up,   up_price, up_size, up_type, REASON_UP);
                let dn_ord = make_order(ctx, Outcome::Down, dn_price, dn_size, dn_type, REASON_DN);

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

/// Pencere stop koşulu: market kapanmasına `stop_before_end_secs` saniye kaldı.
#[inline]
fn is_window_stop(ctx: &StrategyContext<'_>, p: &ElisParams) -> bool {
    matches!(ctx.market_remaining_secs, Some(r) if r <= p.stop_before_end_secs)
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

/// Tek taraf için BUY emri oluştur.
/// `order_type`: Gtc maker (bid) veya Gtc taker (ask) için aynı tip kullanılır;
/// fiyat ask olunca CLOB taraf olarak taker davranır.
#[inline]
fn make_order(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    size: f64,
    order_type: OrderType,
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
        order_type,
        reason: reason.to_string(),
    })
}
