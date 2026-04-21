//! Polymarket istemci facade'ı.
//!
//! Alt modüller: [`auth`] (L1/L2 imza, EIP-712), [`clob`] (REST CLOB),
//! [`gamma`] (markets metadata), [`order`] (EIP-712 build/sign),
//! [`ws`] (Market + User WebSocket akışları).
//!
//! Sık kullanılan tipler buradan re-export edilir; çağırıcılar
//! `polymarket::ClobClient` gibi sığ yollar kullanır.

pub mod auth;
pub mod clob;
pub mod fees;
pub mod gamma;
pub mod order;
pub mod ws;

pub use clob::{shared_http_client, CancelResponse, ClobClient};
pub use fees::polymarket_taker_fee;
pub use gamma::{GammaClient, GammaMarket};
pub use ws::{run_market_ws, run_user_ws, PolymarketEvent, PriceChangeLevel};
