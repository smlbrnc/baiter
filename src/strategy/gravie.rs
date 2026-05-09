//! Gravie stratejisi — Bot 66 (`Lively-Authenticity`) davranış kopyası.
//!
//! ## Çalışma mantığı
//!
//! Her **`TICK_INTERVAL_SECS` = 5 sn**'de bir karar döngüsü çalışır.
//! Bot 66 mikro davranış sondajından (data/bot66_micro_analysis.json) türetilen
//! kuralları izler:
//!
//! 1. **BUY-only dual-side** — pozisyondan SELL ile çıkmaz; hem Up hem Down BUY.
//!    Real bot: 4000/4000 trade BUY; %80 market'te dual-side.
//! 2. **Mid-price civarı entry** — ilk entry medyan 0.50, p95 ≤ 0.78.
//!    Eşik: `ENTRY_ASK_CEILING = 0.85` üstü ask'lar skip edilir (uçtaki market).
//! 3. **Reaktif ucuz-taraf** — `argmin(up_ask, dn_ask)` tarafına BUY.
//! 4. **Second-leg guard** — ilk leg açıldıktan sonra karşı leg için:
//!    - Karşı taraf ask `≤ SECOND_LEG_OPP_TRIGGER = 0.55` olduğunda **ya da**
//!    - `SECOND_LEG_GUARD_MS = 38000ms` geçtikten sonra
//!    karşı tarafa ikinci leg açılır. Real bot: 5m median 38 sn, opp_first_px 0.50.
//! 5. **FAK taker** — emirler Fill-And-Kill. Same-second multi-fill %35 ile uyumlu.
//! 6. **Buy cooldown** — ardışık BUY'lar arası min `BUY_COOLDOWN_MS = 4000ms`.
//!    Real bot: medyan inter-arrival 4-5 sn.
//! 7. **T-cutoff** — kapanışa `T_CUTOFF_SECS = 90 sn` kala yeni emir verme.
//!    Real bot: 5m'de T-78 medyan, %58 case T-90 öncesi durur.
//! 8. **Balance bias** — `balance = min/max < BALANCE_REBALANCE` (0.45) ise
//!    az olan tarafa zorunlu yönel (entry ceiling'i %20 daha esnek uygula).
//! 9. **Sum-avg guard** — pair lock'a yakın market'lerde (`sum_avg ≥ 1.20`)
//!    yeni emir verme; daha fazla harcamak fayda etmez (Real bot top-loss
//!    pattern: balance=1.0 + sum_avg=1.12 = garanti zarar).
//! 10. **PATCH D — Signal Gate** — `effective_score` yön filtresi.
//!     `signal_gate_enabled=true` (default) iken:
//!     - `score > signal_up_threshold` (default 5.5) → sadece UP open/accum
//!     - `score < signal_down_threshold` (default 4.5) → sadece DOWN open/accum
//!     - karasız zone → yeni emir YOK (sadece rebalance signal yön ile uyumluysa)
//!
//!     Bot 91 (4 günlük dryrun, 131 resolved market) analizi: gate kapalıyken
//!     accum trade'lerinin %68'i kaybeden tarafa yığılıyor → WR %32, ROI -%6.34.
//!     Aynı pencerede bonereaper (signal-driven) WR %76+, ROI breakeven/pozitif.
//!     Saf "ucuz tarafı al" mantığı, market'in olasılık-fiyat bilgisini ters
//!     okuyor; sinyal yön filtresi bu hatayı düzeltir.
//!
//! ## Reason etiketleri
//!
//! `gravie:open:{up,down}`      — first leg (henüz pozisyon yok)
//! `gravie:flip:{up,down}`      — second leg (karşı tarafa ilk geçiş)
//! `gravie:accum:{up,down}`     — devam alımı (aynı yönde, dengeli)
//! `gravie:rebalance:{up,down}` — balance bias zorunlu yeniden dengeleme

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::config::GravieParams;
use crate::types::{OrderType, Outcome, Side};

// Reason etiketleri — `format!()` allocation'larını eler.
#[inline]
const fn reason_open(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:open:up",
        Outcome::Down => "gravie:open:down",
    }
}
#[inline]
const fn reason_flip(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:flip:up",
        Outcome::Down => "gravie:flip:down",
    }
}
#[inline]
const fn reason_accum(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:accum:up",
        Outcome::Down => "gravie:accum:down",
    }
}
#[inline]
const fn reason_rebalance(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "gravie:rebalance:up",
        Outcome::Down => "gravie:rebalance:down",
    }
}

