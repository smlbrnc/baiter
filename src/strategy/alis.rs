//! Alis stratejisi — Polymarket BTC up/down 5dk pencerede çalışan FSM.
//!
//! ## Akış (zone bazlı)
//!
//! 1. **DeepTrade (%0-10):** `place_open_pair` — skor `>= 5` ise UP intent,
//!    aksi halde DOWN intent. Asıl emir `BUY {intent} @ best_ask + open_delta`
//!    GTC, eş emir `BUY {opp} @ avg_threshold − asıl_price` GTC (parity size).
//! 2. **NormalTrade (%10-50):** max 1 avg-down + atomic hedge re-align.
//!    Hedef avg = `avg_threshold − best_ask_opp` (lock için yeni avg). Çözüm
//!    `x = (avg_dom − target) × shares_dom / (target − best_bid_dom)`.
//! 3. **AggTrade (%50-90):** max 1 pyramid (taker FAK, küçük delta) — trend
//!    onayı: pencere ortalaması skor > 5 + dominant `best_bid > 0.5`.
//! 4. **FakTrade (%90-97):** max 1 ek pyramid (taker FAK, daha agresif delta).
//! 5. **StopTrade (%97+):** tüm açık emirleri iptal et, `Done`.
//!
//! ## Karar önceliği (her tick)
//!
//! 1. StopTrade → cancel-all + `Done`.
//! 2. Profit-lock check (cooldown=0): `avg_sum ≤ avg_threshold` zaten ise
//!    `Locked`; aksi halde `avg_dom + best_ask_opp ≤ avg_threshold` ise FAK
//!    basıp `Locked`.
//! 3. Locked / Done → `NoOp`.
//! 4. **Re-quote (Kural A):** açık opener/hedge emirleri için hedef fiyat
//!    `o.price`'tan farklıysa atomic `CancelAndPlace`. Opener tarafı `best_ask
//!    + open_delta` peşinde, hedge tarafı `avg_threshold − avg_dominant`.
//! 5. **Reconcile parity (Kural B):** dominant tarafta partial fill geldi ve
//!    eş tarafta hala dolmamış emir var → eş emrin size'ını `dom_filled −
//!    opp_filled`'e eşitle (cancel + replace).
//! 6. **Stale GTC iptali:** `alis:avgdown:*` veya `alis:pyramid:*` reason'lu
//!    + `cooldown_threshold` (default 30s) yaşı + 0 fill ise `CancelOrders`.
//! 7. State'e göre aksiyon (`Pending` + DeepTrade → OpenPair, `PositionOpen`
//!    + zone'a göre avg-down / pyramid).
//!
//! Dominant taraf seçimi: ilk MATCHED fill alan (etiketler değil, gerçeklik).
//! Aynı tick iki taraf da dolmuş + lock şartı sağlanmış ise direkt `Locked`.

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::time::MarketZone;
use crate::types::{Outcome, OrderType, Side};

/// Alis FSM state'i. Tüm varyantlar `Copy` (state stringleri yok — açık
/// emirler `open_orders` listesinde reason prefix'leri ile yönetilir).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum AlisState {
    #[default]
    Pending,
    /// OpenPair basıldı; henüz fill yok ya da intent yön belirsiz.
    /// `intent_dir` sadece niyet etiketi — kim ilk dolarsa o `dominant_dir`
    /// olur. Engine bir sonraki tick'te `detect_dominant` ile geçişi yakalar.
    OpenPlaced {
        intent_dir: Outcome,
        opened_at_ms: u64,
    },
    /// Pozisyon açık; tek taraf dominant. Avg-down ve pyramid hakları flag'ler
    /// ile takip edilir; `score_sum/score_samples` AggTrade trend kontrolü
    /// için pencere boyu skor ortalamasını biriktirir.
    PositionOpen {
        dominant_dir: Outcome,
        avg_down_used: bool,
        agg_pyramid_used: bool,
        fak_pyramid_used: bool,
        score_sum: f64,
        score_samples: u32,
    },
    /// Profit-lock şartı sağlandı (taker FAK basıldı veya pasif hedge fill ile).
    Locked,
    /// Pencere kapandı; pasif.
    Done,
}

