//! Elis stratejisi v2.0 — Hibrit Maker Bid Grid (Alis-tabanlı + Composite Signal Yön Filtresi).
//!
//! Doküman: `.cursor/docs/elis-strategy.md`
//! Backtest raporu: `exports/backtest-final-16-markets.md` (16 marketde yön %92, +$560 net PnL)
//!
//! ## Yapı özeti
//!
//! 1. **Pre-opener (t < 20 tick)**: tick'leri `Pending` state'inde topla
//! 2. **Opening (t = 20 tick)**: composite 5-rule ladder ile yön tahmini → asymmetric open
//! 3. **Managing**: 10-katman decide chain
//! 4. **Lock / Scoop / Stop**: kâr garantili / late scoop / deadline
//!
//! ## 10-katman decide chain
//!
//! ```text
//! 0. Pending (t<20)         → no-op (tick buffer'a ekle)
//! 1. Opening (t=20)         → composite open + hedge (asymmetric)
//! 2. Deadline (rem≤8s)      → STOP, hiç emir yok
//! 3. Pre-resolve scoop      → opp_bid≤0.05 + rem≤35s → $50 dom @ ask-1tick
//! 4. Signal flip            → |dscore_from_open|>5.0 + flip_count<1
//!                              → 2x dom boost, 0.3x hedge, freeze 60s
//! 5. (locked ise 6-9 atla)  — kâr garantili
//! 6. Avg-down (one-shot)    → dom_bid+2.3tick≤avg_dom → $15 dom
//! 7. Pyramid                → ofi≥0.83 + persist 5s + score yönü match → $15 dom
//! 8. Dom requote            → |Δdom_bid|≥2tick + 3s cooldown → $15 dom
//! 9. Hedge requote (KRİTİK!) → opp YÜKSELDİ ≥2tick + opp≥0.15 + freeze geçti → $8 hedge
//!                              (sadece artış — Alis'in en büyük hatası düzeltildi)
//! 10. Parity gap            → |up-dn|>250 + 5s cooldown + freeze geçti → opp_size
//! ```
//!
//! ## Composite opener (5-rule ladder)
//!
//! 1. **BSI reversion**: `|bsi|>2.0` → bsi tersi (extreme reversion)
//! 2. **OFI+CVD exhaustion**: `|ofi|>0.4 + |cvd|>3` → flow tersi
//! 3. **OFI directional**: `|ofi|>0.4` → ofi yönü (aggressive flow)
//! 4. **Strong dscore**: `|dscore|>1.0` → dscore yönü (momentum)
//! 5. **Fallback**: `score_avg ≥ 5` → Up
//!
//! `ctx.bsi/ofi/cvd` `None` ise rule 1-3 atlanır → 2-rule (momentum + score_avg) fallback.
//!
//! ## Forward-compatibility
//!
//! `bsi/ofi/cvd/market_remaining_secs` opsiyonel — RTDS pipeline'da yoksa Elis fallback'e
//! düşer (sadece `signal_score` kullanır), eklendikçe full 5-rule devreye girer.

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::config::ElisParams;
use crate::types::{Outcome, OrderType, Side};

/// Pre-opener tick snapshot'u — ElisState içinde sliding window olarak saklanır.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TickSnapshot {
    pub score: f64,
    pub bsi: Option<f64>,
    pub ofi: Option<f64>,
    pub cvd: Option<f64>,
}

/// Composite opener kuralın hangi dalı tetiklendi (debug + log için).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenerRule {
    BsiReversion,
    Exhaustion,
    OfiDirectional,
    Momentum,
    ScoreAverage,
}

/// Elis FSM state'i.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElisState {
    /// Pre-opener buffer; hem `pre_opener_ticks` hem de `opener_min_secs` dolmadan açılmaz.
    /// `first_tick_ms`: ilk BBA tick'inin timestamp'i (0 = henüz tick yok).
    Pending { ticks: Vec<TickSnapshot>, first_tick_ms: u64 },
    /// Açık pozisyon; tüm decide() katmanları burada.
    Active(Box<ActiveState>),
    /// Deadline / hard stop sonrası — sadece NoOp.
    Done,
}

impl Default for ElisState {
    fn default() -> Self {
        Self::Pending { ticks: Vec::new(), first_tick_ms: 0 }
    }
}

