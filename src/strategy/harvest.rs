//! Harvest strategy — sinyal güdümlü dual açılış + averaging FSM.
//!
//! Durumlar: [Pending] → [OpenDual{deadline}] → [SingleLeg] → [ProfitLock] → [Done]
//!
//! - OpenDual fazı: Binance `effective_score`'a göre simetrik fiyat
//!   (`up_bid + down_bid = 1.00`); ProfitLock burada **tetiklenmez**.
//! - SingleLeg fazı: averaging GTC + ProfitLock (avg_threshold) korunur.
//!
//! Referans: [docs/strategies.md §2](../../../docs/strategies.md).

use serde::{Deserialize, Serialize};

use crate::config::StrategyParams;
use crate::engine::OpenOrder;
use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::{order_size, Decision, PlannedOrder, ZoneSignalMap};
use crate::time::MarketZone;
use crate::types::{OrderType, Outcome, Side};

/// Harvest FSM durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarvestState {
    /// OpenDual henüz gönderilmedi.
    Pending,
    /// İki GTC kitapta; `deadline_ms`'e kadar fill bekleniyor.
    OpenDual { deadline_ms: u64 },
    /// Bir taraf doldu; averaging döngüsünde.
    SingleLeg { filled_side: Outcome },
    /// Kâr kilitlendi — yeni emir yok.
    ProfitLock,
    Done,
}

#[derive(Debug, Clone)]
pub struct HarvestContext<'a> {
    pub params: &'a StrategyParams,
    pub metrics: &'a StrategyMetrics,
    pub yes_token_id: &'a str,
    pub no_token_id: &'a str,
    pub yes_best_bid: f64,
    pub yes_best_ask: f64,
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
    /// En son MATCHED fill fiyatı (averaging kuralı için).
    pub last_fill_price: f64,
    /// Tick boyutu — OpenDual fiyatı snap için.
    pub tick_size: f64,
    /// OpenDual fill bekleme süresi (ms).
    pub dual_timeout: u64,
    /// MarketSession'daki açık emirler — timeout cancel + LIVE notional pos hesabı.
    pub open_orders: &'a [OpenOrder],
    /// SingleLeg ProfitLock eşiği (örn. 0.98).
    pub avg_threshold: f64,
    /// SingleLeg averaging tek tarafta tutulabilir maksimum share.
    pub max_position_size: f64,
    /// Global emir taban fiyatı — strateji içi proaktif clamp.
    pub min_price: f64,
    /// Global emir tavan fiyatı — strateji içi proaktif clamp.
    pub max_price: f64,
    /// Averaging cooldown (ms) — bot config'den gelir; iki rolü vardır:
    /// (1) iki averaging emri arası min süre,
    /// (2) açık averaging GTC max yaş.
    pub cooldown_threshold: u64,
}

