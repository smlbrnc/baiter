//! Emir yürütücü — DryRun Simulator veya Live CLOB.

use std::collections::HashSet;
use std::sync::Arc;

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::config::Credentials;
use crate::db;
use crate::error::AppError;
use crate::ipc;
use crate::polymarket::order::{
    build_order, expiration_for, order_to_json, sign_order, BuildArgs,
};
use crate::polymarket::{polymarket_taker_fee, CancelResponse, ClobClient};
use crate::strategy::harvest::is_averaging_like;
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
            fee_rate_bps: session.fee_rate_bps,
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
        // `status` enumu (Polymarket CLOB `POST /order` sözleşmesi):
        //   "matched"   → karşı taraf at execution time'da var, fill REST
        //                  yanıtında garantili → in-memory ingest tek kaynak
        //                  (Bot 4 / btc-updown-5m-1776773400 spam fix).
        //   "live"      → kitaba girdi, passive bekliyor → `open_orders` push.
        //   "delayed"   → CLOB asenkron eşleştirme kuyruğunda; sonuç User WS
        //                  `trade MATCHED` ile gelir → `open_orders` push;
        //                  WS event'inde idempotency kontrolü gerek değil.
        //   "unmatched" → reject (success=false ile zaten yukarıda yakalandı).
        // Bkz. <https://docs.polymarket.com/developers/CLOB/orders/create-an-order>.
        let filled = resp.status.eq_ignore_ascii_case("matched");
        if filled {
            // Atomic in-memory update: REST yanıtı geldiği anda metrics
            // güncellenir, opener cooldown'u tetiklenir, ID idempotency
            // setine yazılır → sonradan gelen User WS `trade MATCHED` event'i
            // aynı fill'i ikinci kez `ingest_fill` etmez.
            let fee = polymarket_taker_fee(planned.price, planned.size, session.fee_rate_bps);
            session.metrics.ingest_fill(
                planned.outcome,
                planned.side,
                planned.price,
                planned.size,
                fee,
            );
            session.note_recent_fill(resp.order_id.clone());
            if is_averaging_like(&planned.reason) {
                session.last_averaging_ms = now_ms();
            }
        } else {
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
        }
        let executed = ExecutedOrder {
            order_id: resp.order_id.clone(),
            planned: planned.clone(),
            filled,
            fill_price: planned.price,
            fill_size: planned.size,
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
            size_matched: executed.filled.then_some(executed.fill_size),
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
#[derive(Debug, Clone)]
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
        // `last_averaging_ms` `place_batch` sonunda tek noktada güncellenir
        // (averaging cooldown takibi için tek kaynak).

        ExecutedOrder {
            order_id,
            planned: planned.clone(),
            filled: true,
            fill_price,
            fill_size,
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
    session
        .metrics
        .ingest_fill(outcome, Side::Buy, fill_price, fill_size, fee);
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
        Decision::NoOp => {}
        Decision::PlaceOrders(orders) => place_batch(session, exec, orders, &mut out).await?,
        Decision::CancelOrders(ids) => cancel_batch(session, exec, &ids, &mut out).await?,
        Decision::CancelAndPlace { cancels, places } => {
            // Sıra önemli: önce cancel (eski hedge book'tan düşsün), sonra
            // place (yeni hedge aynı tick'te kitapta olsun). REST'ler atomic
            // değil ama tek bir tick içinde sıralı yürütülür → fill geldikten
            // sonra hedge fiyatı update'i için ek tick gecikmesi yok.
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
        if is_averaging_like(&executed.planned.reason) {
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
    let label = session.bot_id.to_string();
    // Sadece Polymarket'in **gerçekten** iptal ettiği id'leri lokal state'ten
    // sil. `not_canceled` (örn. "matched orders can't be canceled") emir hâlâ
    // canlı veya match'te demektir; silersek ardından gelen MATCHED event'inde
    // `extract_our_fills` bu id'yi `our_open_orders` setinde bulamaz ve maker
    // fill atlanır.
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
