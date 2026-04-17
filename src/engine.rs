//! MarketSession + decision loop + DryRun simulator.
//!
//! `RunMode::DryRun`: Simulator deterministic fill üretir (slip yok, fee %0.02).
//! `RunMode::Live`: `ClobClient::post_order` ile gerçek CLOB'a gönderir.
//!
//! Referans: [docs/bot-platform-mimari.md §13 §16 §⚡ Kural 1-2](../../../docs/bot-platform-mimari.md).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::BotConfig;
use crate::error::AppError;
use crate::polymarket::clob::ClobClient;
use crate::strategy::harvest::{decide as harvest_decide, HarvestContext, HarvestState};
use crate::strategy::metrics::{MarketPnL, StrategyMetrics};
use crate::strategy::{Decision, PlannedOrder};
use crate::time::{now_ms, zone_pct, MarketZone};
use crate::types::{Outcome, RunMode, Side, Strategy, TradeStatus};

/// Fee sabiti — DryRun simülasyonu için.
pub const DRYRUN_FEE_RATE: f64 = 0.0002; // %0.02

/// Yürütülen emir sonucu.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub condition_id: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub tick_size: f64,
    pub api_min_order_size: f64,
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
    pub open_order_ids: Vec<String>,
}

impl MarketSession {
    pub fn new(bot_id: i64, slug: String, cfg: &BotConfig) -> Self {
        Self {
            bot_id,
            slug,
            condition_id: String::new(),
            yes_token_id: String::new(),
            no_token_id: String::new(),
            tick_size: 0.01,
            api_min_order_size: 5.0,
            start_ts: 0,
            end_ts: 0,
            strategy: cfg.strategy,
            harvest_state: HarvestState::Pending,
            metrics: StrategyMetrics::default(),
            last_averaging_ms: 0,
            last_fill_price: 0.0,
            yes_best_bid: 0.0,
            yes_best_ask: 1.0,
            no_best_bid: 0.0,
            no_best_ask: 1.0,
            run_mode: cfg.run_mode,
            open_order_ids: Vec::new(),
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
    pub fn tick(&mut self, cfg: &BotConfig, now_ms_v: u64) -> Decision {
        match cfg.strategy {
            Strategy::Harvest => {
                let up_bid = cfg
                    .strategy_params
                    .harvest_open_offset_ticks
                    .map(|o| 0.50 + (o as f64) * self.tick_size)
                    .unwrap_or(0.50);
                let down_bid = cfg
                    .strategy_params
                    .harvest_open_offset_ticks
                    .map(|o| 0.50 + (o as f64) * self.tick_size)
                    .unwrap_or(0.48);
                let avg_threshold = cfg
                    .strategy_params
                    .harvest_profit_lock_pct
                    .map(|p| 1.0 - p.abs())
                    .unwrap_or(0.98);

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
                    effective_score: 5.0,
                    zone: self.current_zone(now_ms_v / 1000),
                    now_ms: now_ms_v,
                    last_averaging_ms: self.last_averaging_ms,
                    last_fill_price: self.last_fill_price,
                    up_bid,
                    down_bid,
                    avg_threshold,
                    cooldown_ms: 30_000,
                    max_position_size: 100.0,
                };
                let (new_state, decision) = harvest_decide(self.harvest_state, &ctx);
                self.harvest_state = new_state;
                // §1 katman kuralı: StopTrade bölgesinde yeni emir üretilmez
                // (zone_signal_map dışında, strateji motorunun üst katmanı enforce eder).
                if matches!(self.current_zone(now_ms_v / 1000), MarketZone::StopTrade) {
                    filter_stop_trade(decision)
                } else {
                    decision
                }
            }
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
        Decision::Batch { cancel, place: _ } => {
            if cancel.is_empty() {
                Decision::NoOp
            } else {
                Decision::CancelOrders(cancel)
            }
        }
    }
}

/// Emir yürütücü — DryRun Simulator veya Live CLOB.
pub enum Executor {
    DryRun(Simulator),
    Live(Arc<ClobClient>),
}

impl Executor {
    pub async fn place(
        &self,
        session: &mut MarketSession,
        planned: &PlannedOrder,
    ) -> Result<ExecutedOrder, AppError> {
        match self {
            Self::DryRun(sim) => Ok(sim.fill(session, planned)),
            Self::Live(client) => {
                // NOTE: Faz 13 içinde EIP-712 order struct imzalama entegre edilir.
                // Şu an basitçe planlanan emri REST'e göndermek için bir sarmalayıcı var.
                tracing::warn!("live executor: EIP-712 order signing Faz 13'te aktifleşir");
                let order_id = format!("stub-{}", Uuid::new_v4());
                let _ = client; // suppress unused
                Ok(ExecutedOrder {
                    order_id,
                    planned: planned.clone(),
                    filled: false,
                    fill_price: None,
                    fill_size: None,
                })
            }
        }
    }

    pub async fn cancel(
        &self,
        _session: &mut MarketSession,
        order_id: &str,
    ) -> Result<(), AppError> {
        match self {
            Self::DryRun(_) => Ok(()),
            Self::Live(client) => {
                let _ = client.cancel_order(order_id).await?;
                Ok(())
            }
        }
    }
}

/// DryRun simülatörü — slip yok, fee %0.02 sabit.
#[derive(Debug, Clone, Default)]
pub struct Simulator;

impl Simulator {
    pub fn fill(&self, session: &mut MarketSession, planned: &PlannedOrder) -> ExecutedOrder {
        let fill_price = planned.price;
        let fill_size = planned.size;
        let fee = fill_price * fill_size * DRYRUN_FEE_RATE;

        session
            .metrics
            .ingest_fill(planned.outcome, fill_price, fill_size, fee);
        session.last_fill_price = fill_price;
        session.last_averaging_ms = now_ms();

        let order_id = format!("dry-{}", Uuid::new_v4());
        session.open_order_ids.retain(|id| id != &order_id);

        ExecutedOrder {
            order_id,
            planned: planned.clone(),
            filled: true,
            fill_price: Some(fill_price),
            fill_size: Some(fill_size),
        }
    }
}

/// Decision sonucu batch'i yürüt — `ExecutedOrder` listesi döner.
pub async fn execute(
    session: &mut MarketSession,
    exec: &Executor,
    decision: Decision,
) -> Result<Vec<ExecutedOrder>, AppError> {
    match decision {
        Decision::NoOp | Decision::Complete => Ok(vec![]),
        Decision::PlaceOrders(orders) => {
            let mut out = Vec::with_capacity(orders.len());
            for o in orders {
                out.push(exec.place(session, &o).await?);
            }
            Ok(out)
        }
        Decision::CancelOrders(ids) => {
            for id in &ids {
                exec.cancel(session, id).await?;
            }
            Ok(vec![])
        }
        Decision::Batch { cancel, place } => {
            for id in &cancel {
                exec.cancel(session, id).await?;
            }
            let mut out = Vec::with_capacity(place.len());
            for o in place {
                out.push(exec.place(session, &o).await?);
            }
            Ok(out)
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

/// Side + outcome kombinasyonu (log için).
pub fn format_side(outcome: Outcome, side: Side) -> String {
    let out = match outcome {
        Outcome::Up => "YES",
        Outcome::Down => "NO",
    };
    let s = match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    };
    format!("{out}/{s}")
}

/// Trade status helper.
pub fn trade_status_ok(status: &str) -> bool {
    matches!(
        status,
        s if s.eq_ignore_ascii_case("MATCHED")
            || s.eq_ignore_ascii_case("MINED")
            || s.eq_ignore_ascii_case("CONFIRMED")
    )
}

/// (Placeholder) TradeStatus serialize eder; test için.
pub fn trade_status_label(ts: TradeStatus) -> &'static str {
    match ts {
        TradeStatus::Matched => "MATCHED",
        TradeStatus::Mined => "MINED",
        TradeStatus::Confirmed => "CONFIRMED",
        TradeStatus::Retrying => "RETRYING",
        TradeStatus::Failed => "FAILED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StrategyParams;

    fn test_cfg(run_mode: RunMode) -> BotConfig {
        BotConfig {
            id: 1,
            name: "test".into(),
            slug_pattern: "btc-updown-5m-1776420900".into(),
            strategy: Strategy::Harvest,
            run_mode,
            order_usdc: 5.0,
            signal_weight: 0.0,
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
        sess.yes_best_ask = 0.52;
        sess.no_best_bid = 0.48;
        sess.no_best_ask = 0.50;

        let dec = sess.tick(&cfg, now_ms());
        let exec = Executor::DryRun(Simulator);
        let filled = execute(&mut sess, &exec, dec).await.unwrap();
        assert_eq!(filled.len(), 2);
        assert!(filled.iter().all(|e| e.filled));
        assert!(sess.metrics.shares_yes > 0.0);
        assert!(sess.metrics.shares_no > 0.0);
    }
}
