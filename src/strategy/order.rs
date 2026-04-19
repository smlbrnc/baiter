//! Açık emir kaydı — engine ↔ strategy ortak tipi.
//!
//! Daha önce `crate::engine::OpenOrder` olarak tanımlıydı; harvest FSM'i bu tipi
//! `&[OpenOrder]` üzerinden okuyor, bu da `engine ↔ strategy::harvest` döngüsüne
//! sebep oluyordu. Tipi `crate::strategy::order` altına taşıyarak ileri yönlü
//! tek bağımlılık kuruyoruz.

use serde::{Deserialize, Serialize};

use crate::types::{OrderType, Outcome, Side};

use super::PlannedOrder;

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

/// `Side::Buy` + `OrderType::Gtc` PlannedOrder kısayolu — strateji içi
/// tekrar eden boilerplate'i azaltır.
pub fn planned_buy_gtc(
    outcome: Outcome,
    token_id: impl Into<String>,
    price: f64,
    size: f64,
    reason: impl Into<String>,
) -> PlannedOrder {
    PlannedOrder {
        outcome,
        token_id: token_id.into(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Gtc,
        reason: reason.into(),
    }
}
