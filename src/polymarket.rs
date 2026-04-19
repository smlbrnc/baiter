//! Polymarket istemci facade'ı.
//!
//! Modüller:
//! - [`auth`]   — L1/L2 imza, EIP-712 başlıkları.
//! - [`clob`]   — REST CLOB istemcisi (orderbook, /order, /cancel, heartbeat).
//! - [`gamma`]  — Gamma API (markets/{slug} metadata).
//! - [`order`]  — EIP-712 Order build + sign yardımcıları.
//! - [`ws`]     — Market + User WebSocket akışları, `PolymarketEvent`.
//!
//! Sık kullanılan tipler doğrudan bu facade'dan re-export edilir; çağırıcılar
//! `polymarket::ClobClient` gibi sığ yollar kullanır, alt modüllere inmek
//! yalnızca özel/iç tipler için gereklidir.

pub mod auth;
pub mod clob;
pub mod gamma;
pub mod order;
pub mod ws;

pub use clob::{shared_http_client, CancelResponse, ClobClient};
pub use gamma::{GammaClient, GammaMarket};
pub use ws::{run_market_ws, run_user_ws, PolymarketEvent};
