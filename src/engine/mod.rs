//! MarketSession + decision loop + DryRun simulator.
//!
//! Alt modüller:
//! - [`executor`] — `Executor` enum (DryRun + Live), `execute()` batch yürütücü.
//! - [`passive`]  — DryRun passive-fill simülatörü.
//!
//! Referans: [docs/bot-platform-mimari.md §13 §16 §⚡ Kural 1-2](../../../docs/bot-platform-mimari.md).

use crate::config::BotConfig;
use crate::strategy::harvest::{
    HarvestContext, HarvestEngine, HarvestState, MAX_POSITION_SIZE,
};
use crate::strategy::metrics::{MarketPnL, StrategyMetrics};
use crate::strategy::{Decision, DecisionEngine, OpenOrder, PlannedOrder};
use crate::time::{zone_pct, MarketZone};
use crate::types::{Outcome, RunMode, Strategy};

pub mod executor;
pub mod passive;

pub use executor::{
    execute, ExecuteOutput, Executor, LiveExecutor, OrderSink, Simulator, DRYRUN_FEE_RATE,
};
pub use passive::simulate_passive_fills;

/// Yürütülen emir sonucu. In-memory engine pipeline'ında kullanılır;
/// JSON serialize/deserialize edilmez (DB persist sub-field'lar üzerinden yapılır).
#[derive(Debug, Clone)]
pub struct ExecutedOrder {
    pub order_id: String,
    pub planned: PlannedOrder,
    pub filled: bool,
    pub fill_price: Option<f64>,
    pub fill_size: Option<f64>,
}

/// Market seansı — bir bot × bir pencere (slug).
#[derive(Debug, Clone)]
pub struct MarketSession {
    pub bot_id: i64,
    pub slug: String,
    /// `market_sessions.id` — DB persist için (orders/trades/pnl FK).
    pub market_session_id: i64,
    pub condition_id: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub tick_size: f64,
    pub api_min_order_size: f64,
    /// NegRisk Exchange'e mi yoksa standart Exchange'e mi yazılacak
    /// (EIP-712 verifying_contract için belirleyici).
    pub neg_risk: bool,
    pub start_ts: u64,
    pub end_ts: u64,

    // Strateji durumu
    pub strategy: Strategy,
    pub harvest_state: HarvestState,
    pub metrics: StrategyMetrics,
    pub last_averaging_ms: u64,
    pub last_fill_price: f64,

    // Anlık best_bid_ask
    pub yes_best_bid: f64,
    pub yes_best_ask: f64,
    pub no_best_bid: f64,
    pub no_best_ask: f64,

    pub run_mode: RunMode,
    pub open_orders: Vec<OpenOrder>,

    /// Global emir taban fiyatı (bot config) — engine guard.
    pub min_price: f64,
    /// Global emir tavan fiyatı (bot config) — engine guard.
    pub max_price: f64,
    /// Averaging cooldown (ms) — strateji ctx'lerine geçirilir (bot config).
    pub cooldown_threshold: u64,
    /// Bir kez `📚 Market book ready` logu basıldı mı?
    pub book_ready_logged: bool,
}

impl MarketSession {
    pub fn new(bot_id: i64, slug: String, cfg: &BotConfig) -> Self {
        Self {
            bot_id,
            slug,
            market_session_id: 0,
            condition_id: String::new(),
            yes_token_id: String::new(),
            no_token_id: String::new(),
            tick_size: 0.01,
            api_min_order_size: 5.0,
            neg_risk: false,
            start_ts: 0,
            end_ts: 0,
            strategy: cfg.strategy,
            harvest_state: HarvestState::Pending,
            metrics: StrategyMetrics::default(),
            last_averaging_ms: 0,
            last_fill_price: 0.0,
            // 0.0 = "henüz quote yok" sentineli; Simulator bu durumda emri live tutar.
            yes_best_bid: 0.0,
            yes_best_ask: 0.0,
            no_best_bid: 0.0,
            no_best_ask: 0.0,
            run_mode: cfg.run_mode,
            open_orders: Vec::new(),
            min_price: cfg.min_price,
            max_price: cfg.max_price,
            cooldown_threshold: cfg.cooldown_threshold,
            book_ready_logged: false,
        }
    }

    /// Güncel market bölgesi (`zone_pct` eşikleri).
    pub fn current_zone(&self, now_secs: u64) -> MarketZone {
        MarketZone::from_pct(zone_pct(self.start_ts, self.end_ts, now_secs))
    }

    /// MTM PnL anında hesaplanır (§17).
    pub fn pnl(&self) -> MarketPnL {
        MarketPnL::from_metrics(&self.metrics, self.yes_best_bid, self.no_best_bid)
    }

