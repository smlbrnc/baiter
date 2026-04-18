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
use crate::polymarket::clob::{CancelResponse, ClobClient};
use crate::strategy::harvest::{decide as harvest_decide, HarvestContext, HarvestState};
use crate::strategy::metrics::{MarketPnL, StrategyMetrics};
use crate::strategy::{Decision, PlannedOrder};
use crate::time::{now_ms, zone_pct, MarketZone};
use crate::types::{OrderType, Outcome, RunMode, Side, Strategy, TradeStatus};

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

/// Kitapta açık (live) emir kaydı — averaging timeout / pos_held için.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenOrder {
    pub id: String,
    pub outcome: Outcome,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub reason: String,
    pub placed_at_ms: u64,
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
    pub open_orders: Vec<OpenOrder>,

    /// Global emir taban fiyatı (bot config) — engine guard.
    pub min_price: f64,
    /// Global emir tavan fiyatı (bot config) — engine guard.
    pub max_price: f64,
    /// Averaging cooldown (ms) — strateji ctx'lerine geçirilir (bot config).
    pub cooldown_threshold: u64,
    /// Bir kez `📚 Market book ready` logu basıldı mı? (bot.rs içinden okunur/yazılır).
    pub book_ready_logged: bool,
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

    /// Açık emir id'leri (cancel için).
    pub fn open_ids(&self) -> Vec<String> {
        self.open_orders.iter().map(|o| o.id.clone()).collect()
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
                let avg_threshold = cfg
                    .strategy_params
                    .harvest_profit_lock_pct
                    .map(|p| 1.0 - p.abs())
                    .unwrap_or(0.98);
                let dual_timeout = cfg
                    .strategy_params
                    .harvest_dual_timeout
                    .unwrap_or(5_000);

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
                    zone: self.current_zone(now_ms_v / 1000),
                    now_ms: now_ms_v,
                    last_averaging_ms: self.last_averaging_ms,
                    last_fill_price: self.last_fill_price,
                    tick_size: self.tick_size,
                    dual_timeout,
                    open_orders: &self.open_orders,
                    avg_threshold,
                    max_position_size: 100.0,
                    min_price: self.min_price,
                    max_price: self.max_price,
                    cooldown_threshold: self.cooldown_threshold,
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
    ) -> Result<CancelResponse, AppError> {
        match self {
            Self::DryRun(_) => Ok(CancelResponse {
                canceled: vec![order_id.to_string()],
                not_canceled: serde_json::json!({}),
            }),
            Self::Live(client) => client.cancel_order(order_id).await,
        }
    }
}

/// DryRun simülatörü — slip yok, fee %0.02 sabit.
#[derive(Debug, Clone, Default)]
pub struct Simulator;

impl Simulator {
    /// Live davranışını yansıtır:
    /// - BUY price >= karşı best_ask  → matched (taker), fill_price = best_ask
    /// - SELL price <= karşı best_bid → matched (taker), fill_price = best_bid
    /// - Karşı fiyat 0.0 (henüz quote yok) veya emir geçmiyorsa → live (orderbook'a girer)
    pub fn fill(&self, session: &mut MarketSession, planned: &PlannedOrder) -> ExecutedOrder {
        let order_id = format!("dry-{}", Uuid::new_v4());

        let counter_price = match planned.side {
            Side::Buy => match planned.outcome {
                Outcome::Up => session.yes_best_ask,
                Outcome::Down => session.no_best_ask,
            },
            Side::Sell => match planned.outcome {
                Outcome::Up => session.yes_best_bid,
                Outcome::Down => session.no_best_bid,
            },
        };

        let crosses = counter_price > 0.0
            && match planned.side {
                Side::Buy => planned.price >= counter_price,
                Side::Sell => planned.price <= counter_price,
            };

        if !crosses {
            session.open_orders.push(OpenOrder {
                id: order_id.clone(),
                outcome: planned.outcome,
                side: planned.side,
                price: planned.price,
                size: planned.size,
                reason: planned.reason.clone(),
                placed_at_ms: now_ms(),
            });
            return ExecutedOrder {
                order_id,
                planned: planned.clone(),
                filled: false,
                fill_price: None,
                fill_size: None,
            };
        }

        let fill_price = counter_price;
        let fill_size = planned.size;
        let fee = fill_price * fill_size * DRYRUN_FEE_RATE;

        session
            .metrics
            .ingest_fill(planned.outcome, fill_price, fill_size, fee);
        session.last_fill_price = fill_price;
        session.last_averaging_ms = now_ms();

        ExecutedOrder {
            order_id,
            planned: planned.clone(),
            filled: true,
            fill_price: Some(fill_price),
            fill_size: Some(fill_size),
        }
    }
}

