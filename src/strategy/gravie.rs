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
//! 3. **Late Winner injection** (Bonereaper karşılığı):
//!    `winner_bid >= lw_bid_thr` + quota OK + LW cooldown geçmişse winner
//!    tarafa büyük taker BUY. Boyut:
//!    `ceil(order_usdc × lw_usdc_factor × lw_mult / w_ask)`,
//!    `lw_mult` lineer: `ask<0.95 → 1x`, `ask≥0.95 → clamp(2 + (ask−0.95)×75, 2, 5)`.
//! 4. **Cooldown** — `now − last_buy_ms < buy_cooldown_ms` → NoOp.
//! 5. **Yön seçimi**:
//!    - **İlk emir (`first_done = false`)** — winner-momentum:
//!      `max(up_bid, dn_bid) >= first_bid_min` (default 0.65) olana kadar bekle.
//!    - `|imb| > imb_thr` → az olan tarafa BUY (rebalance).
//!    - aksi → daha ucuz ask'a sahip tarafa BUY.
//! 6. **Price band** — Bonereaper ile uyumlu: `dir_bid` seçilen yönün bid'i
//!    `min_price..=max_price` aralığında değilse NoOp.
//! 7. **Loser guard** — seçilen yön zayıf taraf (`dir_bid < opp_bid`) ve
//!    `ask > loser_bypass_ask` ve `!is_rebalance` ise NoOp.
//! 8. **Size hesabı**:
//!    - `ask ≤ loser_bypass_ask` (scalp bandı) →
//!      `size = ceil(order_usdc × loser_scalp_usdc_factor / ask)` (Bonereaper
//!      benzeri küçük sabit scalp; default $0.5 × order_usdc).
//!    - aksi → `size_multiplier(ask)` ile `size = ceil(order_usdc × mult / ask)`
//!      (asimetrik parçalı lineer çarpan).
//!    `max_fak_size` cap her iki durumda da uygulanır.
//! 9. **avg_loser_max guard** — `is_loser && own_avg > avg_loser_max` → NoOp.
//! 10. **avg_sum gate** — `new_avg_self + avg_opp >= avg_sum_max` → NoOp.
//!     **Muafiyetler:** (a) `ask <= loser_bypass_ask` (loser-scalp bypass),
//!     (b) `is_rebalance == true` — denge alımları her durumda geçer (polarize
//!     marketlerde zayıf tarafa erişim için kritik).
//! 11. **FAK BUY** — `size = ceil(order_usdc × mult / ask)`, `max_fak_size` cap.
//!
//! ## Reason etiketleri
//!
//! - `gravie:lw:{up,down}` — Late Winner injection.
//! - `gravie:rebalance:{up,down}` — zayıf tarafa zorunlu denge alımı.
//! - `gravie:buy:{up,down}` — normal "ucuz taraf" alımı.

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