/// Alis karar motoru — saf fonksiyon: `(state, ctx) → (next_state, decision)`.
pub struct AlisEngine;

impl AlisEngine {
    pub fn decide(state: AlisState, ctx: &StrategyContext<'_>) -> (AlisState, Decision) {
        // Skor accumulation (sadece PositionOpen biriktirir).
        let state = update_score(state, ctx.effective_score);

        // 1. StopTrade: tüm emirleri iptal et, Done.
        if ctx.zone == MarketZone::StopTrade {
            return stop_trade(ctx);
        }

        // 2. Profit-lock check (her tick, cooldown=0). Locked'a geçirebilir.
        if let Some(d) = profit_lock_check(state, ctx) {
            return d;
        }

        // 3. Locked / Done: pasif.
        if matches!(state, AlisState::Locked | AlisState::Done) {
            return (state, Decision::NoOp);
        }

        // 4. Re-quote (Kural A).
        if let Some(d) = requote_open_pair(state, ctx) {
            return (state, d);
        }

        // 5. Reconcile parity (Kural B).
        if let Some(d) = reconcile_parity(state, ctx) {
            return (state, d);
        }

        // 6. Stale GTC iptali (avg-down / pyramid).
        if let Some(d) = stale_cancel(ctx) {
            return (state, d);
        }

        // 7. State + zone bazlı aksiyon.
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

/// `EPSILON` — tick boyutunun yarısı kadar fark "değişmedi" sayılır
/// (re-quote spam'i engeller).
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

// ---------- 2. Profit-lock check ----------

fn profit_lock_check(
    state: AlisState,
    ctx: &StrategyContext<'_>,
) -> Option<(AlisState, Decision)> {
    if matches!(state, AlisState::Locked | AlisState::Done) {
        return None;
    }
    let m = ctx.metrics;

    // Pasif lock: iki taraf da fill aldı + avg_sum ≤ avg_threshold.
    if m.profit_locked(ctx.avg_threshold) {
        return Some((AlisState::Locked, Decision::NoOp));
    }

    // Aktif lock taker: dominant + opp.best_ask ≤ threshold ise FAK.
    let dom = detect_dominant(ctx)?;
    let opp = dom.opposite();
    let avg_d = dom_avg(m, dom);
    let best_ask_opp = ctx.best_ask(opp);
    if best_ask_opp <= 0.0 || avg_d <= 0.0 {
        return None;
    }
    if avg_d + best_ask_opp > ctx.avg_threshold {
        return None;
    }

    let imb = m.imbalance().abs();
    if imb <= 0.0 {
        return None;
    }
    if imb < min_size_for_notional(ctx.api_min_order_size, best_ask_opp) {
        return None;
    }

    let order = PlannedOrder {
        outcome: opp,
        token_id: ctx.token_id(opp).to_string(),
        side: Side::Buy,
        price: clamp_price(best_ask_opp, ctx.min_price, ctx.max_price),
        size: imb,
        order_type: OrderType::Fak,
        reason: format!("alis:lock:{}", opp.as_lowercase()),
    };
    Some((AlisState::Locked, Decision::PlaceOrders(vec![order])))
}

// ---------- 4. Re-quote (Kural A) ----------

/// Açık opener/hedge emirleri için hedef fiyat hesaplar; `o.price` ile farkı
/// `tick_size/2`'yi aşan ilk emri atomic `CancelAndPlace` ile yenisi ile
/// değiştirir. Tek tick'te tek emir — kalan re-quote'lar sonraki tick'te.
///
/// **Opener tarafı** (dominant taraf): BUY emirleri için sadece **daha ucuza**
/// re-quote (target < price). Daha pahalıya geçmek bizim için zarar.
///
/// **Hedge tarafı** (opp): `target = avg_threshold − avg_dominant` her iki
/// yönde de re-align gerektirir (avg-down → daha yüksek, pyramid → daha düşük).
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
                // Eş (pair) emrin hedefi: avg_threshold − asıl (intent) emrin
                // güncel fiyatı. Asıl emir kitapta yoksa target hesaplanamaz.
                let intent_price = ctx
                    .open_orders
                    .iter()
                    .find(|x| x.outcome == intent_dir && is_managed_pair_order(&x.reason))
                    .map(|x| x.price)?;
                Some((ctx.avg_threshold - intent_price, RequoteRole::Hedge))
            }
        }
        AlisState::PositionOpen { dominant_dir, .. } => {
            if o.outcome == dominant_dir {
                let ba = ctx.best_ask(o.outcome);
                if ba <= 0.0 {
                    None
                } else {
                    Some((ba + open_delta, RequoteRole::Opener))
                }
            } else {
                let avg_d = dom_avg(ctx.metrics, dominant_dir);
                if avg_d <= 0.0 {
                    None
                } else {
                    Some((ctx.avg_threshold - avg_d, RequoteRole::Hedge))
                }
            }
        }
        _ => None,
    }
}

