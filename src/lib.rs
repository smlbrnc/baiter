//! baiter-pro — Polymarket trading bot platformu (kütüphane çatısı).
//!
//! Foundation: [`config`], [`db`], [`error`], [`ipc`], [`slug`], [`time`], [`types`].
//! Domain: [`polymarket`], [`binance`], [`strategy`], [`engine`], [`bot`].
//! HTTP & process: [`api`], [`supervisor`].

pub mod api;
pub mod binance;
pub mod bot;
pub mod config;
pub mod db;
pub mod engine;
pub mod error;
pub mod ipc;
pub mod polymarket;
pub mod slug;
pub mod strategy;
pub mod supervisor;
pub mod time;
pub mod types;

// Sık kullanılan tipler — entegrasyon testleri ve harici binary'ler için kısayol.
pub use error::AppError;
pub use types::{Outcome, RunMode, Side, Strategy};
