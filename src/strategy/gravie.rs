//! Gravie V3 (ASYM) — Sinyal-yönlü asimetrik dual-side accumulator.
//!
//! ## Mantık (basit, tek bir cümlede özetlenebilir)
//!
//! Her tick'te EMA-smoothed `effective_score`'un yönünü çıkar. O yöne (**winner**)
//! büyük emir gönder; karşı tarafa (**hedge**) küçük emir gönder — yalnız
//! `avg_up + avg_down < avg_sum_max` (default `0.80`) koşulu sağlanırsa.
//!
//! 1. **Asimetrik sermaye dağılımı.** Winner $15 / Hedge $5 = 3× daha çok share
//!    kazanan tarafa. Bu, "kazanan tarafta daha çok share, kaybeden tarafta az
//!    share" risk profilini sağlar. Yön doğruysa winner artıyor; yön yanlışsa
//!    hedge azken kayıp sınırlı.
//! 2. **avg_sum guard = mat. arbitraj garantisi.** Her dual pair için
//!    `avg_up + avg_down < 0.80` ⇒ pair her durumda min %20 brut marj
//!    (1.0 − avg_sum_max). Hedge yalnız bu koşul yeni alımdan sonra hâlâ
//!    sağlanırsa açılır → hedge yalnız "iyi fiyatlardan" alınır.
//! 3. **Stability filter.** Son `stability_window=3` tick'in smoothed signal
//!    std'si `> 0.5` ise trade atlanır → kararsız (gürültülü) marketlerde
//!    pasif kalınır.
//! 4. **EMA smoothing.** `ema_alpha=0.3` ile signal_score yumuşatılır →
//!    spike-driven yanlış yön kararları azalır.
//! 5. **Ayrı cooldown.** Winner ve hedge için bağımsız cooldown
//!    (`buy_cooldown_ms`, `hedge_cooldown_ms`) → biri diğerini bloklamaz.
//! 6. **T-cutoff.** Kapanışa `t_cutoff_secs=30` kala yeni emir verilmez.
//! 7. **Late-window winner pasif.** Kapanışa `late_winner_pasif_secs=90`
//!    kala WINNER BUY açılmaz; hedge BUY serbest kalır. Bot 91 backtest:
//!    late-flip kayıplarının %63'ü son %20 pencerede gerçekleşiyor — bu
//!    pencerede winner emirleri kapatınca worst loss -$281 → -$238 (%15
//!    düşüş), ROI +9.70% → +11.10%. Hedge serbest çünkü mevcut pair'leri
//!    profit-lock'a daha yakına çekiyor (avg_sum<X arb koşulu altında).
//!
//! ## Reason etiketleri
//!
//! `gravie:winner:{up,down}` — sinyal yönüne BUY (büyük asimetrik emir)
//! `gravie:hedge:{up,down}`  — sinyal karşıtı tarafa BUY (küçük, avg<X gated)
//!
//! ## Bot 91 backtest sonuçları (4 gün, 135 market)
//!
//! | Profil          | PnL      | ROI    | WR  | Worst   | Dual % |
//! |-----------------|----------|--------|-----|---------|--------|
//! | V3 ASYM default | +$2468   | +9.70% | %61 | -$281   | %49    |
//! | Eski (Bot 91)   | -$1300   | -6.34% | %32 | -$~700  | %80    |

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::config::GravieParams;
use crate::types::{OrderType, Outcome, Side};

#[inline]
const fn reason_winner(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:winner:up",
        Outcome::Down => "gravie:winner:down",
    }
}
#[inline]
const fn reason_hedge(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:hedge:up",
        Outcome::Down => "gravie:hedge:down",
    }
}