impl<'a> HarvestContext<'a> {
    /// `signal_multiplier` (§14.4 harvest tablosu) — averaging size çarpanı.
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

/// Sinyale göre simetrik OpenDual fiyatları — toplam her zaman `1.00`.
///
/// `s = effective_score` ∈ [0, 10], nötr 5; `delta = (s − 5) / 5` ∈ [−1, +1].
/// - `up_bid   = 0.50 + delta · 0.25`  → s=10 ⇒ 0.75, s=0 ⇒ 0.25, s=5 ⇒ 0.50
/// - `down_bid = 0.50 − delta · 0.25`  → s=10 ⇒ 0.25, s=0 ⇒ 0.75, s=5 ⇒ 0.50
/// - `up_bid + down_bid = 1.00` her durumda → dual fazda ProfitLock asla tetiklenmez.
/// - Çıktı `tick_size`'a snap edilir.
pub fn dual_prices(effective_score: f64, tick_size: f64) -> (f64, f64) {
    let snap = |p: f64| (p / tick_size).round() * tick_size;
    let delta = (effective_score - 5.0) / 5.0; // [-1, +1]
    let up_raw = 0.50 + delta * 0.25;
    let down_raw = 0.50 - delta * 0.25;
    (snap(up_raw), snap(down_raw))
}

/// Merkezi FSM fonksiyonu — her olay sonrası çağrılır.
pub fn decide(state: HarvestState, ctx: &HarvestContext) -> (HarvestState, Decision) {
    match state {
        HarvestState::Pending => open_dual(ctx),
        HarvestState::OpenDual { deadline_ms } => evaluate_open_dual(ctx, deadline_ms),
        HarvestState::SingleLeg { filled_side } => single_leg(filled_side, ctx),
        HarvestState::ProfitLock | HarvestState::Done => (HarvestState::Done, Decision::NoOp),
    }
}

fn open_dual(ctx: &HarvestContext) -> (HarvestState, Decision) {
    // Book-ready gate: market quote'u gelmeden emir spam'lamayalım. Fiyat hesabı
    // best_bid'i kullanmasa da, DryRun passive_fill simulator best_ask > 0 ister
    // ve canlı market'te de kitap aktif olmadan emir göndermenin anlamı yok.
    if ctx.yes_best_bid <= 0.0 || ctx.no_best_bid <= 0.0 {
        return (HarvestState::Pending, Decision::NoOp);
    }
    let (up_bid, down_bid) = dual_prices(ctx.effective_score, ctx.tick_size);
    let yes_size = order_size(ctx.order_usdc, up_bid, ctx.api_min_order_size);
    let no_size = order_size(ctx.order_usdc, down_bid, ctx.api_min_order_size);

    let orders = vec![
        PlannedOrder {
            outcome: Outcome::Up,
            token_id: ctx.yes_token_id.to_string(),
            side: Side::Buy,
            price: up_bid,
            size: yes_size,
            order_type: OrderType::Gtc,
            reason: "harvest:open_dual:yes".to_string(),
        },
        PlannedOrder {
            outcome: Outcome::Down,
            token_id: ctx.no_token_id.to_string(),
            side: Side::Buy,
            price: down_bid,
            size: no_size,
            order_type: OrderType::Gtc,
            reason: "harvest:open_dual:no".to_string(),
        },
    ];

    let deadline_ms = ctx.now_ms + ctx.dual_timeout;
    (
        HarvestState::OpenDual { deadline_ms },
        Decision::PlaceOrders(orders),
    )
}

fn evaluate_open_dual(ctx: &HarvestContext, deadline_ms: u64) -> (HarvestState, Decision) {
    let yes_filled = ctx.metrics.shares_yes > 0.0;
    let no_filled = ctx.metrics.shares_no > 0.0;
    let timed_out = ctx.now_ms >= deadline_ms;

    let cancel_open = || -> Decision {
        if ctx.open_orders.is_empty() {
            Decision::NoOp
        } else {
            let ids: Vec<String> = ctx.open_orders.iter().map(|o| o.id.clone()).collect();
            Decision::CancelOrders(ids)
        }
    };

    match (yes_filled, no_filled, timed_out) {
        (true, true, _) => {
            // Sinyal yönüne göre averaging tarafı.
            let side = if ctx.effective_score >= 5.0 {
                Outcome::Up
            } else {
                Outcome::Down
            };
            (HarvestState::SingleLeg { filled_side: side }, cancel_open())
        }
        (true, false, true) => (
            HarvestState::SingleLeg {
                filled_side: Outcome::Up,
            },
            cancel_open(),
        ),
        (false, true, true) => (
            HarvestState::SingleLeg {
                filled_side: Outcome::Down,
            },
            cancel_open(),
        ),
        (false, false, true) => (HarvestState::Pending, cancel_open()),
        _ => (HarvestState::OpenDual { deadline_ms }, Decision::NoOp),
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

    // 1) ProfitLock öncelikli kontrol (averaging fazında korunur).
    if hedge_leg > 0.0 && first_leg + hedge_leg <= ctx.avg_threshold {
        return profit_lock_fak(ctx);
    }

    // 2) StopTrade bölgesinde yeni emir yok.
    if ctx.zone == MarketZone::StopTrade {
        return (HarvestState::SingleLeg { filled_side }, Decision::NoOp);
    }

    // 3) Açık averaging GTC varsa: cooldown_threshold'u geçtiyse cancel; aksi halde bekle.
    let open_avg: Vec<&OpenOrder> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("harvest:averaging") && o.outcome == filled_side)
        .collect();
    if !open_avg.is_empty() {
        let max_age = open_avg
            .iter()
            .map(|o| ctx.now_ms.saturating_sub(o.placed_at_ms))
            .max()
            .unwrap_or(0);
        if max_age >= ctx.cooldown_threshold {
            let cancel_ids: Vec<String> = open_avg.iter().map(|o| o.id.clone()).collect();
            return (
                HarvestState::SingleLeg { filled_side },
                Decision::CancelOrders(cancel_ids),
            );
        }
        return (HarvestState::SingleLeg { filled_side }, Decision::NoOp);
    }

    // 4) Averaging koşulu.
    let first_best_leg = match filled_side {
        Outcome::Up => ctx.yes_best_bid,
        Outcome::Down => ctx.no_best_bid,
    };
    let pos_held = position_held_with_open(ctx, filled_side);

    let cooldown_ok = ctx.now_ms.saturating_sub(ctx.last_averaging_ms) >= ctx.cooldown_threshold;
    let price_fell = ctx.last_fill_price > 0.0 && first_best_leg < ctx.last_fill_price;
    let pos_ok = pos_held < ctx.max_position_size;

    if cooldown_ok && price_fell && pos_ok && first_best_leg > 0.0 {
        // Global price guard: averaging fiyatı [min_price, max_price] dışındaysa atlat.
        if first_best_leg < ctx.min_price || first_best_leg > ctx.max_price {
            return (HarvestState::SingleLeg { filled_side }, Decision::NoOp);
        }
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

/// `pos_held` = filled shares + aynı taraftaki açık BUY emirlerin notional size'ı.
/// `max_position_size` koruması LIVE emirleri de hesaba katmalı (aksi halde
/// kitapta birikmiş averaging GTC'leri sınır kontrolünden kaçar).
fn position_held_with_open(ctx: &HarvestContext, side: Outcome) -> f64 {
    let filled = match side {
        Outcome::Up => ctx.metrics.shares_yes,
        Outcome::Down => ctx.metrics.shares_no,
    };
    let open: f64 = ctx
        .open_orders
        .iter()
        .filter(|o| o.outcome == side && o.side == Side::Buy)
        .map(|o| o.size)
        .sum();
    filled + open
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

    const COOLDOWN_THRESHOLD: u64 = 30_000;

    fn mk_open(id: &str, outcome: Outcome, reason: &str, placed_at_ms: u64, size: f64) -> OpenOrder {
        OpenOrder {
            id: id.to_string(),
            outcome,
            side: Side::Buy,
            price: 0.50,
            size,
            reason: reason.to_string(),
            placed_at_ms,
        }
    }

    fn default_ctx<'a>(
        metrics: &'a StrategyMetrics,
        params: &'a StrategyParams,
        open_orders: &'a [OpenOrder],
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
            tick_size: 0.01,
            dual_timeout: 5_000,
            open_orders,
            avg_threshold: 0.98,
            max_position_size: 100.0,
            min_price: 0.05,
            max_price: 0.95,
            cooldown_threshold: 30_000,
        }
    }

    #[test]
    fn dual_prices_neutral_returns_50_50() {
        // s=5 → up=0.50, down=0.50, toplam 1.00.
        let (up, down) = dual_prices(5.0, 0.01);
        assert!((up - 0.50).abs() < 1e-9);
        assert!((down - 0.50).abs() < 1e-9);
        assert!((up + down - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dual_prices_max_up_signal_returns_75_25() {
        // s=10 → up=0.75, down=0.25, toplam 1.00.
        let (up, down) = dual_prices(10.0, 0.01);
        assert!((up - 0.75).abs() < 1e-9);
        assert!((down - 0.25).abs() < 1e-9);
        assert!((up + down - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dual_prices_max_down_signal_returns_25_75() {
        // s=0 → up=0.25, down=0.75, toplam 1.00.
        let (up, down) = dual_prices(0.0, 0.01);
        assert!((up - 0.25).abs() < 1e-9);
        assert!((down - 0.75).abs() < 1e-9);
        assert!((up + down - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dual_prices_partial_signal_linear() {
        // s=8 → delta=0.6 → up=0.65, down=0.35.
        let (up, down) = dual_prices(8.0, 0.01);
        assert!((up - 0.65).abs() < 1e-9, "up={}", up);
        assert!((down - 0.35).abs() < 1e-9, "down={}", down);
        assert!((up + down - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dual_prices_partial_down_signal_linear() {
        // s=2 → delta=-0.6 → up=0.35, down=0.65.
        let (up, down) = dual_prices(2.0, 0.01);
        assert!((up - 0.35).abs() < 1e-9, "up={}", up);
        assert!((down - 0.65).abs() < 1e-9, "down={}", down);
    }

    #[test]
    fn open_dual_waits_when_book_missing() {
        let metrics = StrategyMetrics::default();
        let params = StrategyParams::default();
        let opens: Vec<OpenOrder> = vec![];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.yes_best_bid = 0.0; // book henüz gelmedi
        let (state, decision) = decide(HarvestState::Pending, &ctx);
        assert_eq!(state, HarvestState::Pending);
        assert!(matches!(decision, Decision::NoOp));
    }

    #[test]
    fn pending_transitions_to_open_dual_with_two_orders() {
        let metrics = StrategyMetrics::default();
        let params = StrategyParams::default();
        let opens: Vec<OpenOrder> = vec![];
        let ctx = default_ctx(&metrics, &params, &opens);
        let (state, decision) = decide(HarvestState::Pending, &ctx);
        match state {
            HarvestState::OpenDual { deadline_ms } => {
                assert_eq!(deadline_ms, ctx.now_ms + ctx.dual_timeout);
            }
            _ => panic!("expected OpenDual{{deadline_ms}}"),
        }
        match decision {
            Decision::PlaceOrders(orders) => assert_eq!(orders.len(), 2),
            _ => panic!("expected PlaceOrders"),
        }
    }

    #[test]
    fn open_dual_high_signal_produces_075_025() {
        // s=10 → up=0.75, down=0.25 (toplam 1.00).
        let metrics = StrategyMetrics::default();
        let params = StrategyParams::default();
        let opens: Vec<OpenOrder> = vec![];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.effective_score = 10.0;
        let (_state, decision) = decide(HarvestState::Pending, &ctx);
        match decision {
            Decision::PlaceOrders(orders) => {
                let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
                let down = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
                assert!((up.price - 0.75).abs() < 1e-9);
                assert!((down.price - 0.25).abs() < 1e-9);
            }
            _ => panic!("expected PlaceOrders"),
        }
    }

    #[test]
    fn open_dual_low_signal_produces_025_075() {
        // s=0 → up=0.25, down=0.75 (toplam 1.00).
        let metrics = StrategyMetrics::default();
        let params = StrategyParams::default();
        let opens: Vec<OpenOrder> = vec![];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.effective_score = 0.0;
        let (_state, decision) = decide(HarvestState::Pending, &ctx);
        match decision {
            Decision::PlaceOrders(orders) => {
                let up = orders.iter().find(|o| o.outcome == Outcome::Up).unwrap();
                let down = orders.iter().find(|o| o.outcome == Outcome::Down).unwrap();
                assert!((up.price - 0.25).abs() < 1e-9);
                assert!((down.price - 0.75).abs() < 1e-9);
            }
            _ => panic!("expected PlaceOrders"),
        }
    }

    #[test]
    fn open_dual_both_filled_transitions_to_single_leg_by_signal() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.55, 10.0, 0.0);
        metrics.ingest_fill(Outcome::Down, 0.50, 10.0, 0.0);
        let params = StrategyParams::default();
        let opens = vec![
            mk_open("o1", Outcome::Up, "harvest:open_dual:yes", 0, 10.0),
            mk_open("o2", Outcome::Down, "harvest:open_dual:no", 0, 10.0),
        ];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.effective_score = 7.0; // sinyal yükseliş → SingleLeg{Up}
        let (state, dec) = decide(
            HarvestState::OpenDual {
                deadline_ms: ctx.now_ms + 1_000,
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
            Decision::CancelOrders(c) => assert_eq!(c.len(), 2),
            _ => panic!("expected CancelOrders"),
        }
    }

    #[test]
    fn open_dual_timeout_no_fill_returns_to_pending() {
        let metrics = StrategyMetrics::default();
        let params = StrategyParams::default();
        let opens = vec![
            mk_open("o1", Outcome::Up, "harvest:open_dual:yes", 0, 10.0),
            mk_open("o2", Outcome::Down, "harvest:open_dual:no", 0, 10.0),
        ];
        let ctx = default_ctx(&metrics, &params, &opens);
        let (state, dec) = decide(
            HarvestState::OpenDual {
                deadline_ms: ctx.now_ms.saturating_sub(1),
            },
            &ctx,
        );
        assert_eq!(state, HarvestState::Pending);
        match dec {
            Decision::CancelOrders(c) => assert_eq!(c.len(), 2),
            _ => panic!("expected CancelOrders"),
        }
    }

    #[test]
    fn open_dual_timeout_one_fill_cancels_other_to_single_leg() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
        let params = StrategyParams::default();
        let opens = vec![mk_open(
            "no_open",
            Outcome::Down,
            "harvest:open_dual:no",
            0,
            10.0,
        )];
        let ctx = default_ctx(&metrics, &params, &opens);
        let (state, dec) = decide(
            HarvestState::OpenDual {
                deadline_ms: ctx.now_ms.saturating_sub(1),
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
            Decision::CancelOrders(c) => assert_eq!(c, vec!["no_open".to_string()]),
            _ => panic!("expected CancelOrders"),
        }
    }

    #[test]
    fn single_leg_profit_lock_triggered_when_sum_under_threshold() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.48, 10.0, 0.0);
        let params = StrategyParams::default();
        let opens: Vec<OpenOrder> = vec![];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.no_best_ask = 0.49; // first_leg(0.48) + hedge_leg(0.49) = 0.97 ≤ 0.98
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
        let opens: Vec<OpenOrder> = vec![];
        let mut ctx = default_ctx(&metrics, &params, &opens);
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
        let opens: Vec<OpenOrder> = vec![];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.last_fill_price = 0.50;
        ctx.yes_best_bid = 0.48; // düştü
        ctx.no_best_ask = 0.55; // ProfitLock tetiklemez
        ctx.now_ms = COOLDOWN_THRESHOLD + 1; // cooldown bitti
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

    #[test]
    fn single_leg_skips_averaging_while_open_avg_within_cooldown() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
        let params = StrategyParams::default();
        let now = COOLDOWN_THRESHOLD + 10_000;
        let opens = vec![mk_open(
            "avg1",
            Outcome::Up,
            "harvest:averaging:Up",
            now - 5_000, // 5s yaş < 30s
            10.0,
        )];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.now_ms = now;
        ctx.last_fill_price = 0.50;
        ctx.yes_best_bid = 0.48;
        ctx.no_best_ask = 0.55;
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
        assert!(matches!(dec, Decision::NoOp));
    }

    #[test]
    fn single_leg_cancels_open_avg_after_cooldown_threshold() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
        let params = StrategyParams::default();
        let now = COOLDOWN_THRESHOLD * 3;
        let opens = vec![mk_open(
            "avg1",
            Outcome::Up,
            "harvest:averaging:Up",
            now - COOLDOWN_THRESHOLD - 1_000, // > 30s
            10.0,
        )];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.now_ms = now;
        ctx.last_fill_price = 0.50;
        ctx.yes_best_bid = 0.48;
        ctx.no_best_ask = 0.55;
        let (_state, dec) = decide(
            HarvestState::SingleLeg {
                filled_side: Outcome::Up,
            },
            &ctx,
        );
        match dec {
            Decision::CancelOrders(ids) => assert_eq!(ids, vec!["avg1".to_string()]),
            _ => panic!("expected CancelOrders for stale averaging"),
        }
    }

    #[test]
    fn single_leg_emits_new_averaging_after_cancel_in_next_tick() {
        // Önce cancel sonrası kitap boş — koşullar uygunsa yeni averaging.
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
        let params = StrategyParams::default();
        let opens: Vec<OpenOrder> = vec![];
        let mut ctx = default_ctx(&metrics, &params, &opens);
        ctx.now_ms = COOLDOWN_THRESHOLD * 3;
        ctx.last_averaging_ms = ctx.now_ms - COOLDOWN_THRESHOLD - 1; // cooldown_ok
        ctx.last_fill_price = 0.50;
        ctx.yes_best_bid = 0.48;
        ctx.no_best_ask = 0.55;
        let (_state, dec) = decide(
            HarvestState::SingleLeg {
                filled_side: Outcome::Up,
            },
            &ctx,
        );
        match dec {
            Decision::PlaceOrders(orders) => {
                assert_eq!(orders.len(), 1);
                assert_eq!(orders[0].outcome, Outcome::Up);
                assert_eq!(orders[0].order_type, OrderType::Gtc);
            }
            _ => panic!("expected new averaging GTC"),
        }
    }

    #[test]
    fn pos_held_includes_open_averaging_size() {
        let mut metrics = StrategyMetrics::default();
        metrics.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
        let params = StrategyParams::default();
        let opens = vec![mk_open(
            "avg1",
            Outcome::Up,
            "harvest:averaging:Up",
            0,
            7.0,
        )];
        let ctx = default_ctx(&metrics, &params, &opens);
        let pos = position_held_with_open(&ctx, Outcome::Up);
        assert!((pos - 17.0).abs() < 1e-9);
    }
}
