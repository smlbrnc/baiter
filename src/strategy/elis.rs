//! Elis stratejisi — Dutch Book Spread Capture.
//!
//! Doküman: `docs/elis.md`
//!
//! ## Strateji Özeti
//!
//! Her 5 dakikalık BTC UP/DOWN piyasasında spread capture (arbitraj yakalama):
//!
//! ```text
//! Arbitraj Marjı = $1.00 − (UP_ask + DOWN_ask)
//! ```
//!
//! Her iki taraf bid-ask spread'i `spread_threshold`'ı aşınca:
//! - BUY UP @ UP_ask + BUY DOWN @ DOWN_ask (taker — ask fiyatından)
//! - `trade_cooldown_ms` ms bekle
//! - Her iki emri iptal et
//!
//! Bu döngü pencere bitimine `stop_before_end_secs` saniye kalana dek tekrar eder.
//!
//! ## Balance Factor Mekanizması
//!
//! ```text
//! imbalance  = |UP_pozisyon − DOWN_pozisyon|
//! adjustment = round(imbalance × balance_factor × 0.5)
//!
//! geride_kalan_taraf_emir = max_buy_order_size + adjustment
//! dominant_taraf_emir     = max(max_buy_order_size − adjustment, 1)
//! ```
//!
//! Geride kalan tarafa daha büyük emir verilir; böylece pozisyon dengede tutulur.
//!
//! ## FSM State'leri
//!
//! ```text
//! Idle         → Spread koşulu bekleniyor; hazır olunca BatchPending'e geçer.
//! BatchPending → UP+DOWN emirleri gönderildi; trade_cooldown_ms geçince iptal.
//! Done         → stop_before_end_secs veya window end → artık işlem yok.
//! ```

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::config::ElisParams;
use crate::types::{OrderType, Outcome, Side};

/// Dutch Book FSM state'i.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElisState {
    /// Spread bekleniyor — emir yok.
    Idle,
    /// Batch emirler gönderildi; `placed_at_ms`'den `trade_cooldown_ms` sonra iptal.
    BatchPending { placed_at_ms: u64 },
    /// Pencere sona erdi veya stop tetiklendi — yalnızca NoOp.
    Done,
}

impl Default for ElisState {
    fn default() -> Self {
        Self::Idle
    }
}

pub struct ElisEngine;

impl ElisEngine {
    /// Tek tick — yeni state + Decision döndürür.
    pub fn decide(state: ElisState, ctx: &StrategyContext<'_>) -> (ElisState, Decision) {
        let p = ElisParams::from_strategy_params(ctx.strategy_params);

        match state {
            // Pencere sona erdi — sadece NoOp.
            ElisState::Done => (ElisState::Done, Decision::NoOp),

            // Batch emir bekleniyor.
            ElisState::BatchPending { placed_at_ms } => {
                // 1. Pencere stop kontrolü — önce deadline'ı yakala.
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, cancel_managed(ctx));
                }
                // 2. Cooldown doldu mu?
                if ctx.now_ms.saturating_sub(placed_at_ms) >= p.trade_cooldown_ms {
                    return (ElisState::Idle, cancel_managed(ctx));
                }
                // Cooldown henüz dolmadı — bekle.
                (ElisState::BatchPending { placed_at_ms }, Decision::NoOp)
            }

