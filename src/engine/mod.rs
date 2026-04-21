//! MarketSession + decision loop + DryRun simulator.
//!
//! Alt modüller: [`executor`] (DryRun + Live + batch yürütücü), [`passive`]
//! (DryRun passive-fill).
//!
//! Referans: [docs/bot-platform-mimari.md §13 §16](../../../docs/bot-platform-mimari.md).

use std::collections::HashSet;

use crate::config::BotConfig;
use crate::strategy::harvest::{HarvestContext, HarvestEngine, HarvestState};
use crate::strategy::metrics::{MarketPnL, StrategyMetrics};
use crate::strategy::{Decision, DecisionEngine, OpenOrder, PlannedOrder};
use crate::time::{zone_pct, MarketZone};
use crate::types::{Outcome, RunMode, Side, Strategy};

pub mod executor;
pub mod passive;

pub use executor::{
    execute, ExecuteOutput, Executor, LiveExecutor, Simulator, DRYRUN_FEE_RATE,
};
pub use passive::simulate_passive_fills;

/// Yürütülen emir sonucu — in-memory pipeline kaydı (DB persist sub-field'lar üzerinden).
/// `fill_price`/`fill_size` daima set edilir: fill olmamış emirlerde planned
/// değerleriyle (kitapta canlı duran emrin beklenen fiyatı/boyutu).
#[derive(Debug, Clone)]
pub struct ExecutedOrder {
    pub order_id: String,
    pub planned: PlannedOrder,
    pub filled: bool,
    pub fill_price: f64,
    pub fill_size: f64,
}

/// Market seansı — bir bot × bir pencere (slug).
#[derive(Debug, Clone)]
pub struct MarketSession {
    pub bot_id: i64,
    pub slug: String,
    /// `market_sessions.id` — DB FK (orders/trades/pnl).
    pub market_session_id: i64,
    pub condition_id: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub tick_size: f64,
    pub api_min_order_size: f64,
    /// NegRisk Exchange mi? EIP-712 verifying_contract belirleyici.
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

    pub run_mode: RunMode,
    pub open_orders: Vec<OpenOrder>,
    /// Live `POST /order` `status=matched` yanıtında lokal `metrics`'i tek
    /// noktadan ingest edip ID'yi buraya yazıyoruz; aynı fill için sonradan
    /// gelen User WS `trade MATCHED` event'i `bot/event.rs::extract_our_fills`
    /// içinde ID'yi bu set'te bulup atlar (REST + WS çift sayım koruması,
    /// Bot 4 / btc-updown-5m-1776773400 spam regresyonu). WS event geldiği
    /// anda set'ten çıkarılır → kendi kendini temizler. WS hiç gelmeyen edge
    /// case'te `note_recent_fill` üst sınır aşılırsa set sıfırlanır.
    pub recently_filled_order_ids: HashSet<String>,

    pub min_price: f64,
    pub max_price: f64,
    /// Averaging cooldown (ms) — strateji ctx'lerine geçirilir.
    pub cooldown_threshold: u64,
    /// CLOB `GET /fee-rate?token_id=YES` sonucu (basis points). Pencere açılırken
    /// Live modda bir kez fetch edilir; DryRun'da 0. `LiveExecutor::place`
    /// `BuildArgs.fee_rate_bps`'e geçirir — server'ın `mbf`'i ile eşleşmezse 400.
    pub fee_rate_bps: u32,
    /// `📚 Market book ready` logu basıldı mı?
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
            run_mode: cfg.run_mode,
            open_orders: Vec::new(),
            recently_filled_order_ids: HashSet::new(),
            min_price: cfg.min_price,
            max_price: cfg.max_price,
            cooldown_threshold: cfg.cooldown_threshold,
            fee_rate_bps: 0,
            book_ready_logged: false,
        }
    }

    /// Güncel market bölgesi (yalnızca `tick` içinden çağrılır).
    fn current_zone(&self, now_secs: u64) -> MarketZone {
        MarketZone::from_pct(zone_pct(self.start_ts, self.end_ts, now_secs))
    }

    /// MTM PnL (§17).
    pub fn pnl(&self) -> MarketPnL {
        MarketPnL::from_metrics(&self.metrics, self.yes_best_bid, self.no_best_bid)
    }

    /// Live REST `status=matched` ID'sini idempotency setine yaz. Set 1024
    /// üstüne çıkarsa (WS hiç gelmeyen patolojik durum) sıfırla — yarış
    /// penceresi dışındaki ID'leri tutmanın anlamı yok.
    pub fn note_recent_fill(&mut self, order_id: String) {
        if self.recently_filled_order_ids.len() > 1024 {
            self.recently_filled_order_ids.clear();
        }
        self.recently_filled_order_ids.insert(order_id);
    }

    /// WS trade event'inden gelen ID daha önce REST'te ingest edildiyse
    /// `true` döndürür ve set'ten çıkarır (yarış penceresi kapandı).
    pub fn consume_recent_fill(&mut self, order_id: &str) -> bool {
        self.recently_filled_order_ids.remove(order_id)
    }

    /// Tek tick — strateji'ye karar ver. Çağıran composite skorunu (5.0 = nötr)
    /// doğrudan geçer; Harvest v2 opener fiyatı ve pyramid `delta`sı bu skoru
    /// kullanır (doc §3, §5).
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
            _ => Decision::NoOp,
        }
    }
}

/// User WS `trade MATCHED` event'inden gelen fill'i absorbla. `side=Sell`
/// (manuel/dış SELL) → pozisyondan çıkış: shares düşer, cost realize olur.
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

/// `best_bid_ask` güncelle.
pub fn update_best(session: &mut MarketSession, asset_id: &str, best_bid: f64, best_ask: f64) {
    if asset_id == session.yes_token_id {
        session.yes_best_bid = best_bid;
        session.yes_best_ask = best_ask;
    } else if asset_id == session.no_token_id {
        session.no_best_bid = best_bid;
        session.no_best_ask = best_ask;
    }
}

/// `asset_id → Outcome`.
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
            min_price: 0.05,
            max_price: 0.95,
            cooldown_threshold: 30_000,
            start_offset: 0,
            strategy_params: StrategyParams::default(),
        }
    }

    #[tokio::test]
    async fn pending_tick_places_open_pair_and_fills_both_legs() {
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

        let dec = sess.tick(&cfg, now_ms(), 5.0, true);
        let exec = Executor::DryRun(Simulator);
        let filled = execute(&mut sess, &exec, dec).await.unwrap();
        assert_eq!(filled.placed.len(), 2, "OpenPair opener + hedge");
        assert!(filled.placed.iter().all(|e| e.filled));
        assert!(sess.metrics.shares_yes > 0.0);
        assert!(sess.metrics.shares_no > 0.0);
        assert_eq!(sess.harvest_state, HarvestState::OpenPair);
    }
}
