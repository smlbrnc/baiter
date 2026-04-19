//! DryRun passive-fill simülatörü.

use crate::strategy::{OpenOrder, PlannedOrder};
use crate::time::now_ms;
use crate::types::{OrderType, Outcome, Side};

use super::executor::{counter_price_for, DRYRUN_FEE_RATE};
use super::{ExecutedOrder, MarketSession};

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
        let counter_price = counter_price_for(session, o.outcome, o.side);
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
        let token_id = match o.outcome {
            Outcome::Up => session.yes_token_id.clone(),
            Outcome::Down => session.no_token_id.clone(),
        };
        filled.push(ExecutedOrder {
            order_id: o.id.clone(),
            planned: PlannedOrder {
                outcome: o.outcome,
                token_id,
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
