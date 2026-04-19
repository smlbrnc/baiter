//! Emir yürütücü — DryRun Simulator veya Live CLOB.

use std::sync::Arc;

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::config::Credentials;
use crate::db;
use crate::error::AppError;
use crate::polymarket::order::{
    build_order, expiration_for, order_to_json, sign_order, BuildArgs,
};
use crate::polymarket::{CancelResponse, ClobClient};
use crate::strategy::{Decision, OpenOrder, PlannedOrder};
use crate::time::now_ms;
use crate::types::{Outcome, Side};

use super::{ExecutedOrder, MarketSession};

/// DryRun fee oranı (%0.02).
pub const DRYRUN_FEE_RATE: f64 = 0.0002;

/// Emir yürütme sözleşmesi — `Simulator` (dryrun) ve `LiveExecutor` (CLOB) sağlar.
#[async_trait::async_trait]
pub trait OrderSink: Send + Sync {
    async fn place(
        &self,
        session: &mut MarketSession,
        planned: &PlannedOrder,
    ) -> Result<ExecutedOrder, AppError>;

    async fn cancel(
        &self,
        session: &mut MarketSession,
        order_id: &str,
    ) -> Result<CancelResponse, AppError>;
}

pub enum Executor {
    DryRun(Simulator),
    Live(LiveExecutor),
}

/// Live mod CLOB emir yürütücü — EIP-712 imza + CLOB POST /order.
pub struct LiveExecutor {
    pub client: Arc<ClobClient>,
    pub creds: Credentials,
    pub chain_id: u64,
    /// GTD timeout (sn). `cooldown_threshold` (ms) → sn dönüşümü.
    pub gtd_timeout_secs: u64,
    /// Fire-and-forget DB persist için (§⚡ Kural 4).
    pub pool: SqlitePool,
}

#[async_trait::async_trait]
impl OrderSink for Executor {
    async fn place(
        &self,
        session: &mut MarketSession,
        planned: &PlannedOrder,
    ) -> Result<ExecutedOrder, AppError> {
        match self {
            Self::DryRun(sim) => sim.place(session, planned).await,
            Self::Live(live) => live.place(session, planned).await,
        }
    }

    async fn cancel(
        &self,
        session: &mut MarketSession,
        order_id: &str,
    ) -> Result<CancelResponse, AppError> {
        match self {
            Self::DryRun(sim) => sim.cancel(session, order_id).await,
            Self::Live(live) => {
                let resp = live.client.cancel_order(order_id).await?;
                live.persist_cancel(session, order_id, &resp);
                Ok(resp)
            }
        }
    }
}

#[async_trait::async_trait]
impl OrderSink for Simulator {
    async fn place(
        &self,
        session: &mut MarketSession,
        planned: &PlannedOrder,
    ) -> Result<ExecutedOrder, AppError> {
        Ok(self.fill(session, planned))
    }

    async fn cancel(
        &self,
        _session: &mut MarketSession,
        order_id: &str,
    ) -> Result<CancelResponse, AppError> {
        Ok(CancelResponse {
            canceled: vec![order_id.to_string()],
            not_canceled: serde_json::json!({}),
        })
    }
}

impl LiveExecutor {
    /// PlannedOrder → EIP-712 → POST /order. GTC/FAK/FOK için `expiration=0`,
    /// GTD için `now + timeout`.
    pub async fn place(
        &self,
        session: &mut MarketSession,
        planned: &PlannedOrder,
    ) -> Result<ExecutedOrder, AppError> {
        let exp = expiration_for(planned.order_type.as_str(), self.gtd_timeout_secs);
        let order = build_order(&BuildArgs {
            creds: &self.creds,
            token_id: &planned.token_id,
            side: planned.side,
            size: planned.size,
            price: planned.price,
            expiration_secs: exp,
            neg_risk: session.neg_risk,
        })?;
        let sig = sign_order(&order, &self.creds, self.chain_id, session.neg_risk).await?;
        let body = order_to_json(&order, &sig);
        let owner = self.creds.poly_api_key.clone();
        let resp = self
            .client
            .post_order(body, planned.order_type.as_str(), &owner)
            .await?;
        if !resp.success {
            return Err(AppError::Clob(format!(
                "POST /order rejected: status={} error={}",
                resp.status, resp.error_msg
            )));
        }
        let filled = resp.status.eq_ignore_ascii_case("matched");
        if !filled {
            session.open_orders.push(OpenOrder {
                id: resp.order_id.clone(),
                outcome: planned.outcome,
                side: planned.side,
                price: planned.price,
                size: planned.size,
                reason: planned.reason.clone(),
                placed_at_ms: now_ms(),
            });
        } else {
            session
                .metrics
                .ingest_fill(planned.outcome, planned.price, planned.size, 0.0);
            session.last_averaging_ms = now_ms();
        }
        let executed = ExecutedOrder {
            order_id: resp.order_id.clone(),
            planned: planned.clone(),
            filled,
            fill_price: filled.then_some(planned.price),
            fill_size: filled.then_some(planned.size),
        };
        self.persist_place(session, &executed, &resp.status);
        Ok(executed)
    }