/// Open sonrası Elis durumu — `Box`'lı çünkü enum variant size'ı dengelemek gerek.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveState {
    pub intent: Outcome,
    pub opener_score: f64,
    pub opener_rule: OpenerRule,
    pub flip_count: u32,
    pub flip_freeze_until_ms: u64,
    pub avg_down_used: bool,
    pub last_pyr_ms: Option<u64>,
    pub last_dom_price: Option<f64>,
    pub last_hedge_price: Option<f64>,
    pub last_requote_dom_ms: u64,
    pub last_requote_hedge_ms: u64,
    pub last_parity_ms: u64,
    pub last_scoop_ms: u64,
    pub score_persist_since_ms: u64,
    pub locked: bool,
}

pub struct ElisEngine;

impl ElisEngine {
    /// Tek tick — yeni state + Decision döndürür. State enum heap-alloc içerebilir
    /// (Pending ticks Vec, Active Box) → her tick `clone` küçük ama döndürmek için
    /// move-by-value yapısı kullanıyoruz.
    pub fn decide(state: ElisState, ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        let p = ElisParams::from_strategy_params(ctx.strategy_params);

        match state {
            ElisState::Done => (ElisState::Done, Decision::NoOp),

            ElisState::Pending { mut ticks, first_tick_ms } => {
                // İlk tick'te zaman damgasını kaydet
                let first_ms = if first_tick_ms == 0 { ctx.now_ms } else { first_tick_ms };
                ticks.push(TickSnapshot {
                    score: ctx.effective_score,
                    bsi: ctx.bsi,
                    ofi: ctx.ofi,
                    cvd: ctx.cvd,
                });
                // Hem minimum tick sayısı hem de minimum süre (BBA spam koruması)
                let elapsed_secs = (ctx.now_ms.saturating_sub(first_ms)) as f64 / 1000.0;
                if ticks.len() < p.pre_opener_ticks || elapsed_secs < p.opener_min_secs {
                    return (ElisState::Pending { ticks, first_tick_ms: first_ms }, Decision::NoOp);
                }
                // opener_min_secs geçti ve yeterli tick var — composite open
                let (intent, rule) = predict_opener(&ticks, &p);
                let active = open_pair(ctx, &p, intent, rule);
                (ElisState::Active(Box::new(active.0)), active.1)
            }

            ElisState::Active(mut active) => {
                let decision = decide_active(&mut active, ctx, &p);
                // Deadline → Done'a geç
                let new_state = if matches!(decision, Decision::CancelOrders(_))
                    && active_is_deadline(&active, ctx, &p)
                {
                    ElisState::Done
                } else {
                    ElisState::Active(active)
                };
                (new_state, decision)
            }
        }
    }
}

// ============================================================================
// COMPOSITE OPENER (5-rule ladder)
// ============================================================================

/// Pre-opener feature'lar.
struct PreFeatures {
    dscore: f64,
    score_avg: f64,
    bsi: Option<f64>,
    ofi_avg: Option<f64>,
    cvd: Option<f64>,
}

fn compute_features(ticks: &[TickSnapshot]) -> PreFeatures {
    let n = ticks.len() as f64;
    let dscore = ticks.last().unwrap().score - ticks.first().unwrap().score;
    let score_avg = ticks.iter().map(|t| t.score).sum::<f64>() / n;

    let bsi = ticks.last().and_then(|t| t.bsi);
    let cvd = ticks.last().and_then(|t| t.cvd);

    // OFI ortalaması — tüm ticklerde Some olmalı, aksi halde None
    let ofi_avg = if ticks.iter().all(|t| t.ofi.is_some()) {
        Some(ticks.iter().map(|t| t.ofi.unwrap()).sum::<f64>() / n)
    } else {
        None
    };

    PreFeatures {
        dscore,
        score_avg,
        bsi,
        ofi_avg,
        cvd,
    }
}

