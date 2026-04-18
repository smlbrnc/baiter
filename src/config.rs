//! Runtime konfigürasyonu — `.env` yükleme + BotConfig/Credentials yapıları.
//!
//! Referans: [docs/bot-platform-mimari.md §18](../../../docs/bot-platform-mimari.md).

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::types::{RunMode, Strategy};

/// Supervisor + bot süreçlerinin ortak runtime ayarları (§18.1).
#[derive(Debug, Clone)]
pub struct RuntimeEnv {
    pub port: u16,
    pub db_path: String,
    pub bot_binary: String,
    pub heartbeat_dir: String,
    pub gamma_base_url: String,
    pub clob_base_url: String,
    pub clob_ws_base: String,
    pub polygon_chain_id: u64,
    pub fallback_creds: Option<Credentials>,
}

impl RuntimeEnv {
    /// `.env` ve ortam değişkenlerinden yükle.
    pub fn from_env() -> Result<Self, AppError> {
        let _ = dotenvy::dotenv();

        let port = parse_env_or("PORT", 3000u16)?;
        let db_path = env_or("DB_PATH", "./data/baiter.db");
        let bot_binary = env_or("BOT_BINARY", default_bot_binary());
        let heartbeat_dir = env_or("HEARTBEAT_DIR", "./data/heartbeat");
        let gamma_base_url = env_or("GAMMA_BASE_URL", "https://gamma-api.polymarket.com");
        let clob_base_url = env_or("CLOB_BASE_URL", "https://clob.polymarket.com");
        let clob_ws_base = env_or(
            "CLOB_WS_BASE",
            "wss://ws-subscriptions-clob.polymarket.com/ws",
        );
        let polygon_chain_id = parse_env_or("POLYGON_CHAIN_ID", 137u64)?;

        let fallback_creds = Credentials::from_env();

        Ok(Self {
            port,
            db_path,
            bot_binary,
            heartbeat_dir,
            gamma_base_url,
            clob_base_url,
            clob_ws_base,
            polygon_chain_id,
            fallback_creds,
        })
    }
}

fn env_or(key: &str, default: impl Into<String>) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

fn parse_env_or<T: std::str::FromStr>(key: &str, default: T) -> Result<T, AppError> {
    match std::env::var(key) {
        Ok(v) => v
            .parse()
            .map_err(|_| AppError::Config(format!("env var {} parse hatası: '{}'", key, v))),
        Err(_) => Ok(default),
    }
}

fn default_bot_binary() -> String {
    if cfg!(debug_assertions) {
        "./target/debug/bot".to_string()
    } else {
        "./target/release/bot".to_string()
    }
}

/// Polymarket kimlik bilgileri (L1 + L2).
/// Bot başlatılırken önce SQLite `bot_credentials` okunur; yoksa `.env` fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub poly_address: String,
    pub poly_api_key: String,
    pub poly_passphrase: String,
    pub poly_secret: String,
    pub polygon_private_key: String,
    pub signature_type: i32,
    pub funder: Option<String>,
}

impl Credentials {
    /// Tüm POLY_* + POLYGON_PRIVATE_KEY dolu ise `Some` döner.
    pub fn from_env() -> Option<Self> {
        Some(Self {
            poly_address: std::env::var("POLY_ADDRESS").ok()?,
            poly_api_key: std::env::var("POLY_API_KEY").ok()?,
            poly_passphrase: std::env::var("POLY_PASSPHRASE").ok()?,
            poly_secret: std::env::var("POLY_SECRET").ok()?,
            polygon_private_key: std::env::var("POLYGON_PRIVATE_KEY").ok()?,
            signature_type: std::env::var("POLY_SIGNATURE_TYPE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            funder: std::env::var("POLY_FUNDER").ok(),
        })
    }
}

/// Bir bot'un tam konfigürasyonu — DB `bots` tablosundan yüklenir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub id: i64,
    pub name: String,
    /// Polymarket slug öneki veya tam slug. Bot bir sonraki pencereyi bekler.
    pub slug_pattern: String,
    pub strategy: Strategy,
    pub run_mode: RunMode,
    pub order_usdc: f64,
    /// 0-10 arası; Binance sinyal ağırlığı.
    pub signal_weight: f64,
    /// Global emir taban fiyatı — bu değerin altındaki emirler reject (default 0.05).
    pub min_price: f64,
    /// Global emir tavan fiyatı — bu değerin üstündeki emirler reject (default 0.95).
    pub max_price: f64,
    /// Averaging cooldown (ms) — tüm stratejiler için iki rolü vardır:
    /// (1) iki averaging emri arası min süre,
    /// (2) açık averaging GTC max yaş.
    /// Default: `30_000`.
    pub cooldown_threshold: u64,
    pub strategy_params: StrategyParams,
}

/// Strateji-spesifik parametreler — JSON sütunundan parse edilir.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyParams {
    /// Harvest OpenDual fill bekleme süresi (ms; default 5_000).
    #[serde(default)]
    pub harvest_dual_timeout: Option<u64>,
    /// SingleLeg ProfitLock FAK tetik oranı (örn. 0.05 → avg_threshold = 0.95).
    #[serde(default)]
    pub harvest_profit_lock_pct: Option<f64>,
    /// Dutch book scale up çarpanı.
    #[serde(default)]
    pub dutch_scale_up: Option<f64>,
    /// Serbest form (ileride stratejiye özel alanlar).
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_env_defaults() {
        // Env temiz değilse test parse etmeye çalışır; en azından default fallback'ler
        // panic atmamalı. Port override ayarsızsa 3000 dönmeli.
        std::env::remove_var("PORT");
        std::env::remove_var("DB_PATH");
        let env = RuntimeEnv::from_env().expect("env load");
        assert_eq!(env.port, 3000);
        assert!(env.gamma_base_url.contains("gamma-api"));
    }
}
