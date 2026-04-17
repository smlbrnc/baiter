//! Harvest strategy — delta-nötr arbitraj FSM.
//!
//! Durumlar: [Başlangıç] → [OpenDual] → [SingleLeg] → [ProfitLock] → [Bitti]
//!
//! Referans: [docs/strategies.md §2](../../../docs/strategies.md).

use serde::{Deserialize, Serialize};

use crate::config::StrategyParams;
use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::{order_size, Decision, PlannedOrder, ZoneSignalMap};
use crate::time::MarketZone;
use crate::types::{OrderType, Outcome, Side};

/// Harvest FSM durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarvestState {
    /// OpenDual henüz gönderilmedi.
    Pending,
    /// Hem YES hem NO GTC emirleri açık.
    OpenDualOpen,
    /// Yalnız bir taraf doldu, averaging döngüsünde.
    SingleLeg {
        filled_side: Outcome,
    },
    /// Kâr kilitlendi — yeni emir yok.
    ProfitLock,
    Done,
}

#[derive(Debug, Clone)]
pub struct HarvestContext<'a> {
    pub params: &'a StrategyParams,
    pub metrics: &'a StrategyMetrics,
    /// YES asset id.
    pub yes_token_id: &'a str,
    pub no_token_id: &'a str,
    /// YES best bid/ask (WS'ten).
    pub yes_best_bid: f64,
    pub yes_best_ask: f64,
    /// NO best bid/ask.
    pub no_best_bid: f64,
    pub no_best_ask: f64,
    pub api_min_order_size: f64,
    pub order_usdc: f64,
    pub signal_weight: f64,
    pub effective_score: f64,
    pub zone: MarketZone,
    pub now_ms: u64,
    /// Son averaging turu zamanı (ms); ilk turda 0.
    pub last_averaging_ms: u64,
    /// En son MATCHED fill fiyatı (her iki taraf için tek değer — averaging kuralı).
    pub last_fill_price: f64,
    /// OpenDual YES bid.
    pub up_bid: f64,
    /// OpenDual NO bid.
    pub down_bid: f64,
    pub avg_threshold: f64,
    pub cooldown_ms: u64,
    pub max_position_size: f64,
}

impl<'a> HarvestContext<'a> {
    /// `signal_multiplier` (§14.4 harvest tablosu).
    fn signal_multiplier(&self, averaging_side: Outcome) -> f64 {
        if !ZoneSignalMap::HARVEST.is_active(self.zone) || self.signal_weight <= 0.0 {
            return 1.0;
        }
        let s = self.effective_score;
        match averaging_side {
            Outcome::Up => {
                if s >= 8.0 {
                    1.0
                } else if s >= 6.0 {
                    0.9
                } else if s >= 4.0 {
                    1.0
                } else if s >= 2.0 {
                    1.1
                } else {
                    1.2
                }
            }
            Outcome::Down => {
                if s >= 8.0 {
                    1.3
                } else if s >= 6.0 {
                    1.1
                } else if s >= 4.0 {
                    1.0
                } else if s >= 2.0 {
                    0.9
                } else {
                    0.7
                }
            }
        }
    }
}

/// Merkezi FSM fonksiyonu — her olay sonrası çağrılır.
/// Stateless: state parametresi okunur ve güncel state döner.
pub fn decide(state: HarvestState, ctx: &HarvestContext) -> (HarvestState, Decision) {
    match state {
        HarvestState::Pending => open_dual(ctx),
        HarvestState::OpenDualOpen => evaluate_open_dual(ctx),
        HarvestState::SingleLeg { filled_side } => single_leg(filled_side, ctx),
        HarvestState::ProfitLock | HarvestState::Done => (HarvestState::Done, Decision::NoOp),
    }
}

fn open_dual(ctx: &HarvestContext) -> (HarvestState, Decision) {
    let yes_size = order_size(ctx.order_usdc, ctx.up_bid, ctx.api_min_order_size);
    let no_size = order_size(ctx.order_usdc, ctx.down_bid, ctx.api_min_order_size);

    let orders = vec![
        PlannedOrder {
            outcome: Outcome::Up,
            token_id: ctx.yes_token_id.to_string(),
            side: Side::Buy,
            price: ctx.up_bid,
            size: yes_size,
            order_type: OrderType::Gtc,
            reason: "harvest:open_dual:yes".to_string(),
        },
        PlannedOrder {
            outcome: Outcome::Down,
            token_id: ctx.no_token_id.to_string(),
            side: Side::Buy,
            price: ctx.down_bid,
            size: no_size,
            order_type: OrderType::Gtc,
            reason: "harvest:open_dual:no".to_string(),
        },
    ];

    (HarvestState::OpenDualOpen, Decision::PlaceOrders(orders))
}

