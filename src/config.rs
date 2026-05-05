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

    // === Elis (Dutch Book Spread Capture) — docs/elis.md §5 ===
    /// Bid-ask spread eşiği (her iki taraf). Default: 0.02
    #[serde(default)]
    pub elis_spread_threshold: Option<f64>,
    /// Taraf başına max emir büyüklüğü (share). Default: 20.0
    #[serde(default)]
    pub elis_max_buy_order_size: Option<f64>,
    /// Emir → iptal arası bekleme (ms). Default: 5000
    #[serde(default)]
    pub elis_trade_cooldown_ms: Option<u64>,
    /// Pozisyon dengeleme agresifliği (0=pasif, 1=max). Default: 0.7
    #[serde(default)]
    pub elis_balance_factor: Option<f64>,
    /// Pencere kapanmadan bu kadar saniye önce dur. Default: 60.0
    #[serde(default)]
    pub elis_stop_before_end_secs: Option<f64>,

    // === Bonereaper parametreleri ===
    /// BSI mutlak değer eşiği — yön kararı için primer sinyal. Default: 0.30
    #[serde(default)]
    pub bonereaper_bsi_threshold: Option<f64>,
    /// Scoop tetikleyici — karşı tarafın ask fiyatı bu eşiğin altında ise scoop. Default: 0.25
    #[serde(default)]
    pub bonereaper_scoop_threshold: Option<f64>,
    /// Lottery tail emri aktif mi? Default: false (yüksek risk — opt-in)
    #[serde(default)]
    pub bonereaper_lottery_enabled: Option<bool>,
    /// Signal emirlerinde dominant taraf (bid > 0.50) için taker (ask) kullanılsın mı?
    /// Default: true (live'da anında fill).
    #[serde(default)]
    pub bonereaper_signal_taker: Option<bool>,
    /// Rebalance emirlerinde dominant taraf (bid > 0.50) için taker (ask) kullanılsın mı?
    /// Default: true (kritik imbalance düzeltmesinde anında fill).
    #[serde(default)]
    pub bonereaper_rebalance_taker: Option<bool>,
    /// Rebalance tetiklenme eşiği (share). Bu kadar imbalance oluşunca devreye girer.
    /// Default 50.0 — 24 market grid search optimum; rebalance signal'a karşı çalıştığı
    /// için yüksek trigger (=daha az tetik) daha iyi PnL veriyor (50: +$628, 20: +$513).
    #[serde(default)]
    pub bonereaper_rebalance_trigger: Option<f64>,
    /// Signal güçlü iken (|effective_score - 5| > 2.5) rebalance pasif mi?
    /// `false` → pasif (default, kayıp önler), `true` → her zaman aktif (eski davranış).
    #[serde(default)]
    pub bonereaper_rebalance_when_signal_strong: Option<bool>,
    /// Signal yön onayı için kaç ardışık tick gerekli? K=1 mevcut anlık karar.
    /// K=2 (default) → yeni yön için 2 ardışık tick onayı; flip-flop'u azaltır.
    #[serde(default)]
    pub bonereaper_signal_persistence_k: Option<u32>,
    /// Convergence guard sliding window (tick sayısı). Bu kadar tick içinde herhangi
    /// bir tick conv idiyse guard aktif. N=1 → mevcut anlık kontrol; N=5 (default)
    /// conv intermittent durumlarda guard'ı stabil tutar.
    #[serde(default)]
    pub bonereaper_conv_guard_window: Option<u32>,
    /// Polymarket UP_bid sinyalinin composite içindeki ağırlığı [0, 1].
    /// Yön kararı: `signal × (1-w) + market × w`. 0 = sadece Binance/OKX (eski);
    /// 0.7 (default) = Polymarket dominant — 82 market analizinde %55→%76 doğruluk.
    #[serde(default)]
    pub bonereaper_signal_w_market: Option<f64>,
    /// Composite skor EMA smoothing α ∈ (0, 1]. 1.0 (default) = smoothing yok
    /// — 24 market grid search'te en yüksek PnL veren değer (persistence K zaten
    /// gürültü filtreliyor, EMA üst üste fazla → lag yaratıp kayıp). 0.10-0.30
    /// arası daha pürüzsüz ama yön değişiminde geç kalır.
    #[serde(default)]
    pub bonereaper_signal_ema_alpha: Option<f64>,
    /// Profit lock: aktif ise her iki tarafta da fill oluşup imbalance rebalance
    /// trigger'ın altına düştüğünde sinyal ve rebalance emirleri durur.
    /// Market sonuna kadar mevcut pozisyon korunur, yeni emir verilmez.
    /// Default: false (devre dışı).
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
    pub fn bonereaper_bsi_threshold(&self) -> f64 {
        self.bonereaper_bsi_threshold.unwrap_or(0.30).clamp(0.05, 2.0)
    }
    pub fn bonereaper_scoop_threshold(&self) -> f64 {
        self.bonereaper_scoop_threshold.unwrap_or(0.25).clamp(0.05, 0.50)
    }
    pub fn bonereaper_lottery_enabled(&self) -> bool {
        self.bonereaper_lottery_enabled.unwrap_or(false)
    }
    pub fn bonereaper_signal_taker(&self) -> bool {
        self.bonereaper_signal_taker.unwrap_or(true)
    }
    pub fn bonereaper_rebalance_taker(&self) -> bool {
        self.bonereaper_rebalance_taker.unwrap_or(true)
    }
    pub fn bonereaper_rebalance_trigger(&self) -> f64 {
        self.bonereaper_rebalance_trigger.unwrap_or(50.0).clamp(1.0, 200.0)
    }
    pub fn bonereaper_rebalance_when_signal_strong(&self) -> bool {
        self.bonereaper_rebalance_when_signal_strong.unwrap_or(false)
    }
    pub fn bonereaper_signal_persistence_k(&self) -> u32 {
        self.bonereaper_signal_persistence_k.unwrap_or(2).clamp(1, 20)
    }
    pub fn bonereaper_conv_guard_window(&self) -> u32 {
        self.bonereaper_conv_guard_window.unwrap_or(5).clamp(1, 60)
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
/// Dutch Book spread capture parametreleri.
///
/// Doküman: `docs/elis.md` §5 (Konfigürasyon Parametreleri).
#[derive(Debug, Clone, Copy)]
pub struct ElisParams {
    /// Her iki tarafta bid-ask spread'in geçmesi gereken minimum eşik.
    /// Dokümandan: `spread_threshold = 0.02`
    pub spread_threshold: f64,
    /// Taraf başına maksimum emir büyüklüğü (share). Balance factor öncesi taban.
    /// Dokümandan: `max_buy_order_size = 20`
    pub max_buy_order_size: f64,
    /// Emir gönderme → iptal arası bekleme süresi (ms).
    /// Dokümandan: `trade_cooldown = 5000`
    pub trade_cooldown_ms: u64,
    /// Pozisyon dengeleme agresifliği (0.0 = pasif, 1.0 = maksimum).
    /// Dokümandan: `balance_factor = 0.7`
    pub balance_factor: f64,
    /// Pencere kapanmadan bu kadar saniye önce işlemleri durdur.
    /// Dokümandan: `stop_before_end_ms = 60000` → 60.0s
    pub stop_before_end_secs: f64,
}

impl Default for ElisParams {
    fn default() -> Self {
        Self {
            spread_threshold: 0.02,
            max_buy_order_size: 20.0,
            trade_cooldown_ms: 5000,
            balance_factor: 0.7,
            stop_before_end_secs: 60.0,
        }
    }
}

impl ElisParams {
    /// `StrategyParams`'tan opsiyonel override'ları uygular; eksik alanlar default kalır.
    #[inline(always)]
    pub fn from_strategy_params(p: &StrategyParams) -> Self {
        let d = Self::default();
        Self {
            spread_threshold: p.elis_spread_threshold.unwrap_or(d.spread_threshold),
            max_buy_order_size: p
                .elis_max_buy_order_size
                .unwrap_or(d.max_buy_order_size),
            trade_cooldown_ms: p.elis_trade_cooldown_ms.unwrap_or(d.trade_cooldown_ms),
            balance_factor: p.elis_balance_factor.unwrap_or(d.balance_factor),
            stop_before_end_secs: p
                .elis_stop_before_end_secs
                .unwrap_or(d.stop_before_end_secs),
        }
    }
}
