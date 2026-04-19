//! DryRun passive-fill simülatörü.

use crate::strategy::{OpenOrder, PlannedOrder};
use crate::time::now_ms;
use crate::types::{Outcome, OrderType};

use super::executor::{apply_dryrun_fill, dryrun_cross};
use super::{ExecutedOrder, MarketSession};

/// Book güncellemesinden sonra `session.open_orders` içindeki her live emri
/// karşı best fiyatla karşılaştırır; geçenleri doldurur, kalanları korur.
/// Filled emirler `metrics`'i ve `last_averaging_ms`'yi günceller.
/// Live modda çağrılmaz (gerçek user WS yapar).
pub fn simulate_passive_fills(session: &mut MarketSession) -> Vec<ExecutedOrder> {
    let mut filled: Vec<ExecutedOrder> = Vec::new();
    let mut keep: Vec<OpenOrder> = Vec::with_capacity(session.open_orders.len());
    let snapshot = std::mem::take(&mut session.open_orders);

    for o in snapshot {
        let Some(fill_price) = dryrun_cross(session, o.outcome, o.side, o.price) else {
            keep.push(o);
            continue;
        };
        let fill_size = o.size;
        apply_dryrun_fill(session, o.outcome, fill_price, fill_size);
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
