//! Elis stratejisi — BAITER pair trading.
//!
//! Saf fonksiyon: `(state, ctx) → (next_state, decision)`. Matematik
//! [`super::metrics`], fiyat eps'i [`super::common::requote_threshold`].
//! `MarketZone` (deep/normal/agg/fak/stop) **kullanılmaz** — strateji
//! spread regime + state machine + balance ratio ile çalışır.
//!
//! Karar sırası:
//!   1. `Locked` → `NoOp` (geri dönüş yok).
//!   2. Book hazır değilse → `Idle`.
//!   3. `pair_cost ≤ avg_threshold && balance_ratio ≤ balance_lock` → `Locked`.
//!   4. `balance_ratio > balance_urgent` → eksik tarafa 2× ladder.
//!   5. Normal plan: `regime × score_weights` → ladder.
//!   6. Open orders reconcile (yalnız `elis:*` reason'lı emirler).

use serde::{Deserialize, Serialize};

use super::common::{requote_threshold, Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::types::{Outcome, OrderType, Side};

/// Spread regime üst sınırları (fiyat ölçeği, 0.01 = 1¢). Tick yarısı eklenerek
/// kenar fiyatlar doğru regime'e düşer (`0.01 → Tight`, `0.02 → Medium`, …).
const TIGHT_MAX: f64 = 0.011;
const MEDIUM_MAX: f64 = 0.021;
const WIDE_MAX: f64 = 0.041;

/// Ladder split pct'leri — toplam = 1.0. Seviye 0 best-bid'e (Wide'da mid'e)
/// oturur, sonraki seviyeler `base - k*tick`.
const TIGHT_LADDER: &[f64] = &[0.40, 0.30, 0.20, 0.10];
const MEDIUM_LADDER: &[f64] = &[0.50, 0.30, 0.20];
const WIDE_LADDER: &[f64] = &[0.70, 0.30];

/// Extreme regime: ask + premium FAK (taker), bid - discount deep maker.
const EXTREME_TAKER_PREMIUM: f64 = 0.03;
const EXTREME_MAKER_DISCOUNT: f64 = 0.10;

/// Hedge-urgent: eksik tarafın ladder ağırlık çarpanı.
const HEDGE_URGENT_BOOST: f64 = 2.0;

const REASON_PREFIX: &str = "elis:";

/// Elis FSM. `Resolved` state'i yok — engine `MarketResolved`'da session'ı
/// kapatıp `cancel_all_open` çağırır (bkz. `bot/window.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ElisState {
    /// Book hazır değil.
    #[default]
    Idle,
    /// En az bir tarafta fill yok; ladder döşeniyor.
    Acquiring,
    /// İki tarafta da fill var ama henüz lock şartı yok.
    Hedging,
    /// `pair_cost ≤ avg_threshold && balance_ratio ≤ balance_lock`. Geri dönüş yok.
    Locked,
}

pub struct ElisEngine;

