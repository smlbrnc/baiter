//! Bonereaper stratejisi — Polymarket "Bonereaper" wallet
//! (`0xeebde7a0e019a63e6b476eb425505b7b3e6eba30`) davranış kopyası.
//!
//! Strateji **signal-driven değildir**; pure order-book reactive martingale +
//! late winner injection. `data/genel.log`'da 4000 BUY trade analizi sonucu:
//! real bot triple gate / composite signal / profit lock / freeze kullanmıyor;
//! her tick orderbook'a bakıp bid değişen tarafa BUY ediyor, kapanışa <30 sn
//! kala kazanan tarafa massive inject yapıyor.
//!
//! ## Karar zinciri
//!
//! 1. **Window**: `now ∈ [start, end]`; OB ready.
//! 2. **LATE WINNER** (`t_to_end ≤ secs && max(bid) ≥ thr`): winner tarafa
//!    `late_winner_usdc` notional taker BUY @ ask. Cooldown bypass.
//! 3. **Cooldown** (`now − last_buy < buy_cooldown_ms`): NoOp.
//! 4. **Yön seçimi**:
//!    - `|up_filled − down_filled| > imbalance_thr` → weaker side
//!    - aksi: `|Δup_bid|` vs `|Δdn_bid|` → büyük delta tarafı (`ob_driven`)
//! 5. **Min/max price filter**: `bid ∉ [min, max]` → NoOp.
//! 6. **Dinamik size** (USDC notional bid bucket'ına göre):
//!    - `bid ≤ 0.30`: longshot
//!    - `0.30 < bid ≤ 0.85`: mid
//!    - `bid > 0.85`: high
//! 7. **avg_sum soft cap** (`new_avg + opp_avg > max_avg_sum`): NoOp.
//! 8. **Place taker BUY @ ask** (GTC limit, anında fill).
//!
//! ## Reason etiketleri
//!
//! `bonereaper:buy:{up,down}` — normal BUY.
//! `bonereaper:lw:{up,down}` — late winner injection.

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
    /// Önceki tick UP bid (delta hesabı).
    #[serde(default)]
    pub last_up_bid: f64,
    /// Önceki tick DOWN bid.
    #[serde(default)]
    pub last_dn_bid: f64,
    /// Late winner injection sayacı (telemetri/log için).
    #[serde(default)]
    pub lw_injections: u32,
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
                    last_up_bid: ctx.up_best_bid,
                    last_dn_bid: ctx.down_best_bid,
                    lw_injections: 0,
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

                // ── LATE WINNER ─────────────────────────────────────────
                // T ≤ X sn ve max(bid) ≥ thr → winner tarafa massive taker BUY.
                // `lw_max_per_session` ile session başına sınırlı (default 1) —
                // real bot ~0.2-0.33 big-bet/market pattern'i. 0 = sınırsız.
                let lw_secs = p.bonereaper_late_winner_secs() as f64;
                let lw_usdc = p.bonereaper_late_winner_usdc();
                let lw_thr = p.bonereaper_late_winner_bid_thr();
                let lw_max = p.bonereaper_lw_max_per_session();
                let lw_quota_ok = lw_max == 0 || st.lw_injections < lw_max;
                if lw_usdc > 0.0
                    && lw_secs > 0.0
                    && to_end > 0.0
                    && to_end <= lw_secs
                    && lw_quota_ok
                {
                    let (winner, w_bid, w_ask) = if ctx.up_best_bid >= ctx.down_best_bid {
                        (Outcome::Up, ctx.up_best_bid, ctx.up_best_ask)
                    } else {
                        (Outcome::Down, ctx.down_best_bid, ctx.down_best_ask)
                    };
                    if w_bid >= lw_thr && w_ask > 0.0 {
                        let size = (lw_usdc / w_ask).ceil();
                        if let Some(o) = make_buy(ctx, winner, w_ask, size, reason_lw(winner)) {
                            st.last_buy_ms = ctx.now_ms;
                            st.lw_injections = st.lw_injections.saturating_add(1);
                            st.last_up_bid = ctx.up_best_bid;
                            st.last_dn_bid = ctx.down_best_bid;
                            return (
                                BonereaperState::Active(st),
                                Decision::PlaceOrders(vec![o]),
                            );
                        }
                    }
                }

                // ── COOLDOWN ────────────────────────────────────────────
                let cd_ms = p.bonereaper_buy_cooldown_ms();
                if st.last_buy_ms > 0 && ctx.now_ms.saturating_sub(st.last_buy_ms) < cd_ms {
                    st.last_up_bid = ctx.up_best_bid;
                    st.last_dn_bid = ctx.down_best_bid;
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── YÖN SEÇİMİ ──────────────────────────────────────────
                let m = ctx.metrics;
                let imb = m.up_filled - m.down_filled;
                let imb_thr = p.bonereaper_imbalance_thr();
                let dir = if imb.abs() > imb_thr {
                    // Weaker side rebalance
                    if imb > 0.0 { Outcome::Down } else { Outcome::Up }
                } else {
                    // ob_driven: bid'i daha çok değişen taraf
                    let d_up = (ctx.up_best_bid - st.last_up_bid).abs();
                    let d_dn = (ctx.down_best_bid - st.last_dn_bid).abs();
                    if d_up == 0.0 && d_dn == 0.0 {
                        // Delta yoksa: bid'i yüksek olan taraf (winner momentum)
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
                };

                // Bid history güncelle (her tick)
                st.last_up_bid = ctx.up_best_bid;
                st.last_dn_bid = ctx.down_best_bid;

                let bid = ctx.best_bid(dir);
                let ask = ctx.best_ask(dir);
                if bid <= 0.0 || ask <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Min/max price filter (session config)
                if bid < ctx.min_price || bid > ctx.max_price {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Dinamik size
                let usdc = if bid <= 0.30 {
                    p.bonereaper_size_longshot_usdc()
                } else if bid <= 0.85 {
                    p.bonereaper_size_mid_usdc()
                } else {
                    p.bonereaper_size_high_usdc()
                };
                if usdc <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }
                let size = (usdc / ask).ceil();

                // avg_sum soft cap — yeni alımdan SONRA cur_avg + opp_avg
                let max_avg_sum = p.bonereaper_max_avg_sum();
                let (cur_filled, cur_avg, opp_filled, opp_avg) = match dir {
                    Outcome::Up => (m.up_filled, m.avg_up, m.down_filled, m.avg_down),
                    Outcome::Down => (m.down_filled, m.avg_down, m.up_filled, m.avg_up),
                };
                if opp_filled > 0.0 {
                    let new_avg = if cur_filled > 0.0 {
                        (cur_avg * cur_filled + ask * size) / (cur_filled + size)
                    } else {
                        ask
                    };
                    if new_avg + opp_avg > max_avg_sum {
                        return (BonereaperState::Active(st), Decision::NoOp);
                    }
                }

                if let Some(o) = make_buy(ctx, dir, ask, size, reason_buy(dir)) {
                    st.last_buy_ms = ctx.now_ms;
                    return (
                        BonereaperState::Active(st),
                        Decision::PlaceOrders(vec![o]),
                    );
                }
                (BonereaperState::Active(st), Decision::NoOp)
            }
        }
    }
}

/// BUY GTC limit emir. `price ≤ 0`, `size ≤ 0` veya notional < min → `None`.
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
        order_type: OrderType::Gtc,
        reason: reason.to_string(),
    })
}