// ---------- 5. Reconcile parity (Kural B) ----------

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

    // Hedef kalan size = dom_filled − opp_filled (parity'ye getirme).
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

// ---------- 6. Stale GTC iptali ----------

fn stale_cancel(ctx: &StrategyContext<'_>) -> Option<Decision> {
    let mut cancels = Vec::new();
    for o in ctx.open_orders.iter() {
        let stale_eligible = o.reason.starts_with("alis:avgdown:")
            || o.reason.starts_with("alis:pyramid:");
        if !stale_eligible {
            continue;
        }
        if o.size_matched > 0.0 {
            continue;
        }
        if o.age_ms(ctx.now_ms) >= ctx.cooldown_threshold {
            cancels.push(o.id.clone());
        }
    }
    if cancels.is_empty() {
        None
    } else {
        Some(Decision::CancelOrders(cancels))
    }
}

// ---------- 7a. OpenPair ----------

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

// ---------- 7b. Avg-down ----------

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
    let opp = dom.opposite();
    let m = ctx.metrics;
    let avg_d = dom_avg(m, dom);
    let shares_d = dom_filled(m, dom);
    let best_ask_dom = ctx.best_ask(dom);
    let best_bid_dom = ctx.best_bid(dom);
    let best_ask_opp = ctx.best_ask(opp);

    if avg_d <= 0.0
        || shares_d <= 0.0
        || best_ask_dom <= 0.0
        || best_bid_dom <= 0.0
        || best_ask_opp <= 0.0
    {
        return (state, Decision::NoOp);
    }
    // Market dominant tarafında ucuzlamış olmalı (avg-down anlamlı olsun).
    if best_ask_dom >= avg_d {
        return (state, Decision::NoOp);
    }

    // Hedef avg = avg_threshold − best_ask_opp (lock için yeni avg).
    let target = ctx.avg_threshold - best_ask_opp;
    if target <= best_bid_dom {
        return (state, Decision::NoOp);
    }
    if target >= avg_d {
        return (state, Decision::NoOp);
    }

    // x = (avg_d − target) × shares_d / (target − best_bid_dom)
    let x = (avg_d - target) * shares_d / (target - best_bid_dom);
    if x <= 0.0 {
        return (state, Decision::NoOp);
    }
    if x * best_bid_dom < ctx.api_min_order_size {
        return (state, Decision::NoOp);
    }

    let avg_down_order = PlannedOrder {
        outcome: dom,
        token_id: ctx.token_id(dom).to_string(),
        side: Side::Buy,
        price: clamp_price(best_bid_dom, ctx.min_price, ctx.max_price),
        size: x,
        order_type: OrderType::Gtc,
        reason: format!("alis:avgdown:{}", dom.as_lowercase()),
    };

    let new_hedge_size = x + shares_d - dom_filled(m, opp);
    let new_hedge = if new_hedge_size > 0.0 {
        Some(PlannedOrder {
            outcome: opp,
            token_id: ctx.token_id(opp).to_string(),
            side: Side::Buy,
            price: clamp_price(best_ask_opp, ctx.min_price, ctx.max_price),
            size: new_hedge_size,
            order_type: OrderType::Gtc,
            reason: format!("alis:hedge:{}", opp.as_lowercase()),
        })
    } else {
        None
    };

    let cancels: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| o.outcome == opp && is_managed_pair_order(&o.reason))
        .map(|o| o.id.clone())
        .collect();

    let mut places = vec![avg_down_order];
    if let Some(h) = new_hedge {
        places.push(h);
    }

    let new_state = AlisState::PositionOpen {
        dominant_dir,
        avg_down_used: true,
        agg_pyramid_used,
        fak_pyramid_used,
        score_sum,
        score_samples,
    };

    let decision = if cancels.is_empty() {
        Decision::PlaceOrders(places)
    } else {
        Decision::CancelAndPlace { cancels, places }
    };
    (new_state, decision)
}