    /// Strateji'ye karar ver — tek döngü tick.
    ///
    /// `effective_score` Binance sinyalinden türetilir (`5.0` = nötr).
    pub fn tick(&mut self, cfg: &BotConfig, now_ms_v: u64, effective_score: f64) -> Decision {
        match cfg.strategy {
            Strategy::Harvest => {
                let avg_threshold = cfg.strategy_params.harvest_avg_threshold();
                let dual_timeout = cfg.strategy_params.harvest_dual_timeout();
                let zone = self.current_zone(now_ms_v / 1000);

                let ctx = HarvestContext {
                    params: &cfg.strategy_params,
                    metrics: &self.metrics,
                    yes_token_id: &self.yes_token_id,
                    no_token_id: &self.no_token_id,
                    yes_best_bid: self.yes_best_bid,
                    yes_best_ask: self.yes_best_ask,
                    no_best_bid: self.no_best_bid,
                    no_best_ask: self.no_best_ask,
                    api_min_order_size: self.api_min_order_size,
                    order_usdc: cfg.order_usdc,
                    signal_weight: cfg.signal_weight,
                    effective_score,
                    zone,
                    now_ms: now_ms_v,
                    last_averaging_ms: self.last_averaging_ms,
                    last_fill_price: self.last_fill_price,
                    tick_size: self.tick_size,
                    dual_timeout,
                    open_orders: &self.open_orders,
                    avg_threshold,
                    max_position_size: MAX_POSITION_SIZE,
                    min_price: self.min_price,
                    max_price: self.max_price,
                    cooldown_threshold: self.cooldown_threshold,
                };
                let (new_state, decision) =
                    <HarvestEngine as DecisionEngine>::decide(self.harvest_state, &ctx);
                self.harvest_state = new_state;
                if matches!(zone, MarketZone::StopTrade) {
                    filter_stop_trade(decision)
                } else {
                    decision
                }
            }
            // bot/ctx.rs aktif olmayan stratejileri start anında reddeder; defansif NoOp.
            _ => Decision::NoOp,
        }
    }
}

/// StopTrade kuralı: yeni emir üretilmez; yalnız cancel/no-op pass eder.
fn filter_stop_trade(d: Decision) -> Decision {
    match d {
        Decision::NoOp | Decision::Complete => d,
        Decision::PlaceOrders(_) => Decision::NoOp,
        Decision::CancelOrders(ids) => Decision::CancelOrders(ids),
        Decision::Batch { cancel, .. } => {
            if cancel.is_empty() {
                Decision::NoOp
            } else {
                Decision::CancelOrders(cancel)
            }
        }
    }
}

/// User WS `trade MATCHED` event'inden gelen fill'i session'a absorbla.
pub fn absorb_trade_matched(
    session: &mut MarketSession,
    outcome: Outcome,
    price: f64,
    size: f64,
    fee: f64,
) {
    use crate::time::now_ms;
    session.metrics.ingest_fill(outcome, price, size, fee);
    session.last_fill_price = price;
    session.last_averaging_ms = now_ms();
}

/// Bir WS event'in ardından `best_bid_ask`'ı güncelle.
pub fn update_best(session: &mut MarketSession, asset_id: &str, best_bid: f64, best_ask: f64) {
    if asset_id == session.yes_token_id {
        session.yes_best_bid = best_bid;
        session.yes_best_ask = best_ask;
    } else if asset_id == session.no_token_id {
        session.no_best_bid = best_bid;
        session.no_best_ask = best_ask;
    }
}

/// Outcome çıkarım yardımcısı — asset_id ↔ Outcome.
pub fn outcome_from_asset_id(session: &MarketSession, asset_id: &str) -> Option<Outcome> {
    if asset_id == session.yes_token_id {
        Some(Outcome::Up)
    } else if asset_id == session.no_token_id {
        Some(Outcome::Down)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StrategyParams;
    use crate::time::now_ms;

    fn test_cfg(run_mode: RunMode) -> BotConfig {
        BotConfig {
            id: 1,
            name: "test".into(),
            slug_pattern: "btc-updown-5m-1776420900".into(),
            strategy: Strategy::Harvest,
            run_mode,
            order_usdc: 5.0,
            signal_weight: 0.0,
            min_price: 0.05,
            max_price: 0.95,
            cooldown_threshold: 30_000,
            start_offset: 0,
            strategy_params: StrategyParams::default(),
        }
    }

    #[tokio::test]
    async fn dryrun_open_dual_creates_two_filled_orders() {
        let cfg = test_cfg(RunMode::Dryrun);
        let mut sess = MarketSession::new(1, "btc-updown-5m-1776420900".into(), &cfg);
        sess.yes_token_id = "yes".into();
        sess.no_token_id = "no".into();
        sess.tick_size = 0.01;
        sess.api_min_order_size = 5.0;
        sess.start_ts = now_ms() / 1000;
        sess.end_ts = sess.start_ts + 300;
        sess.yes_best_bid = 0.50;
        sess.yes_best_ask = 0.50;
        sess.no_best_bid = 0.48;
        sess.no_best_ask = 0.48;

        let dec = sess.tick(&cfg, now_ms(), 5.0);
        let exec = Executor::DryRun(Simulator);
        let filled = execute(&mut sess, &exec, dec).await.unwrap();
        assert_eq!(filled.placed.len(), 2);
        assert!(filled.placed.iter().all(|e| e.filled));
        assert!(sess.metrics.shares_yes > 0.0);
        assert!(sess.metrics.shares_no > 0.0);
    }
}
