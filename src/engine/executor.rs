//! Emir yürütücü — DryRun Simulator veya Live CLOB.

use std::collections::HashSet;
use std::sync::Arc;

use uuid::Uuid;

use crate::config::Credentials;
use crate::error::AppError;
use crate::ipc;
use crate::polymarket::order::{
    build_order, expiration_for, order_to_json, sign_order, BuildArgs,
};
use crate::polymarket::{CancelResponse, ClobClient};
use crate::strategy::harvest::is_averaging_like;
use crate::strategy::{Decision, OpenOrder, PlannedOrder};
use crate::time::now_ms;
use crate::types::{Outcome, Side};

use super::{ExecutedOrder, MarketSession};

/// DryRun fee oranı (%0.02).
pub const DRYRUN_FEE_RATE: f64 = 0.0002;

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

/// Live CLOB yürütücü — EIP-712 imza + POST /order.
pub struct LiveExecutor {
    pub client: Arc<ClobClient>,
    pub creds: Credentials,
    pub chain_id: u64,
    pub gtd_timeout_secs: u64,
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
            Self::Live(live) => live.client.cancel_order(order_id).await,
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
    /// PlannedOrder → EIP-712 → POST /order. GTD için `now + timeout`, diğerleri `0`.
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
            fee_rate_bps: session.fee_rate_bps,
            tick_size: session.tick_size,
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
                resp.status.as_str(),
                resp.error_msg
            )));
        }
        // REST `status=matched` partial fill'i de işaretler. `size_matched=0` push;
        // gerçek fill miktarı + metrics yalnız User WS `trade MATCHED`'den toplanır
        // (REST `planned.price` VWAP'ı bozabilir, partial size REST'te yok).
        let filled = resp.status.is_filled();
        session.open_orders.push(OpenOrder {
            id: resp.order_id.clone(),
            outcome: planned.outcome,
            side: planned.side,
            price: planned.price,
            size: planned.size,
            reason: planned.reason.clone(),
            placed_at_ms: now_ms(),
            size_matched: 0.0,
        });
        Ok(ExecutedOrder {
            order_id: resp.order_id,
            planned: planned.clone(),
            filled,
            fill_price: planned.price,
            fill_size: planned.size,
        })
    }
}

/// DryRun simülatörü — slip yok, sabit fee.
#[derive(Debug)]
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
                size_matched: 0.0,
            });
            return ExecutedOrder {
                order_id,
                planned: planned.clone(),
                filled: false,
                fill_price: planned.price,
                fill_size: planned.size,
            };
        };

        let fill_size = planned.size;
        apply_dryrun_fill(session, planned.outcome, fill_price, fill_size);

        ExecutedOrder {
            order_id,
            planned: planned.clone(),
            filled: true,
            fill_price,
            fill_size,
        }
    }
}

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

/// Emir karşı best fiyatı geçtiyse fill fiyatını döndürür (taker + passive ortak).
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

/// DryRun fill: fee hesaplar ve metrics'i günceller. Cooldown caller'a aittir.
pub(crate) fn apply_dryrun_fill(
    session: &mut MarketSession,
    outcome: Outcome,
    fill_price: f64,
    fill_size: f64,
) {
    let fee = fill_price * fill_size * DRYRUN_FEE_RATE;
    session
        .metrics
        .ingest_fill(outcome, Side::Buy, fill_price, fill_size, fee);
}

/// Averaging-like reason ise cooldown saatini ileri al. Tek tetik noktası
/// `place_batch` (Live + DryRun) ve `simulate_passive_fills`.
pub(crate) fn maybe_arm_averaging_cooldown(session: &mut MarketSession, reason: &str) {
    if is_averaging_like(reason) {
        session.last_averaging_ms = now_ms();
    }
}

#[derive(Debug, Default)]
pub struct ExecuteOutput {
    pub placed: Vec<ExecutedOrder>,
    pub canceled: Vec<CancelResponse>,
}

/// `[min_price, max_price]` dışındaysa reject.
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
        Decision::NoOp => {}
        Decision::PlaceOrders(orders) => place_batch(session, exec, orders, &mut out).await?,
        Decision::CancelOrders(ids) => cancel_batch(session, exec, &ids, &mut out).await?,
        Decision::CancelAndPlace { cancels, places } => {
            // Sıra: önce cancel, sonra place — yeni hedge aynı tick'te kitapta olsun.
            if !cancels.is_empty() {
                cancel_batch(session, exec, &cancels, &mut out).await?;
            }
            if !places.is_empty() {
                place_batch(session, exec, places, &mut out).await?;
            }
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
        maybe_arm_averaging_cooldown(session, &executed.planned.reason);
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
    let label = session.bot_id.to_string();
    // Yalnız Polymarket'in gerçekten iptal ettiği id'leri lokal state'ten sil;
    // `not_canceled` emir hâlâ canlı/matched, MATCHED event'inde maker fill için lazım.
    let mut truly_canceled: HashSet<String> = HashSet::new();
    for id in ids {
        let resp = exec.cancel(session, id).await?;
        for c in &resp.canceled {
            truly_canceled.insert(c.clone());
        }
        if let Some(map) = resp.not_canceled.as_object() {
            for (nc_id, reason) in map {
                let reason_s = reason
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| reason.to_string());
                ipc::log_line(
                    &label,
                    format!("⚠️ cancel rejected id={nc_id} reason={reason_s}"),
                );
            }
        }
        out.canceled.push(resp);
    }
    session.open_orders.retain(|o| !truly_canceled.contains(&o.id));
    Ok(())
}
