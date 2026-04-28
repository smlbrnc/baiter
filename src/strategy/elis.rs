//! Elis stratejisi — Polymarket dual-side inventory arbitrage
//! ([elis-strategy.md](.cursor/docs/elis-strategy.md)).
//!
//! ## Amaç
//!
//! YES/NO best-bid toplamı `< 0.985` iken iki tarafa da maker bid yerleştirip
//! pair_count maksimize ederek `avg_up + avg_down ≤ avg_threshold` kilidini
//! yakalamak. Yön tahmini yok; tek motor envanter dengesi (`imbalance`) ve
//! ortalama maliyet (`avg_sum`).
//!
//! ## Karar önceliği (her tick)
//!
//! 1. **Hard stop** (`avg_sum > 1.01`): tüm Elis emirleri iptal + hafif tarafa
//!    tek hedge bid (imbalance varsa).
//! 2. **Lock** (`metrics.profit_locked(avg_threshold)`): ağır taraftaki Elis
//!    emirlerini iptal + hafif tarafa hedge bid; `locked = true` set'lenir.
//! 3. **StopTrade** (`zone == StopTrade`): lock ile aynı (yeni pair giriş yok,
//!    sadece imbalance hedge).
//! 4. **Momentum** (`|score - 5| > 0.5` veya `|Δscore| > 0.5`): aynı hedge-only
//!    davranış.
//! 5. **Spread kapalı** (`up_bid + down_bid >= 0.985`): tüm Elis emirleri iptal,
//!    yeni emir verme.
//! 6. **Normal** (imbalance bantları):
//!    - `|imb| < 3` → her iki outcome `best_bid` maker bid (BALANCED).
//!    - `3 ≤ |imb|` → ağır taraf iptal + hafif `best_ask - tick` hedge.
//!
//! Tek tick'te tüm aksiyonlar tek `Decision` (idealde `CancelAndPlace`)
//! envelope'unda batch'lenir.

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::time::MarketZone;
use crate::types::{Outcome, OrderType, Side};

/// §3 — `yes_bid + no_bid` bu eşiğin altında ise yeni pair girişi açıktır.
const ENTRY_THRESHOLD: f64 = 0.990;
/// §3 — Hysteresis: spread bu eşiği aşarsa mevcut emirler iptal edilir.
/// ENTRY ile EXIT arasında ([0.990, 1.000)) kalan spreadlerde mevcut emirlere
/// dokunulmaz — 1-2 saniyelik spread kapanmaları emirleri öldürmesin.
const EXIT_THRESHOLD: f64 = 1.000;
/// §11 — `avg_up + avg_down` bu eşiği aşarsa hard stop.
const HARD_STOP_AVG: f64 = 1.01;
/// §9 MODE1 — `|imb|` bu sınırın altında ise iki taraf da pair quoting.
const BALANCED_IMB: f64 = 3.0;
/// §15 — composite skorun nötrden (`5.0`) mutlak sapması bu eşiği aşarsa
/// momentum dondurma.
const MOMENTUM_ABS: f64 = 1.0;
/// §15 — tick-to-tick skor sıçraması bu eşiği aşarsa momentum dondurma.
const MOMENTUM_DELTA: f64 = 1.0;
/// Composite skorun nötr orta noktası.
const NEUTRAL_SCORE: f64 = 5.0;

/// Elis FSM state'i.
///
/// `Pending` ilk tick'te (skor henüz okunmamışken) bir kez kullanılır; sonraki
/// tüm tick'lerde `Active` döner. `last_score` momentum delta için, `locked`
/// ise `metrics.profit_locked()` bir kez gerçekleştikten sonra geri dönmemesi
/// için latch'lenir.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ElisState {
    #[default]
    Pending,
    Active {
        last_score: f64,
        locked: bool,
    },
}

pub struct ElisEngine;