/// `execute()` çıktısı — placed + canceled (gerçek `CancelResponse`'lar).
#[derive(Debug, Default)]
pub struct ExecuteOutput {
    pub placed: Vec<ExecutedOrder>,
    pub canceled: Vec<CancelResponse>,
}

/// Global price guard: emir fiyatı [min_price, max_price] dışındaysa reject.
/// `info` seviyesinde log basar (supervisor stdout'tan parse eder).
fn within_price_bounds(session: &MarketSession, planned: &PlannedOrder) -> bool {
    if planned.price < session.min_price || planned.price > session.max_price {
        tracing::info!(
            "🚧 Order rejected: price={:.4} outside [{:.2}, {:.2}] reason={}",
            planned.price,
            session.min_price,
            session.max_price,
            planned.reason
        );
        return false;
    }
    true
}

/// Decision sonucu batch'i yürüt.
pub async fn execute(
    session: &mut MarketSession,
    exec: &Executor,
    decision: Decision,
) -> Result<ExecuteOutput, AppError> {
    let mut out = ExecuteOutput::default();
    match decision {
        Decision::NoOp | Decision::Complete => {}
        Decision::PlaceOrders(orders) => {
            for o in orders {
                if !within_price_bounds(session, &o) {
                    continue;
                }
                let executed = exec.place(session, &o).await?;
                if executed.planned.reason.starts_with("harvest:averaging") {
                    session.last_averaging_ms = now_ms();
                }
                out.placed.push(executed);
            }
        }
        Decision::CancelOrders(ids) => {
            for id in &ids {
                out.canceled.push(exec.cancel(session, id).await?);
            }
            session.open_orders.retain(|o| !ids.contains(&o.id));
        }
        Decision::Batch { cancel, place } => {
            for id in &cancel {
                out.canceled.push(exec.cancel(session, id).await?);
            }
            session.open_orders.retain(|o| !cancel.contains(&o.id));
            for o in place {
                if !within_price_bounds(session, &o) {
                    continue;
                }
                let executed = exec.place(session, &o).await?;
                if executed.planned.reason.starts_with("harvest:averaging") {
                    session.last_averaging_ms = now_ms();
                }
                out.placed.push(executed);
            }
        }
    }
    Ok(out)
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

/// **DryRun passive-fill simülatörü.**
///
/// Market WS book güncellemesinden sonra çağrılır: `session.open_orders` içindeki
/// her live emir mevcut book'la karşılaştırılır:
/// - **BUY** (`outcome=Up` → karşı `yes_best_ask`, `outcome=Down` → `no_best_ask`):
///   `best_ask > 0 && order.price >= best_ask` ise emir o anda dolar (`fill_price = best_ask`).
/// - **SELL** sırasıyla karşı `best_bid` ile karşılaştırılır.
///
/// Filled emirler `open_orders`'tan silinir; `metrics`/`last_fill_price`/
/// `last_averaging_ms` güncellenir. Live modda çağrılmaz (gerçek user WS yapar).
pub fn simulate_passive_fills(session: &mut MarketSession) -> Vec<ExecutedOrder> {
    let mut filled: Vec<ExecutedOrder> = Vec::new();
    let mut keep: Vec<OpenOrder> = Vec::with_capacity(session.open_orders.len());
    let snapshot = std::mem::take(&mut session.open_orders);

    for o in snapshot {
        let counter_price = match o.side {
            Side::Buy => match o.outcome {
                Outcome::Up => session.yes_best_ask,
                Outcome::Down => session.no_best_ask,
            },
            Side::Sell => match o.outcome {
                Outcome::Up => session.yes_best_bid,
                Outcome::Down => session.no_best_bid,
            },
        };
        let crosses = counter_price > 0.0
            && match o.side {
                Side::Buy => o.price >= counter_price,
                Side::Sell => o.price <= counter_price,
            };
        if !crosses {
            keep.push(o);
            continue;
        }
        let fill_price = counter_price;
        let fill_size = o.size;
        let fee = fill_price * fill_size * DRYRUN_FEE_RATE;
        session
            .metrics
            .ingest_fill(o.outcome, fill_price, fill_size, fee);
        session.last_fill_price = fill_price;
        if o.reason.starts_with("harvest:averaging") {
            session.last_averaging_ms = now_ms();
        }
        filled.push(ExecutedOrder {
            order_id: o.id.clone(),
            planned: PlannedOrder {
                outcome: o.outcome,
                token_id: match o.outcome {
                    Outcome::Up => session.yes_token_id.clone(),
                    Outcome::Down => session.no_token_id.clone(),
                },
                side: o.side,
                price: o.price,
                size: o.size,
                order_type: OrderType::Gtc,
                reason: o.reason.clone(),
            },
            filled: true,
            fill_price: Some(fill_price),
            fill_size: Some(fill_size),
        });
    }
    session.open_orders = keep;
    filled
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
            min_price: 0.05,
            max_price: 0.95,
            cooldown_threshold: 30_000,
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