fn evaluate_open_dual(ctx: &HarvestContext) -> (HarvestState, Decision) {
    let yes_filled = ctx.metrics.shares_yes > 0.0;
    let no_filled = ctx.metrics.shares_no > 0.0;

    match (yes_filled, no_filled) {
        (true, true) => {
            // Her iki taraf da dolmuş → avg_sum threshold kontrolü
            if ctx.metrics.avg_sum <= ctx.avg_threshold {
                // imbalance = 0 ise direkt ProfitLock; değilse FAK üret
                if ctx.metrics.imbalance.abs() < f64::EPSILON {
                    (HarvestState::ProfitLock, Decision::NoOp)
                } else {
                    profit_lock_fak(ctx)
                }
            } else {
                // Threshold sağlanmadı — imbalance kadar SingleLeg dolu sayılır
                let side = if ctx.metrics.imbalance >= 0.0 {
                    Outcome::Up
                } else {
                    Outcome::Down
                };
                (
                    HarvestState::SingleLeg { filled_side: side },
                    Decision::NoOp,
                )
            }
        }
        (true, false) => single_leg(Outcome::Up, ctx),
        (false, true) => single_leg(Outcome::Down, ctx),
        (false, false) => (HarvestState::OpenDualOpen, Decision::NoOp),
    }
}

fn single_leg(filled_side: Outcome, ctx: &HarvestContext) -> (HarvestState, Decision) {
    let first_leg = match filled_side {
        Outcome::Up => ctx.metrics.avg_yes,
        Outcome::Down => ctx.metrics.avg_no,
    };
    let hedge_leg = match filled_side {
        Outcome::Up => ctx.no_best_ask,
        Outcome::Down => ctx.yes_best_ask,
    };

    // 1) ProfitLock öncelikli kontrol
    if hedge_leg > 0.0 && first_leg + hedge_leg <= ctx.avg_threshold {
        return profit_lock_fak(ctx);
    }

    // 2) StopTrade bölgesinde yeni emir yok
    if ctx.zone == MarketZone::StopTrade {
        return (HarvestState::SingleLeg { filled_side }, Decision::NoOp);
    }

    // 3) Averaging koşulu
    let first_best_leg = match filled_side {
        Outcome::Up => ctx.yes_best_bid,
        Outcome::Down => ctx.no_best_bid,
    };
    let pos_held = match filled_side {
        Outcome::Up => ctx.metrics.shares_yes,
        Outcome::Down => ctx.metrics.shares_no,
    };

    let cooldown_ok = ctx.now_ms.saturating_sub(ctx.last_averaging_ms) >= ctx.cooldown_ms;
    let price_fell = ctx.last_fill_price > 0.0 && first_best_leg < ctx.last_fill_price;
    let pos_ok = pos_held < ctx.max_position_size;

    if cooldown_ok && price_fell && pos_ok && first_best_leg > 0.0 {
        let base = order_size(ctx.order_usdc, first_best_leg, ctx.api_min_order_size);
        let mult = ctx.signal_multiplier(filled_side);
        let effective = (base * mult).round().max(ctx.api_min_order_size);

        let token_id = match filled_side {
            Outcome::Up => ctx.yes_token_id,
            Outcome::Down => ctx.no_token_id,
        };

        let order = PlannedOrder {
            outcome: filled_side,
            token_id: token_id.to_string(),
            side: Side::Buy,
            price: first_best_leg,
            size: effective,
            order_type: OrderType::Gtc,
            reason: format!("harvest:averaging:{:?}", filled_side),
        };
        return (
            HarvestState::SingleLeg { filled_side },
            Decision::PlaceOrders(vec![order]),
        );
    }

    (HarvestState::SingleLeg { filled_side }, Decision::NoOp)
}

