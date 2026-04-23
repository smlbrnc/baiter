//! Polymarket istemci facade'ı; alt modüller: [`auth`] (L1/L2 EIP-712),
//! [`clob`] (REST), [`gamma`] (markets), [`order`] (build/sign), [`ws`]
//! (Market + User WebSocket). Sık kullanılan tipler re-export edilir.

pub mod auth;
pub mod clob;
pub mod fees;
pub mod gamma;
pub mod order;
pub mod ws;

pub use clob::{shared_http_client, CancelResponse, ClobClient, PostOrderStatus, TakerFee};
pub use fees::{fee_for_role, FeeParams};
pub use gamma::{GammaClient, GammaMarket};
pub use ws::{
    run_market_ws, run_user_ws, MarketResolvedPayload, OrderLifecycle, OrderPayload,
    PolymarketEvent, PriceChangeLevel, TradePayload, TradeStatus, WsChannels,
};