/// 5-rule ladder; bsi/ofi/cvd `None` ise rule 1-3 atlanır.
fn predict_opener(ticks: &[TickSnapshot], p: &ElisParams) -> (Outcome, OpenerRule) {
    let f = compute_features(ticks);

    // Rule 1: BSI extreme reversion
    if let Some(bsi) = f.bsi {
        if bsi.abs() > p.bsi_rev_threshold {
            return (
                if bsi > 0.0 { Outcome::Down } else { Outcome::Up },
                OpenerRule::BsiReversion,
            );
        }
    }

    // Rule 2: OFI+CVD exhaustion
    if let (Some(ofi), Some(cvd)) = (f.ofi_avg, f.cvd) {
        if ofi.abs() > p.ofi_exhaustion_threshold && cvd.abs() > p.cvd_exhaustion_threshold {
            if ofi > 0.0 && cvd > 0.0 {
                return (Outcome::Down, OpenerRule::Exhaustion);
            }
            if ofi < 0.0 && cvd < 0.0 {
                return (Outcome::Up, OpenerRule::Exhaustion);
            }
        }
    }

    // Rule 3: OFI directional
    if let Some(ofi) = f.ofi_avg {
        if ofi.abs() > p.ofi_directional_threshold {
            return (
                if ofi > 0.0 { Outcome::Up } else { Outcome::Down },
                OpenerRule::OfiDirectional,
            );
        }
    }

    // Rule 4: Strong dscore momentum
    if f.dscore.abs() > p.dscore_strong_threshold {
        return (
            if f.dscore > 0.0 {
                Outcome::Up
            } else {
                Outcome::Down
            },
            OpenerRule::Momentum,
        );
    }

    // Rule 5: Fallback — score_avg
    let dir = if f.score_avg >= p.score_neutral {
        Outcome::Up
    } else {
        Outcome::Down
    };
    (dir, OpenerRule::ScoreAverage)
}

// ============================================================================
// OPEN PAIR (asymmetric)
// ============================================================================

fn open_pair(
    ctx: &StrategyContext<'_>,
    p: &ElisParams,
    intent: Outcome,
    rule: OpenerRule,
) -> (ActiveState, Decision) {
    let dom_b = ctx.best_bid(intent);
    let hedge_b = ctx.best_bid(intent.opposite());

    let mut places: Vec<PlannedOrder> = Vec::new();

    if let Some(o) = build_bid(ctx, intent, dom_b - 2.0 * ctx.tick_size, p.open_usdc_dom, "open:dom") {
        places.push(o);
    }
    if let Some(o) = build_bid(
        ctx,
        intent.opposite(),
        hedge_b - 2.0 * ctx.tick_size,
        p.open_usdc_hedge,
        "open:hedge",
    ) {
        places.push(o);
    }

    let active = ActiveState {
        intent,
        opener_score: ctx.effective_score,
        opener_rule: rule,
        flip_count: 0,
        flip_freeze_until_ms: 0,
        avg_down_used: false,
        last_pyr_ms: None,
        last_dom_price: Some(dom_b),
        last_hedge_price: Some(hedge_b),
        last_requote_dom_ms: ctx.now_ms,
        last_requote_hedge_ms: ctx.now_ms,
        last_parity_ms: 0,
        last_scoop_ms: 0,
        score_persist_since_ms: ctx.now_ms,
        locked: false,
    };

    let decision = if places.is_empty() {
        Decision::NoOp
    } else {
        Decision::PlaceOrders(places)
    };
    (active, decision)
}

// ============================================================================
// DECIDE ACTIVE — 10-katman zincir
// ============================================================================

