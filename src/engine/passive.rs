//! DryRun passive-fill simülatörü.

use crate::strategy::{OpenOrder, PlannedOrder};
use crate::types::{Outcome, OrderType};

use super::executor::{apply_dryrun_fill, dryrun_cross};
use super::{ExecutedOrder, MarketSession};

/// Book güncellemesinden sonra resting emirleri karşı best ile karşılaştırır;
/// geçenleri doldurur, kalanları korur. Live modda çağrılmaz.
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
        let token_id = match o.outcome {
            Outcome::Up => session.up_token_id.clone(),
            Outcome::Down => session.down_token_id.clone(),
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
            fill_price,
            fill_size,
        });
    }
    session.open_orders = keep;
    filled
}