impl ElisEngine {
    pub fn decide(state: ElisState, ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        let prev_score = match state {
            ElisState::Active { last_score, .. } => last_score,
            ElisState::Pending => ctx.effective_score,
        };
        let prev_locked = matches!(state, ElisState::Active { locked: true, .. });
        let next_locked = prev_locked || ctx.metrics.profit_locked(ctx.avg_threshold);

        let decision = compute_decision(ctx, prev_score, next_locked);

        let next_state = ElisState::Active {
            last_score: ctx.effective_score,
            locked: next_locked,
        };
        (next_state, decision)
    }
}

/// Tick başına kararı türetir; state mutasyonu çağıran tarafta yapılır.
fn compute_decision(ctx: &StrategyContext<'_>, prev_score: f64, locked: bool) -> Decision {
    let m = ctx.metrics;
    let abs_imb = m.imbalance().abs();

    if m.avg_sum() > HARD_STOP_AVG {
        return hedge_only(ctx, /* cancel_all = */ true);
    }

    if locked {
        return hedge_only(ctx, false);
    }

    if ctx.zone == MarketZone::StopTrade {
        return hedge_only(ctx, false);
    }

    let mom_abs = (ctx.effective_score - NEUTRAL_SCORE).abs() > MOMENTUM_ABS;
    let mom_delta = (ctx.effective_score - prev_score).abs() > MOMENTUM_DELTA;
    if mom_abs || mom_delta {
        return hedge_only(ctx, false);
    }

    let spread = ctx.up_best_bid + ctx.down_best_bid;
    if spread >= EXIT_THRESHOLD {
        // Spread açıkça kârsız — tüm Elis emirlerini iptal et.
        return cancel_only(ctx);
    }
    if spread >= ENTRY_THRESHOLD {
        // Hysteresis bölgesi: yeni pair girişi yok, mevcut emirlere dokunma.
        return Decision::NoOp;
    }

    if abs_imb < BALANCED_IMB {
        balanced(ctx)
    } else {
        hedge_only(ctx, false)
    }
}

/// Tüm Elis emirlerini iptal etmek için kısa yol; başka aksiyon yok.
fn cancel_only(ctx: &StrategyContext<'_>) -> Decision {
    let cancels = collect_elis_open_ids(ctx);
    if cancels.is_empty() {
        Decision::NoOp
    } else {
        Decision::CancelOrders(cancels)
    }
}

/// Hedge-only modu: ağır taraftaki Elis emirlerini iptal, hafif tarafa tek
/// hedge bid. `cancel_all = true` ise hafif taraf da dahil tüm Elis emirleri
/// iptal edilip ardından light hedge yeniden yerleştirilir (hard-stop için).
fn hedge_only(ctx: &StrategyContext<'_>, cancel_all: bool) -> Decision {
    let imb = ctx.metrics.imbalance();
    let (heavy, light) = if imb > 0.0 {
        (Some(Outcome::Up), Some(Outcome::Down))
    } else if imb < 0.0 {
        (Some(Outcome::Down), Some(Outcome::Up))
    } else {
        (None, None)
    };

    // Heavy-side cancellations (cancel_all=true ise tüm Elis emirleri).
    let heavy_cancels: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| is_managed(&o.reason))
        .filter(|o| match (cancel_all, heavy) {
            (true, _) => true,
            (false, Some(h)) => o.outcome == h,
            // Imbalance == 0: light yok, tüm Elis emirleri iptal.
            (false, None) => true,
        })
        .map(|o| o.id.clone())
        .collect();

    let Some(light_outcome) = light else {
        return materialize(heavy_cancels, Vec::new());
    };

    let Some(planned) = build_hedge_bid(ctx, light_outcome) else {
        return materialize(heavy_cancels, Vec::new());
    };

    let existing_light: Vec<&OpenOrder> = ctx
        .open_orders
        .iter()
        .filter(|o| is_managed(&o.reason) && o.outcome == light_outcome)
        .collect();

    // Mevcut hafif emir hedef fiyata yakın **ve** iptal listesinde değilse
    // (yani cancel_all=false ve heavy != light) yerinde bırakılır. cancel_all
    // modunda emir zaten iptal edileceği için bu kısa devre uygulanmaz.
    let eps = requote_threshold(ctx.tick_size);
    let light_at_target = existing_light.len() == 1
        && (existing_light[0].price - planned.price).abs() < eps
        && !heavy_cancels.contains(&existing_light[0].id);

    if light_at_target {
        return materialize(heavy_cancels, Vec::new());
    }

    let mut cancels = heavy_cancels;
    for o in &existing_light {
        if !cancels.contains(&o.id) {
            cancels.push(o.id.clone());
        }
    }
    materialize(cancels, vec![planned])
}

