//! Polymarket istemci facade'ı: `auth`, `clob`, `gamma`, `order`, `ws`.

pub mod auth;
pub mod clob;
pub mod fees;
pub mod gamma;
pub mod order;
pub mod ws;

pub use clob::{shared_http_client, CancelResponse, ClobClient, PostOrderItem};
pub use fees::{fee_for_role, FeeParams};
pub use gamma::{GammaClient, GammaMarket};
pub use ws::{
    run_market_ws, run_user_ws, MarketResolvedPayload, OrderLifecycle, OrderPayload,
    PolymarketEvent, PriceChangeLevel, TradePayload, TradeStatus, WsChannels,
};
