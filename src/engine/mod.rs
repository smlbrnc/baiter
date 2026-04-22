//! MarketSession + decision loop + DryRun simulator.

use crate::config::BotConfig;
use crate::strategy::alis::AlisEngine;
use crate::strategy::aras::ArasEngine;
use crate::strategy::elis::ElisEngine;
use crate::strategy::metrics::{MarketPnL, StrategyMetrics};
use crate::strategy::{Decision, OpenOrder, PlannedOrder, StrategyContext, StrategyState};
use crate::time::{zone_pct, MarketZone};
use crate::types::{Outcome, Side};

pub mod executor;
pub mod passive;

pub use executor::{
    execute, ExecuteOutput, Executor, LiveExecutor, Simulator, DRYRUN_FEE_RATE,
};
pub use passive::simulate_passive_fills;

/// Yürütülen emir sonucu — fill olmamışsa `fill_price/size` planned değerlerini taşır.
#[derive(Debug)]
pub struct ExecutedOrder {
    pub order_id: String,
    pub planned: PlannedOrder,
    pub filled: bool,
    pub fill_price: f64,
    pub fill_size: f64,
}

/// Market seansı — bir bot × bir pencere (slug).
#[derive(Debug)]
pub struct MarketSession {
    pub bot_id: i64,
    pub slug: String,
    pub market_session_id: i64,
    pub condition_id: String,
    pub up_token_id: String,
    pub down_token_id: String,
    pub tick_size: f64,
    pub api_min_order_size: f64,
    pub neg_risk: bool,
    pub start_ts: u64,
    pub end_ts: u64,

    pub state: StrategyState,
    pub metrics: StrategyMetrics,
    pub last_averaging_ms: u64,

    pub up_best_bid: f64,
    pub up_best_ask: f64,
    pub down_best_bid: f64,
    pub down_best_ask: f64,

    pub open_orders: Vec<OpenOrder>,

    pub min_price: f64,
    pub max_price: f64,
    pub cooldown_threshold: u64,
    pub fee_rate_bps: u32,
    pub book_ready_logged: bool,
}

impl MarketSession {
    pub fn new(bot_id: i64, slug: String, cfg: &BotConfig) -> Self {
        Self {
            bot_id,
            slug,
            market_session_id: 0,
            condition_id: String::new(),
            up_token_id: String::new(),
            down_token_id: String::new(),
            tick_size: 0.01,
            api_min_order_size: 5.0,
            neg_risk: false,
            start_ts: 0,
            end_ts: 0,
            state: StrategyState::pending_for(cfg.strategy),
            metrics: StrategyMetrics::default(),
            last_averaging_ms: 0,
            up_best_bid: 0.0,
            up_best_ask: 0.0,
            down_best_bid: 0.0,
            down_best_ask: 0.0,
            open_orders: Vec::new(),
            min_price: cfg.min_price,
            max_price: cfg.max_price,
            cooldown_threshold: cfg.cooldown_threshold,
            fee_rate_bps: 0,
            book_ready_logged: false,
        }
    }

    fn current_zone(&self, now_secs: u64) -> MarketZone {
        MarketZone::from_pct(zone_pct(self.start_ts, self.end_ts, now_secs))
    }

    pub fn pnl(&self) -> MarketPnL {
        MarketPnL::from_metrics(&self.metrics, self.up_best_bid, self.down_best_bid)
    }

    /// Tek tick — aktif stratejiye karar ver. `effective_score`: composite skor (5.0 = nötr).
    pub fn tick(
        &mut self,
        cfg: &BotConfig,
        now_ms_v: u64,
        effective_score: f64,
        signal_ready: bool,
    ) -> Decision {
        let zone = self.current_zone(now_ms_v / 1000);
        let ctx = StrategyContext {
            metrics: &self.metrics,
            up_token_id: &self.up_token_id,
            down_token_id: &self.down_token_id,
            up_best_bid: self.up_best_bid,
            up_best_ask: self.up_best_ask,
            down_best_bid: self.down_best_bid,
            down_best_ask: self.down_best_ask,
            api_min_order_size: self.api_min_order_size,
            order_usdc: cfg.order_usdc,
            effective_score,
            zone,
            now_ms: now_ms_v,
            last_averaging_ms: self.last_averaging_ms,
            tick_size: self.tick_size,
            open_orders: &self.open_orders,
            min_price: self.min_price,
            max_price: self.max_price,
            cooldown_threshold: self.cooldown_threshold,
            avg_threshold: cfg.strategy_params.avg_threshold(),
            signal_ready,
        };
        let (next_state, decision) = match self.state {
            StrategyState::Alis(s) => {
                let (ns, d) = AlisEngine::decide(s, &ctx);
                (StrategyState::Alis(ns), d)
            }
            StrategyState::Elis(s) => {
                let (ns, d) = ElisEngine::decide(s, &ctx);
                (StrategyState::Elis(ns), d)
            }
            StrategyState::Aras(s) => {
                let (ns, d) = ArasEngine::decide(s, &ctx);
                (StrategyState::Aras(ns), d)
            }
        };
        self.state = next_state;
        decision
    }
}

/// User WS `trade MATCHED` fill'ini metrics'e yansıt + son fill zamanını
/// `last_averaging_ms`'e damgala. Strateji bu damgayı cooldown referansı olarak
/// okur (`now_ms - last_averaging_ms ≥ cooldown_threshold`).
pub fn absorb_trade_matched(
    session: &mut MarketSession,
    outcome: Outcome,
    side: Side,
    price: f64,
    size: f64,
    fee: f64,
) {
    use crate::time::now_ms;
    session.metrics.ingest_fill(outcome, side, price, size, fee);
    session.last_averaging_ms = now_ms();
}

pub fn update_best(session: &mut MarketSession, asset_id: &str, best_bid: f64, best_ask: f64) {
    if asset_id == session.up_token_id {
        session.up_best_bid = best_bid;
        session.up_best_ask = best_ask;
    } else if asset_id == session.down_token_id {
        session.down_best_bid = best_bid;
        session.down_best_ask = best_ask;
    }
}

pub fn outcome_from_asset_id(session: &MarketSession, asset_id: &str) -> Option<Outcome> {
    if asset_id == session.up_token_id {
        Some(Outcome::Up)
    } else if asset_id == session.down_token_id {
        Some(Outcome::Down)
    } else {
        None
    }
}
