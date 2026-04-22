//! Alis stratejisi — Polymarket BTC up/down 5dk pencerede çalışan FSM.
//!
//! ## Akış (zone bazlı)
//!
//! 1. **DeepTrade (%0-10):** `place_open_pair` — skor `>= 5` ise UP intent,
//!    aksi halde DOWN intent. Asıl emir `BUY {intent} @ best_ask + open_delta`
//!    GTC, eş emir `BUY {opp} @ avg_threshold − asıl_price` GTC (parity size).
//! 2. **NormalTrade (%10-50):** max 1 avg-down. Dominant tarafta bekleyen alis
//!    GTC varsa iptal + AYNI size ile `best_bid_dom`'dan `alis:avgdown:dom`
//!    olarak re-place. Bekleyen yoksa NoOp.
//! 3. **AggTrade (%50-90):** max 1 pyramid (taker FAK) — pencere ortalama
//!    skor + dominant `best_bid > 0.5` trend onayı.
//! 4. **FakTrade (%90-97):** max 1 ek pyramid (taker FAK, daha agresif delta).
//! 5. **StopTrade (%97+):** tüm açık emirleri iptal et, `Done`.
//!
//! ## Karar önceliği (her tick)
//!
//! 1. StopTrade → cancel-all + `Done`.
//! 2. Profit-lock check: pasif lock (`avg_sum ≤ avg_threshold`) → tüm alis
//!    emirleri iptal + `Locked`. Aktif lock için ayrı branch yok — Kural A
//!    requote opp tarafını `min(safe, agg)` peşinde tutar; şart sağlandığında
//!    fiyat otomatik agresif maker'a çekilir, fill olunca pasif lock alır.
//! 3. Locked / Done → `NoOp`.
//! 4. Re-quote (Kural A): açık opener/hedge için hedef fiyat değiştiyse atomic
//!    `CancelAndPlace`. Opener: sadece daha ucuza. Hedge: her iki yönde.
//! 5. Reconcile parity (Kural B): dominant partial fill + opp dolmamış emir →
//!    opp size'ını `dom_filled − opp_filled`'e eşitle.
//! 6. State + zone bazlı aksiyon (Pending+DeepTrade → OpenPair, PositionOpen
//!    + zone → avg-down / pyramid).
//!
//! Dominant taraf seçimi: ilk MATCHED fill alan (etiketler değil, gerçeklik).

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::time::MarketZone;
use crate::types::{Outcome, OrderType, Side};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum AlisState {
    #[default]
    Pending,
    OpenPlaced {
        intent_dir: Outcome,
        opened_at_ms: u64,
    },
    PositionOpen {
        dominant_dir: Outcome,
        avg_down_used: bool,
        agg_pyramid_used: bool,
        fak_pyramid_used: bool,
        score_sum: f64,
        score_samples: u32,
    },
    Locked,
    Done,
}

pub struct AlisEngine;

impl AlisEngine {
    pub fn decide(state: AlisState, ctx: &StrategyContext<'_>) -> (AlisState, Decision) {
        let state = update_score(state, ctx.effective_score);
        let state = sync_dominant(state, ctx);

        if ctx.zone == MarketZone::StopTrade {
            return stop_trade(ctx);
        }

        if let Some(d) = profit_lock_check(state, ctx) {
            return d;
        }

        if matches!(state, AlisState::Locked | AlisState::Done) {
            return (state, Decision::NoOp);
        }

        if let Some(d) = requote_open_pair(state, ctx) {
            return (state, d);
        }

        if let Some(d) = reconcile_parity(state, ctx) {
            return (state, d);
        }

        match state {
            AlisState::Pending => {
                if ctx.zone == MarketZone::DeepTrade && ctx.signal_ready {
                    place_open_pair(ctx)
                } else {
                    (state, Decision::NoOp)
                }
            }
            AlisState::OpenPlaced { .. } => {
                if let Some(dom) = detect_dominant(ctx) {
                    let new_state = AlisState::PositionOpen {
                        dominant_dir: dom,
                        avg_down_used: false,
                        agg_pyramid_used: false,
                        fak_pyramid_used: false,
                        score_sum: ctx.effective_score,
                        score_samples: 1,
                    };
                    (new_state, Decision::NoOp)
                } else {
                    (state, Decision::NoOp)
                }
            }
            AlisState::PositionOpen {
                avg_down_used,
                agg_pyramid_used,
                fak_pyramid_used,
                ..
            } => match ctx.zone {
                MarketZone::NormalTrade if !avg_down_used => try_avg_down(state, ctx),
                MarketZone::AggTrade if !agg_pyramid_used => {
                    try_pyramid(state, ctx, MarketZone::AggTrade)
                }
                MarketZone::FakTrade if !fak_pyramid_used => {
                    try_pyramid(state, ctx, MarketZone::FakTrade)
                }
                _ => (state, Decision::NoOp),
            },
            _ => (state, Decision::NoOp),
        }
    }
}