// ─────────────────────────────────────────────
// FSM State
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum GravieState {
    /// OB henüz hazır değil; ilk tick bekleniyor.
    #[default]
    Idle,
    /// Market aktif — emir döngüsü çalışıyor.
    Active(Box<GravieActive>),
    /// T-cutoff geçildi veya kapanışa çok yakın; pasif kalır.
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GravieActive {
    /// 1-tick gate için son işlem yapılan saniye.
    pub last_acted_secs: u64,
    /// Son winner BUY emrinin verildiği zaman (ms).
    #[serde(default)]
    pub last_winner_buy_ms: u64,
    /// Son hedge BUY emrinin verildiği zaman (ms).
    #[serde(default)]
    pub last_hedge_buy_ms: u64,
    /// EMA smoothed signal (centered: smoothed = state + 5).
    #[serde(default)]
    pub ema_state: Option<f64>,
    /// Son N tick'in smoothed signal'leri (stability filter için).
    #[serde(default)]
    pub signal_history: Vec<f64>,
}

// ─────────────────────────────────────────────
// Karar motoru
// ─────────────────────────────────────────────

pub struct GravieEngine;

impl GravieEngine {
    pub fn decide(state: GravieState, ctx: &StrategyContext<'_>) -> (GravieState, Decision) {
        let p = GravieParams::from_strategy_params(ctx.strategy_params);
        let to_end = ctx.market_remaining_secs.unwrap_or(f64::MAX);
        let rel_secs = (ctx.now_ms / 1000).saturating_sub(ctx.start_ts);

        match state {
            // ── Pencere kapandı / cutoff geçti ──────────────────────────────
            GravieState::Stopped => (GravieState::Stopped, Decision::NoOp),

            // ── OB hazırlığı ────────────────────────────────────────────────
            GravieState::Idle => {
                let book_ready = ctx.up_best_bid > 0.0
                    && ctx.up_best_ask > 0.0
                    && ctx.down_best_bid > 0.0
                    && ctx.down_best_ask > 0.0;
                if !book_ready {
                    return (GravieState::Idle, Decision::NoOp);
                }
                let active = GravieActive {
                    last_acted_secs: u64::MAX,
                    last_winner_buy_ms: 0,
                    last_hedge_buy_ms: 0,
                    ema_state: None,
                    signal_history: Vec::with_capacity(p.stability_window as usize),
                };
                (GravieState::Active(Box::new(active)), Decision::NoOp)
            }

            // ── Aktif emir döngüsü ──────────────────────────────────────────
            GravieState::Active(mut st) => {
                // T-cutoff: kapanışa yakın → pasif.
                if to_end <= p.t_cutoff_secs {
                    return (GravieState::Stopped, cancel_all_open_gravie(ctx.open_orders));
                }

                // tick_interval gate.
                if !rel_secs.is_multiple_of(p.tick_interval_secs) {
                    return (GravieState::Active(st), Decision::NoOp);
                }
                if rel_secs == st.last_acted_secs {
                    return (GravieState::Active(st), Decision::NoOp);
                }
                st.last_acted_secs = rel_secs;

                // OB güvenliği.
                if ctx.up_best_ask <= 0.0 || ctx.down_best_ask <= 0.0 {
                    return (GravieState::Active(st), Decision::NoOp);
                }
                if ctx.effective_score <= 0.0 || !ctx.signal_ready {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // ── Signal smoothing (EMA) ──────────────────────────────────
                // signal_score 0..10 → centered (-5..+5) → smooth → +5 geri.
                let centered = ctx.effective_score - 5.0;
                let smoothed_centered = match st.ema_state {
                    None => centered,
                    Some(prev) => p.ema_alpha * centered + (1.0 - p.ema_alpha) * prev,
                };
                st.ema_state = Some(smoothed_centered);
                let smoothed = smoothed_centered + 5.0;

                // ── Stability filter ────────────────────────────────────────
                // Son N tick'in std'si > eşik ise pasif kal (kararsız market).
                if p.stability_window > 0 {
                    if st.signal_history.len() >= p.stability_window as usize {
                        st.signal_history.remove(0);
                    }
                    st.signal_history.push(smoothed);
                    if st.signal_history.len() < p.stability_window as usize {
                        return (GravieState::Active(st), Decision::NoOp);
                    }
                    let n = st.signal_history.len() as f64;
                    let mean = st.signal_history.iter().sum::<f64>() / n;
                    let var = st.signal_history.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
                    if var.sqrt() > p.stability_max_std {
                        return (GravieState::Active(st), Decision::NoOp);
                    }
                }

                // ── Sinyal yönü → winner side ──────────────────────────────
                let winner = if smoothed > p.signal_up_threshold {
                    Outcome::Up
                } else if smoothed < p.signal_down_threshold {
                    Outcome::Down
                } else {
                    return (GravieState::Active(st), Decision::NoOp);
                };
                let hedge = winner.opposite();

                let mut orders: Vec<PlannedOrder> = Vec::with_capacity(2);

                // Late-window: kapanışa yakın → winner BUY engellenir, hedge serbest.
                let winner_allowed =
                    p.late_winner_pasif_secs <= 0.0 || to_end > p.late_winner_pasif_secs;

                // ── Winner BUY (büyük emir, asimetrik) ──────────────────────
                let winner_ask = ctx.best_ask(winner);
                if winner_allowed
                    && winner_ask > 0.0
                    && winner_ask <= p.winner_max_price
                    && ctx.now_ms.saturating_sub(st.last_winner_buy_ms) >= p.buy_cooldown_ms
                {
                    if let Some(order) = try_buy(
                        ctx,
                        winner,
                        winner_ask,
                        p.winner_order_usdc,
                        &p,
                        reason_winner(winner),
                    ) {
                        orders.push(order);
                        st.last_winner_buy_ms = ctx.now_ms;
                    }
                }

                // ── Hedge BUY (küçük emir, sıkı avg_sum gated) ──────────────
                let winner_filled = match winner {
                    Outcome::Up => ctx.metrics.up_filled,
                    Outcome::Down => ctx.metrics.down_filled,
                };
                let hedge_ask = ctx.best_ask(hedge);
                if hedge_ask > 0.0
                    && hedge_ask <= p.hedge_max_price
                    && winner_filled > 0.0
                    && ctx.now_ms.saturating_sub(st.last_hedge_buy_ms) >= p.hedge_cooldown_ms
                {
                    if let Some(order) = try_buy(
                        ctx,
                        hedge,
                        hedge_ask,
                        p.hedge_order_usdc,
                        &p,
                        reason_hedge(hedge),
                    ) {
                        orders.push(order);
                        st.last_hedge_buy_ms = ctx.now_ms;
                    }
                }

                let decision = if orders.is_empty() {
                    Decision::NoOp
                } else {
                    Decision::PlaceOrders(orders)
                };
                (GravieState::Active(st), decision)
            }
        }
    }
}

// ─────────────────────────────────────────────
// Yardımcılar
// ─────────────────────────────────────────────

/// FAK BUY emri planla. avg_sum kontrolü (yeni alımdan sonra `avg_up +
/// avg_down < avg_sum_max` olmalı) burada yapılır — winner ve hedge için
/// aynı koşul: pair açıkken her zaman matematiksel arbitraj garantisi.
///
/// Pozisyon yokken (henüz dual değilken) avg_sum kontrolü pas geçilir;
/// yalnız fiyat tavanı / size kuralları geçerlidir.
fn try_buy(
    ctx: &StrategyContext<'_>,
    side: Outcome,
    ask: f64,
    order_usdc: f64,
    p: &GravieParams,
    reason: &'static str,
) -> Option<PlannedOrder> {
    if ask <= 0.0 || ask > 1.0 {
        return None;
    }
    let raw_size = (order_usdc / ask).ceil();
    let mut size = if p.max_fak_size > 0.0 {
        raw_size.min(p.max_fak_size)
    } else {
        raw_size
    };

    let m = ctx.metrics;
    let (own_filled, own_spent, opp_filled, opp_spent) = match side {
        Outcome::Up => (
            m.up_filled,
            m.avg_up * m.up_filled,
            m.down_filled,
            m.avg_down * m.down_filled,
        ),
        Outcome::Down => (
            m.down_filled,
            m.avg_down * m.down_filled,
            m.up_filled,
            m.avg_up * m.up_filled,
        ),
    };

    if p.max_size_per_side > 0.0 {
        size = size.min((p.max_size_per_side - own_filled).max(0.0));
    }
    if size <= 0.0 || size * ask < ctx.api_min_order_size {
        return None;
    }

    // avg_sum gating — yalnız karşı tarafta pozisyon varken anlamlı.
    if opp_filled > 0.0 {
        let new_own_avg = (own_spent + size * ask) / (own_filled + size);
        let opp_avg = opp_spent / opp_filled;
        if new_own_avg + opp_avg >= p.avg_sum_max {
            return None;
        }
    }

    Some(PlannedOrder {
        outcome: side,
        token_id: ctx.token_id(side).to_string(),
        side: Side::Buy,
        price: ask,
        size,
        order_type: OrderType::Fak,
        reason: reason.to_string(),
    })
}

/// T-cutoff anında açık `gravie:` emirlerini iptal et (FAK olmayan kalmışsa).
fn cancel_all_open_gravie(open_orders: &[OpenOrder]) -> Decision {
    let ids: Vec<String> = open_orders
        .iter()
        .filter(|o| o.reason.starts_with("gravie:"))
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(ids)
    }
}