fn profit_lock_fak(ctx: &HarvestContext) -> (HarvestState, Decision) {
    let imb = ctx.metrics.imbalance;
    if imb.abs() < f64::EPSILON {
        return (HarvestState::ProfitLock, Decision::NoOp);
    }
    let (hedge_side, token_id, price, size) = if imb > 0.0 {
        // YES fazla → NO tarafına FAK
        (Outcome::Down, ctx.no_token_id, ctx.no_best_ask, imb.abs())
    } else {
        (Outcome::Up, ctx.yes_token_id, ctx.yes_best_ask, imb.abs())
    };
    let fak = PlannedOrder {
        outcome: hedge_side,
        token_id: token_id.to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Fak,
        reason: "harvest:profit_lock:fak".to_string(),
    };
    (HarvestState::ProfitLock, Decision::PlaceOrders(vec![fak]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ctx<'a>(
        metrics: &'a StrategyMetrics,
        params: &'a StrategyParams,
    ) -> HarvestContext<'a> {
        HarvestContext {
            params,
            metrics,
            yes_token_id: "yes",
            no_token_id: "no",
            yes_best_bid: 0.50,
            yes_best_ask: 0.52,
            no_best_bid: 0.46,
            no_best_ask: 0.48,
            api_min_order_size: 5.0,
            order_usdc: 5.0,
            signal_weight: 0.0,
            effective_score: 5.0,
            zone: MarketZone::NormalTrade,
            now_ms: 1_000_000,
            last_averaging_ms: 0,
            last_fill_price: 0.0,
            up_bid: 0.50,
            down_bid: 0.48,
            avg_threshold: 0.98,
            cooldown_ms: 30_000,
            max_position_size: 100.0,
        }
    }

    #[test]
    fn pending_transitions_to_open_dual_with_two_orders() {
        let metrics = StrategyMetrics::default();
        let params = StrategyParams::default();
        let ctx = default_ctx(&metrics, &params);
        let (state, decision) = decide(HarvestState::Pending, &ctx);
        assert_eq!(state, HarvestState::OpenDualOpen);
        match decision {
            Decision::PlaceOrders(orders) => assert_eq!(orders.len(), 2),
            _ => panic!("expected PlaceOrders"),
        }
    }

    #[test]
    fn open_dual_filled_both_under_threshold_triggers_profit_lock() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.49, 10.0, 0.0);
        metrics.ingest_fill(Outcome::Down, 0.48, 10.0, 0.0);
        // avg_sum = 0.97 <= 0.98 ve imbalance = 0 → ProfitLock + NoOp
        let params = StrategyParams::default();
        let ctx = default_ctx(&metrics, &params);
        let (state, dec) = decide(HarvestState::OpenDualOpen, &ctx);
        assert_eq!(state, HarvestState::ProfitLock);
        matches!(dec, Decision::NoOp);
    }

    #[test]
    fn open_dual_over_threshold_goes_to_single_leg() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
        metrics.ingest_fill(Outcome::Down, 0.50, 5.0, 0.0);
        // avg_sum = 1.05 > 0.98; imbalance = 5 → filled_side = UP
        let params = StrategyParams::default();
        let ctx = default_ctx(&metrics, &params);
        let (state, _) = decide(HarvestState::OpenDualOpen, &ctx);
        assert_eq!(
            state,
            HarvestState::SingleLeg {
                filled_side: Outcome::Up
            }
        );
    }

    #[test]
    fn single_leg_profit_lock_triggered_when_sum_under_threshold() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.48, 10.0, 0.0);
        let params = StrategyParams::default();
        let mut ctx = default_ctx(&metrics, &params);
        ctx.no_best_ask = 0.49; // first_leg(0.48) + hedge_leg(0.49) = 0.97 <= 0.98
        let (state, dec) = decide(
            HarvestState::SingleLeg {
                filled_side: Outcome::Up,
            },
            &ctx,
        );
        assert_eq!(state, HarvestState::ProfitLock);
        match dec {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 1);
                assert_eq!(orders[0].order_type, OrderType::Fak);
                assert_eq!(orders[0].outcome, Outcome::Down);
            }
            _ => panic!("expected FAK order"),
        }
    }

    #[test]
    fn stop_trade_zone_blocks_averaging() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.48, 10.0, 0.0);
        let params = StrategyParams::default();
        let mut ctx = default_ctx(&metrics, &params);
        ctx.zone = MarketZone::StopTrade;
        ctx.no_best_ask = 0.80; // ProfitLock tetiklenmez
        let (state, dec) = decide(
            HarvestState::SingleLeg {
                filled_side: Outcome::Up,
            },
            &ctx,
        );
        assert_eq!(
            state,
            HarvestState::SingleLeg {
                filled_side: Outcome::Up
            }
        );
        matches!(dec, Decision::NoOp);
    }

    #[test]
    fn averaging_when_price_falls_and_cooldown_passed() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
        let params = StrategyParams::default();
        let mut ctx = default_ctx(&metrics, &params);
        ctx.last_fill_price = 0.50;
        ctx.yes_best_bid = 0.48; // düştü
        ctx.no_best_ask = 0.55; // ProfitLock tetiklemez (0.5 + 0.55 = 1.05 > 0.98)
        let (state, dec) = decide(
            HarvestState::SingleLeg {
                filled_side: Outcome::Up,
            },
            &ctx,
        );
        assert_eq!(
            state,
            HarvestState::SingleLeg {
                filled_side: Outcome::Up
            }
        );
        match dec {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 1);
                assert_eq!(orders[0].order_type, OrderType::Gtc);
                assert_eq!(orders[0].outcome, Outcome::Up);
            }
            _ => panic!("expected averaging GTC"),
        }
    }
}
