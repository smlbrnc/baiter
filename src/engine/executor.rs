//! Emir yürütücü — DryRun Simulator veya Live CLOB.

use std::sync::Arc;

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::config::Credentials;
use crate::db;
use crate::error::AppError;
use crate::polymarket::clob::{CancelResponse, ClobClient};
use crate::polymarket::order::{
    build_order, expiration_for, order_to_json, sign_order, BuildArgs,
};
use crate::strategy::{Decision, OpenOrder, PlannedOrder};
use crate::time::now_ms;
use crate::types::{Outcome, Side};

use super::{ExecutedOrder, MarketSession};

/// Fee sabiti — DryRun simülasyonu için.
pub const DRYRUN_FEE_RATE: f64 = 0.0002; // %0.02

/// Emir yürütücü — DryRun Simulator veya Live CLOB.
pub enum Executor {
    DryRun(Simulator),
    Live(LiveExecutor),
}

/// Live mod CLOB emir yürütücü — EIP-712 imza + CLOB POST /order.
pub struct LiveExecutor {
    pub client: Arc<ClobClient>,
    pub creds: Credentials,
    pub chain_id: u64,
    /// Açık emir GTD timeout (sn). `cooldown_threshold` (ms) → sn dönüşümü.
    pub gtd_timeout_secs: u64,
    /// Persistans için pool — fire-and-forget yazımlar (§⚡ Kural 4).
    pub pool: SqlitePool,
}

impl Executor {
    pub async fn place(
        &self,
        session: &mut MarketSession,
        planned: &PlannedOrder,
    ) -> Result<ExecutedOrder, AppError> {
        match self {
            Self::DryRun(sim) => Ok(sim.fill(session, planned)),
            Self::Live(live) => live.place(session, planned).await,
        }
    }

    pub async fn cancel(
        &self,
        session: &mut MarketSession,
        order_id: &str,
    ) -> Result<CancelResponse, AppError> {
        match self {
            Self::DryRun(_) => Ok(CancelResponse {
                canceled: vec![order_id.to_string()],
                not_canceled: serde_json::json!({}),
            }),
            Self::Live(live) => {
                let resp = live.client.cancel_order(order_id).await?;
                live.persist_cancel(session, order_id, &resp);
                Ok(resp)
            }
        }
    }
}

impl LiveExecutor {
    /// PlannedOrder → EIP-712 Order → imza → POST /order.
    ///
    /// Doc §13: GTC emirler `expiration=0`, GTD emirler `now + timeout` ile
    /// imzalanır; FAK/FOK emirler de GTC eşdeğeri (CLOB tarafında matching
    /// politikası ayrı parametreyle yönetilir).
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
            session.metrics.ingest_fill(
                planned.outcome,
                planned.price,
                planned.size,
                0.0, // fee CLOB feeRateBps=0 ile gönderildi
            );
            session.last_fill_price = planned.price;
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

    /// CLOB POST /order başarılı yanıtını fire-and-forget olarak DB'ye yazar
    /// (§⚡ Kural 4: kritik yolu bloke etmez).
    fn persist_place(
        &self,
        session: &MarketSession,
        executed: &ExecutedOrder,
        post_status: &str,
    ) {
        let pool = self.pool.clone();
        let record = db::orders::OrderRecord {
            order_id: executed.order_id.clone(),
            bot_id: session.bot_id,
            market_session_id: Some(session.market_session_id),
            source: "rest_post".into(),
            lifecycle_type: Some("PLACEMENT".into()),
            market: Some(session.condition_id.clone()),
            asset_id: Some(executed.planned.token_id.clone()),
            side: Some(executed.planned.side.as_str().into()),
            price: Some(executed.planned.price),
            outcome: Some(executed.planned.outcome.as_str().into()),
            order_type: Some(executed.planned.order_type.as_str().into()),
            original_size: Some(executed.planned.size),
            size_matched: executed.fill_size,
            expiration: None,
            associate_trades: None,
            post_status: Some(post_status.to_string()),
            order_status: None,
            ts_ms: now_ms() as i64,
            raw_payload: None,
            delete_canceled: None,
            delete_not_canceled: None,
        };
        tokio::spawn(async move {
            if let Err(e) = db::orders::upsert_order(&pool, &record).await {
                tracing::warn!(error=%e, "rest_post upsert_order failed");
            }
        });
    }

    /// CLOB DELETE /order sonucunu fire-and-forget olarak DB'ye yazar.
    fn persist_cancel(&self, session: &MarketSession, order_id: &str, resp: &CancelResponse) {
        let pool = self.pool.clone();
        let record = db::orders::OrderRecord {
            order_id: order_id.to_string(),
            bot_id: session.bot_id,
            market_session_id: Some(session.market_session_id),
            source: "rest_delete".into(),
            lifecycle_type: Some("CANCELLATION".into()),
            market: Some(session.condition_id.clone()),
            asset_id: None,
            side: None,
            price: None,
            outcome: None,
            order_type: None,
            original_size: None,
            size_matched: None,
            expiration: None,
            associate_trades: None,
            post_status: None,
            order_status: Some("CANCELED".into()),
            ts_ms: now_ms() as i64,
            raw_payload: None,
            delete_canceled: Some(serde_json::to_string(&resp.canceled).unwrap_or_default()),
            delete_not_canceled: Some(resp.not_canceled.to_string()),
        };
        tokio::spawn(async move {
            if let Err(e) = db::orders::upsert_order(&pool, &record).await {
                tracing::warn!(error=%e, "rest_delete upsert_order failed");
            }
        });
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
        let counter_price = counter_price_for(session, planned.outcome, planned.side);

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

/// `execute()` çıktısı — placed + canceled (gerçek `CancelResponse`'lar).
#[derive(Debug, Default)]
pub struct ExecuteOutput {
    pub placed: Vec<ExecutedOrder>,
    pub canceled: Vec<CancelResponse>,
}

/// Global price guard: emir fiyatı [min_price, max_price] dışındaysa reject.
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
        Decision::PlaceOrders(orders) => place_batch(session, exec, orders, &mut out).await?,
        Decision::CancelOrders(ids) => cancel_batch(session, exec, &ids, &mut out).await?,
        Decision::Batch { cancel, place } => {
            cancel_batch(session, exec, &cancel, &mut out).await?;
            place_batch(session, exec, place, &mut out).await?;
        }
    }
    Ok(out)
}

async fn place_batch(
    session: &mut MarketSession,
    exec: &Executor,
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

async fn cancel_batch(
    session: &mut MarketSession,
    exec: &Executor,
    ids: &[String],
    out: &mut ExecuteOutput,
) -> Result<(), AppError> {
    for id in ids {
        out.canceled.push(exec.cancel(session, id).await?);
    }
    session.open_orders.retain(|o| !ids.contains(&o.id));
    Ok(())
}