// ---------- Helpers ----------

/// `PositionOpen.dominant_dir`'i metrics'e göre re-derive eder. Plan §2
/// "etiketler değil, gerçeklik komuta verir": hedge fill dominantı flip
/// ettirebilir; aksi halde requote/reconcile/profit-lock yanlış karar verir.
fn sync_dominant(state: AlisState, ctx: &StrategyContext<'_>) -> AlisState {
    let AlisState::PositionOpen {
        dominant_dir,
        avg_down_used,
        agg_pyramid_used,
        fak_pyramid_used,
        score_sum,
        score_samples,
    } = state
    else {
        return state;
    };
    let Some(real_dom) = detect_dominant(ctx) else {
        return state;
    };
    if real_dom == dominant_dir {
        return state;
    }
    AlisState::PositionOpen {
        dominant_dir: real_dom,
        avg_down_used,
        agg_pyramid_used,
        fak_pyramid_used,
        score_sum,
        score_samples,
    }
}

fn update_score(state: AlisState, score: f64) -> AlisState {
    if let AlisState::PositionOpen {
        dominant_dir,
        avg_down_used,
        agg_pyramid_used,
        fak_pyramid_used,
        score_sum,
        score_samples,
    } = state
    {
        AlisState::PositionOpen {
            dominant_dir,
            avg_down_used,
            agg_pyramid_used,
            fak_pyramid_used,
            score_sum: score_sum + score,
            score_samples: score_samples + 1,
        }
    } else {
        state
    }
}

fn clamp_price(p: f64, min: f64, max: f64) -> f64 {
    p.clamp(min, max)
}

/// Tick yarısı kadar fark "değişmedi" sayılır (re-quote spam'i engeller).
fn requote_threshold(tick_size: f64) -> f64 {
    (tick_size / 2.0).max(1e-6)
}

fn stop_trade(ctx: &StrategyContext<'_>) -> (AlisState, Decision) {
    let ids: Vec<String> = ctx.open_orders.iter().map(|o| o.id.clone()).collect();
    let decision = if ids.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(ids)
    };
    (AlisState::Done, decision)
}

fn detect_dominant(ctx: &StrategyContext<'_>) -> Option<Outcome> {
    let m = ctx.metrics;
    if m.up_filled <= 0.0 && m.down_filled <= 0.0 {
        return None;
    }
    if m.up_filled >= m.down_filled {
        Some(Outcome::Up)
    } else {
        Some(Outcome::Down)
    }
}

fn dom_avg(m: &crate::strategy::metrics::StrategyMetrics, dom: Outcome) -> f64 {
    match dom {
        Outcome::Up => m.avg_up,
        Outcome::Down => m.avg_down,
    }
}

fn dom_filled(m: &crate::strategy::metrics::StrategyMetrics, dom: Outcome) -> f64 {
    match dom {
        Outcome::Up => m.up_filled,
        Outcome::Down => m.down_filled,
    }
}

fn min_size_for_notional(min_notional: f64, price: f64) -> f64 {
    if price <= 0.0 {
        f64::MAX
    } else {
        min_notional / price
    }
}

// ---------- Profit-lock check ----------

fn profit_lock_check(
    state: AlisState,
    ctx: &StrategyContext<'_>,
) -> Option<(AlisState, Decision)> {
    if matches!(state, AlisState::Locked | AlisState::Done) {
        return None;
    }
    let m = ctx.metrics;

    if m.profit_locked(ctx.avg_threshold) {
        let cancels = collect_alis_open_ids(ctx);
        let decision = if cancels.is_empty() {
            Decision::NoOp
        } else {
            Decision::CancelOrders(cancels)
        };
        return Some((AlisState::Locked, decision));
    }

    None
}

fn collect_alis_open_ids(ctx: &StrategyContext<'_>) -> Vec<String> {
    ctx.open_orders
        .iter()
        .filter(|o| o.reason.starts_with("alis:"))
        .map(|o| o.id.clone())
        .collect()
}

// ---------- Re-quote (Kural A) ----------

