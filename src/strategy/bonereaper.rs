//! Bonereaper stratejisi — Polymarket "Bonereaper" wallet
//! (`0xeebde7a0e019a63e6b476eb425505b7b3e6eba30`) davranış kopyası.
//!
//! ## Karar zinciri
//!
//! 1. **Window**: `now ∈ [start, end]`; OB ready.
//! 2. **LATE WINNER** — `max(bid) ≥ lw_thr` ve cooldown OK ise winner tarafa
//!    taker BUY (`size = lw_usdc × arb_mult / ask`). `arb_mult` lineer
//!    5×@lw_thr → 10×@0.99. Quota: `lw_max_per_session`. LW ile birlikte
//!    loser cheap scalp (FAK at ask).
//! 3. **COOLDOWN** — `now − last_buy < buy_cooldown_ms` → NoOp.
//! 4. **YÖN SEÇİMİ** — first_done=false → spread gate + BSI/OB fallback;
//!    sonrasında `|imb| > N×est_size` ise zayıf yöne rebalance, aksi halde
//!    `|Δbid|` büyük olan tarafa.
//! 5. **LOSER NORMAL + GUARD** — loser yön seçildiğinde:
//!    - Normal: winner ile aynı sizing (spread_shares / interp) uygulanır.
//!    - `avg_loser_max` guard: loser avg bu eşiği aşarsa `scalp_usdc` ile küçük alım.
//!    - Analiz: gerçek bot 0.01-0.25 bandında da spread_shares lot kullanıyor.
//! 6. **NORMAL BUY** taker @ ask. Sizing iki moddan biri:
//!    - `shares_const > 0`: `size = shares_const` sabit (15m bot için optimal).
//!    - aksi: piecewise lineer interp (anchor 0.30/0.65/lw_thr).
//! 7. **avg_sum cap** (default 1.00; loser scalp muaf).
//!
//! ## Reason etiketleri
//!
//! - `bonereaper:buy:{up,down}` — normal BUY
//! - `bonereaper:scalp:{up,down}` — loser scalp
//! - `bonereaper:lw:{up,down}` — late winner

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

#[inline]
const fn reason_buy(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:buy:up",
        Outcome::Down => "bonereaper:buy:down",
    }
}

#[inline]
const fn reason_lw(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:lw:up",
        Outcome::Down => "bonereaper:lw:down",
    }
}

#[inline]
const fn reason_scalp(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:scalp:up",
        Outcome::Down => "bonereaper:scalp:down",
    }
}