// ---------- 7c. Pyramid ----------

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

    // Trend onayı: pencere ortalama skor + dominant tarafın best_bid > 0.5.
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

    // Anlık skor hala uygun yönde mi?
    let score_ok = match dominant_dir {
        Outcome::Up => ctx.effective_score > 5.0,
        Outcome::Down => ctx.effective_score < 5.0,
    };
    if !score_ok {
        return (state, Decision::NoOp);
    }

    // best_ask(dominant) > son fill price (trend hala aktif).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StrategyParams;
    use crate::strategy::metrics::StrategyMetrics;

    /// Test ctx kurulumunu sıkıştırılmış halde tutar — opsiyonel zaman/skor
    /// alanları builder benzeri override'larla atanır.
    struct CtxArgs<'a> {
        m: &'a StrategyMetrics,
        params: &'a StrategyParams,
        zone: MarketZone,
        score: f64,
        signal_ready: bool,
        now_ms: u64,
        last_avg: u64,
        open_orders: &'a [OpenOrder],
        bests: (f64, f64, f64, f64),
    }

    fn make_ctx<'a>(args: CtxArgs<'a>) -> StrategyContext<'a> {
        StrategyContext {
            metrics: args.m,
            up_token_id: "tok-up",
            down_token_id: "tok-down",
            up_best_bid: args.bests.0,
            up_best_ask: args.bests.1,
            down_best_bid: args.bests.2,
            down_best_ask: args.bests.3,
            api_min_order_size: 5.0,
            order_usdc: 5.0,
            effective_score: args.score,
            zone: args.zone,
            now_ms: args.now_ms,
            last_averaging_ms: args.last_avg,
            tick_size: 0.01,
            open_orders: args.open_orders,
            min_price: 0.01,
            max_price: 0.99,
            cooldown_threshold: 30_000,
            avg_threshold: 0.98,
            signal_ready: args.signal_ready,
            strategy_params: args.params,
        }
    }

    #[test]
    fn pending_in_deeptrade_with_signal_ready_places_open_pair() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let ctx = make_ctx(CtxArgs {
            m: &m,
            params: &p,
            zone: MarketZone::DeepTrade,
            score: 7.0,
            signal_ready: true,
            now_ms: 1_000,
            last_avg: 0,
            open_orders: &[],
            bests: (0.52, 0.53, 0.46, 0.47),
        });
        let (next, d) = AlisEngine::decide(AlisState::Pending, &ctx);
        match (next, d) {
            (AlisState::OpenPlaced { intent_dir, .. }, Decision::PlaceOrders(orders)) => {
                assert_eq!(intent_dir, Outcome::Up);
                assert_eq!(orders.len(), 2);
                let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
                let dn = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
                assert!((up.price - 0.54).abs() < 1e-6); // 0.53 + 0.01
                assert!((dn.price - (0.98 - 0.54)).abs() < 1e-6); // 0.44
                assert!((up.size - dn.size).abs() < 1e-6);
                assert_eq!(up.order_type, OrderType::Gtc);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn pending_low_score_picks_down_intent() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let ctx = make_ctx(CtxArgs {
            m: &m,
            params: &p,
            zone: MarketZone::DeepTrade,
            score: 3.0,
            signal_ready: true,
            now_ms: 1_000,
            last_avg: 0,
            open_orders: &[],
            bests: (0.46, 0.47, 0.52, 0.53),
        });
        let (next, _d) = AlisEngine::decide(AlisState::Pending, &ctx);
        match next {
            AlisState::OpenPlaced { intent_dir, .. } => {
                assert_eq!(intent_dir, Outcome::Down);
            }
            _ => panic!("expected OpenPlaced down"),
        }
    }

    #[test]
    fn open_placed_transitions_to_position_open_on_first_fill() {
        let m = StrategyMetrics {
            up_filled: 9.0,
            avg_up: 0.54,
            last_filled_up: 0.54,
            ..Default::default()
        };
        let p = StrategyParams::default();
        let order = OpenOrder {
            id: "o-up".into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.54,
            size: 9.0,
            reason: "alis:open:up".into(),
            placed_at_ms: 1_000,
            size_matched: 9.0,
        };
        let ctx = make_ctx(CtxArgs {
            m: &m,
            params: &p,
            zone: MarketZone::DeepTrade,
            score: 7.0,
            signal_ready: true,
            now_ms: 2_000,
            last_avg: 1_500,
            open_orders: std::slice::from_ref(&order),
            bests: (0.53, 0.54, 0.46, 0.47),
        });
        let state = AlisState::OpenPlaced {
            intent_dir: Outcome::Up,
            opened_at_ms: 1_000,
        };
        let (next, _d) = AlisEngine::decide(state, &ctx);
        match next {
            AlisState::PositionOpen { dominant_dir, .. } => {
                assert_eq!(dominant_dir, Outcome::Up);
            }
            _ => panic!("expected PositionOpen up"),
        }
    }

    #[test]
    fn profit_lock_taker_when_threshold_satisfied() {
        // up_filled=100, down_filled=0 → imb=100, notional 100×0.44=44 USDC
        // ≥ api_min_order_size (5).
        let m = StrategyMetrics {
            up_filled: 100.0,
            avg_up: 0.54,
            ..Default::default()
        };
        let p = StrategyParams::default();
        let ctx = make_ctx(CtxArgs {
            m: &m,
            params: &p,
            zone: MarketZone::NormalTrade,
            score: 5.0,
            signal_ready: true,
            now_ms: 10_000,
            last_avg: 5_000,
            open_orders: &[],
            bests: (0.53, 0.54, 0.43, 0.44),
        });
        let state = AlisState::PositionOpen {
            dominant_dir: Outcome::Up,
            avg_down_used: false,
            agg_pyramid_used: false,
            fak_pyramid_used: false,
            score_sum: 5.0,
            score_samples: 1,
        };
        let (next, d) = AlisEngine::decide(state, &ctx);
        assert_eq!(next, AlisState::Locked);
        match d {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 1);
                assert_eq!(orders[0].outcome, Outcome::Down);
                assert_eq!(orders[0].order_type, OrderType::Fak);
                assert!((orders[0].size - 100.0).abs() < 1e-6);
            }
            other => panic!("expected FAK place, got {:?}", other),
        }
    }

    #[test]
    fn passive_lock_already_satisfied_emits_locked_noop() {
        let m = StrategyMetrics {
            up_filled: 9.0,
            avg_up: 0.54,
            down_filled: 9.0,
            avg_down: 0.43,
            ..Default::default()
        };
        let p = StrategyParams::default();
        let ctx = make_ctx(CtxArgs {
            m: &m,
            params: &p,
            zone: MarketZone::AggTrade,
            score: 5.0,
            signal_ready: true,
            now_ms: 100_000,
            last_avg: 5_000,
            open_orders: &[],
            bests: (0.53, 0.55, 0.42, 0.44),
        });
        let state = AlisState::PositionOpen {
            dominant_dir: Outcome::Up,
            avg_down_used: false,
            agg_pyramid_used: false,
            fak_pyramid_used: false,
            score_sum: 5.0,
            score_samples: 1,
        };
        let (next, d) = AlisEngine::decide(state, &ctx);
        assert_eq!(next, AlisState::Locked);
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn stop_trade_cancels_all_open_orders() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let order = OpenOrder {
            id: "o-1".into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.54,
            size: 9.0,
            reason: "alis:open:up".into(),
            placed_at_ms: 0,
            size_matched: 0.0,
        };
        let ctx = make_ctx(CtxArgs {
            m: &m,
            params: &p,
            zone: MarketZone::StopTrade,
            score: 5.0,
            signal_ready: true,
            now_ms: 300_000,
            last_avg: 0,
            open_orders: std::slice::from_ref(&order),
            bests: (0.50, 0.51, 0.49, 0.50),
        });
        let (next, d) = AlisEngine::decide(AlisState::Pending, &ctx);
        assert_eq!(next, AlisState::Done);
        match d {
            Decision::CancelOrders(ids) => assert_eq!(ids, vec!["o-1".to_string()]),
            other => panic!("expected CancelOrders, got {:?}", other),
        }
    }

    #[test]
    fn reconcile_parity_resizes_hedge_after_partial_fill() {
        // UP 40/90 doldu, DOWN hala kitapta size=90 → reconcile target=40,
        // notional 40×0.50=20 USDC ≥ api_min_order_size.
        let m = StrategyMetrics {
            up_filled: 40.0,
            avg_up: 0.54,
            ..Default::default()
        };
        let p = StrategyParams::default();
        let up_o = OpenOrder {
            id: "o-up".into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.54,
            size: 90.0,
            reason: "alis:open:up".into(),
            placed_at_ms: 0,
            size_matched: 40.0,
        };
        let dn_o = OpenOrder {
            id: "o-dn".into(),
            outcome: Outcome::Down,
            side: Side::Buy,
            price: 0.44,
            size: 90.0,
            reason: "alis:open:down".into(),
            placed_at_ms: 0,
            size_matched: 0.0,
        };
        let orders = vec![up_o, dn_o];
        // best_ask_down = 0.50 → 0.54+0.50=1.04 > 0.98, profit-lock taker
        // şartı sağlanmaz; reconcile devreye girer.
        let ctx = make_ctx(CtxArgs {
            m: &m,
            params: &p,
            zone: MarketZone::NormalTrade,
            score: 7.0,
            signal_ready: true,
            now_ms: 10_000,
            last_avg: 5_000,
            open_orders: &orders,
            bests: (0.53, 0.54, 0.45, 0.50),
        });
        let state = AlisState::PositionOpen {
            dominant_dir: Outcome::Up,
            avg_down_used: false,
            agg_pyramid_used: false,
            fak_pyramid_used: false,
            score_sum: 5.0,
            score_samples: 1,
        };
        let (next, d) = AlisEngine::decide(state, &ctx);
        assert!(matches!(next, AlisState::PositionOpen { .. }));
        match d {
            Decision::CancelAndPlace { cancels, places } => {
                assert_eq!(cancels, vec!["o-dn".to_string()]);
                assert_eq!(places.len(), 1);
                assert_eq!(places[0].outcome, Outcome::Down);
                // size = dom_filled - opp_filled = 40 - 0 = 40
                assert!((places[0].size - 40.0).abs() < 1e-6);
                assert!(places[0].reason.starts_with("alis:hedge:"));
            }
            other => panic!("expected CancelAndPlace, got {:?}", other),
        }
    }
}