fn decide_active(
    active: &mut ActiveState,
    ctx: &StrategyContext<'_>,
    p: &ElisParams,
) -> Decision {
    let now = ctx.now_ms;
    let m = ctx.metrics;
    let intent = active.intent;
    let opp = intent.opposite();
    let dom_b = ctx.best_bid(intent);
    let opp_b = ctx.best_bid(opp);

    // 2. Deadline safety
    if let Some(rem) = ctx.market_remaining_secs {
        if rem <= p.deadline_safety_secs {
            return cancel_all_managed(ctx);
        }
    }

    // 3. Pre-resolve scoop (lock'a aldırmaz)
    if let Some(rem) = ctx.market_remaining_secs {
        if opp_b <= p.scoop_opp_bid_max
            && rem <= p.scoop_min_remaining_secs
            && elapsed_secs(now, active.last_scoop_ms) >= p.scoop_cooldown_secs
        {
            let dom_a = ctx.best_ask(intent);
            let price = (dom_a - ctx.tick_size).max(ctx.min_price);
            if let Some(o) = build_bid_at_price(ctx, intent, price, p.scoop_usdc, "scoop") {
                active.last_scoop_ms = now;
                return Decision::PlaceOrders(vec![o]);
            }
        }
    }

    // 4. Signal flip (lock'a aldırmaz, max 1 kez)
    let dscore_from_open = ctx.effective_score - active.opener_score;
    if dscore_from_open.abs() > p.signal_flip_threshold
        && active.flip_count < p.signal_flip_max_count
    {
        let new_intent = if dscore_from_open > 0.0 {
            Outcome::Up
        } else {
            Outcome::Down
        };
        if new_intent != intent {
            return execute_flip(active, ctx, p, new_intent, dscore_from_open);
        }
    }

    // 5. Lock check
    let avg_sum = m.avg_up + m.avg_down;
    let both_filled = m.up_filled > 0.0 && m.down_filled > 0.0;
    let locked = both_filled && avg_sum <= p.lock_avg_threshold;
    active.locked = locked;
    if locked {
        return Decision::NoOp;
    }

    // Skor persist tracking — yön değişiyorsa reset
    let score_dir_match = (ctx.effective_score >= p.score_neutral
        && intent == Outcome::Up)
        || (ctx.effective_score < p.score_neutral && intent == Outcome::Down);
    if !score_dir_match {
        active.score_persist_since_ms = now;
    }

    // 6. Avg-down (one-shot)
    let avg_dom = if intent == Outcome::Up { m.avg_up } else { m.avg_down };
    if !active.avg_down_used && avg_dom > 0.0 && dom_b + p.avg_down_min_edge <= avg_dom {
        active.avg_down_used = true;
        if let Some(o) = build_bid(ctx, intent, dom_b, p.order_usdc_dom, "avg_down") {
            active.last_dom_price = Some(dom_b);
            return Decision::PlaceOrders(vec![o]);
        }
    }

    // 7. Pyramid
    if let Some(ofi) = ctx.ofi {
        let persist_secs = elapsed_secs(now, active.score_persist_since_ms);
        let cooldown_ok = active
            .last_pyr_ms
            .is_none_or(|t| elapsed_secs(now, t) >= p.pyramid_cooldown_secs);
        if ofi >= p.pyramid_ofi_min
            && persist_secs >= p.pyramid_score_persist_secs
            && cooldown_ok
            && score_dir_match
            && dscore_from_open.abs() < 1.0
        {
            if let Some(o) = build_bid(ctx, intent, dom_b, p.pyramid_usdc, "pyramid") {
                active.last_pyr_ms = Some(now);
                active.last_dom_price = Some(dom_b);
                return Decision::PlaceOrders(vec![o]);
            }
        }
    }

    // 8. Dom requote (fiyat 2 tick değişti + 3s cooldown)
    let mut places: Vec<PlannedOrder> = Vec::new();
    if let Some(last) = active.last_dom_price {
        if (dom_b - last).abs() >= p.requote_price_eps
            && elapsed_secs(now, active.last_requote_dom_ms) >= p.requote_cooldown_secs
        {
            if let Some(o) = build_bid(ctx, intent, dom_b, p.order_usdc_dom, "requote_dom") {
                places.push(o);
                active.last_dom_price = Some(dom_b);
                active.last_requote_dom_ms = now;
            }
        }
    }

    // 9. Hedge requote — SADECE opp YÜKSELDİĞİNDE (kritik!)
    if let Some(last_hedge) = active.last_hedge_price {
        let hedge_drift = opp_b - last_hedge;
        if hedge_drift >= p.requote_price_eps
            && elapsed_secs(now, active.last_requote_hedge_ms) >= p.requote_cooldown_secs
            && opp_b >= p.parity_opp_bid_min
            && now >= active.flip_freeze_until_ms
        {
            if let Some(o) = build_bid(ctx, opp, opp_b, p.order_usdc_hedge, "requote_hedge") {
                places.push(o);
                active.last_hedge_price = Some(opp_b);
                active.last_requote_hedge_ms = now;
            }
        }
    }

    // 10. Parity gap
    let gap = (m.up_filled - m.down_filled).abs();
    if gap > p.parity_min_gap_qty
        && elapsed_secs(now, active.last_parity_ms) >= p.parity_cooldown_secs
        && opp_b >= p.parity_opp_bid_min
        && now >= active.flip_freeze_until_ms
    {
        if let Some(o) = build_bid(ctx, opp, opp_b, p.order_usdc_hedge, "parity_topup") {
            places.push(o);
            active.last_parity_ms = now;
        }
    }

    if places.is_empty() {
        Decision::NoOp
    } else {
        Decision::PlaceOrders(places)
    }
}

