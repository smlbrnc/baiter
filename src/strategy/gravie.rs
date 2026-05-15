//! Gravie — Dual-Balance Accumulator stratejisi.
//!
//! ## Felsefe
//!
//! Yön tahmini YAPMAZ. Her markette iki koşulu güvence altına almayı amaçlar:
//!
//! 1. `up_filled == down_filled` (eşit share)
//! 2. `avg_up + avg_down < avg_sum_max` (default `0.95`)
//!
//! Bu iki koşul birlikte sağlandığında, hangi sonuç gelirse gelsin:
//!
//! ```text
//! profit = N × (1 − (avg_up + avg_down))   > 0
//! ```
//!
//! Yani: ucuz fiyattan iki tarafı da doldur, dengeyi koru, kapat — garantili
//! marj.
//!
//! ## Karar zinciri
//!
//! 1. **OB guard** — iki tarafın da bid/ask > 0.
//! 2. **T-cutoff** — `to_end <= t_cutoff_secs` → `Stopped`.
//! 3. **Cooldown** — `now − last_buy_ms < buy_cooldown_ms` → NoOp.
//! 4. **Price ceiling** — `up_ask > max_ask` veya `dn_ask > max_ask` → NoOp.
//! 5. **Yön seçimi**:
//!    - **İlk emir (`first_done = false`)** — winner-momentum:
//!      `max(up_bid, dn_bid) >= first_bid_min` (default 0.65) olana kadar
//!      bekle, sonra **o yüksek-bid tarafına** BUY.
//!    - `imb = up_filled − down_filled`
//!    - `|imb| > imb_thr` → az olan tarafa BUY (rebalance, fiyat fark etmez).
//!    - aksi → daha ucuz ask'a sahip tarafa BUY.
//! 6. **Size çarpanı** — `size_multiplier(ask)` ile `order_usdc × mult`
//!    notional. **Saf lineer**, 0.5 merkezli simetri:
//!    ```text
//!    mult(p) = clamp(2 + |p − 0.5| × 10,  2.0, 7.0)
//!    ```
//!    Örnek: 0.5→2x, 0.6→3x, 0.65→3.5x, 0.7→4x, 0.8→5x, 0.9→6x, 1.0→7x.
//!    Simetri: 0.4→3x, 0.3→4x, …, 0.0→7x.
//! 7. **avg_sum gate** — yeni alım sonrası `new_avg_self + avg_opp >= avg_sum_max`
//!    olacaksa NoOp. (İlk alımda `opp_filled == 0` → gate pas geçilir.)
//!    **Loser-scalp bypass:** `ask <= loser_bypass_ask` (default 0.45) ise gate
//!    atlanır. Ucuz taraftan alım avg_sum'u bozmaz; Bonereaper "scalp muaf"
//!    mantığının Gravie karşılığı. Böylece market netleşince (~0.10–0.45'e düşen
//!    loser) sistematik denge sağlanır.
//! 8. **FAK BUY** — `size = ceil(order_usdc × mult / ask)`, `max_fak_size` cap.
//!
//! ## Reason etiketleri
//!
//! - `gravie:rebalance:{up,down}` — zayıf tarafa zorunlu denge alımı.
//! - `gravie:buy:{up,down}` — normal "ucuz taraf" alımı.
//! - (bypass alımlar da `gravie:buy` veya `gravie:rebalance` etiketiyle gelir.)

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::config::GravieParams;
use crate::types::{OrderType, Outcome, Side};

#[inline]
const fn reason_buy(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:buy:up",
        Outcome::Down => "gravie:buy:down",
    }
}

#[inline]
const fn reason_rebalance(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:rebalance:up",
        Outcome::Down => "gravie:rebalance:down",
    }
}