impl ElisEngine {
    pub fn decide(state: ElisState, ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        if state == ElisState::Locked {
            return (ElisState::Locked, Decision::NoOp);
        }

        if ctx.up_best_bid <= 0.0 || ctx.down_best_bid <= 0.0 {
            return (ElisState::Idle, Decision::NoOp);
        }

        let m = ctx.metrics;
        let pair_cost = m.pair_cost();
        let balance_ratio = m.balance_ratio();
        let balance_lock = ctx.strategy_params.balance_lock_or_default();

        if pair_cost <= ctx.avg_threshold && balance_ratio <= balance_lock {
            let cancels = elis_open_ids(ctx);
            let dec = if cancels.is_empty() {
                Decision::NoOp
            } else {
                Decision::CancelOrders(cancels)
            };
            return (ElisState::Locked, dec);
        }

        let balance_urgent = ctx.strategy_params.balance_urgent_or_default();
        if balance_ratio.is_finite() && balance_ratio > balance_urgent {
            let plan = build_hedge_urgent_plan(ctx);
            return (ElisState::Hedging, reconcile_with_open(ctx, plan));
        }

        let plan = build_plan(ctx);
        let decision = reconcile_with_open(ctx, plan);
        let next = if m.up_filled > 0.0 && m.down_filled > 0.0 {
            ElisState::Hedging
        } else {
            ElisState::Acquiring
        };
        (next, decision)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Regime {
    Tight,
    Medium,
    Wide,
    Extreme,
}

impl Regime {
    /// Maker ladder pct'leri (Extreme'de tek emir tüm weight'i alır).
    fn ladder(self) -> &'static [f64] {
        match self {
            Self::Tight => TIGHT_LADDER,
            Self::Medium => MEDIUM_LADDER,
            Self::Wide => WIDE_LADDER,
            Self::Extreme => &[1.0],
        }
    }
}

/// İki taraftan **geniş** olan spread regime'i belirler — tek-taraflı dar
/// spread stratejiyi Tight'a düşürmemeli.
fn classify_regime(spread_up: f64, spread_down: f64) -> Regime {
    let s = spread_up.max(spread_down);
    if s <= TIGHT_MAX {
        Regime::Tight
    } else if s <= MEDIUM_MAX {
        Regime::Medium
    } else if s <= WIDE_MAX {
        Regime::Wide
    } else {
        Regime::Extreme
    }
}

/// Score → `(up_w, down_w)`. Merkez (3 ≤ s < 5) eşit. Yön kararı **vermez**;
/// yalnızca UP/DOWN ladder share'lerinin nominal ağırlığını ayarlar.
fn score_weights(s: f64) -> (f64, f64) {
    match s {
        s if s >= 7.0 => (0.5, 1.5),
        s if s >= 5.0 => (1.3, 0.7),
        s if s >= 3.0 => (1.0, 1.0),
        _ => (1.4, 0.6),
    }
}

fn weight_for(side: Outcome, score: f64) -> f64 {
    let (up_w, down_w) = score_weights(score);
    match side {
        Outcome::Up => up_w,
        Outcome::Down => down_w,
    }
}

/// Plan-içi geçici emir tipi (Elis'e özel).
#[derive(Debug, Clone)]
struct PlanOrder {
    outcome: Outcome,
    price: f64,
    size: f64,
    order_type: OrderType,
    reason: String,
}

fn current_regime(ctx: &StrategyContext<'_>) -> Regime {
    let spread_up = (ctx.up_best_ask - ctx.up_best_bid).max(0.0);
    let spread_down = (ctx.down_best_ask - ctx.down_best_bid).max(0.0);
    classify_regime(spread_up, spread_down)
}

fn build_plan(ctx: &StrategyContext<'_>) -> Vec<PlanOrder> {
    let regime = current_regime(ctx);
    let base_shares = ctx.strategy_params.base_shares_or_default();
    let mut plan = Vec::new();
    for side in [Outcome::Up, Outcome::Down] {
        plan.extend(build_side_plan(ctx, side, regime, base_shares, 1.0));
    }
    plan
}

/// `boost`: hedge-urgent için `HEDGE_URGENT_BOOST`, normalde `1.0`.
fn build_side_plan(
    ctx: &StrategyContext<'_>,
    side: Outcome,
    regime: Regime,
    base_shares: f64,
    boost: f64,
) -> Vec<PlanOrder> {
    let weight = weight_for(side, ctx.effective_score) * boost;
    let effective_shares = base_shares * weight;
    if effective_shares <= 0.0 {
        return Vec::new();
    }

    match regime {
        Regime::Tight | Regime::Medium | Regime::Wide => {
            build_maker_ladder(ctx, side, regime, effective_shares)
        }
        Regime::Extreme => build_extreme_plan(ctx, side, effective_shares),
    }
}

fn build_maker_ladder(
    ctx: &StrategyContext<'_>,
    side: Outcome,
    regime: Regime,
    effective_shares: f64,
) -> Vec<PlanOrder> {
    let bid = ctx.best_bid(side);
    let ask = ctx.best_ask(side);
    if bid <= 0.0 {
        return Vec::new();
    }

    let base_price = match regime {
        Regime::Wide if ask > 0.0 => (bid + ask) / 2.0,
        _ => bid,
    };
    let ladder = regime.ladder();
    let tick = ctx.tick_size.max(1e-6);
    let mut out = Vec::with_capacity(ladder.len());

    for (idx, pct) in ladder.iter().enumerate() {
        let price = (base_price - (idx as f64) * tick).clamp(ctx.min_price, ctx.max_price);
        let size = effective_shares * pct;
        if price <= 0.0 || size < ctx.api_min_order_size {
            continue;
        }
        out.push(PlanOrder {
            outcome: side,
            price,
            size,
            order_type: OrderType::Gtc,
            reason: format!("{}maker:{}:{}", REASON_PREFIX, side.as_lowercase(), idx),
        });
    }
    out
}

fn build_extreme_plan(
    ctx: &StrategyContext<'_>,
    side: Outcome,
    effective_shares: f64,
) -> Vec<PlanOrder> {
    let bid = ctx.best_bid(side);
    if bid <= 0.0 {
        return Vec::new();
    }
    let ask = ctx.best_ask(side);
    let opp_ask = ctx.best_ask(side.opposite());
    let mut out = Vec::new();

    let deep_price = (bid - EXTREME_MAKER_DISCOUNT).clamp(ctx.min_price, ctx.max_price);
    if deep_price > 0.0 && effective_shares >= ctx.api_min_order_size {
        out.push(PlanOrder {
            outcome: side,
            price: deep_price,
            size: effective_shares,
            order_type: OrderType::Gtc,
            reason: format!("{}maker:{}:0", REASON_PREFIX, side.as_lowercase()),
        });
    }

    // FAK (taker): karşı tarafı da en kötü senaryoda taker'la kapatacağımızı
    // varsay (`opp_ask + premium`); pair_cost projeksiyonu lock eşiğinin
    // altında kalmazsa ucuz olmayan FAK'a girmiyoruz.
    if ask > 0.0 && opp_ask > 0.0 && effective_shares >= ctx.api_min_order_size {
        let taker_price = (ask + EXTREME_TAKER_PREMIUM).clamp(ctx.min_price, ctx.max_price);
        let projected_pair_cost = taker_price + (opp_ask + EXTREME_TAKER_PREMIUM);
        if taker_price > 0.0 && projected_pair_cost <= ctx.avg_threshold {
            out.push(PlanOrder {
                outcome: side,
                price: taker_price,
                size: effective_shares,
                order_type: OrderType::Fak,
                reason: format!("{}taker:{}", REASON_PREFIX, side.as_lowercase()),
            });
        }
    }
    out
}

/// Eksik tarafa `HEDGE_URGENT_BOOST × ladder`; dominant tarafa plan üretmez —
/// reconcile dominant `elis:*` emirleri otomatik cancel'lar.
fn build_hedge_urgent_plan(ctx: &StrategyContext<'_>) -> Vec<PlanOrder> {
    let dominant = if ctx.metrics.imbalance() >= 0.0 {
        Outcome::Up
    } else {
        Outcome::Down
    };
    let weak = dominant.opposite();
    let regime = current_regime(ctx);
    let base_shares = ctx.strategy_params.base_shares_or_default();

    build_side_plan(ctx, weak, regime, base_shares, HEDGE_URGENT_BOOST)
        .into_iter()
        .map(|mut o| {
            o.reason = format!("{}hedge_urgent:{}", REASON_PREFIX, weak.as_lowercase());
            o
        })
        .collect()
}

fn elis_open_ids(ctx: &StrategyContext<'_>) -> Vec<String> {
    ctx.open_orders
        .iter()
        .filter(|o| o.reason.starts_with(REASON_PREFIX))
        .map(|o| o.id.clone())
        .collect()
}

/// Plan vs. açık `elis:*` emirleri karşılaştır:
///   * Aynı `(reason, outcome)` + `|Δprice| < eps_price` + `|Δremaining| <
///     eps_size` → tut.
///   * Aksi halde iptal et, yenisini yerleştir.
///   * Plan'da olmayan açık `elis:*` emirleri iptal.
///
/// Alis emirlerine dokunulmaz (reason filter garanti).
fn reconcile_with_open(ctx: &StrategyContext<'_>, plan: Vec<PlanOrder>) -> Decision {
    let eps_price = requote_threshold(ctx.tick_size);
    let eps_size = (ctx.api_min_order_size / 2.0).max(1e-6);

    let elis_open: Vec<&OpenOrder> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with(REASON_PREFIX))
        .collect();

