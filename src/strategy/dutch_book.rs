//! Dutch Book stratejisi — **iskelet stub**.
//!
//! Kural seti henüz tanımlanmadı (TBD); bu modül yalnızca enum/sözleşme yerini
//! tutar. `bot/ctx.rs` aktif strateji olarak `Strategy::DutchBook` seçilmiş bir
//! botu start anında reddeder; dolayısıyla `decide()` runtime'da çağrılmaz.
//!
//! Tam FSM doldurulurken bu dosya:
//! 1. `DutchBookState` durumlarını listeler (`Pending → ... → Done`),
//! 2. `DutchBookContext`'i bot/strategy ortak alanlarıyla genişletir,
//! 3. `decide()` içine kuralları yazar,
//! 4. `strategy::required_metrics(Strategy::DutchBook)` maskesini doldurur,
//! 5. `strategy::ZoneSignalMap::DUTCH_BOOK` sabiti tanımlanır,
//! 6. `engine::MarketSession::tick` içine match kolu eklenir.
//!
//! Referans: [docs/strategies.md §1](../../../docs/strategies.md).

use serde::{Deserialize, Serialize};

use crate::strategy::Decision;

/// Dutch Book FSM durumu — TBD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DutchBookState {
    #[default]
    Pending,
    Done,
}

/// Karar fonksiyonu — TBD; şimdilik no-op döndürür.
///
/// Aktif olmayan strateji `bot/ctx.rs::load`'da reddedildiği için bu fonksiyon
/// runtime'da çağrılmaz; yalnız modül imzasının derlenebilirliğini garanti eder.
pub fn decide(_state: DutchBookState) -> (DutchBookState, Decision) {
    (DutchBookState::Done, Decision::NoOp)
}