/// Loser tarafı belirler. Spread < 0.20'de belirsiz bölge → None.
/// None → loser_guard uygulanmaz.
#[inline]
fn loser_side(up_bid: f64, dn_bid: f64) -> Option<Outcome> {
    const LOSER_SPREAD_MIN: f64 = 0.20;
    let spread = (up_bid - dn_bid).abs();
    if spread < LOSER_SPREAD_MIN {
        None
    } else if up_bid >= dn_bid {
        Some(Outcome::Down)
    } else {
        Some(Outcome::Up)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum BonereaperState {
    #[default]
    Idle,
    Active(BonereaperActive),
    /// Geriye uyumlu (eski serde); yeni akışta üretilmiyor.
    Done,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BonereaperActive {
    /// Son BUY emrinin ms zamanı; 0 = henüz emir yok.
    #[serde(default)]
    pub last_buy_ms: u64,
    /// Son LW emrinin ms zamanı; 0 = henüz LW yok. LW'ye özgü cooldown için.
    #[serde(default)]
    pub last_lw_buy_ms: u64,
    /// Önceki tick UP bid (delta hesabı).
    #[serde(default)]
    pub last_up_bid: f64,
    /// Önceki tick DOWN bid.
    #[serde(default)]
    pub last_dn_bid: f64,
    /// Late winner injection sayacı.
    #[serde(default)]
    pub lw_injections: u32,
    /// İlk emir verildi mi? Spread-gated start için kullanılır.
    #[serde(default)]
    pub first_done: bool,
}

pub struct BonereaperEngine;

impl BonereaperEngine {
    pub fn decide(
        state: BonereaperState,
        ctx: &StrategyContext<'_>,
    ) -> (BonereaperState, Decision) {
        let to_end = ctx.market_remaining_secs.unwrap_or(f64::MAX);
        let p = ctx.strategy_params;

        match state {
            BonereaperState::Done => (BonereaperState::Done, Decision::NoOp),

            BonereaperState::Idle => {
                let book_ready = ctx.up_best_bid > 0.0
                    && ctx.up_best_ask > 0.0
                    && ctx.down_best_bid > 0.0
                    && ctx.down_best_ask > 0.0;
                if !book_ready {
                    return (BonereaperState::Idle, Decision::NoOp);
                }
                let active = BonereaperActive {
                    last_buy_ms: 0,
                    last_lw_buy_ms: 0,
                    last_up_bid: ctx.up_best_bid,
                    last_dn_bid: ctx.down_best_bid,
                    lw_injections: 0,
                    first_done: false,
                };
                (BonereaperState::Active(active), Decision::NoOp)
            }

            BonereaperState::Active(mut st) => {
                if to_end < 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }
                if ctx.up_best_bid <= 0.0 || ctx.down_best_bid <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── LATE WINNER ─────────────────────────────────────────────
                let lw_secs = p.bonereaper_late_winner_secs() as f64;
                let lw_usdc = p.bonereaper_late_winner_usdc(ctx.order_usdc);
                let lw_thr = p.bonereaper_late_winner_bid_thr();
                let lw_max = p.bonereaper_lw_max_per_session();
                let lw_quota_ok = lw_max == 0 || st.lw_injections < lw_max;
                // LW cooldown: lw_cooldown_ms > 0 ise kendi zamanlayıcısı;
                // aksi halde normal buy_cooldown_ms.
                let lw_cd_ms_specific = p.bonereaper_lw_cooldown_ms();
                let lw_in_cd = if lw_cd_ms_specific > 0 {
                    st.last_lw_buy_ms > 0
                        && ctx.now_ms.saturating_sub(st.last_lw_buy_ms) < lw_cd_ms_specific
                } else {
                    let lw_cd_ms = p.bonereaper_buy_cooldown_ms();
                    st.last_buy_ms > 0
                        && ctx.now_ms.saturating_sub(st.last_buy_ms) < lw_cd_ms
                };

                let lw_active = lw_usdc > 0.0 && lw_secs > 0.0 && to_end <= lw_secs;
                if lw_quota_ok && lw_active && !lw_in_cd && to_end > 0.0 {
                    let (winner, w_bid, w_ask) = if ctx.up_best_bid >= ctx.down_best_bid {
                        (Outcome::Up, ctx.up_best_bid, ctx.up_best_ask)
                    } else {
                        (Outcome::Down, ctx.down_best_bid, ctx.down_best_ask)
                    };
                    if w_bid >= lw_thr && w_ask > 0.0 {
                        // arb_mult lineer: lw_thr'de 5×, 0.99'da 10×.
                        let arb_mult = (5.0 + 5.0 * (w_ask - lw_thr) / (0.99 - lw_thr))
                            .clamp(5.0, 10.0);
                        let size = (lw_usdc * arb_mult / w_ask).ceil();
                        if let Some(o) = make_buy(ctx, winner, w_ask, size, reason_lw(winner)) {
                            st.last_buy_ms = ctx.now_ms;
                            st.last_lw_buy_ms = ctx.now_ms;
                            st.lw_injections = st.lw_injections.saturating_add(1);
                            st.last_up_bid = ctx.up_best_bid;
                            st.last_dn_bid = ctx.down_best_bid;
                            st.first_done = true;
                            // LW sweep: loser tarafa cheap scalp (FAK at ask).
                            let loser = if winner == Outcome::Up { Outcome::Down } else { Outcome::Up };
                            let loser_ask = ctx.best_ask(loser);
                            let scalp_usdc = p.bonereaper_loser_scalp_usdc(ctx.order_usdc);
                            let mut orders = vec![o];
                            if loser_ask > 0.0 && scalp_usdc > 0.0 {
                                let loser_size = (scalp_usdc / loser_ask).ceil();
                                if loser_size > 0.0 {
                                    orders.push(PlannedOrder {
                                        outcome: loser,
                                        token_id: ctx.token_id(loser).to_string(),
                                        side: Side::Buy,
                                        price: loser_ask,
                                        size: loser_size,
                                        order_type: OrderType::Fak,
                                        reason: reason_scalp(loser).to_string(),
                                    });
                                }
                            }
                            return (
                                BonereaperState::Active(st),
                                Decision::PlaceOrders(orders),
                            );
                        }
                    }
                }

                // ── COOLDOWN ─────────────────────────────────────────────────
                let cd_ms = p.bonereaper_buy_cooldown_ms();
                if st.last_buy_ms > 0 && ctx.now_ms.saturating_sub(st.last_buy_ms) < cd_ms {
                    st.last_up_bid = ctx.up_best_bid;
                    st.last_dn_bid = ctx.down_best_bid;
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── YÖN SEÇİMİ ──────────────────────────────────────────────
                // Winner-bias erken çıkış: spread > thr iken yüksek bidli taraf
                // her zaman seçilir (Δbid mantığı atlanır). 6 market simülasyonu:
                // bant uyum hatasını korurken PnL +$265 (gerçek +$277'ye yakın).
                let winner_bias_thr = p.bonereaper_winner_bias_spread_thr();
                let cur_ob_spread = (ctx.up_best_bid - ctx.down_best_bid).abs();
                let winner_bias_active = winner_bias_thr > 0.0
                    && cur_ob_spread > winner_bias_thr
                    && st.first_done;

                let dir = if winner_bias_active {
                    if ctx.up_best_bid >= ctx.down_best_bid {
                        Outcome::Up
                    } else {
                        Outcome::Down
                    }
                } else if !st.first_done {
                    // İlk emir: spread gate. BSI primer, OB fallback.
                    let spread_min = p.bonereaper_first_spread_min();
                    let spread = ctx.up_best_bid - ctx.down_best_bid;
                    if spread.abs() < spread_min {
                        st.last_up_bid = ctx.up_best_bid;
                        st.last_dn_bid = ctx.down_best_bid;
                        return (BonereaperState::Active(st), Decision::NoOp);
                    }
                    const BSI_THRESHOLD: f64 = 0.30;
                    if let Some(bsi) = ctx.bsi {
                        if bsi >= BSI_THRESHOLD {
                            Outcome::Up
                        } else if bsi <= -BSI_THRESHOLD {
                            Outcome::Down
                        } else {
                            if spread > 0.0 { Outcome::Up } else { Outcome::Down }
                        }
                    } else {
                        if spread > 0.0 { Outcome::Up } else { Outcome::Down }
                    }
                } else {
                    let m = ctx.metrics;
                    let imb = m.up_filled - m.down_filled;
                    // Dinamik imbalance eşiği: N(to_end) × est_trade_size (share).
                    // shares_const>0 ise direkt o; aksi halde interp anchor
                    // medyanı (longshot+mid)/2 share = ((lo+mid)/2) / 0.5 ≈ ortalama.
                    // Sabit "share" beklentisi sayısal olarak share cinsinden olmalı.
                    let est_trade_size = {
                        let sc = p.bonereaper_size_shares_const();
                        if sc > 0.0 {
                            sc
                        } else {
                            // 0.30-0.65 arası bir referans bid (~0.475) ile usdc ÷ ask
                            let avg_usdc = p.bonereaper_interp_usdc(0.475, ctx.order_usdc);
                            (avg_usdc / 0.475).round().max(1.0)
                        }
                    };
                    let n_trades = if to_end >= 120.0 {
                        3.0_f64
                    } else if to_end >= 60.0 {
                        6.0_f64
                    } else if to_end >= 30.0 {
                        9.0_f64
                    } else {
                        12.0_f64
                    };
                    let dynamic_imb = (n_trades * est_trade_size).clamp(15.0, 600.0);
                    let param_imb = p.bonereaper_imbalance_thr();
                    let imb_thr = if param_imb < 500.0 { param_imb } else { dynamic_imb };
                    if imb.abs() > imb_thr {
                        if imb > 0.0 { Outcome::Down } else { Outcome::Up }
                    } else {
                        let d_up = (ctx.up_best_bid - st.last_up_bid).abs();
                        let d_dn = (ctx.down_best_bid - st.last_dn_bid).abs();
                        if d_up == 0.0 && d_dn == 0.0 {
                            if ctx.up_best_bid >= ctx.down_best_bid {
                                Outcome::Up
                            } else {
                                Outcome::Down
                            }
                        } else if d_up >= d_dn {
                            Outcome::Up
                        } else {
                            Outcome::Down
                        }
                    }
                };

                st.last_up_bid = ctx.up_best_bid;
                st.last_dn_bid = ctx.down_best_bid;

                let bid = ctx.best_bid(dir);
                let ask = ctx.best_ask(dir);
                if bid <= 0.0 || ask <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Mevcut OB spread: |up_bid − dn_bid|. Spread-aware sizing için.
                let ob_spread = (ctx.up_best_bid - ctx.down_best_bid).abs();

                let metrics = ctx.metrics;
                let loser_opt = loser_side(ctx.up_best_bid, ctx.down_best_bid);
                let is_loser_dir = loser_opt.map_or(false, |l| dir == l);

                let effective_min = if is_loser_dir {
                    p.bonereaper_loser_min_price().min(ctx.min_price)
                } else {
                    ctx.min_price
                };
                if bid < effective_min || bid > ctx.max_price {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Loser tarafta avg fiyatı avg_loser_max'ı aşarsa sadece scalp.
                let avg_loser_max = p.bonereaper_avg_loser_max();
                let (cur_filled, cur_avg, opp_filled, opp_avg) = match dir {
                    Outcome::Up => (
                        metrics.up_filled,
                        metrics.avg_up,
                        metrics.down_filled,
                        metrics.avg_down,
                    ),
                    Outcome::Down => (
                        metrics.down_filled,
                        metrics.avg_down,
                        metrics.up_filled,
                        metrics.avg_up,
                    ),
                };
                let scalp_only = is_loser_dir && cur_filled > 0.0 && cur_avg > avg_loser_max;

                let scalp_usdc = p.bonereaper_loser_scalp_usdc(ctx.order_usdc);
                // Scalp bandı SADECE avg_loser_max guard tetiklendiğinde aktif.
                // Gerçek bot analizi (12k emir): loser yöne 0.01-0.25 bandında da
                // normal sizing (spread_shares) kullanılıyor — scalp_usdc küçük lot
                // değil, yalnızca martingale guard (scalp_only) devreye girince kullanılır.
                let is_scalp_band = false; // scalp band kaldırıldı — loser her fiyatta normal sizing
                let usdc = if scalp_only && scalp_usdc > 0.0 {
                    // avg_loser_max aşıldı: sadece bu durumda küçük sabit scalp
                    scalp_usdc
                } else {
                    // Tüm normal emirler (winner + loser): spread-aware lot sizing
                    // shares_const > 0 → bonereaper_spread_shares(ob_spread) ile dinamik lot
                    // 0 → piecewise lineer interp: anchor 0.30/0.65/lw_thr
                    let base = {
                        let shares = p.bonereaper_spread_shares(ob_spread);
                        if shares > 0.0 {
                            shares * ask
                        } else {
                            p.bonereaper_interp_usdc(bid, ctx.order_usdc)
                        }
                    };
                    let lp_secs = p.bonereaper_late_pyramid_secs() as f64;
                    if !is_loser_dir && lp_secs > 0.0 && to_end > 0.0 && to_end <= lp_secs {
                        base * p.bonereaper_winner_size_factor()
                    } else {
                        base
                    }
                };
                if usdc <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                let is_any_scalp = scalp_only; // is_scalp_band artık false

                let order_price = ask; // taker
                let size = (usdc / order_price).ceil().max(1.0);

                // avg_sum soft cap — scalp muaf.
                if !is_any_scalp && opp_filled > 0.0 {
                    let max_avg_sum = p.bonereaper_max_avg_sum();
                    let new_avg = if cur_filled > 0.0 {
                        (cur_avg * cur_filled + order_price * size) / (cur_filled + size)
                    } else {
                        order_price
                    };
                    if new_avg + opp_avg > max_avg_sum {
                        return (BonereaperState::Active(st), Decision::NoOp);
                    }
                }

                let reason = if is_any_scalp {
                    reason_scalp(dir)
                } else {
                    reason_buy(dir)
                };
                if let Some(o) = make_buy(ctx, dir, order_price, size, reason) {
                    st.last_buy_ms = ctx.now_ms;
                    st.first_done = true;

                    // Loser fırsat alımı (çift emir): spread ≥ force_thr iken
                    // ana emrin yanında loser yönüne sabit küçük lot ek emir.
                    // Gerçek bot 0.20-0.30 fiyatlarında loser tarafta birikim
                    // yapıyor; tek-yön taker mantığı bu fırsatı kaçırıyordu.
                    let force_thr = p.bonereaper_force_both_spread_thr();
                    let force_lot = p.bonereaper_force_both_loser_shares();
                    let mut orders = vec![o];
                    if force_thr > 0.0 && force_lot > 0.0 && cur_ob_spread >= force_thr {
                        let loser_oc = if dir == Outcome::Up {
                            Outcome::Down
                        } else {
                            Outcome::Up
                        };
                        let loser_ask = ctx.best_ask(loser_oc);
                        if loser_ask > 0.0 && loser_ask < ctx.max_price {
                            if let Some(lo) = make_buy(
                                ctx,
                                loser_oc,
                                loser_ask,
                                force_lot,
                                reason_buy(loser_oc),
                            ) {
                                orders.push(lo);
                            }
                        }
                    }

                    return (
                        BonereaperState::Active(st),
                        Decision::PlaceOrders(orders),
                    );
                }
                (BonereaperState::Active(st), Decision::NoOp)
            }
        }
    }
}

/// BUY FAK emir. `price ≤ 0`, `size ≤ 0` veya notional < min → `None`.
fn make_buy(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    size: f64,
    reason: &str,
) -> Option<PlannedOrder> {
    if price <= 0.0 || size <= 0.0 {
        return None;
    }
    if size * price < ctx.api_min_order_size {
        return None;
    }
    Some(PlannedOrder {
        outcome,
        token_id: ctx.token_id(outcome).to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Fak,
        reason: reason.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use crate::config::StrategyParams;

    fn params_5m() -> StrategyParams {
        let mut p = StrategyParams::default();
        p.bonereaper_size_longshot_usdc = Some(10.0);
        p.bonereaper_size_mid_usdc = Some(25.0);
        p.bonereaper_size_high_usdc = Some(80.0);
        p.bonereaper_late_winner_bid_thr = Some(0.95);
        p
    }

    #[test]
    fn interp_anchors_match_exactly() {
        let p = params_5m();
        assert!((p.bonereaper_interp_usdc(0.30, 10.0) - 10.0).abs() < 1e-9);
        assert!((p.bonereaper_interp_usdc(0.65, 10.0) - 25.0).abs() < 1e-9);
        assert!((p.bonereaper_interp_usdc(0.95, 10.0) - 80.0).abs() < 1e-9);
    }

    #[test]
    fn interp_below_longshot_is_constant() {
        let p = params_5m();
        assert!((p.bonereaper_interp_usdc(0.00, 10.0) - 10.0).abs() < 1e-9);
        assert!((p.bonereaper_interp_usdc(0.10, 10.0) - 10.0).abs() < 1e-9);
        assert!((p.bonereaper_interp_usdc(0.29, 10.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn interp_above_lw_thr_is_high_constant() {
        let p = params_5m();
        assert!((p.bonereaper_interp_usdc(0.95, 10.0) - 80.0).abs() < 1e-9);
        assert!((p.bonereaper_interp_usdc(0.99, 10.0) - 80.0).abs() < 1e-9);
    }

    #[test]
    fn interp_low_band_midpoint() {
        // 0.475 = (0.30 + 0.65) / 2 → t = 0.5 → (10 + 25) / 2 = 17.5
        let p = params_5m();
        assert!((p.bonereaper_interp_usdc(0.475, 10.0) - 17.5).abs() < 1e-6);
    }

    #[test]
    fn interp_high_band_midpoint() {
        // lw_thr=0.95, mid_band = (0.65 + 0.95) / 2 = 0.80
        // t = 0.5 → (25 + 80) / 2 = 52.5
        let p = params_5m();
        assert!((p.bonereaper_interp_usdc(0.80, 10.0) - 52.5).abs() < 1e-6);
    }

    #[test]
    fn interp_monotone_non_decreasing() {
        let p = params_5m();
        let mut prev = 0.0;
        let mut bid = 0.0;
        while bid <= 1.0 {
            let v = p.bonereaper_interp_usdc(bid, 10.0);
            assert!(
                v >= prev - 1e-9,
                "monoton bozuldu @ bid={} ({} < {})",
                bid,
                v,
                prev
            );
            prev = v;
            bid += 0.005;
        }
    }

    #[test]
    fn interp_15m_bot_anchors() {
        let mut p = StrategyParams::default();
        p.bonereaper_size_longshot_usdc = Some(3.0);
        p.bonereaper_size_mid_usdc = Some(7.0);
        p.bonereaper_size_high_usdc = Some(20.0);
        p.bonereaper_late_winner_bid_thr = Some(0.95);
        assert!((p.bonereaper_interp_usdc(0.30, 10.0) - 3.0).abs() < 1e-9);
        assert!((p.bonereaper_interp_usdc(0.65, 10.0) - 7.0).abs() < 1e-9);
        assert!((p.bonereaper_interp_usdc(0.95, 10.0) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn interp_lw_thr_eq_065_safe() {
        // Edge: lw_thr = 0.65 → span clamped to 0.01
        let mut p = StrategyParams::default();
        p.bonereaper_size_longshot_usdc = Some(10.0);
        p.bonereaper_size_mid_usdc = Some(25.0);
        p.bonereaper_size_high_usdc = Some(80.0);
        p.bonereaper_late_winner_bid_thr = Some(0.65);
        // bid >= lw_thr (0.65) → high (high band girişi 0.65 < lw_thr 0.65 değil)
        let v = p.bonereaper_interp_usdc(0.65, 10.0);
        assert!((v - 25.0).abs() < 1e-9 || (v - 80.0).abs() < 1e-9);
    }

    #[test]
    fn shares_const_default_is_disabled() {
        let p = StrategyParams::default();
        assert!((p.bonereaper_size_shares_const() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn shares_const_clamps_negative_to_zero() {
        let mut p = StrategyParams::default();
        p.bonereaper_size_shares_const = Some(-5.0);
        assert!((p.bonereaper_size_shares_const() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn shares_const_passes_through_value() {
        let mut p = StrategyParams::default();
        p.bonereaper_size_shares_const = Some(10.0);
        assert!((p.bonereaper_size_shares_const() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn shares_const_clamps_huge_value() {
        let mut p = StrategyParams::default();
        p.bonereaper_size_shares_const = Some(1_000_000.0);
        assert!((p.bonereaper_size_shares_const() - 10_000.0).abs() < 1e-9);
    }
}