    let mut keep: Vec<bool> = vec![false; elis_open.len()];
    let mut places: Vec<PlannedOrder> = Vec::new();

    for target in plan {
        let matched = elis_open.iter().enumerate().find(|(idx, open)| {
            if keep[*idx] || open.reason != target.reason || open.outcome != target.outcome {
                return false;
            }
            let remaining = (open.size - open.size_matched).max(0.0);
            (open.price - target.price).abs() < eps_price
                && (remaining - target.size).abs() < eps_size
        });

        if let Some((idx, _)) = matched {
            keep[idx] = true;
        } else {
            places.push(PlannedOrder {
                outcome: target.outcome,
                token_id: ctx.token_id(target.outcome).to_string(),
                side: Side::Buy,
                price: target.price,
                size: target.size,
                order_type: target.order_type,
                reason: target.reason,
            });
        }
    }

    let cancels: Vec<String> = elis_open
        .iter()
        .zip(keep.iter())
        .filter(|(_, &k)| !k)
        .map(|(o, _)| o.id.clone())
        .collect();

    match (cancels.is_empty(), places.is_empty()) {
        (true, true) => Decision::NoOp,
        (true, false) => Decision::PlaceOrders(places),
        (false, true) => Decision::CancelOrders(cancels),
        (false, false) => Decision::CancelAndPlace { cancels, places },
    }
}