            // Spread fırsatı bekle.
            ElisState::Idle => {
                // Pencere stop kontrolü.
                if is_window_stop(ctx, &p) {
                    return (ElisState::Done, Decision::NoOp);
                }

                // Spread koşulu (dok §8): her iki tarafta bid-ask spread ≥ threshold.
                // UP_spread = UP_ask − UP_bid, DOWN_spread = DOWN_ask − DOWN_bid
                // Marjın negatif/pozitif olması önemli değil; spread büyüklüğü baz alınır.
                let up_spread = ctx.up_best_ask - ctx.up_best_bid;
                let dn_spread = ctx.down_best_ask - ctx.down_best_bid;
                if up_spread < p.spread_threshold || dn_spread < p.spread_threshold {
                    return (ElisState::Idle, Decision::NoOp);
                }

                // Fiyat aralığı kontrolü (bid fiyatına göre — maker emirler bid'den girer).
                if ctx.up_best_bid < ctx.min_price
                    || ctx.up_best_bid > ctx.max_price
                    || ctx.down_best_bid < ctx.min_price
                    || ctx.down_best_bid > ctx.max_price
                {
                    return (ElisState::Idle, Decision::NoOp);
                }

                // Balance factor ile emir boyutlarını hesapla.
                let (up_size, dn_size) = balance_sizes(ctx, &p);

                let mut orders: Vec<PlannedOrder> = Vec::with_capacity(2);

                // Maker limit emirler: bid fiyatından gir (spread'i sat, maker ol).
                // UP_bid + DOWN_bid < $1.00 → fill olursa kârlı Dutch Book.
                if let Some(o) = make_order(ctx, Outcome::Up, ctx.up_best_bid, up_size) {
                    orders.push(o);
                }
                if let Some(o) = make_order(ctx, Outcome::Down, ctx.down_best_bid, dn_size) {
                    orders.push(o);
                }

                if orders.is_empty() {
                    return (ElisState::Idle, Decision::NoOp);
                }

                (
                    ElisState::BatchPending { placed_at_ms: ctx.now_ms },
                    Decision::PlaceOrders(orders),
                )
            }
        }
    }
}

// ============================================================================
// BALANCE FACTOR
// ============================================================================

/// Dokümandaki formül:
/// ```text
/// imbalance  = |UP_pos − DOWN_pos|
/// adjustment = round(imbalance × balance_factor × 0.5)
/// lagging    = max_buy_order_size + adjustment
/// dominant   = max(max_buy_order_size − adjustment, 1)
/// ```
fn balance_sizes(ctx: &StrategyContext<'_>, p: &ElisParams) -> (f64, f64) {
    let up_pos = ctx.metrics.up_filled;
    let dn_pos = ctx.metrics.down_filled;
    let imbalance = (up_pos - dn_pos).abs();
    let adjustment = (imbalance * p.balance_factor * 0.5).round();

    let base = p.max_buy_order_size;
    // Dominant = fazla olan taraf, Lagging = geride kalan taraf.
    if up_pos >= dn_pos {
        // UP dominant → UP az al, DOWN fazla al.
        let up_size = (base - adjustment).max(1.0);
        let dn_size = base + adjustment;
        (up_size, dn_size)
    } else {
        // DOWN dominant → DOWN az al, UP fazla al.
        let up_size = base + adjustment;
        let dn_size = (base - adjustment).max(1.0);
        (up_size, dn_size)
    }
}

// ============================================================================
// HELPERS
// ============================================================================

/// Pencere stop koşulu: `market_remaining_secs` doluysa ve kalan ≤ eşik.
fn is_window_stop(ctx: &StrategyContext<'_>, p: &ElisParams) -> bool {
    ctx.market_remaining_secs
        .map(|r| r <= p.stop_before_end_secs)
        .unwrap_or(false)
}

/// `elis:` prefix'li tüm açık emirleri iptal eder.
fn cancel_managed(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("elis:"))
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) }
}