fn execute_flip(
    active: &mut ActiveState,
    ctx: &StrategyContext<'_>,
    p: &ElisParams,
    new_intent: Outcome,
    dscore_from_open: f64,
) -> Decision {
    active.flip_count += 1;
    active.flip_freeze_until_ms = ctx.now_ms + (p.flip_freeze_opp_secs * 1000.0) as u64;
    active.intent = new_intent;
    active.opener_score = ctx.effective_score;
    active.avg_down_used = false;
    active.score_persist_since_ms = ctx.now_ms;

    let dom_b = ctx.best_bid(new_intent);
    let hedge_b = ctx.best_bid(new_intent.opposite());

    let mut places: Vec<PlannedOrder> = Vec::new();
    // Flip sonrası dom'a 2x boost
    if let Some(o) = build_bid(
        ctx,
        new_intent,
        dom_b,
        p.order_usdc_dom * 2.0,
        "signal_flip",
    ) {
        places.push(o);
    }
    // Hedge çok küçük (eski intent'e zaten çok pozisyon var)
    if let Some(o) = build_bid(
        ctx,
        new_intent.opposite(),
        hedge_b,
        p.order_usdc_hedge * 0.3,
        "flip_hedge",
    ) {
        places.push(o);
    }
    active.last_dom_price = Some(dom_b);
    active.last_hedge_price = Some(hedge_b);
    let _ = dscore_from_open; // placeholder for log

    if places.is_empty() {
        Decision::NoOp
    } else {
        Decision::PlaceOrders(places)
    }
}

// ============================================================================
// HELPERS
// ============================================================================

fn build_bid(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    target_price: f64,
    usdc: f64,
    tag: &str,
) -> Option<PlannedOrder> {
    let price = target_price.clamp(ctx.min_price, ctx.max_price);
    if price < ctx.min_price {
        return None;
    }
    build_bid_at_price(ctx, outcome, price, usdc, tag)
}

fn build_bid_at_price(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    usdc: f64,
    tag: &str,
) -> Option<PlannedOrder> {
    if price <= 0.0 || usdc <= 0.0 {
        return None;
    }
    let size = usdc / price;
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
        reason: format!("elis:{}:{}", tag, outcome.as_lowercase()),
    })
}

fn cancel_all_managed(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| is_managed(&o.reason))
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(ids)
    }
}

fn is_managed(reason: &str) -> bool {
    reason.starts_with("elis:")
}

fn elapsed_secs(now_ms: u64, then_ms: u64) -> f64 {
    if now_ms <= then_ms {
        return 0.0;
    }
    (now_ms - then_ms) as f64 / 1000.0
}

