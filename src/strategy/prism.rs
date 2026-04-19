//! Prism stratejisi — **iskelet stub**.
//!
//! Kural seti henüz tanımlanmadı (TBD); bu modül yalnızca FSM durum enum'unun
//! yerini tutar. `bot/ctx.rs::load` aktif strateji olarak `Strategy::Prism`
//! seçilmiş bir botu start anında reddeder; bu modül runtime'da çağrılmaz.
//!
//! Tam FSM doldurulurken bu dosya:
//! 1. `PrismState` durumlarını listeler (`Pending → ... → Done`),
//! 2. `PrismContext`'i bot/strategy ortak alanlarıyla genişletir,
//! 3. `decide()` fonksiyonu eklenir ve `DecisionEngine` impl'i sağlanır,
//! 4. `engine::MarketSession::tick` içine match kolu eklenir.
//!
//! Referans: [docs/strategies.md §3](../../../docs/strategies.md).

use serde::{Deserialize, Serialize};

/// Prism FSM durumu — TBD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PrismState {
    #[default]
    Pending,
    Done,
}
