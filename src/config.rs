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
    pub polygon_chain_id: u64,
    pub rtds_ws_url: String,
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

/// Tüm botlar için tek standart V2 builder code (bytes32 hex). Per-credential
/// override yok — order JSON `builder` alanına `SignerCache` üzerinden injekte
/// edilir.
pub const BUILDER_CODE_HEX: &str =
    "0xa5ff679c20c755da3ebdb8a1a4066823b402053c199ceae78e31f01695f48f5a";

/// `bots` tablosundan yüklenen tek bir bot konfigürasyonu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub id: i64,
    pub name: String,
    pub slug_pattern: String,
    pub strategy: Strategy,
    pub run_mode: RunMode,
    pub order_usdc: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub cooldown_threshold: u64,
    pub start_offset: u32,
    pub strategy_params: StrategyParams,
}

/// Strateji-spesifik parametreler; `bots.strategy_params` JSON sütunundan
/// parse edilir, tüm stratejiler (Alis/Elis) buradan okur.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyParams {
    #[serde(default)]
    pub profit_lock_pct: Option<f64>,
    #[serde(default)]
    pub rtds_enabled: Option<bool>,
    #[serde(default)]
    pub window_delta_weight: Option<f64>,
    #[serde(default)]
    pub signal_lookahead_secs: Option<f64>,
    #[serde(default)]
    pub open_delta: Option<f64>,
    #[serde(default)]
    pub pyramid_agg_delta: Option<f64>,
    #[serde(default)]
    pub pyramid_fak_delta: Option<f64>,
    #[serde(default)]
    pub pyramid_usdc: Option<f64>,

    // === Elis (Dutch Book Bid Loop) — docs/gabagool.md ===
    /// Taraf başına temel emir büyüklüğü (share). Default: 20.0
    #[serde(default)]
    pub elis_max_buy_order_size: Option<f64>,
    /// Loop süresi: emir → iptal arası bekleme (ms). Default: 2000
    #[serde(default)]
    pub elis_trade_cooldown_ms: Option<u64>,
    /// Pencere kapanmadan bu kadar saniye önce dur. Default: 30.0
    #[serde(default)]
    pub elis_stop_before_end_secs: Option<f64>,
    /// P4: avg pair cost'u bu kadar düşürmeyen alım yapılmaz. Default: 0.005
    #[serde(default)]
    pub elis_min_improvement: Option<f64>,
    /// P5 Vol filter: bid-ask spread bu eşiği aşarsa NoOp (OB ince). Default: 0.05
    #[serde(default)]
    pub elis_vol_threshold: Option<f64>,
    /// P5 BSI filter: |BSI| bu eşiği aşarsa karşı tarafı engelle. Default: 0.50
    #[serde(default)]
    pub elis_bsi_filter_threshold: Option<f64>,
    /// P2 Lock threshold: avg_sum bu değerin altına düşünce pozisyon kilitli sayılır. Default: 0.98
    #[serde(default)]
    pub elis_lock_threshold: Option<f64>,
    /// P6 Stale cleanup: emirler bu süreden eskiyse zorla iptal (ms). Default: 30000
    #[serde(default)]
    pub elis_max_order_age_ms: Option<u64>,
    // Eski alanlar (backend artık kullanmıyor, DB uyumu için tutuldu)
    #[serde(default)]
    pub elis_spread_threshold: Option<f64>,
    #[serde(default)]
    pub elis_balance_factor: Option<f64>,

    // === Bonereaper parametreleri ===
    /// Signal emirlerinde taker (ask) kullanılsın mı? Default: true (live'da anında fill).
    /// `false` ise best_bid'den maker GTC emir verilir.
    #[serde(default)]
    pub bonereaper_signal_taker: Option<bool>,
    /// Profit-lock için imbalance eşiği (share). |up_filled − down_filled| bu değerin
    /// altında VE her iki tarafta da fill varsa profit_lock devreye girer.
    /// Default 50.0.
    #[serde(default)]
    pub bonereaper_profit_lock_imbalance: Option<f64>,
    /// Signal yön onayı için kaç ardışık tick gerekli? K=1 (default) anlık karar.
    /// K=2+ → yeni yön için K ardışık tick onayı; flip-flop'u azaltır.
    #[serde(default)]
    pub bonereaper_signal_persistence_k: Option<u32>,
    /// Polymarket UP_bid sinyalinin composite içindeki ağırlığı [0, 1].
    /// Yön kararı: `signal × (1-w) + market × w`. 0 = sadece Binance/OKX;
    /// 0.7 (default) = Polymarket dominant.
    #[serde(default)]
    pub bonereaper_signal_w_market: Option<f64>,
    /// Composite skor EMA smoothing α ∈ (0, 1]. 1.0 (default) = smoothing yok.
    /// 0.5 → yumuşak ama yön değişiminde gecikme.
    #[serde(default)]
    pub bonereaper_signal_ema_alpha: Option<f64>,
    /// Profit lock: aktif ise her iki tarafta da fill oluşup imbalance
    /// `bonereaper_profit_lock_imbalance` altına düştüğünde sinyal emirleri durur.
    /// Market sonuna kadar mevcut pozisyon korunur. Default: false.
    #[serde(default)]
    pub bonereaper_profit_lock: Option<bool>,
}

