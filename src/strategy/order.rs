//! Açık emir kaydı — engine ↔ strategy ortak tipi.
//!
//! Daha önce `crate::engine::OpenOrder` olarak tanımlıydı; harvest FSM'i bu tipi
//! `&[OpenOrder]` üzerinden okuyor, bu da `engine ↔ strategy::harvest` döngüsüne
//! sebep oluyordu. Tipi `crate::strategy::order` altına taşıyarak ileri yönlü
//! tek bağımlılık kuruyoruz.

use serde::{Deserialize, Serialize};

use crate::types::{Outcome, Side};

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
