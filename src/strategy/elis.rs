//! Elis stratejisi — Dutch Book Spread Capture.
//!
//! Doküman: `docs/elis.md`
//!
//! ## Strateji Özeti
//!
//! Her iki taraf bid-ask spread'i `spread_threshold`'ı aşınca:
//! - BUY UP @ UP_bid + BUY DOWN @ DOWN_bid (maker — bid fiyatından)
//! - `trade_cooldown_ms` ms bekle → emirleri iptal et
//!
//! Bu döngü pencere bitimine `stop_before_end_secs` saniye kalana dek tekrar eder.
//!
//! ## Balance Factor Mekanizması
//!
//! ```text
//! imbalance  = |UP_pozisyon − DOWN_pozisyon|
//! adjustment = round(imbalance × balance_factor × 0.5)
//!
//! geride_kalan_taraf_emir = max_buy_order_size + adjustment
//! dominant_taraf_emir     = max(max_buy_order_size − adjustment, 1)
//! ```
//!
//! Geride kalan tarafa daha büyük emir verilir; böylece pozisyon dengede tutulur.
//!
//! ## FSM State'leri
//!
//! ```text
//! Idle         → Spread koşulu bekleniyor; hazır olunca BatchPending'e geçer.
//! BatchPending → UP+DOWN emirleri gönderildi; trade_cooldown_ms geçince iptal.
//! Done         → stop_before_end_secs veya window end → artık işlem yok.
//! ```

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::config::ElisParams;
use crate::types::{OrderType, Outcome, Side};

const REASON_UP: &str = "elis:dutch:up";
const REASON_DN: &str = "elis:dutch:down";

/// Dutch Book FSM state'i.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElisState {
    /// Spread bekleniyor — emir yok.
    Idle,
    /// Batch emirler gönderildi; `placed_at_ms`'den `trade_cooldown_ms` sonra iptal.
    BatchPending { placed_at_ms: u64 },
    /// Pencere sona erdi veya stop tetiklendi — yalnızca NoOp.
    Done,
}

impl Default for ElisState {
    #[inline]
    fn default() -> Self {
        Self::Idle
    }
}

pub struct ElisEngine;

impl ElisEngine {
    /// Tek tick — yeni state + Decision döndürür.
    ///
    /// Hot-path: `BatchPending` dalı zero-allocation; tüm helper'lar `#[inline]`.
    #[inline]
    pub fn decide(state: ElisState, ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        // Params: her tick çözümlenir; `#[inline(always)]` ile sıfır çağrı maliyeti.
        let p = ElisParams::from_strategy_params(ctx.strategy_params);

        match state {
            // ── Pencere sona erdi — sadece NoOp. ─────────────────────────────
            ElisState::Done => (ElisState::Done, Decision::NoOp),

            // ── Batch emir bekleniyor. ────────────────────────────────────────
            ElisState::BatchPending { placed_at_ms } => {
                // 1. Pencere stop — önce deadline'ı yakala.
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, cancel_managed(ctx));
                }
                // 2. Cooldown doldu mu?
                if ctx.now_ms.saturating_sub(placed_at_ms) >= p.trade_cooldown_ms {
                    return (ElisState::Idle, cancel_managed(ctx));
                }
                (ElisState::BatchPending { placed_at_ms }, Decision::NoOp)
            }

            // ── Spread fırsatı bekle. ─────────────────────────────────────────
            ElisState::Idle => {
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, Decision::NoOp);
                }

                // Bid-ask spread koşulu: her iki tarafta spread ≥ threshold.
                let up_bid = ctx.up_best_bid;
                let dn_bid = ctx.down_best_bid;
                let up_spread = ctx.up_best_ask - up_bid;
                let dn_spread = ctx.down_best_ask - dn_bid;
                if up_spread < p.spread_threshold || dn_spread < p.spread_threshold {
                    return (ElisState::Idle, Decision::NoOp);
                }

                // Kârlılık koşulu: up_bid + dn_bid < $1.00 olmalı.
                // Spread ≥ threshold yeterli değil; combined bid > 1.00 → guaranteed loss.
                if up_bid + dn_bid >= 1.0 {
                    return (ElisState::Idle, Decision::NoOp);
                }

                // Fiyat aralığı (bid fiyatına göre — maker emirler bid'den girer).
                if up_bid < ctx.min_price
                    || up_bid > ctx.max_price
                    || dn_bid < ctx.min_price
                    || dn_bid > ctx.max_price
                {
                    return (ElisState::Idle, Decision::NoOp);
                }

                // Balance factor: imbalance düzeltmesi.
                let (up_size, dn_size) = balance_sizes(ctx.metrics.up_filled,
                                                        ctx.metrics.down_filled,
                                                        p.max_buy_order_size,
                                                        p.balance_factor);

                // Maker limit emirleri (bid). UP_bid + DOWN_bid < $1.00 → kârlı Dutch Book.
                let up_ord = make_order(ctx, Outcome::Up,   up_bid, up_size, REASON_UP);
                let dn_ord = make_order(ctx, Outcome::Down, dn_bid, dn_size, REASON_DN);

                let orders: Vec<PlannedOrder> = match (up_ord, dn_ord) {
                    (Some(u), Some(d)) => vec![u, d],
                    (Some(u), None)    => vec![u],
                    (None,    Some(d)) => vec![d],
                    (None,    None)    => return (ElisState::Idle, Decision::NoOp),
                };

                (ElisState::BatchPending { placed_at_ms: ctx.now_ms }, Decision::PlaceOrders(orders))
            }
        }
    }
}

// ============================================================================
// HELPERS  (tümü #[inline] — call overhead yok)
// ============================================================================

/// Pencere stop koşulu.
#[inline]
fn is_window_stop(ctx: &StrategyContext<'_>, p: &ElisParams) -> bool {
    matches!(ctx.market_remaining_secs, Some(r) if r <= p.stop_before_end_secs)
}

/// `elis:` prefix'li tüm açık emirleri iptal eder.
#[inline]
fn cancel_managed(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("elis:"))
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) }
}

/// Balance factor formülü (§ docs/elis.md).
/// Tüm argümanlar scalar — heap allocation yok.
#[inline]
fn balance_sizes(up_pos: f64, dn_pos: f64, base: f64, factor: f64) -> (f64, f64) {
    let imbalance = (up_pos - dn_pos).abs();
    let adj = (imbalance * factor * 0.5).round();
    if up_pos >= dn_pos {
        ((base - adj).max(1.0), base + adj)
    } else {
        (base + adj, (base - adj).max(1.0))
    }
}

/// Tek BUY limit emri. `reason` statik &str — heap allocation yok.
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
