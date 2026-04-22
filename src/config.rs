//! Runtime konfigürasyonu — `.env` yükleme + `BotConfig` / `Credentials` (§18).

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
    /// EIP-712 imza için zincir id (Polygon mainnet = 137).
    pub polygon_chain_id: u64,
    pub rtds_ws_url: String,
    /// Tick boşluğu eşiği (ms); aşıldığında force reconnect (zombie detection).
    pub rtds_stale_threshold_ms: u64,
    pub rtds_reconnect_max_backoff_ms: u64,
}

impl RuntimeEnv {
    pub fn from_env() -> Result<Self, AppError> {
        let _ = dotenvy::dotenv();
        Ok(Self {
            port: parse_env_or("PORT", 3000u16)?,
            db_path: env_or("DB_PATH", "./data/baiter.db"),
            bot_binary: env_or("BOT_BINARY", default_bot_binary()),
            heartbeat_dir: env_or("HEARTBEAT_DIR", "./data/heartbeat"),
            gamma_base_url: env_or("GAMMA_BASE_URL", "https://gamma-api.polymarket.com"),
            clob_base_url: env_or("CLOB_BASE_URL", "https://clob.polymarket.com"),
            clob_ws_base: env_or(
                "CLOB_WS_BASE",
                "wss://ws-subscriptions-clob.polymarket.com/ws",
            ),
            polygon_chain_id: parse_env_or("POLYGON_CHAIN_ID", 137u64)?,
            rtds_ws_url: env_or("RTDS_WS_URL", "wss://ws-live-data.polymarket.com"),
            rtds_stale_threshold_ms: parse_env_or("RTDS_STALE_THRESHOLD_MS", 30_000u64)?,
            rtds_reconnect_max_backoff_ms: parse_env_or(
                "RTDS_RECONNECT_MAX_BACKOFF_MS",
                60_000u64,
            )?,
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
            .map_err(|_| AppError::Config(format!("env var {key} parse hatası: '{v}'"))),
        Err(_) => Ok(default),
    }
}

fn default_bot_binary() -> String {
    if cfg!(debug_assertions) {
        "./target/debug/bot".into()
    } else {
        "./target/release/bot".into()
    }
}

/// Polymarket kimlik bilgileri (L1 + L2). `bot_credentials` tablosundan okunur.
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

/// `bots` tablosundan yüklenen tek bir bot konfigürasyonu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub id: i64,
    pub name: String,
    /// Polymarket slug öneki veya tam slug.
    pub slug_pattern: String,
    pub strategy: Strategy,
    pub run_mode: RunMode,
    pub order_usdc: f64,
    pub min_price: f64,
    pub max_price: f64,
    /// Averaging cooldown (ms): iki averaging emri arası min süre + açık
    /// averaging GTC max yaş.
    pub cooldown_threshold: u64,
    /// Pencere ofseti: 0 = aktif, 1 = sonraki.
    pub start_offset: u32,
    pub strategy_params: StrategyParams,
}

/// Strateji-spesifik parametreler — `bots.strategy_params` JSON sütunundan parse edilir.
/// Tüm stratejiler (Alis/Elis/Aras) bu paylaşımlı set üzerinden okur; strateji-özgü
/// alanlar gerektikçe burada `Option<T>` ile eklenir.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyParams {
    /// ProfitLock eşiği oranı (örn. `0.02` → `avg_threshold = 0.98`).
    /// `metrics.profit_locked()` ve `hedge_price()` bu eşik üzerinden çalışır.
    #[serde(default)]
    pub profit_lock_pct: Option<f64>,
    /// RTDS Chainlink sinyali aktif mi. `None` → default `true`.
    #[serde(default)]
    pub rtds_enabled: Option<bool>,
    /// Composite ağırlığı — window_delta payı. `None` → default `0.70`.
    #[serde(default)]
    pub window_delta_weight: Option<f64>,
    /// Sinyal projeksiyon ileri-bakış süresi (sn). `tick.rs` velocity'yi bu
    /// süreyle çarpıp `window_delta_bps`'e ekler → 3-4 sn ileri tahmin.
    /// `None` → default `3.0`. `0.0` → projeksiyon kapalı (eski davranış).
    #[serde(default)]
    pub signal_lookahead_secs: Option<f64>,
}

impl StrategyParams {
    /// Profit-lock canonical eşiği. `profit_lock_pct` varsayılanı `0.02` →
    /// `avg_threshold = 0.98`. `StrategyContext.avg_threshold` bunu okur.
    pub fn avg_threshold(&self) -> f64 {
        self.profit_lock_pct
            .map(|p| 1.0 - p.abs())
            .unwrap_or(0.98)
    }

    pub fn rtds_enabled_or_default(&self) -> bool {
        self.rtds_enabled.unwrap_or(true)
    }

    /// `[0, 1]`'e clamp; default `0.70`.
    pub fn window_delta_weight_or_default(&self) -> f64 {
        self.window_delta_weight.unwrap_or(0.70).clamp(0.0, 1.0)
    }

    /// `[0, 30]` sn'ye clamp; default `3.0`. Üst sınır spike koruması.
    pub fn signal_lookahead_secs_or_default(&self) -> f64 {
        self.signal_lookahead_secs.unwrap_or(3.0).clamp(0.0, 30.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_env_defaults() {
        std::env::remove_var("PORT");
        std::env::remove_var("DB_PATH");
        let env = RuntimeEnv::from_env().expect("env load");
        assert_eq!(env.port, 3000);
        assert!(env.gamma_base_url.contains("gamma-api"));
    }

    #[test]
    fn avg_threshold_default_is_098() {
        let p = StrategyParams::default();
        assert!((p.avg_threshold() - 0.98).abs() < 1e-9);
    }

    #[test]
    fn avg_threshold_uses_profit_lock_pct() {
        let p = StrategyParams {
            profit_lock_pct: Some(0.05),
            ..Default::default()
        };
        assert!((p.avg_threshold() - 0.95).abs() < 1e-9);
    }
}
