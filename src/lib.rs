//! baiter-pro — Polymarket trading bot platformu (kütüphane çatısı).
//!
//! Üst düzey modüller:
//! - [`config`]      — `BotConfig`, `Credentials`, `StrategyParams`.
//! - [`db`]          — SQLite kalıcı katmanı (orders/trades/sessions/pnl/logs).
//! - [`error`]       — `AppError` (axum `IntoResponse` impl'iyle birlikte).
//! - [`ipc`]         — Frontend IPC (FrontendEvent + log_line).
//! - [`slug`]        — `{asset}-updown-{interval}-{unix_ts}` parse/format.
//! - [`time`]        — `now_ms`, `now_secs`, `MarketZone`.
//! - [`types`]       — `Outcome`, `Side`, `OrderType`, `Strategy`, `RunMode`.
//! - [`polymarket`]  — Polymarket REST + WS facade
//!   ([`polymarket::ClobClient`], [`polymarket::PolymarketEvent`], …).
//! - [`binance`]     — Binance market data + sinyal hesabı.
//! - [`strategy`]    — Strateji FSM'leri ([`strategy::DecisionEngine`]).
//! - [`engine`]      — `MarketSession`, `Executor`/`OrderSink`, dryrun simülatörü.
//! - [`bot`]         — Bot süreç döngüsü (window, ctx, persist, event).
//! - [`api`]         — HTTP API (axum router).
//! - [`supervisor`]  — Bot sürecini yönetir (start/stop/spawn).

pub mod config;
pub mod db;
pub mod error;
pub mod ipc;
pub mod slug;
pub mod time;
pub mod types;

pub mod polymarket;

pub mod binance;

pub mod strategy;

pub mod engine;

pub mod bot;

pub mod api;
pub mod supervisor;

// Sık kullanılan tipler — entegrasyon testleri ve harici binary'ler için kısayol.
pub use error::AppError;
pub use types::{Outcome, RunMode, Side, Strategy};