fn active_is_deadline(_active: &ActiveState, ctx: &StrategyContext<'_>, p: &ElisParams) -> bool {
    ctx.market_remaining_secs
        .map(|r| r <= p.deadline_safety_secs)
        .unwrap_or(false)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StrategyParams;
    use crate::strategy::common::OpenOrder;
    use crate::strategy::metrics::StrategyMetrics;
    use crate::time::MarketZone;

    fn ctx_full<'a>(
        m: &'a StrategyMetrics,
        params: &'a StrategyParams,
        open_orders: &'a [OpenOrder],
        up_bid: f64,
        up_ask: f64,
        down_bid: f64,
        down_ask: f64,
        score: f64,
        bsi: Option<f64>,
        ofi: Option<f64>,
        cvd: Option<f64>,
        rem_secs: Option<f64>,
        now_ms: u64,
    ) -> StrategyContext<'a> {
        StrategyContext {
            metrics: m,
            up_token_id: "UP_TOKEN",
            down_token_id: "DOWN_TOKEN",
            up_best_bid: up_bid,
            up_best_ask: up_ask,
            down_best_bid: down_bid,
            down_best_ask: down_ask,
            api_min_order_size: 1.0,
            order_usdc: 10.0,
            effective_score: score,
            zone: MarketZone::DeepTrade,
            now_ms,
            last_averaging_ms: 0,
            tick_size: 0.01,
            open_orders,
            min_price: 0.01,
            max_price: 0.99,
            cooldown_threshold: 0,
            avg_threshold: 0.98,
            signal_ready: true,
            strategy_params: params,
            bsi,
            ofi,
            cvd,
            market_remaining_secs: rem_secs,
        }
    }

    #[test]
    fn pending_buffers_ticks_until_open() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let mut state = ElisState::default();
        // İlk 19 tick: hep Pending kalır, NoOp.
        for i in 0..19 {
            let c = ctx_full(
                &m, &p, &[], 0.50, 0.51, 0.50, 0.51, 5.0,
                None, None, None, Some(290.0), 1000 * i,
            );
            let (s, d) = ElisEngine::decide(state, &c);
            assert!(matches!(s, ElisState::Pending { .. }));
            assert!(matches!(d, Decision::NoOp));
            state = s;
        }
        // 20. tick: composite open (signaller None, fallback ScoreAverage)
        let c = ctx_full(
            &m, &p, &[], 0.50, 0.51, 0.50, 0.51, 5.5,
            None, None, None, Some(290.0), 20_000,
        );
        let (s, d) = ElisEngine::decide(state, &c);
        assert!(matches!(s, ElisState::Active(_)));
        match d {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 2);
                let dom = &orders[0];
                let hedge = &orders[1];
                assert_eq!(dom.outcome, Outcome::Up);
                assert_eq!(hedge.outcome, Outcome::Down);
            }
            other => panic!("beklenen PlaceOrders, gelen {:?}", other),
        }
    }

    #[test]
    fn predict_opener_bsi_reversion() {
        let p = ElisParams::default();
        let mut ticks = Vec::new();
        for _ in 0..20 {
            ticks.push(TickSnapshot {
                score: 5.0,
                bsi: Some(3.5),
                ofi: Some(0.0),
                cvd: Some(0.0),
            });
        }
        let (intent, rule) = predict_opener(&ticks, &p);
        assert_eq!(intent, Outcome::Down);
        assert_eq!(rule, OpenerRule::BsiReversion);
    }

    #[test]
    fn predict_opener_exhaustion() {
        let p = ElisParams::default();
        let mut ticks = Vec::new();
        for _ in 0..20 {
            ticks.push(TickSnapshot {
                score: 6.5,
                bsi: Some(0.5),
                ofi: Some(0.6),
                cvd: Some(5.0),
            });
        }
        let (intent, rule) = predict_opener(&ticks, &p);
        assert_eq!(intent, Outcome::Down);
        assert_eq!(rule, OpenerRule::Exhaustion);
    }

    #[test]
    fn predict_opener_momentum_when_signals_missing() {
        // bsi/ofi/cvd None → rule 1-3 atlanır → momentum
        let p = ElisParams::default();
        let ticks = (0..20)
            .map(|i| TickSnapshot {
                score: 4.0 + i as f64 * 0.1,
                bsi: None,
                ofi: None,
                cvd: None,
            })
            .collect::<Vec<_>>();
        let (intent, rule) = predict_opener(&ticks, &p);
        assert_eq!(intent, Outcome::Up);
        assert_eq!(rule, OpenerRule::Momentum);
    }

    #[test]
    fn predict_opener_score_avg_fallback() {
        // bsi/ofi/cvd None + dscore küçük → score_avg
        let p = ElisParams::default();
        let ticks = (0..20)
            .map(|_| TickSnapshot {
                score: 4.0,
                bsi: None,
                ofi: None,
                cvd: None,
            })
            .collect::<Vec<_>>();
        let (intent, rule) = predict_opener(&ticks, &p);
        assert_eq!(intent, Outcome::Down);
        assert_eq!(rule, OpenerRule::ScoreAverage);
    }

    #[test]
    fn signal_flip_triggers_when_threshold_exceeded() {
        let m = StrategyMetrics::default();
        let p_params = StrategyParams::default();
        let p = ElisParams::default();

        let mut active = ActiveState {
            intent: Outcome::Up,
            opener_score: 5.0,
            opener_rule: OpenerRule::ScoreAverage,
            flip_count: 0,
            flip_freeze_until_ms: 0,
            avg_down_used: false,
            last_pyr_ms: None,
            last_dom_price: Some(0.50),
            last_hedge_price: Some(0.50),
            last_requote_dom_ms: 0,
            last_requote_hedge_ms: 0,
            last_parity_ms: 0,
            last_scoop_ms: 0,
            score_persist_since_ms: 0,
            locked: false,
        };
        // dscore = -6.0, |6| > 5.0 (threshold)
        let c = ctx_full(
            &m, &p_params, &[], 0.30, 0.31, 0.70, 0.71, -1.0,
            None, None, None, Some(200.0), 100_000,
        );
        let d = decide_active(&mut active, &c, &p);
        assert_eq!(active.intent, Outcome::Down);
        assert_eq!(active.flip_count, 1);
        assert!(matches!(d, Decision::PlaceOrders(_)));
    }

    #[test]
    fn lock_blocks_new_orders() {
        let mut m = StrategyMetrics::default();
        m.up_filled = 100.0;
        m.avg_up = 0.45;
        m.down_filled = 100.0;
        m.avg_down = 0.50;
        // avg_sum = 0.95 ≤ 0.97 → lock

        let p_params = StrategyParams::default();
        let p = ElisParams::default();
        let mut active = ActiveState {
            intent: Outcome::Up,
            opener_score: 5.0,
            opener_rule: OpenerRule::ScoreAverage,
            flip_count: 0,
            flip_freeze_until_ms: 0,
            avg_down_used: false,
            last_pyr_ms: None,
            last_dom_price: Some(0.45),
            last_hedge_price: Some(0.50),
            last_requote_dom_ms: 0,
            last_requote_hedge_ms: 0,
            last_parity_ms: 0,
            last_scoop_ms: 0,
            score_persist_since_ms: 0,
            locked: false,
        };
        let c = ctx_full(
            &m, &p_params, &[], 0.45, 0.46, 0.50, 0.51, 5.0,
            None, None, None, Some(200.0), 100_000,
        );
        let d = decide_active(&mut active, &c, &p);
        assert!(active.locked);
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn deadline_cancels_all() {
        let m = StrategyMetrics::default();
        let p_params = StrategyParams::default();
        let p = ElisParams::default();
        let mut active = ActiveState {
            intent: Outcome::Up,
            opener_score: 5.0,
            opener_rule: OpenerRule::ScoreAverage,
            flip_count: 0,
            flip_freeze_until_ms: 0,
            avg_down_used: false,
            last_pyr_ms: None,
            last_dom_price: Some(0.50),
            last_hedge_price: Some(0.50),
            last_requote_dom_ms: 0,
            last_requote_hedge_ms: 0,
            last_parity_ms: 0,
            last_scoop_ms: 0,
            score_persist_since_ms: 0,
            locked: false,
        };
        let orders = [OpenOrder {
            id: "o1".into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.50,
            size: 20.0,
            reason: "elis:open:dom:up".into(),
            placed_at_ms: 0,
            size_matched: 0.0,
        }];
        let c = ctx_full(
            &m, &p_params, &orders, 0.50, 0.51, 0.50, 0.51, 5.0,
            None, None, None, Some(5.0), 295_000,
        );
        let d = decide_active(&mut active, &c, &p);
        match d {
            Decision::CancelOrders(ids) => assert_eq!(ids, vec!["o1".to_string()]),
            other => panic!("beklenen CancelOrders, gelen {:?}", other),
        }
    }

    #[test]
    fn scoop_triggers_late_when_opp_cheap() {
        let m = StrategyMetrics::default();
        let p_params = StrategyParams::default();
        let p = ElisParams::default();
        let mut active = ActiveState {
            intent: Outcome::Up,
            opener_score: 5.0,
            opener_rule: OpenerRule::ScoreAverage,
            flip_count: 0,
            flip_freeze_until_ms: 0,
            avg_down_used: false,
            last_pyr_ms: None,
            last_dom_price: Some(0.95),
            last_hedge_price: Some(0.05),
            last_requote_dom_ms: 0,
            last_requote_hedge_ms: 0,
            last_parity_ms: 0,
            last_scoop_ms: 0,
            score_persist_since_ms: 0,
            locked: false,
        };
        // up_bid=0.95, down_bid=0.03 (≤0.05), rem=20s (≤35)
        let c = ctx_full(
            &m, &p_params, &[], 0.95, 0.96, 0.03, 0.04, 8.0,
            None, None, None, Some(20.0), 280_000,
        );
        let d = decide_active(&mut active, &c, &p);
        match d {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 1);
                assert_eq!(orders[0].outcome, Outcome::Up);
                assert!(orders[0].reason.contains("scoop"));
            }
            other => panic!("beklenen PlaceOrders (scoop), gelen {:?}", other),
        }
        assert_eq!(active.last_scoop_ms, 280_000);
    }
}