// ─────────────────────────────────────────────
// FSM State
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GravieState {
    /// OB henüz hazır değil; ilk tick bekleniyor.
    Idle,
    /// Market aktif — emir döngüsü çalışıyor.
    Active(Box<GravieActive>),
    /// T-cutoff geçildi veya kapanışa çok yakın; pasif kalır.
    Stopped,
}

impl Default for GravieState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GravieActive {
    /// 1-tick gate için son işlem yapılan saniye.
    pub last_acted_secs: u64,
    /// Son BUY emrinin verildiği zaman (ms). Cooldown için.
    #[serde(default)]
    pub last_buy_ms: u64,
    /// İlk leg'in açıldığı side (None = henüz hiç pozisyon yok).
    #[serde(default)]
    pub first_leg_side: Option<Outcome>,
    /// İlk leg'in açıldığı zaman (ms). Second-leg guard için.
    #[serde(default)]
    pub first_leg_ms: u64,
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
                // İlk pozisyondan miras alma — eğer metric'lerde önceden fill varsa
                // (örn. restart) first_leg_side'ı çıkar.
                let inferred_first_leg = if ctx.metrics.up_filled > 0.0
                    && ctx.metrics.up_filled >= ctx.metrics.down_filled
                {
                    Some(Outcome::Up)
                } else if ctx.metrics.down_filled > 0.0 {
                    Some(Outcome::Down)
                } else {
                    None
                };
                let active = GravieActive {
                    last_acted_secs: u64::MAX,
                    last_buy_ms: 0,
                    first_leg_side: inferred_first_leg,
                    first_leg_ms: 0,
                };
                (GravieState::Active(Box::new(active)), Decision::NoOp)
            }

            // ── Aktif emir döngüsü ──────────────────────────────────────────
            GravieState::Active(mut st) => {
                // T-cutoff: kapanışa yakın → pasif.
                if to_end <= p.t_cutoff_secs {
                    return (GravieState::Stopped, cancel_all_open_gravie(ctx));
                }

                // 1-sn × tick_interval_secs gate.
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

                // Cooldown.
                if st.last_buy_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_buy_ms) < p.buy_cooldown_ms
                {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // Sum-avg guard — pair zaten pahalı, daha fazla harcama.
                let m = ctx.metrics;
                if m.up_filled > 0.0 && m.down_filled > 0.0 && m.avg_sum() >= p.sum_avg_ceiling {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // PATCH A — Lose-side ASK cap (asymmetric trend reversal guard).
                // Bir tarafın ask'ı eşiğin üstüne çıktığında market o tarafı %X+
                // olası görüyor. "Ucuz" karşı tarafa daha çok yatırım = collapse riski.
                if ctx.up_best_ask.max(ctx.down_best_ask) >= p.opp_ask_stop_threshold {
                    return (GravieState::Active(st), Decision::NoOp);
                }

                // Karar: hangi side, hangi reason?
                let plan = decide_buy(&st, ctx, &p);
                let Some(buy_plan) = plan else {
                    return (GravieState::Active(st), Decision::NoOp);
                };
                let order = match make_fak_buy(
                    ctx,
                    buy_plan.dir,
                    buy_plan.price,
                    buy_plan.reason,
                    p.max_fak_size,
                ) {
                    Some(o) => o,
                    None => return (GravieState::Active(st), Decision::NoOp),
                };

                // State güncellemeleri.
                st.last_buy_ms = ctx.now_ms;
                if st.first_leg_side.is_none() {
                    st.first_leg_side = Some(buy_plan.dir);
                    st.first_leg_ms = ctx.now_ms;
                }

                (
                    GravieState::Active(st),
                    Decision::PlaceOrders(vec![order]),
                )
            }
        }
    }
}

// ─────────────────────────────────────────────
// BUY karar mantığı
// ─────────────────────────────────────────────

struct BuyPlan {
    dir: Outcome,
    price: f64,
    reason: &'static str,
}