/// Balanced mod: her iki outcome için `best_bid` maker bid; mevcut emir
/// hedeftekiyle aynıysa skip.
fn balanced(ctx: &StrategyContext<'_>) -> Decision {
    let mut cancels: Vec<String> = Vec::new();
    let mut places: Vec<PlannedOrder> = Vec::new();

    for outcome in [Outcome::Up, Outcome::Down] {
        let target = build_normal_bid(ctx, outcome);
        let existing: Vec<&OpenOrder> = ctx
            .open_orders
            .iter()
            .filter(|o| is_managed(&o.reason) && o.outcome == outcome)
            .collect();

        match target {
            Some(planned) => {
                let eps = requote_threshold(ctx.tick_size);
                if existing.len() == 1
                    && (existing[0].price - planned.price).abs() < eps
                {
                    continue;
                }
                for o in &existing {
                    cancels.push(o.id.clone());
                }
                places.push(planned);
            }
            None => {
                // Geçerli fiyat yoksa eldeki emirleri iptal et, yeni koyma.
                for o in &existing {
                    cancels.push(o.id.clone());
                }
            }
        }
    }

    materialize(cancels, places)
}

/// Outcome için maker normal pair bid.
///
/// Adaptif fiyatlama: henüz hiç pair fill yoksa (`pair_count == 0`)
/// `best_bid - tick_size` ile konservatif başlar (avg_sum'u düşük tutar);
/// en az bir pair fill oluştuktan sonra `best_bid`'e döner.
fn build_normal_bid(ctx: &StrategyContext<'_>, outcome: Outcome) -> Option<PlannedOrder> {
    let bb = ctx.best_bid(outcome);
    if bb <= 0.0 {
        return None;
    }
    let target = if ctx.metrics.pair_count() == 0.0 {
        // İlk fill'i ucuza almaya çalış; avg_sum'u düşük başlatır.
        (bb - ctx.tick_size).max(ctx.min_price)
    } else {
        bb
    };
    let price = target.clamp(ctx.min_price, ctx.max_price);
    let size = ctx.order_usdc / price;
    if size <= 0.0 || size * price < ctx.api_min_order_size {
        return None;
    }
    Some(PlannedOrder {
        outcome,
        token_id: ctx.token_id(outcome).to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Gtc,
        reason: format!("elis:bid:{}", outcome.as_lowercase()),
    })
}

/// Hafif outcome için `best_ask - tick` agresif maker hedge bid.
fn build_hedge_bid(ctx: &StrategyContext<'_>, outcome: Outcome) -> Option<PlannedOrder> {
    let ba = ctx.best_ask(outcome);
    if ba <= 0.0 {
        return None;
    }
    let raw = ba - ctx.tick_size;
    if raw <= 0.0 {
        return None;
    }
    let price = raw.clamp(ctx.min_price, ctx.max_price);
    let size = ctx.order_usdc / price;
    if size <= 0.0 || size * price < ctx.api_min_order_size {
        return None;
    }
    Some(PlannedOrder {
        outcome,
        token_id: ctx.token_id(outcome).to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Gtc,
        reason: format!("elis:hedge:{}", outcome.as_lowercase()),
    })
}

fn is_managed(reason: &str) -> bool {
    reason.starts_with("elis:")
}