impl StrategyParams {
    pub fn avg_threshold(&self) -> f64 {
        self.profit_lock_pct
            .map(|p| 1.0 - p.abs())
            .unwrap_or(0.98)
    }

    /// RTDS Chainlink task'ını başlatmak için kontrol (sinyal hesabında kullanılmaz).
    pub fn rtds_enabled_or_default(&self) -> bool {
        self.rtds_enabled.unwrap_or(true)
    }

    pub fn open_delta_or_default(&self) -> f64 {
        self.open_delta.unwrap_or(0.01).max(0.0)
    }

    pub fn pyramid_agg_delta_or_default(&self) -> f64 {
        self.pyramid_agg_delta.unwrap_or(0.015).max(0.0)
    }

    pub fn pyramid_fak_delta_or_default(&self) -> f64 {
        self.pyramid_fak_delta.unwrap_or(0.025).max(0.0)
    }

    pub fn pyramid_usdc_or(&self, fallback: f64) -> f64 {
        self.pyramid_usdc.unwrap_or(fallback).max(0.0)
    }

    // === Bonereaper accessors ===
    pub fn bonereaper_signal_taker(&self) -> bool {
        self.bonereaper_signal_taker.unwrap_or(true)
    }
    pub fn bonereaper_profit_lock_imbalance(&self) -> f64 {
        self.bonereaper_profit_lock_imbalance.unwrap_or(50.0).clamp(1.0, 200.0)
    }
    pub fn bonereaper_signal_persistence_k(&self) -> u32 {
        self.bonereaper_signal_persistence_k.unwrap_or(1).clamp(1, 20)
    }
    pub fn bonereaper_signal_w_market(&self) -> f64 {
        self.bonereaper_signal_w_market.unwrap_or(0.7).clamp(0.0, 1.0)
    }
    pub fn bonereaper_signal_ema_alpha(&self) -> f64 {
        self.bonereaper_signal_ema_alpha.unwrap_or(1.0).clamp(0.01, 1.0)
    }
    pub fn bonereaper_profit_lock(&self) -> bool {
        self.bonereaper_profit_lock.unwrap_or(false)
    }
}

/// Elis stratejisi parametreleri — `StrategyParams`'tan resolve edilir.
/// Dutch Book Bid Loop + Gabagool pattern'ları (P2/P4/P5/P6).
#[derive(Debug, Clone, Copy)]
pub struct ElisParams {
    /// Taraf başına temel emir büyüklüğü (share). Önceki loop'ta dolmayan
    /// miktar bu taban üstüne eklenir. Default: 20.0
    pub max_buy_order_size: f64,
    /// Loop süresi: emir gönderme → iptal arası (ms). Default: 2000
    pub trade_cooldown_ms: u64,
    /// Pencere kapanmadan bu kadar saniye önce döngüyü durdur. Default: 30.0
    pub stop_before_end_secs: f64,
    /// P4: Improvement threshold — avg pair cost bu kadar düşmüyorsa emir yok.
    /// Default: 0.005
    pub min_improvement: f64,
    /// P5 Vol filter: bid-ask spread bu eşiği aşarsa OB ince sayılır. Default: 0.05
    pub vol_threshold: f64,
    /// P5 BSI filter: |BSI| bu eşiği aşarsa karşı taraf engellenir. Default: 0.50
    pub bsi_filter_threshold: f64,
    /// P2 Lock threshold: `avg_up + avg_down` bu değerin altına düşünce pozisyon
    /// kilitli sayılır ve yeni emir verilmez. Default: 0.98
    pub lock_threshold: f64,
    /// P6 Stale cleanup: bu süreden daha eski emirler zorla iptal edilir (ms).
    /// Default: 30_000
    pub max_order_age_ms: u64,
}

impl Default for ElisParams {
    fn default() -> Self {
        Self {
            max_buy_order_size: 20.0,
            trade_cooldown_ms: 2000,
            stop_before_end_secs: 30.0,
            min_improvement: 0.005,
            vol_threshold: 0.05,
            bsi_filter_threshold: 0.50,
            lock_threshold: 0.98,
            max_order_age_ms: 30_000,
        }
    }
}

impl ElisParams {
    /// `StrategyParams`'tan opsiyonel override'ları uygular; eksik alanlar default kalır.
    #[inline(always)]
    pub fn from_strategy_params(p: &StrategyParams) -> Self {
        let d = Self::default();
        Self {
            max_buy_order_size: p.elis_max_buy_order_size.unwrap_or(d.max_buy_order_size),
            trade_cooldown_ms: p.elis_trade_cooldown_ms.unwrap_or(d.trade_cooldown_ms),
            stop_before_end_secs: p.elis_stop_before_end_secs.unwrap_or(d.stop_before_end_secs),
            min_improvement: p.elis_min_improvement.unwrap_or(d.min_improvement),
            vol_threshold: p.elis_vol_threshold.unwrap_or(d.vol_threshold),
            bsi_filter_threshold: p.elis_bsi_filter_threshold.unwrap_or(d.bsi_filter_threshold),
            lock_threshold: p.elis_lock_threshold.unwrap_or(d.lock_threshold),
            max_order_age_ms: p.elis_max_order_age_ms.unwrap_or(d.max_order_age_ms),
        }
    }
}