/// Tek bir BUY emri oluşturur. `size` share cinsinden, `price` ask fiyatından.
fn make_order(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    size: f64,
) -> Option<PlannedOrder> {
    if price <= 0.0 || size <= 0.0 {
        return None;
    }
    // CLOB minimum notional kontrolü.
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
        reason: format!("elis:dutch:{}", outcome.as_lowercase()),
    })
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

    fn ctx<'a>(
        m: &'a StrategyMetrics,
        params: &'a StrategyParams,
        open_orders: &'a [OpenOrder],
        up_bid: f64,
        up_ask: f64,
        down_bid: f64,
        down_ask: f64,
        rem_secs: Option<f64>,
        now_ms: u64,
    ) -> StrategyContext<'a> {
        StrategyContext {
            metrics: m,
            up_token_id: "UP",
            down_token_id: "DOWN",
            up_best_bid: up_bid,
            up_best_ask: up_ask,
            down_best_bid: down_bid,
            down_best_ask: down_ask,
            api_min_order_size: 1.0,
            order_usdc: 10.0,
            effective_score: 5.0,
            zone: MarketZone::DeepTrade,
            now_ms,
            last_averaging_ms: 0,
            tick_size: 0.01,
            open_orders,
            min_price: 0.15,
            max_price: 0.89,
            cooldown_threshold: 0,
            avg_threshold: 0.98,
            signal_ready: true,
            strategy_params: params,
            bsi: None,
            ofi: None,
            cvd: None,
            market_remaining_secs: rem_secs,
        }
    }

    // Geniş spread senaryosu: UP $0.38/$0.40, DOWN $0.59/$0.61
    // UP_spread=0.02 ✓, DOWN_spread=0.02 ✓ — arb marjının işareti önemli değil.
    fn good_spread<'a>(
        m: &'a StrategyMetrics,
        p: &'a StrategyParams,
        oo: &'a [OpenOrder],
        rem: Option<f64>,
        now_ms: u64,
    ) -> StrategyContext<'a> {
        ctx(m, p, oo, 0.38, 0.40, 0.59, 0.61, rem, now_ms)
    }

    #[test]
    fn idle_no_op_when_spread_too_narrow() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        // arb = 1 - 0.50 - 0.51 = -0.01 < 0.02 → koşul sağlanmıyor.
        let c = ctx(&m, &p, &[], 0.49, 0.50, 0.49, 0.51, Some(200.0), 1000);
        let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
        assert!(matches!(s, ElisState::Idle));
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn places_batch_when_spread_met() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        // UP_spread=0.02 ✓, DOWN_spread=0.02 ✓
        let c = good_spread(&m, &p, &[], Some(200.0), 1000);
        let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
        assert!(matches!(s, ElisState::BatchPending { placed_at_ms: 1000 }));
        match d {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 2);
                let up_ord = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
                let dn_ord = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
                // Bid fiyatından emir (maker limit — spread'i sat).
                assert!((up_ord.price - 0.38).abs() < 1e-9);
                assert!((dn_ord.price - 0.59).abs() < 1e-9);
                // Sıfır pozisyonda her iki taraf eşit = max_buy_order_size.
                assert!((up_ord.size - 20.0).abs() < 1e-9);
                assert!((dn_ord.size - 20.0).abs() < 1e-9);
            }
            other => panic!("PlaceOrders beklendi, gelen {:?}", other),
        }
    }

    #[test]
    fn batch_pending_returns_noop_during_cooldown() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        // placed_at=0, now=2000, cooldown=5000 → henüz dolmadı.
        let c = good_spread(&m, &p, &[], Some(200.0), 2000);
        let state = ElisState::BatchPending { placed_at_ms: 0 };
        let (s, d) = ElisEngine::decide(state, &c);
        assert!(matches!(s, ElisState::BatchPending { .. }));
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn cancels_after_cooldown() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let orders = [
            OpenOrder {
                id: "o1".into(),
                outcome: Outcome::Up,
                side: Side::Buy,
                price: 0.40,
                size: 20.0,
                reason: "elis:dutch:up".into(),
                placed_at_ms: 0,
                size_matched: 0.0,
            },
            OpenOrder {
                id: "o2".into(),
                outcome: Outcome::Down,
                side: Side::Buy,
                price: 0.61,
                size: 20.0,
                reason: "elis:dutch:down".into(),
                placed_at_ms: 0,
                size_matched: 0.0,
            },
        ];
        // placed_at=0, now=5001 → cooldown (5000ms) doldu.
        let c = good_spread(&m, &p, &orders, Some(200.0), 5001);
        let state = ElisState::BatchPending { placed_at_ms: 0 };
        let (s, d) = ElisEngine::decide(state, &c);
        assert!(matches!(s, ElisState::Idle));
        match d {
            Decision::CancelOrders(ids) => {
                assert!(ids.contains(&"o1".to_string()));
                assert!(ids.contains(&"o2".to_string()));
            }
            other => panic!("CancelOrders beklendi, gelen {:?}", other),
        }
    }

    #[test]
    fn stops_before_window_end() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        // Kalan = 50s < stop_before_end_secs (60s) → Done.
        let c = good_spread(&m, &p, &[], Some(50.0), 1000);
        let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
        assert!(matches!(s, ElisState::Done));
        assert!(matches!(d, Decision::NoOp)); // açık emir yok
    }

    #[test]
    fn window_stop_from_batch_pending_cancels() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let orders = [OpenOrder {
            id: "o1".into(),
            outcome: Outcome::Up,
            side: Side::Buy,
            price: 0.40,
            size: 20.0,
            reason: "elis:dutch:up".into(),
            placed_at_ms: 0,
            size_matched: 0.0,
        }];
        // Kalan = 30s < 60s → stop tetikle.
        let c = ctx(&m, &p, &orders, 0.38, 0.40, 0.59, 0.61, Some(30.0), 1000);
        let state = ElisState::BatchPending { placed_at_ms: 0 };
        let (s, d) = ElisEngine::decide(state, &c);
        assert!(matches!(s, ElisState::Done));
        assert!(matches!(d, Decision::CancelOrders(_)));
    }

    #[test]
    fn price_range_filter_blocks_order() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        // UP ask = 0.91 > max_price (0.89) → emir gönderilmez.
        let c = ctx(&m, &p, &[], 0.87, 0.91, 0.06, 0.08, Some(200.0), 1000);
        let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
        assert!(matches!(s, ElisState::Idle));
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn balance_factor_increases_lagging_side() {
        // UP = 54 shares, DOWN = 78 shares → DOWN dominant, UP geride.
        // imbalance = 24, adjustment = round(24 × 0.7 × 0.5) = round(8.4) = 8
        // UP = 20 + 8 = 28, DOWN = 20 - 8 = 12
        let mut m = StrategyMetrics::default();
        m.up_filled = 54.0;
        m.down_filled = 78.0;

        let p = StrategyParams::default();
        let c = good_spread(&m, &p, &[], Some(200.0), 1000);
        let (_, d) = ElisEngine::decide(ElisState::Idle, &c);
        match d {
            Decision::PlaceOrders(orders) => {
                let up_ord = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
                let dn_ord = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
                assert!((up_ord.size - 28.0).abs() < 1e-9, "UP size: {}", up_ord.size);
                assert!((dn_ord.size - 12.0).abs() < 1e-9, "DOWN size: {}", dn_ord.size);
            }
            other => panic!("PlaceOrders beklendi, gelen {:?}", other),
        }
    }

    #[test]
    fn done_state_returns_noop() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let c = good_spread(&m, &p, &[], Some(10.0), 1000);
        let (s, d) = ElisEngine::decide(ElisState::Done, &c);
        assert!(matches!(s, ElisState::Done));
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn only_one_side_narrow_spread_blocks() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        // UP_spread=0.02 ✓ ama DOWN_spread=0.01 < 0.02 ✗ → engellenmeli.
        let c = ctx(&m, &p, &[], 0.38, 0.40, 0.60, 0.61, Some(200.0), 1000);
        let (s, d) = ElisEngine::decide(ElisState::Idle, &c);
        assert!(matches!(s, ElisState::Idle));
        assert!(matches!(d, Decision::NoOp));
    }

    #[test]
    fn cooldown_boundary_exact_triggers_cancel() {
        let m = StrategyMetrics::default();
        let p = StrategyParams::default();
        let params_p = ElisParams::from_strategy_params(&p);
        // Tam cooldown anında (now - placed_at == trade_cooldown_ms).
        let now = params_p.trade_cooldown_ms;
        // arb = 1 - 0.37 - 0.61 = 0.02 ✓ (BatchPending'de spread kontrol edilmez, sadece zamana bakılır)
        let c = ctx(&m, &p, &[], 0.36, 0.37, 0.60, 0.61, Some(200.0), now);
        let state = ElisState::BatchPending { placed_at_ms: 0 };
        let (s, _) = ElisEngine::decide(state, &c);
        assert!(matches!(s, ElisState::Idle), "Tam cooldown anında Idle'a dönmeli");
    }
}