    fn persist_place(
        &self,
        session: &MarketSession,
        executed: &ExecutedOrder,
        post_status: &str,
    ) {
        let record = db::orders::OrderRecord::rest_placement(db::orders::RestPlacementInput {
            bot_id: session.bot_id,
            market_session_id: session.market_session_id,
            market: session.condition_id.clone(),
            order_id: executed.order_id.clone(),
            asset_id: executed.planned.token_id.clone(),
            side: executed.planned.side.as_str(),
            outcome: executed.planned.outcome.as_str(),
            order_type: executed.planned.order_type.as_str(),
            price: executed.planned.price,
            original_size: executed.planned.size,
            size_matched: executed.fill_size,
            post_status: post_status.to_string(),
            ts_ms: now_ms() as i64,
        });
        db::orders::persist_order(&self.pool, record, "rest_post upsert_order");
    }

    fn persist_cancel(&self, session: &MarketSession, order_id: &str, resp: &CancelResponse) {
        let record = db::orders::OrderRecord::rest_cancellation(
            session.bot_id,
            session.market_session_id,
            session.condition_id.clone(),
            order_id.to_string(),
            &resp.canceled,
            &resp.not_canceled,
            now_ms() as i64,
        );
        db::orders::persist_order(&self.pool, record, "rest_delete upsert_order");
    }
}

/// DryRun simülatörü — slip yok, sabit fee.
#[derive(Debug, Clone, Default)]
pub struct Simulator;

impl Simulator {
    /// Live davranışını yansıtır:
    /// - BUY `price >= karşı best_ask` → matched (taker), `fill_price = best_ask`
    /// - SELL `price <= karşı best_bid` → matched (taker), `fill_price = best_bid`
    /// - aksi halde live (orderbook'a girer)
    pub fn fill(&self, session: &mut MarketSession, planned: &PlannedOrder) -> ExecutedOrder {
        let order_id = format!("dry-{}", Uuid::new_v4());
        let Some(fill_price) = dryrun_cross(session, planned.outcome, planned.side, planned.price)
        else {
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
        };

        let fill_size = planned.size;
        apply_dryrun_fill(session, planned.outcome, fill_price, fill_size);
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

/// Karşı taraf fiyatı — DryRun fill kararı için.
pub(crate) fn counter_price_for(session: &MarketSession, outcome: Outcome, side: Side) -> f64 {
    match side {
        Side::Buy => match outcome {
            Outcome::Up => session.yes_best_ask,
            Outcome::Down => session.no_best_ask,
        },
        Side::Sell => match outcome {
            Outcome::Up => session.yes_best_bid,
            Outcome::Down => session.no_best_bid,
        },
    }
}

/// Emir karşı taraf en iyi fiyatı geçtiyse fill fiyatını döndürür.
/// `Simulator::fill` (taker) ve `simulate_passive_fills` (resting) için ortak.
pub(crate) fn dryrun_cross(
    session: &MarketSession,
    outcome: Outcome,
    side: Side,
    price: f64,
) -> Option<f64> {
    let counter = counter_price_for(session, outcome, side);
    if counter <= 0.0 {
        return None;
    }
    let crosses = match side {
        Side::Buy => price >= counter,
        Side::Sell => price <= counter,
    };
    crosses.then_some(counter)
}

/// DryRun fill ortak kuyruğu: fee hesaplar ve `metrics`'i günceller.
/// `last_averaging_ms` caller'a aittir (taker vs passive farklı politika).
pub(crate) fn apply_dryrun_fill(
    session: &mut MarketSession,
    outcome: Outcome,
    fill_price: f64,
    fill_size: f64,
) {
    let fee = fill_price * fill_size * DRYRUN_FEE_RATE;
    session.metrics.ingest_fill(outcome, fill_price, fill_size, fee);
}

/// `execute()` çıktısı.
#[derive(Debug, Default)]
pub struct ExecuteOutput {
    pub placed: Vec<ExecutedOrder>,
    pub canceled: Vec<CancelResponse>,
}

/// Global price guard: `[min_price, max_price]` dışındaysa reject.
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
pub async fn execute<S: OrderSink + ?Sized>(
    session: &mut MarketSession,
    exec: &S,
    decision: Decision,
) -> Result<ExecuteOutput, AppError> {
    let mut out = ExecuteOutput::default();
    match decision {
        Decision::NoOp | Decision::Complete => {}
        Decision::PlaceOrders(orders) => place_batch(session, exec, orders, &mut out).await?,
        Decision::CancelOrders(ids) => cancel_batch(session, exec, &ids, &mut out).await?,
        Decision::Batch { cancel, place } => {
            cancel_batch(session, exec, &cancel, &mut out).await?;
            place_batch(session, exec, place, &mut out).await?;
        }
    }
    Ok(out)
}

async fn place_batch<S: OrderSink + ?Sized>(
    session: &mut MarketSession,
    exec: &S,
    orders: Vec<PlannedOrder>,
    out: &mut ExecuteOutput,
) -> Result<(), AppError> {
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
    Ok(())
}

async fn cancel_batch<S: OrderSink + ?Sized>(
    session: &mut MarketSession,
    exec: &S,
    ids: &[String],
    out: &mut ExecuteOutput,
) -> Result<(), AppError> {
    for id in ids {
        out.canceled.push(exec.cancel(session, id).await?);
    }
    session.open_orders.retain(|o| !ids.contains(&o.id));
    Ok(())
}
