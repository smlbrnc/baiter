//! Emir yürütücü — DryRun Simulator veya Live CLOB (tek-batch HTTP).

use std::collections::HashSet;
use std::sync::Arc;

use futures_util::stream::{FuturesOrdered, StreamExt};
use uuid::Uuid;

use crate::config::Credentials;
use crate::error::AppError;
use crate::ipc;
use crate::polymarket::order::{
    build_order, expiration_for, order_to_json, sign_order, BuildArgs, SignerCache,
};
use crate::polymarket::{CancelResponse, ClobClient};
use crate::strategy::{Decision, OpenOrder, PlannedOrder};
use crate::time::now_ms;
use crate::types::{Outcome, Side};

use super::{ExecutedOrder, MarketSession};

/// DryRun fee oranı (%0.02).
pub const DRYRUN_FEE_RATE: f64 = 0.0002;

pub enum Executor {
    DryRun(Simulator),
    Live(Box<LiveExecutor>),
}

/// Live CLOB yürütücü; `SignerCache` boot'ta bir kez kurulur.
pub struct LiveExecutor {
    pub client: Arc<ClobClient>,
    pub owner: String,
    pub gtd_timeout_secs: u64,
    pub signer: Arc<SignerCache>,
}

impl LiveExecutor {
    /// Boot anında bir kez çağrılır; başarısızsa fatal.
    pub fn new(
        client: Arc<ClobClient>,
        creds: &Credentials,
        chain_id: u64,
        gtd_timeout_secs: u64,
    ) -> Result<Self, AppError> {
        let signer = Arc::new(SignerCache::from_creds(creds, chain_id)?);
        Ok(Self {
            client,
            owner: creds.poly_api_key.clone(),
            gtd_timeout_secs,
            signer,
        })
    }

    /// Tek `PlannedOrder`'ı imzala (cache'lenmiş signer + EIP-712 domain).
    async fn build_signed_body(
        &self,
        planned: &PlannedOrder,
        tick_size: f64,
        neg_risk: bool,
    ) -> Result<serde_json::Value, AppError> {
        let exp = expiration_for(planned.order_type.as_str(), self.gtd_timeout_secs);
        let order = build_order(&BuildArgs {
            cache: &self.signer,
            token_id: &planned.token_id,
            side: planned.side,
            size: planned.size,
            price: planned.price,
            tick_size,
        })?;
        let sig = sign_order(&order, &self.signer, neg_risk).await?;
        Ok(order_to_json(&self.signer, &order, exp, &sig))
    }

    /// İmza + `POST /order` `FuturesOrdered` üzerinden eş zamanlı; sonuçlar
    /// planned[] sırasında toplanır. Tek planned'da tek future = tek HTTP,
    /// poll overhead'i nano-saniye seviyesi.
    pub async fn place_many(
        &self,
        session: &mut MarketSession,
        planned: &[PlannedOrder],
    ) -> Result<Vec<ExecutedOrder>, AppError> {
        if planned.is_empty() {
            return Ok(Vec::new());
        }
        let tick_size = session.tick_size;
        let neg_risk = session.neg_risk;
        let owner = self.owner.as_str();
        let mut futs: FuturesOrdered<_> = planned
            .iter()
            .map(|p| async move {
                let body = self.build_signed_body(p, tick_size, neg_risk).await?;
                self.client
                    .post_order(&body, owner, p.order_type.as_str())
                    .await
            })
            .collect();
        let mut resps = Vec::with_capacity(planned.len());
        while let Some(r) = futs.next().await {
            resps.push(r?);
        }
        let label = session.bot_label.as_ref();
        let mut placed = Vec::with_capacity(planned.len());
        for (p, resp) in planned.iter().zip(resps.into_iter()) {
            if !resp.success {
                ipc::log_line(
                    label,
                    format!(
                        "❌ POST /orders rejected status={} error={} reason={}",
                        resp.status.as_str(),
                        resp.error_msg,
                        p.reason
                    ),
                );
                tracing::warn!(
                    bot_id = session.bot_id,
                    status = resp.status.as_str(),
                    error = %resp.error_msg,
                    reason = %p.reason,
                    "order rejected in batch"
                );
                continue;
            }
            let filled = resp.status.is_filled();
            session
                .open_orders
                .push(open_order_from_planned(resp.order_id.clone(), p));
            placed.push(ExecutedOrder {
                order_id: resp.order_id,
                planned: p.clone(),
                filled,
                fill_price: p.price,
                fill_size: p.size,
            });
        }
        Ok(placed)
    }