/// Fiyat-bazlı size çarpanı — **asimetrik parçalı lineer**.
///
/// ```text
/// price ≥ 0.50 (winner):  dist = price − 0.5
///   dist ≤ 0.20 → mult = 2 + dist × 10        (slope 10: 2x→4x)
///   dist >  0.20 → mult = 4 + (dist−0.20) × 20 (slope 20: 4x→10x)
///   clamp [2.0, 10.0]
///
/// price <  0.50 (loser):  dist = 0.5 − price
///   mult = 2 + dist × 10  (slope 10: 2x→7x)
///   clamp [2.0, 7.0]
/// ```
///
/// Tablo:
/// - 1.00 → 10.0 (winner tavan)
/// - 0.90 →  8.0
/// - 0.80 →  6.0
/// - 0.70 →  4.0 ← kırılma
/// - 0.60 →  3.0
/// - 0.50 →  2.0 (taban)
/// - 0.40 →  3.0
/// - 0.30 →  4.0
/// - 0.20 →  5.0
/// - 0.10 →  6.0
/// - 0.00 →  7.0 (loser tavan)
#[inline]
fn size_multiplier(price: f64) -> f64 {
    let d = price - 0.5;
    if d >= 0.0 {
        // winner tarafı: 0.70'den sonra hızlanır, max 10x
        let mult = if d <= 0.20 { 2.0 + d * 10.0 } else { 4.0 + (d - 0.20) * 20.0 };
        mult.clamp(2.0, 10.0)
    } else {
        // loser tarafı: saf lineer, max 7x
        (2.0 + (-d) * 10.0).clamp(2.0, 7.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum GravieState {
    /// OB henüz hazır değil.
    #[default]
    Idle,
    /// Aktif emir döngüsü.
    Active(Box<GravieActive>),
    /// T-cutoff geçildi; pasif.
    Stopped,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GravieActive {
    /// Son BUY emrinin verildiği zaman (ms). 0 = henüz emir yok.
    #[serde(default)]
    pub last_buy_ms: u64,
    /// İlk emir verildi mi? `false` iken winner-momentum (`first_bid_min`)
    /// gate kullanılır.
    #[serde(default)]
    pub first_done: bool,
}

pub struct GravieEngine;

impl GravieEngine {
    pub fn decide(state: GravieState, ctx: &StrategyContext<'_>) -> (GravieState, Decision) {
        let p = GravieParams::from_strategy_params(ctx.strategy_params);
        let to_end = ctx.market_remaining_secs.unwrap_or(f64::MAX);

        match state {
            GravieState::Stopped => (GravieState::Stopped, Decision::NoOp),

            GravieState::Idle => {
                let book_ready = ctx.up_best_bid > 0.0
                    && ctx.up_best_ask > 0.0
                    && ctx.down_best_bid > 0.0
                    && ctx.down_best_ask > 0.0;
                if !book_ready {
                    return (GravieState::Idle, Decision::NoOp);
                }
                (
                    GravieState::Active(Box::new(GravieActive::default())),
                    Decision::NoOp,
                )
            }

            GravieState::Active(mut st) => {
                if to_end <= p.t_cutoff_secs {
                    return (GravieState::Stopped, Decision::NoOp);
                }

                if ctx.up_best_ask <= 0.0 || ctx.down_best_ask <= 0.0 {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                if st.last_buy_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_buy_ms) < p.buy_cooldown_ms
                {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                if ctx.up_best_ask > p.max_ask || ctx.down_best_ask > p.max_ask {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                let m = ctx.metrics;
                let imb = m.up_filled - m.down_filled;
                let is_rebalance = imb.abs() > p.imb_thr;

                let dir = if !st.first_done {
                    let (winner, winner_bid) = if ctx.up_best_bid >= ctx.down_best_bid {
                        (Outcome::Up, ctx.up_best_bid)
                    } else {
                        (Outcome::Down, ctx.down_best_bid)
                    };
                    if winner_bid < p.first_bid_min {
                        return (GravieState::Active(st), Decision::NoOp);
                    }
                    winner
                } else if is_rebalance {
                    if imb > 0.0 {
                        Outcome::Down
                    } else {
                        Outcome::Up
                    }
                } else if ctx.up_best_ask <= ctx.down_best_ask {
                    Outcome::Up
                } else {
                    Outcome::Down
                };

                let ask = ctx.best_ask(dir);
                if ask <= 0.0 || ask > p.max_ask {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                let mult = size_multiplier(ask);
                let raw_size = (ctx.order_usdc * mult / ask).ceil();
                let size = if p.max_fak_size > 0.0 {
                    raw_size.min(p.max_fak_size)
                } else {
                    raw_size
                };
                if size <= 0.0 || size * ask < ctx.api_min_order_size {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                let (own_filled, own_avg, opp_filled, opp_avg) = match dir {
                    Outcome::Up => (m.up_filled, m.avg_up, m.down_filled, m.avg_down),
                    Outcome::Down => (m.down_filled, m.avg_down, m.up_filled, m.avg_up),
                };

                // Loser-scalp bypass: ucuz taraftan alım avg_sum'u bozmaz;
                // gate'i atla. Bonereaper'daki "scalp muaf" mantığının Gravie karşılığı.
                let is_loser_bypass =
                    p.loser_bypass_ask > 0.0 && ask <= p.loser_bypass_ask;

                if !is_loser_bypass && opp_filled > 0.0 {
                    let new_own_avg = (own_avg * own_filled + ask * size) / (own_filled + size);
                    if new_own_avg + opp_avg >= p.avg_sum_max {
                        return (GravieState::Active(st), Decision::NoOp);
                    }
                }

                let reason = if is_rebalance {
                    reason_rebalance(dir)
                } else {
                    reason_buy(dir)
                };

                let order = PlannedOrder {
                    outcome: dir,
                    token_id: ctx.token_id(dir).to_string(),
                    side: Side::Buy,
                    price: ask,
                    size,
                    order_type: OrderType::Fak,
                    reason: reason.to_string(),
                };
                st.last_buy_ms = ctx.now_ms;
                st.first_done = true;
                (
                    GravieState::Active(st),
                    Decision::PlaceOrders(vec![order]),
                )
            }
        }
    }
}