#[inline]
const fn reason_lw(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:lw:up",
        Outcome::Down => "gravie:lw:down",
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
    /// Son Late Winner emrinin zamanı (ms). 0 = henüz LW yok.
    #[serde(default)]
    pub last_lw_buy_ms: u64,
    /// Session içinde verilen toplam Late Winner shot sayısı (quota sayacı).
    #[serde(default)]
    pub lw_injections: u32,
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

                // ── LATE WINNER injection ────────────────────────────────────
                // Bonereaper'ın diskret arb_mult'ı yerine lineer skala:
                //   ask<0.95 → 1x;  ask≥0.95 → clamp(2 + (ask−0.95)×75, 2, 5)
                // Quota: lw_max_per_session; LW kendi cooldown'unu buy_cooldown_ms
                // ile paylaşır (son LW emrinin ms'i baz alınır).
                let lw_quota_ok =
                    p.lw_max_per_session == 0 || st.lw_injections < p.lw_max_per_session;
                let lw_in_cd = st.last_lw_buy_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_lw_buy_ms) < p.buy_cooldown_ms;
                if lw_quota_ok && !lw_in_cd && p.lw_usdc_factor > 0.0 {
                    let (lw_dir, w_bid, w_ask) = if ctx.up_best_bid >= ctx.down_best_bid {
                        (Outcome::Up, ctx.up_best_bid, ctx.up_best_ask)
                    } else {
                        (Outcome::Down, ctx.down_best_bid, ctx.down_best_ask)
                    };
                    // Bonereaper LW gibi: ask filtresi YOK. Winner aşırı pahalı
                    // olsa bile (ask=1.00) winner momentum'una büyük taker BUY at;
                    // dual-balance polarize markette UP'a erişim için bu kritik.
                    if w_bid >= p.lw_bid_thr && w_ask > 0.0 {
                        let lw_mult = if w_ask < 0.95 {
                            1.0_f64
                        } else {
                            (2.0 + (w_ask - 0.95) * 75.0).clamp(2.0, 5.0)
                        };
                        let raw = (ctx.order_usdc * p.lw_usdc_factor * lw_mult / w_ask).ceil();
                        let lw_size = if p.max_fak_size > 0.0 {
                            raw.min(p.max_fak_size)
                        } else {
                            raw
                        };
                        if lw_size > 0.0 && lw_size * w_ask >= ctx.api_min_order_size {
                            let order = PlannedOrder {
                                outcome: lw_dir,
                                token_id: ctx.token_id(lw_dir).to_string(),
                                side: Side::Buy,
                                price: w_ask,
                                size: lw_size,
                                order_type: OrderType::Fak,
                                reason: reason_lw(lw_dir).to_string(),
                            };
                            st.last_buy_ms = ctx.now_ms;
                            st.last_lw_buy_ms = ctx.now_ms;
                            st.lw_injections = st.lw_injections.saturating_add(1);
                            st.first_done = true;
                            return (GravieState::Active(st), Decision::PlaceOrders(vec![order]));
                        }
                    }
                }

                if st.last_buy_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_buy_ms) < p.buy_cooldown_ms
                {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // Global ask filtresi YOK — Bonereaper'da da yok. Yön seçildikten
                // sonra `bid > max_price` kontrolü yapılır (aşağıda).

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
                let dir_bid = ctx.best_bid(dir);
                if ask <= 0.0 || dir_bid <= 0.0 {
                    return (GravieState::Active(st), Decision::NoOp);
                }
                // Bonereaper ile uyumlu: bid (ask değil) ile fiyat tavanı kontrolü.
                // Bot'un BotConfig.max_price'ı (default 0.99) seçilen yönün bid'ini
                // sınırlar; ask = bid + 0.01 spread normaldir, engellenmemeli.
                if dir_bid < ctx.min_price || dir_bid > ctx.max_price {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // ── Loser guard: seçilen yön zayıf tarafsa ve fiyat scalp
                // bandının üstündeyse alma (rebalance hariç). Bonereaper'da
                // "loser dir + bid > 0.30 → NoOp" kuralının karşılığı.
                let opp_bid = match dir {
                    Outcome::Up => ctx.down_best_bid,
                    Outcome::Down => ctx.up_best_bid,
                };
                let is_loser = dir_bid < opp_bid;
                if is_loser && !is_rebalance && ask > p.loser_bypass_ask {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // Bypass aktif mi? ask ucuz taraf eşiğinin altındaysa Bonereaper
                // mantığıyla "loser scalp" davranışı: sabit küçük USDC, scalp boyutu.
                let is_loser_bypass =
                    p.loser_bypass_ask > 0.0 && ask <= p.loser_bypass_ask;

                let raw_size = if is_loser_bypass && p.loser_scalp_usdc_factor > 0.0 {
                    // Sabit scalp: order_usdc × factor / ask. Bonereaper'ın loser
                    // scalp boyutuyla birebir (factor=0.5 default).
                    let scalp_usdc = ctx.order_usdc * p.loser_scalp_usdc_factor;
                    (scalp_usdc / ask).ceil()
                } else {
                    let mult = size_multiplier(ask);
                    (ctx.order_usdc * mult / ask).ceil()
                };
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

                // ── avg_loser_max guard: loser yönüne avg fiyat eşiği aşıldıysa
                // yeni alım yapma (pahalı martingale-down koruması).
                if is_loser && own_filled > 0.0 && own_avg > p.avg_loser_max {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // avg_sum gate muafiyetleri:
                //  - Loser-scalp bypass: ucuz taraftan alım (ask ≤ loser_bypass_ask).
                //  - Rebalance: dual-balance için zorunlu denge alımları gate'i bypass
                //    eder; aksi halde polarize marketlerde zayıf taraf hiç tetiklenmez.

                if !is_loser_bypass && !is_rebalance && opp_filled > 0.0 {
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