    /// Tek `DELETE /orders` + lokal prune (terminal red'ler dahil; non-terminal'ler canlı tutulur).
    pub async fn cancel_many(
        &self,
        session: &mut MarketSession,
        ids: &[String],
    ) -> Result<CancelResponse, AppError> {
        let label = session.bot_label.as_ref();
        let resp = self.client.cancel_orders(ids).await?;
        let mut truly_canceled: HashSet<String> = HashSet::new();
        let mut terminal_reject: HashSet<String> = HashSet::new();
        for c in &resp.canceled {
            truly_canceled.insert(c.clone());
        }
        if let Some(map) = resp.not_canceled.as_object() {
            for (nc_id, reason) in map {
                let reason_s = reason
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| reason.to_string());
                let terminal = is_terminal_not_canceled(&reason_s);
                ipc::log_line(
                    label,
                    format!(
                        "⚠️ cancel rejected id={nc_id} reason={reason_s}{}",
                        if terminal { " [terminal → pruning]" } else { "" }
                    ),
                );
                if terminal {
                    terminal_reject.insert(nc_id.clone());
                }
            }
        }
        session
            .open_orders
            .retain(|o| !truly_canceled.contains(&o.id) && !terminal_reject.contains(&o.id));
        Ok(resp)
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
            session
                .open_orders
                .push(open_order_from_planned(order_id.clone(), planned));
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

fn open_order_from_planned(id: String, planned: &PlannedOrder) -> OpenOrder {
    OpenOrder {
        id,
        outcome: planned.outcome,
        side: planned.side,
        price: planned.price,
        size: planned.size,
        reason: planned.reason.clone(),
        placed_at_ms: now_ms(),
        size_matched: 0.0,
    }
}

pub(crate) fn counter_price_for(session: &MarketSession, outcome: Outcome, side: Side) -> f64 {
    match side {
        Side::Buy => match outcome {
            Outcome::Up => session.up_best_ask,
            Outcome::Down => session.down_best_ask,
        },
        Side::Sell => match outcome {
            Outcome::Up => session.up_best_bid,
            Outcome::Down => session.down_best_bid,
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

/// Decision'ı yürüt; `CancelAndPlace` sırası: önce cancel, sonra place.
pub async fn execute(
    session: &mut MarketSession,
    executor: &Executor,
    decision: Decision,
) -> Result<ExecuteOutput, AppError> {
    let mut out = ExecuteOutput::default();
    match decision {
        Decision::NoOp => {}
        Decision::PlaceOrders(orders) => {
            place_batch(session, executor, orders, &mut out).await?
        }
        Decision::CancelOrders(ids) => {
            cancel_batch(session, executor, &ids, &mut out).await?
        }
        Decision::CancelAndPlace { cancels, places } => {
            if !cancels.is_empty() {
                cancel_batch(session, executor, &cancels, &mut out).await?;
            }
            if !places.is_empty() {
                place_batch(session, executor, places, &mut out).await?;
            }
        }
    }
    Ok(out)
}

async fn place_batch(
    session: &mut MarketSession,
    executor: &Executor,
    orders: Vec<PlannedOrder>,
    out: &mut ExecuteOutput,
) -> Result<(), AppError> {
    let filtered: Vec<PlannedOrder> = orders
        .into_iter()
        .filter(|o| within_price_bounds(session, o))
        .collect();
    if filtered.is_empty() {
        return Ok(());
    }
    match executor {
        Executor::DryRun(sim) => {
            for o in &filtered {
                let executed = sim.fill(session, o);
                out.placed.push(executed);
            }
        }
        Executor::Live(live) => {
            let executed = live.place_many(session, &filtered).await?;
            out.placed.extend(executed);
        }
    }
    Ok(())
}

/// Order'ın kesin düştüğünü belirten `not_canceled` reason substring'leri (prune trigger).
const TERMINAL_NOT_CANCELED_REASONS: &[&str] = &[
    "order can't be found",
    "matched orders can't",
    "order not found",
    "order is already canceled",
];

fn is_terminal_not_canceled(reason: &str) -> bool {
    TERMINAL_NOT_CANCELED_REASONS
        .iter()
        .any(|s| reason.contains(s))
}

async fn cancel_batch(
    session: &mut MarketSession,
    executor: &Executor,
    ids: &[String],
    out: &mut ExecuteOutput,
) -> Result<(), AppError> {
    if ids.is_empty() {
        return Ok(());
    }
    match executor {
        Executor::DryRun(_) => {
            let id_set: HashSet<&String> = ids.iter().collect();
            session.open_orders.retain(|o| !id_set.contains(&o.id));
            out.canceled.push(CancelResponse {
                canceled: ids.to_vec(),
                not_canceled: serde_json::json!({}),
            });
        }
        Executor::Live(live) => {
            let resp = live.cancel_many(session, ids).await?;
            out.canceled.push(resp);
        }
    }
    Ok(())
}