/// Bir sonraki BUY hedefini seçer:
///
/// 1. **Signal Gate (PATCH D)**: `effective_score` yönü çıkar (UP / DOWN /
///    NEUTRAL). NEUTRAL'de yeni emir verilmez; sadece signal yönü ile uyumlu
///    rebalance açıktır.
/// 2. **Rebalance bias**: Pozisyon dengesizse (`balance < balance_rebalance`)
///    az tarafa BUY — fakat signal gate açıkken yalnız signal yönü weak side
///    ile aynıysa rebalance trigger olur (karşı yöndeki "kaybeden tarafı
///    pump etme" hatasını engeller).
/// 3. **İkinci leg (flip)**: İlk leg açık + karşı taraf ucuz veya guard süresi
///    geçti. Signal gate açıkken yalnız signal yönü opp ile uyumluysa flip.
/// 4. **Accum / Open**: Signal gate açıkken signal yönüne BUY; kapalıyken
///    klasik "en ucuz ask'a BUY".
fn decide_buy(
    st: &GravieActive,
    ctx: &StrategyContext<'_>,
    p: &GravieParams,
) -> Option<BuyPlan> {
    let m = ctx.metrics;
    let up_ask = ctx.up_best_ask;
    let dn_ask = ctx.down_best_ask;

    // ── PATCH D — Signal Gate ──────────────────────────────────────────────
    // `signal_gate_enabled=true` ve `signal_ready` iken effective_score yönünü
    // çıkar; karasız zone'da (UP_THR ile DOWN_THR arası) bot pasif kalır
    // (rebalance hariç). signal_ready=false ise gate atlanır (warmup).
    let signal_dir: Option<Outcome> =
        if p.signal_gate_enabled && ctx.signal_ready {
            if ctx.effective_score > p.signal_up_threshold {
                Some(Outcome::Up)
            } else if ctx.effective_score < p.signal_down_threshold {
                Some(Outcome::Down)
            } else {
                // Karasız sinyal — yeni open/accum/flip YOK. Mevcut pozisyon
                // dengesizliği için rebalance kontrolü aşağıda yapılır.
                return decide_rebalance_only(ctx, p, None);
            }
        } else {
            None
        };

    // ── Rebalance bias ─────────────────────────────────────────────────────
    // Bot 91 reason analizi: rebalance %62 doğru tarafa gidiyor (en güvenilir).
    // Signal gate açıkken weak_side, signal yönü ile uyumluysa devam eder;
    // aksi halde rebalance atlanır ve aşağıdaki signal-zorla buy çalışır.
    if let Some(plan) = check_rebalance(ctx, p, signal_dir) {
        return Some(plan);
    }

    // ── İkinci leg (first → opposite) ──────────────────────────────────────
    if let Some(first_side) = st.first_leg_side {
        let opp = first_side.opposite();
        let opp_filled = match opp {
            Outcome::Up => m.up_filled,
            Outcome::Down => m.down_filled,
        };
        if opp_filled <= 0.0 {
            let opp_ask = match opp {
                Outcome::Up => up_ask,
                Outcome::Down => dn_ask,
            };
            let guard_passed =
                ctx.now_ms.saturating_sub(st.first_leg_ms) >= p.second_leg_guard_ms;
            let opp_cheap = opp_ask > 0.0 && opp_ask <= p.second_leg_opp_trigger;
            // Signal gate açıkken: flip sadece signal opp yönünde olursa
            // (karşı tarafa hedge için BUY, kaybeden tarafı pump değil).
            let flip_signal_ok = signal_dir.is_none_or(|sd| sd == opp);
            if flip_signal_ok
                && (guard_passed || opp_cheap)
                && opp_ask > 0.0
                && opp_ask <= p.entry_ask_ceiling
            {
                return Some(BuyPlan {
                    dir: opp,
                    price: opp_ask,
                    reason: reason_flip(opp),
                });
            }
            // Aynı yön accum: signal gate açıkken sadece signal == first_side
            let accum_signal_ok = signal_dir.is_none_or(|sd| sd == first_side);
            if accum_signal_ok {
                let first_ask = match first_side {
                    Outcome::Up => up_ask,
                    Outcome::Down => dn_ask,
                };
                if first_ask > 0.0 && first_ask <= p.entry_ask_ceiling {
                    return Some(BuyPlan {
                        dir: first_side,
                        price: first_ask,
                        reason: reason_accum(first_side),
                    });
                }
            }
            return None;
        }
    }

    // ── İlk leg / accumulation ─────────────────────────────────────────────
    // Signal gate açıkken: signal yönüne git (en ucuz fiyat değil).
    // Kapalıyken: klasik "en ucuz ask'a BUY".
    let target_dir = match signal_dir {
        Some(d) => d,
        None => {
            if up_ask > 0.0 && (dn_ask <= 0.0 || up_ask <= dn_ask) {
                Outcome::Up
            } else {
                Outcome::Down
            }
        }
    };
    let target_ask = match target_dir {
        Outcome::Up => up_ask,
        Outcome::Down => dn_ask,
    };
    if target_ask > 0.0 && target_ask <= p.entry_ask_ceiling {
        let reason = match st.first_leg_side {
            None => reason_open(target_dir),
            Some(_) => reason_accum(target_dir),
        };
        return Some(BuyPlan {
            dir: target_dir,
            price: target_ask,
            reason,
        });
    }

    None
}

