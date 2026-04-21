//! MarketSession + decision loop + DryRun simulator.

use crate::config::BotConfig;
use crate::strategy::harvest::{HarvestContext, HarvestEngine, HarvestState};
use crate::strategy::metrics::{MarketPnL, StrategyMetrics};
use crate::strategy::{Decision, DecisionEngine, OpenOrder, PlannedOrder};
use crate::time::{zone_pct, MarketZone};
use crate::types::{Outcome, Side, Strategy};

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
    pub yes_token_id: String,
    pub no_token_id: String,
    pub tick_size: f64,
    pub api_min_order_size: f64,
    pub neg_risk: bool,
    pub start_ts: u64,
    pub end_ts: u64,

    pub strategy: Strategy,
    pub harvest_state: HarvestState,
    pub metrics: StrategyMetrics,
    pub last_averaging_ms: u64,

    pub yes_best_bid: f64,
    pub yes_best_ask: f64,
    pub no_best_bid: f64,
    pub no_best_ask: f64,

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
            yes_best_bid: 0.0,
            yes_best_ask: 0.0,
            no_best_bid: 0.0,
            no_best_ask: 0.0,
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
        MarketPnL::from_metrics(&self.metrics, self.yes_best_bid, self.no_best_bid)
    }

    /// Tek tick — strateji'ye karar ver. `effective_score`: composite skor (5.0 = nötr).
    pub fn tick(
        &mut self,
        cfg: &BotConfig,
        now_ms_v: u64,
        effective_score: f64,
        signal_ready: bool,
    ) -> Decision {
        match cfg.strategy {
            Strategy::Harvest => {
                let zone = self.current_zone(now_ms_v / 1000);
                let ctx = HarvestContext {
                    metrics: &self.metrics,
                    yes_token_id: &self.yes_token_id,
                    no_token_id: &self.no_token_id,
                    yes_best_bid: self.yes_best_bid,
                    yes_best_ask: self.yes_best_ask,
                    no_best_bid: self.no_best_bid,
                    no_best_ask: self.no_best_ask,
                    api_min_order_size: self.api_min_order_size,
                    order_usdc: cfg.order_usdc,
                    effective_score,
                    zone,
                    now_ms: now_ms_v,
                    last_averaging_ms: self.last_averaging_ms,
                    tick_size: self.tick_size,
                    open_orders: &self.open_orders,
                    avg_threshold: cfg.strategy_params.harvest_avg_threshold(),
                    min_price: self.min_price,
                    max_price: self.max_price,
                    cooldown_threshold: self.cooldown_threshold,
                    signal_ready,
                };
                let (new_state, decision) =
                    <HarvestEngine as DecisionEngine>::decide(self.harvest_state, &ctx);
                self.harvest_state = new_state;
                decision
            }
            Strategy::DutchBook | Strategy::Prism => unreachable!(
                "bot/ctx.rs::load only allows Strategy::Harvest at start time"
            ),
        }
    }
}

/// User WS `trade MATCHED` fill'ini metrics'e yansıt + cooldown saatini ileri al.
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
    if asset_id == session.yes_token_id {
        session.yes_best_bid = best_bid;
        session.yes_best_ask = best_ask;
    } else if asset_id == session.no_token_id {
        session.no_best_bid = best_bid;
        session.no_best_ask = best_ask;
    }
}

pub fn outcome_from_asset_id(session: &MarketSession, asset_id: &str) -> Option<Outcome> {
    if asset_id == session.yes_token_id {
        Some(Outcome::Up)
    } else if asset_id == session.no_token_id {
        Some(Outcome::Down)
    } else {
        None
    }
}