fn collect_elis_open_ids(ctx: &StrategyContext<'_>) -> Vec<String> {
    ctx.open_orders
        .iter()
        .filter(|o| is_managed(&o.reason))
        .map(|o| o.id.clone())
        .collect()
}

/// Tick yarısı kadar fark "değişmedi" sayılır (re-quote spam'i engeller).
fn requote_threshold(tick_size: f64) -> f64 {
    (tick_size / 2.0).max(1e-6)
}

/// Cancel/places listesini en uygun `Decision` varyantına paketler.
fn materialize(cancels: Vec<String>, places: Vec<PlannedOrder>) -> Decision {
    match (cancels.is_empty(), places.is_empty()) {
        (true, true) => Decision::NoOp,
        (false, true) => Decision::CancelOrders(cancels),
        (true, false) => Decision::PlaceOrders(places),
        (false, false) => Decision::CancelAndPlace { cancels, places },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StrategyParams;
    use crate::strategy::metrics::StrategyMetrics;

    fn ctx<'a>(
        m: &'a StrategyMetrics,
        params: &'a StrategyParams,
        open_orders: &'a [OpenOrder],
        up_bid: f64,
        up_ask: f64,
        down_bid: f64,
        down_ask: f64,
        score: f64,
        zone: MarketZone,
    ) -> StrategyContext<'a> {
        StrategyContext {
            metrics: m,
            up_token_id: "UP_TOKEN",
            down_token_id: "DOWN_TOKEN",
            up_best_bid: up_bid,
            up_best_ask: up_ask,
            down_best_bid: down_bid,
            down_best_ask: down_ask,
            api_min_order_size: 5.0,
            order_usdc: 10.0,
            effective_score: score,
            zone,
            now_ms: 1_000,
            last_averaging_ms: 0,
            tick_size: 0.01,
            open_orders,
            min_price: 0.01,
            max_price: 0.99,
            cooldown_threshold: 0,
            avg_threshold: 0.98,
            signal_ready: true,
            strategy_params: params,
        }
    }

    fn open(id: &str, outcome: Outcome, price: f64, size: f64, reason: &str) -> OpenOrder {
        OpenOrder {
            id: id.to_string(),
            outcome,
            side: Side::Buy,
            price,
            size,
            reason: reason.to_string(),
            placed_at_ms: 0,
            size_matched: 0.0,
        }
    }

    #[test]
    fn balanced_places_both_sides_when_spread_open() {
        // pair_count == 0 → adaptif fiyat: best_bid - tick_size (0.45-0.01=0.44, 0.50-0.01=0.49)
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let c = ctx(
            &m,
            &p,
            &[],
            0.45,
            0.46,
            0.50,
            0.51,
            5.0,
            MarketZone::DeepTrade,
        );
        let (state, d) = ElisEngine::decide(ElisState::Pending, &c);
        assert!(matches!(state, ElisState::Active { locked: false, .. }));
        match d {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 2);
                let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
                let dn = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
                assert_eq!(up.reason, "elis:bid:up");
                assert!((up.price - 0.44).abs() < 1e-9);
                assert_eq!(dn.reason, "elis:bid:down");
                assert!((dn.price - 0.49).abs() < 1e-9);
            }
            other => panic!("beklenen PlaceOrders, gelen {:?}", other),
        }
    }

    #[test]
    fn balanced_uses_best_bid_after_first_pair_fill() {
        // pair_count > 0 ve avg_sum = 0.99 (> avg_threshold=0.98, < hard_stop=1.01)
        // → profit_locked değil → balanced mod → normal fiyat: best_bid
        let mut m = StrategyMetrics::default();
        m.up_filled = 5.0;
        m.avg_up = 0.50;
        m.down_filled = 5.0;
        m.avg_down = 0.49;
        let p = StrategyParams::default();
        let c = ctx(
            &m,
            &p,
            &[],
            0.45,
            0.46,
            0.50,
            0.51,
            5.0,
            MarketZone::DeepTrade,
        );
        let (_, d) = ElisEngine::decide(ElisState::Pending, &c);
        match d {
            Decision::PlaceOrders(orders) => {
                let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
                let dn = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
                assert!((up.price - 0.45).abs() < 1e-9, "pair fill sonrası up best_bid beklendi");
                assert!((dn.price - 0.50).abs() < 1e-9, "pair fill sonrası down best_bid beklendi");
            }
            other => panic!("beklenen PlaceOrders, gelen {:?}", other),
        }
    }

    #[test]
    fn spread_closed_cancels_open_orders() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let orders = [open("o1", Outcome::Up, 0.49, 20.0, "elis:bid:up")];
        let c = ctx(
            &m,
            &p,
            &orders,
            0.49,
            0.50,
            0.51,
            0.52,
            5.0,
            MarketZone::DeepTrade,
        );
        let (_, d) = ElisEngine::decide(ElisState::Pending, &c);
        match d {
            Decision::CancelOrders(ids) => assert_eq!(ids, vec!["o1".to_string()]),
            other => panic!("beklenen CancelOrders, gelen {:?}", other),
        }
    }

    #[test]
    fn imbalance_above_band_triggers_hedge_only() {
        let mut m = StrategyMetrics::default();
        m.up_filled = 5.0;
        m.avg_up = 0.45;
        m.down_filled = 0.0;
        let p = StrategyParams::default();
        // d1 normal bid 0.49; hedge hedef = 0.51 - 0.01 = 0.50 (≠ 0.49).
        let orders = [
            open("u1", Outcome::Up, 0.45, 22.0, "elis:bid:up"),
            open("d1", Outcome::Down, 0.49, 20.0, "elis:bid:down"),
        ];
        let c = ctx(
            &m,
            &p,
            &orders,
            0.45,
            0.46,
            0.49,
            0.51,
            5.0,
            MarketZone::DeepTrade,
        );
        let (_, d) = ElisEngine::decide(ElisState::Pending, &c);
        match d {
            Decision::CancelAndPlace { cancels, places } => {
                assert!(cancels.contains(&"u1".to_string()));
                assert!(cancels.contains(&"d1".to_string()));
                assert_eq!(places.len(), 1);
                assert_eq!(places[0].outcome, Outcome::Down);
                assert_eq!(places[0].reason, "elis:hedge:down");
                assert!((places[0].price - 0.50).abs() < 1e-9);
            }
            other => panic!("beklenen CancelAndPlace, gelen {:?}", other),
        }
    }

    #[test]
    fn hard_stop_cancels_all_and_hedges_light() {
        let mut m = StrategyMetrics::default();
        m.up_filled = 5.0;
        m.avg_up = 0.55;
        m.down_filled = 1.0;
        m.avg_down = 0.50;
        let p = StrategyParams::default();
        // avg_sum = 1.05 > 1.01 → HARD STOP. Hedge hedef = 0.52 - 0.01 = 0.51.
        let orders = [
            open("u1", Outcome::Up, 0.45, 22.0, "elis:bid:up"),
            open("d1", Outcome::Down, 0.49, 20.0, "elis:hedge:down"),
        ];
        let c = ctx(
            &m,
            &p,
            &orders,
            0.45,
            0.46,
            0.49,
            0.52,
            5.0,
            MarketZone::DeepTrade,
        );
        let (_, d) = ElisEngine::decide(ElisState::Pending, &c);
        match d {
            Decision::CancelAndPlace { cancels, places } => {
                assert!(cancels.contains(&"u1".to_string()));
                assert!(cancels.contains(&"d1".to_string()));
                assert_eq!(places.len(), 1);
                assert_eq!(places[0].outcome, Outcome::Down);
                assert_eq!(places[0].reason, "elis:hedge:down");
                assert!((places[0].price - 0.51).abs() < 1e-9);
            }
            other => panic!("beklenen CancelAndPlace, gelen {:?}", other),
        }
    }

    #[test]
    fn lock_latches_and_keeps_position() {
        // avg_up + avg_down ≤ avg_threshold AND pair_count > 0 → profit_locked.
        let mut m = StrategyMetrics::default();
        m.up_filled = 5.0;
        m.avg_up = 0.45;
        m.down_filled = 5.0;
        m.avg_down = 0.50;
        let p = StrategyParams::default();
        let c = ctx(
            &m,
            &p,
            &[],
            0.45,
            0.46,
            0.50,
            0.51,
            5.0,
            MarketZone::DeepTrade,
        );
        let (state, _) = ElisEngine::decide(ElisState::Pending, &c);
        assert!(matches!(state, ElisState::Active { locked: true, .. }));

        // Tekrar tick: avg değişmese bile locked kalır.
        let mut m2 = StrategyMetrics::default();
        m2.up_filled = 5.0;
        m2.avg_up = 0.50;
        m2.down_filled = 5.0;
        m2.avg_down = 0.55; // avg_sum = 1.05 > avg_threshold → profit_locked false
        let c2 = ctx(
            &m2,
            &p,
            &[],
            0.45,
            0.46,
            0.50,
            0.51,
            5.0,
            MarketZone::DeepTrade,
        );
        let (state2, _) = ElisEngine::decide(state, &c2);
        assert!(matches!(state2, ElisState::Active { locked: true, .. }));
    }

    #[test]
    fn momentum_score_jump_triggers_hedge_only() {
        let mut m = StrategyMetrics::default();
        m.up_filled = 1.0;
        m.avg_up = 0.45;
        let p = StrategyParams::default();
        let orders = [open("u1", Outcome::Up, 0.45, 22.0, "elis:bid:up")];
        // last_score 5.0 → effective_score 6.2: |Δ| = 1.2 > MOMENTUM_DELTA(1.0) → momentum.
        // Hedge hedef = 0.52 - 0.01 = 0.51 (down outcome'da hiç emir yok).
        let c = ctx(
            &m,
            &p,
            &orders,
            0.45,
            0.46,
            0.49,
            0.52,
            6.2,
            MarketZone::DeepTrade,
        );
        let prev_state = ElisState::Active {
            last_score: 5.0,
            locked: false,
        };
        let (_, d) = ElisEngine::decide(prev_state, &c);
        match d {
            Decision::CancelAndPlace { cancels, places } => {
                assert!(cancels.contains(&"u1".to_string()));
                assert_eq!(places.len(), 1);
                assert_eq!(places[0].outcome, Outcome::Down);
                assert_eq!(places[0].reason, "elis:hedge:down");
                assert!((places[0].price - 0.51).abs() < 1e-9);
            }
            other => panic!("beklenen CancelAndPlace (momentum hedge), gelen {:?}", other),
        }
    }

    #[test]
    fn stoptrade_zone_blocks_new_pairs() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let c = ctx(
            &m,
            &p,
            &[],
            0.45,
            0.46,
            0.50,
            0.51,
            5.0,
            MarketZone::StopTrade,
        );
        let (_, d) = ElisEngine::decide(ElisState::Pending, &c);
        // imbalance == 0 ve hiç emir yok → NoOp.
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn balanced_existing_at_target_is_noop() {
        // pair_count == 0 → adaptif hedef: best_bid - tick (0.44, 0.49).
        // Mevcut emirler hedefte → NoOp.
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let orders = [
            open("u1", Outcome::Up, 0.44, 22.0, "elis:bid:up"),
            open("d1", Outcome::Down, 0.49, 20.0, "elis:bid:down"),
        ];
        let c = ctx(
            &m,
            &p,
            &orders,
            0.45,
            0.46,
            0.50,
            0.51,
            5.0,
            MarketZone::DeepTrade,
        );
        let (_, d) = ElisEngine::decide(ElisState::Pending, &c);
        assert!(matches!(d, Decision::NoOp));
    }
}