/// Signal-gate karasız zone'unda yalnızca rebalance'a izin ver. Yeni open/accum
/// bloklanır; pozisyon dengesizliği zayıf tarafa BUY ile düzeltilir.
#[inline]
fn decide_rebalance_only(
    ctx: &StrategyContext<'_>,
    p: &GravieParams,
    signal_dir: Option<Outcome>,
) -> Option<BuyPlan> {
    check_rebalance(ctx, p, signal_dir)
}

/// Rebalance kontrolü — pozisyon `balance = min/max < balance_rebalance` ise
/// zayıf tarafa BUY planla. `signal_dir` verilmişse weak_side ile uyumlu olmak
/// zorunda; aksi halde `None` döner (signal zıt yöne işaret ediyorsa zayıf
/// tarafı pump etmek = kaybeden tarafa yığılma).
#[inline]
fn check_rebalance(
    ctx: &StrategyContext<'_>,
    p: &GravieParams,
    signal_dir: Option<Outcome>,
) -> Option<BuyPlan> {
    let m = ctx.metrics;
    if m.up_filled <= 0.0 || m.down_filled <= 0.0 {
        return None;
    }
    let max_filled = m.up_filled.max(m.down_filled);
    let min_filled = m.up_filled.min(m.down_filled);
    let balance = if max_filled > 0.0 {
        min_filled / max_filled
    } else {
        0.0
    };
    if balance >= p.balance_rebalance {
        return None;
    }
    let weak_side = if m.up_filled < m.down_filled {
        Outcome::Up
    } else {
        Outcome::Down
    };
    if let Some(sd) = signal_dir {
        if sd != weak_side {
            return None;
        }
    }
    let weak_ask = match weak_side {
        Outcome::Up => ctx.up_best_ask,
        Outcome::Down => ctx.down_best_ask,
    };
    if weak_ask > 0.0 && weak_ask <= p.entry_ask_ceiling * p.rebalance_ceiling_multiplier {
        return Some(BuyPlan {
            dir: weak_side,
            price: weak_ask,
            reason: reason_rebalance(weak_side),
        });
    }
    None
}

// ─────────────────────────────────────────────
// Yardımcılar
// ─────────────────────────────────────────────

/// FAK (Fill-And-Kill) BUY emir. Anında fill olmazsa iptal — multi-fill burst
/// pattern'ine uygun. `size = ceil(order_usdc / price)` (price-aware sizing).
///
/// PATCH C — `max_fak_size > 0` ise size üstten cap'lenir; düşen fiyatlarda
/// (örn. price=0.05 → 200 share) tek emirle aşırı likidite emilmesini önler.
fn make_fak_buy(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    reason: &'static str,
    max_fak_size: f64,
) -> Option<PlannedOrder> {
    if price <= 0.0 || price > 1.0 {
        return None;
    }
    let raw_size = (ctx.order_usdc / price).ceil();
    let size = if max_fak_size > 0.0 {
        raw_size.min(max_fak_size)
    } else {
        raw_size
    };
    if size <= 0.0 {
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

/// T-cutoff anında açık `gravie:` emirlerini iptal et (eğer FAK olmayan kalmışsa).
fn cancel_all_open_gravie(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
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