/// Açık opener/hedge için hedef fiyat hesaplar; `o.price`'tan farkı
/// `tick_size/2`'yi aşan ilk emri atomic `CancelAndPlace` ile değiştirir.
/// Opener (dominant): sadece daha ucuza. Hedge (opp): her iki yönde.
fn requote_open_pair(state: AlisState, ctx: &StrategyContext<'_>) -> Option<Decision> {
    let eps = requote_threshold(ctx.tick_size);
    for o in ctx.open_orders.iter() {
        if !is_managed_pair_order(&o.reason) {
            continue;
        }
        let remaining = (o.size - o.size_matched).max(0.0);
        if remaining <= 0.0 {
            continue;
        }
        let (target, role) = match compute_target_and_role(state, ctx, o) {
            Some((t, r)) if t > 0.0 => (t, r),
            _ => continue,
        };
        let needs_requote = match role {
            RequoteRole::Opener => target < o.price - eps,
            RequoteRole::Hedge => (o.price - target).abs() >= eps,
        };
        if !needs_requote {
            continue;
        }
        let new_order = PlannedOrder {
            outcome: o.outcome,
            token_id: ctx.token_id(o.outcome).to_string(),
            side: Side::Buy,
            price: clamp_price(target, ctx.min_price, ctx.max_price),
            size: remaining,
            order_type: OrderType::Gtc,
            reason: o.reason.clone(),
        };
        return Some(Decision::CancelAndPlace {
            cancels: vec![o.id.clone()],
            places: vec![new_order],
        });
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RequoteRole {
    Opener,
    Hedge,
}

fn is_managed_pair_order(reason: &str) -> bool {
    reason.starts_with("alis:open:") || reason.starts_with("alis:hedge:")
}

fn compute_target_and_role(
    state: AlisState,
    ctx: &StrategyContext<'_>,
    o: &OpenOrder,
) -> Option<(f64, RequoteRole)> {
    let open_delta = ctx.strategy_params.open_delta_or_default();
    match state {
        AlisState::OpenPlaced { intent_dir, .. } => {
            if o.outcome == intent_dir {
                let ba = ctx.best_ask(o.outcome);
                if ba <= 0.0 {
                    None
                } else {
                    Some((ba + open_delta, RequoteRole::Opener))
                }
            } else {
                let intent_price = ctx
                    .open_orders
                    .iter()
                    .find(|x| x.outcome == intent_dir && is_managed_pair_order(&x.reason))
                    .map(|x| x.price)?;
                Some((ctx.avg_threshold - intent_price, RequoteRole::Hedge))
            }
        }
        AlisState::PositionOpen { dominant_dir, .. } => {
            // Dominant tarafa requote dokunmaz: kalan opener / partial-fill
            // artıkları try_avg_down'un re-quote hakkıdır.
            if o.outcome == dominant_dir {
                None
            } else {
                let avg_d = dom_avg(ctx.metrics, dominant_dir);
                if avg_d <= 0.0 {
                    return None;
                }
                // safe = lock-garanti maker fiyatı; agg = en agresif satıcı.
                // Aktif lock şartı `agg ≤ safe` ⇔ fiyat otomatik agg'a çekilir;
                // şart yoksa safe'te kalır. Tek emir, lock orphan bug'ı yok.
                let safe = ctx.avg_threshold - avg_d;
                let agg = ctx.best_ask(o.outcome);
                let target = if agg > 0.0 && agg < safe { agg } else { safe };
                Some((target, RequoteRole::Hedge))
            }
        }
        _ => None,
    }
}

// ---------- Reconcile parity (Kural B) ----------

fn reconcile_parity(state: AlisState, ctx: &StrategyContext<'_>) -> Option<Decision> {
    let AlisState::PositionOpen { dominant_dir, .. } = state else {
        return None;
    };
    let m = ctx.metrics;
    let dom = dominant_dir;
    let opp = dom.opposite();
    let dom_f = dom_filled(m, dom);
    let opp_f = dom_filled(m, opp);

    let opp_orders: Vec<&OpenOrder> = ctx
        .open_orders
        .iter()
        .filter(|o| o.outcome == opp && is_managed_pair_order(&o.reason))
        .collect();

    let total_opp_remaining: f64 = opp_orders
        .iter()
        .map(|o| (o.size - o.size_matched).max(0.0))
        .sum();

    let target_remaining = (dom_f - opp_f).max(0.0);
    let best_ask_opp = ctx.best_ask(opp);
    let parity_eps = if best_ask_opp > 0.0 {
        min_size_for_notional(ctx.api_min_order_size, best_ask_opp)
    } else {
        ctx.api_min_order_size
    };
    if (total_opp_remaining - target_remaining).abs() < parity_eps {
        return None;
    }
    if opp_orders.is_empty() {
        return None;
    }

    let cancels: Vec<String> = opp_orders.iter().map(|o| o.id.clone()).collect();

    if target_remaining <= 0.0 {
        return Some(Decision::CancelOrders(cancels));
    }

    let avg_d = dom_avg(m, dom);
    if avg_d <= 0.0 {
        return None;
    }
    let target_price = clamp_price(
        ctx.avg_threshold - avg_d,
        ctx.min_price,
        ctx.max_price,
    );
    let new_hedge = PlannedOrder {
        outcome: opp,
        token_id: ctx.token_id(opp).to_string(),
        side: Side::Buy,
        price: target_price,
        size: target_remaining,
        order_type: OrderType::Gtc,
        reason: format!("alis:hedge:{}", opp.as_lowercase()),
    };
    Some(Decision::CancelAndPlace {
        cancels,
        places: vec![new_hedge],
    })
}

// ---------- OpenPair ----------

fn place_open_pair(ctx: &StrategyContext<'_>) -> (AlisState, Decision) {
    let intent_dir = if ctx.effective_score >= 5.0 {
        Outcome::Up
    } else {
        Outcome::Down
    };
    let opp = intent_dir.opposite();

    let intent_ask = ctx.best_ask(intent_dir);
    if intent_ask <= 0.0 {
        return (AlisState::Pending, Decision::NoOp);
    }

    let open_delta = ctx.strategy_params.open_delta_or_default();
    let intent_price = clamp_price(
        intent_ask + open_delta,
        ctx.min_price,
        ctx.max_price,
    );
    let pair_price = clamp_price(
        ctx.avg_threshold - intent_price,
        ctx.min_price,
        ctx.max_price,
    );
    if intent_price <= 0.0 || pair_price <= 0.0 {
        return (AlisState::Pending, Decision::NoOp);
    }

    let size = ctx.order_usdc / intent_price;
    if size <= 0.0 || size * intent_price < ctx.api_min_order_size {
        return (AlisState::Pending, Decision::NoOp);
    }

    let intent_order = PlannedOrder {
        outcome: intent_dir,
        token_id: ctx.token_id(intent_dir).to_string(),
        side: Side::Buy,
        price: intent_price,
        size,
        order_type: OrderType::Gtc,
        reason: format!("alis:open:{}", intent_dir.as_lowercase()),
    };
    let pair_order = PlannedOrder {
        outcome: opp,
        token_id: ctx.token_id(opp).to_string(),
        side: Side::Buy,
        price: pair_price,
        size,
        order_type: OrderType::Gtc,
        reason: format!("alis:open:{}", opp.as_lowercase()),
    };

    let new_state = AlisState::OpenPlaced {
        intent_dir,
        opened_at_ms: ctx.now_ms,
    };
    (
        new_state,
        Decision::PlaceOrders(vec![intent_order, pair_order]),
    )
}

// ---------- Avg-down ----------
//
// Dominant tarafta bekleyen alis GTC (lock haricinde) varsa iptal et ve AYNI
// toplam remaining size ile `best_bid_dom` fiyatından `alis:avgdown:dom`
// olarak yeniden koy. Bekleyen yoksa NoOp (budget zaten harcanmış).

fn try_avg_down(state: AlisState, ctx: &StrategyContext<'_>) -> (AlisState, Decision) {
    let AlisState::PositionOpen {
        dominant_dir,
        agg_pyramid_used,
        fak_pyramid_used,
        score_sum,
        score_samples,
        ..
    } = state
    else {
        return (state, Decision::NoOp);
    };

    if ctx
        .now_ms
        .saturating_sub(ctx.last_averaging_ms)
        < ctx.cooldown_threshold
    {
        return (state, Decision::NoOp);
    }

    let dom = dominant_dir;
    let m = ctx.metrics;
    let avg_d = dom_avg(m, dom);
    let best_bid_dom = ctx.best_bid(dom);
    let best_ask_dom = ctx.best_ask(dom);

    if avg_d <= 0.0 || best_bid_dom <= 0.0 || best_ask_dom <= 0.0 {
        return (state, Decision::NoOp);
    }
    // Market dominant'ta ucuzlamış olmalı; aksi halde "avg-up" olur.
    if best_ask_dom >= avg_d {
        return (state, Decision::NoOp);
    }

    let pending: Vec<&OpenOrder> = ctx
        .open_orders
        .iter()
        .filter(|o| {
            o.outcome == dom
                && o.reason.starts_with("alis:")
                && (o.size - o.size_matched) > 0.0
        })
        .collect();
    if pending.is_empty() {
        return (state, Decision::NoOp);
    }

    let total_remaining: f64 = pending
        .iter()
        .map(|o| (o.size - o.size_matched).max(0.0))
        .sum();
    if total_remaining * best_bid_dom < ctx.api_min_order_size {
        return (state, Decision::NoOp);
    }

    let new_price = clamp_price(best_bid_dom, ctx.min_price, ctx.max_price);
    let eps = requote_threshold(ctx.tick_size);

    if pending.len() == 1
        && pending[0].reason == format!("alis:avgdown:{}", dom.as_lowercase())
        && (pending[0].price - new_price).abs() < eps
    {
        return (state, Decision::NoOp);
    }

    let cancels: Vec<String> = pending.iter().map(|o| o.id.clone()).collect();
    let new_order = PlannedOrder {
        outcome: dom,
        token_id: ctx.token_id(dom).to_string(),
        side: Side::Buy,
        price: new_price,
        size: total_remaining,
        order_type: OrderType::Gtc,
        reason: format!("alis:avgdown:{}", dom.as_lowercase()),
    };

    let new_state = AlisState::PositionOpen {
        dominant_dir,
        avg_down_used: true,
        agg_pyramid_used,
        fak_pyramid_used,
        score_sum,
        score_samples,
    };
    (
        new_state,
        Decision::CancelAndPlace {
            cancels,
            places: vec![new_order],
        },
    )
}

// ---------- Pyramid ----------

fn try_pyramid(
    state: AlisState,
    ctx: &StrategyContext<'_>,
    phase: MarketZone,
) -> (AlisState, Decision) {
    let AlisState::PositionOpen {
        dominant_dir,
        avg_down_used,
        agg_pyramid_used,
        fak_pyramid_used,
        score_sum,
        score_samples,
    } = state
    else {
        return (state, Decision::NoOp);
    };

    if score_samples == 0 {
        return (state, Decision::NoOp);
    }
    let score_avg = score_sum / (score_samples as f64);

    // Trend onayı: pencere ortalama skor + dominant best_bid > 0.5.
    let trend_dir = match dominant_dir {
        Outcome::Up if score_avg > 5.0 && ctx.up_best_bid > 0.5 => Some(Outcome::Up),
        Outcome::Down if score_avg < 5.0 && ctx.down_best_bid > 0.5 => Some(Outcome::Down),
        _ => None,
    };
    if trend_dir != Some(dominant_dir) {
        return (state, Decision::NoOp);
    }

    if ctx
        .now_ms
        .saturating_sub(ctx.last_averaging_ms)
        < ctx.cooldown_threshold
    {
        return (state, Decision::NoOp);
    }

    let score_ok = match dominant_dir {
        Outcome::Up => ctx.effective_score > 5.0,
        Outcome::Down => ctx.effective_score < 5.0,
    };
    if !score_ok {
        return (state, Decision::NoOp);
    }

    let best_ask_dom = ctx.best_ask(dominant_dir);
    let last_filled = match dominant_dir {
        Outcome::Up => ctx.metrics.last_filled_up,
        Outcome::Down => ctx.metrics.last_filled_down,
    };
    if best_ask_dom <= 0.0 || best_ask_dom <= last_filled {
        return (state, Decision::NoOp);
    }

    let delta = match phase {
        MarketZone::AggTrade => ctx.strategy_params.pyramid_agg_delta_or_default(),
        MarketZone::FakTrade => ctx.strategy_params.pyramid_fak_delta_or_default(),
        _ => return (state, Decision::NoOp),
    };
    let pyramid_usdc = ctx.strategy_params.pyramid_usdc_or(ctx.order_usdc);
    let price = clamp_price(best_ask_dom + delta, ctx.min_price, ctx.max_price);
    if price <= 0.0 {
        return (state, Decision::NoOp);
    }
    let size = pyramid_usdc / price;
    if size <= 0.0 || size * price < ctx.api_min_order_size {
        return (state, Decision::NoOp);
    }

    let label = match phase {
        MarketZone::AggTrade => "agg",
        MarketZone::FakTrade => "fak",
        _ => "n/a",
    };
    let order = PlannedOrder {
        outcome: dominant_dir,
        token_id: ctx.token_id(dominant_dir).to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Fak,
        reason: format!("alis:pyramid:{}:{}", label, dominant_dir.as_lowercase()),
    };

    let new_state = AlisState::PositionOpen {
        dominant_dir,
        avg_down_used,
        agg_pyramid_used: agg_pyramid_used || phase == MarketZone::AggTrade,
        fak_pyramid_used: fak_pyramid_used || phase == MarketZone::FakTrade,
        score_sum,
        score_samples,
    };
    (new_state, Decision::PlaceOrders(vec![order]))
}
