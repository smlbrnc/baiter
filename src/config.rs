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
/// parse edilir, tüm stratejiler (Alis/Elis/Aras) buradan okur.
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

    // === Elis-spesifik (16 marketde optimize) ===
    #[serde(default)]
    pub elis_pre_opener_ticks: Option<usize>,
    /// Açılımın erken tetiklenme önlemi: ilk tick'ten bu kadar saniye geçmeden opener ateşlenmez.
    /// BBA tick'leri 1-3/sn gelir; 20 tick ~9s ediyor — bu guard ~20s garantiler.
    #[serde(default)]
    pub elis_opener_min_secs: Option<f64>,
    #[serde(default)]
    pub elis_bsi_rev_threshold: Option<f64>,
    #[serde(default)]
    /// Rule 1.5: |down_bid - up_bid| ≥ bu eşik → piyasa fiyatını takip et (BSI'dan sonra çalışır).
    pub elis_price_anchor_threshold: Option<f64>,
    #[serde(default)]
    pub elis_ofi_exhaustion_threshold: Option<f64>,
    #[serde(default)]
    pub elis_cvd_exhaustion_threshold: Option<f64>,
    #[serde(default)]
    pub elis_ofi_directional_threshold: Option<f64>,
    #[serde(default)]
    pub elis_dscore_strong_threshold: Option<f64>,
    #[serde(default)]
    pub elis_score_neutral: Option<f64>,
    #[serde(default)]
    pub elis_signal_flip_threshold: Option<f64>,
    #[serde(default)]
    pub elis_signal_flip_max_count: Option<u32>,
    #[serde(default)]
    pub elis_flip_freeze_opp_secs: Option<f64>,
    #[serde(default)]
    pub elis_open_usdc_dom: Option<f64>,
    #[serde(default)]
    pub elis_open_usdc_hedge: Option<f64>,
    #[serde(default)]
    pub elis_order_usdc_dom: Option<f64>,
    #[serde(default)]
    pub elis_order_usdc_hedge: Option<f64>,
    #[serde(default)]
    pub elis_pyramid_usdc: Option<f64>,
    #[serde(default)]
    pub elis_scoop_usdc: Option<f64>,
    #[serde(default)]
    pub elis_requote_price_eps: Option<f64>,
    #[serde(default)]
    pub elis_requote_cooldown_secs: Option<f64>,
    #[serde(default)]
    pub elis_avg_down_min_edge: Option<f64>,
    #[serde(default)]
    pub elis_pyramid_ofi_min: Option<f64>,
    #[serde(default)]
    pub elis_pyramid_score_persist_secs: Option<f64>,
    #[serde(default)]
    pub elis_pyramid_cooldown_secs: Option<f64>,
    #[serde(default)]
    pub elis_parity_min_gap_qty: Option<f64>,
    #[serde(default)]
    pub elis_parity_cooldown_secs: Option<f64>,
    #[serde(default)]
    pub elis_parity_opp_bid_min: Option<f64>,
    #[serde(default)]
    pub elis_lock_avg_threshold: Option<f64>,
    /// DOM fiyatı bu eşiğin altına düşünce pozisyon büyütme (hard stop).
    /// Yanlış yönde açılan marketlerde avg-down + requote sarmalını keser.
    #[serde(default)]
    pub elis_hard_stop_dom_bid_min: Option<f64>,
    #[serde(default)]
    pub elis_scoop_opp_bid_max: Option<f64>,
    #[serde(default)]
    pub elis_scoop_min_remaining_secs: Option<f64>,
    #[serde(default)]
    pub elis_scoop_cooldown_secs: Option<f64>,
    #[serde(default)]
    pub elis_deadline_safety_secs: Option<f64>,
}

impl StrategyParams {
    pub fn avg_threshold(&self) -> f64 {
        self.profit_lock_pct
            .map(|p| 1.0 - p.abs())
            .unwrap_or(0.98)
    }

    pub fn rtds_enabled_or_default(&self) -> bool {
        self.rtds_enabled.unwrap_or(true)
    }

    pub fn window_delta_weight_or_default(&self) -> f64 {
        self.window_delta_weight.unwrap_or(0.70).clamp(0.0, 1.0)
    }

    pub fn signal_lookahead_secs_or_default(&self) -> f64 {
        self.signal_lookahead_secs.unwrap_or(3.0).clamp(0.0, 30.0)
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
}

/// Elis stratejisi parametreleri — `StrategyParams`'tan resolve edilir.
/// Default değerler **v4b** 24-market combined backtestle optimize edildi
/// (yön %85, kesin PnL +$862; bkz. `exports/backtest-final-24-markets.md`).
///
/// v3 → v4b kritik değişiklikler:
///   - `requote_price_eps`: 0.02 → 0.04 (spam %50 azaltıcı, en kritik fix)
///   - `bsi_rev_threshold`: 2.0 → 1.5 (bsi_rev daha agresif)
///   - `dscore_strong_threshold`: 1.0 → 1.5 (momentum daha kati)
///   - `ofi_directional_threshold`: 0.4 → 0.3 (ofi_dir daha agresif)
#[derive(Debug, Clone, Copy)]
pub struct ElisParams {
    pub pre_opener_ticks: usize,
    /// Minimum süre (saniye) — opener bu süreden önce ateşlenmez (BBA spam koruması).
    pub opener_min_secs: f64,
    pub bsi_rev_threshold: f64,
    /// Rule 1.5: |down_bid - up_bid| ≥ bu eşik → fiyat yönünü seç (BSI'dan sonra çalışır).
    pub price_anchor_threshold: f64,
    pub ofi_exhaustion_threshold: f64,
    pub cvd_exhaustion_threshold: f64,
    pub ofi_directional_threshold: f64,
    pub dscore_strong_threshold: f64,
    pub score_neutral: f64,
    pub signal_flip_threshold: f64,
    pub signal_flip_max_count: u32,
    pub flip_freeze_opp_secs: f64,
    pub open_usdc_dom: f64,
    pub open_usdc_hedge: f64,
    pub order_usdc_dom: f64,
    pub order_usdc_hedge: f64,
    pub pyramid_usdc: f64,
    pub scoop_usdc: f64,
    pub requote_price_eps: f64,
    pub requote_cooldown_secs: f64,
    pub avg_down_min_edge: f64,
    pub pyramid_ofi_min: f64,
    pub pyramid_score_persist_secs: f64,
    pub pyramid_cooldown_secs: f64,
    pub parity_min_gap_qty: f64,
    pub parity_cooldown_secs: f64,
    pub parity_opp_bid_min: f64,
    pub lock_avg_threshold: f64,
    /// DOM fiyatı bu eşiğin altına düşünce pozisyon büyütme (hard stop).
    pub hard_stop_dom_bid_min: f64,
    pub scoop_opp_bid_max: f64,
    pub scoop_min_remaining_secs: f64,
    pub scoop_cooldown_secs: f64,
    pub deadline_safety_secs: f64,
}

impl Default for ElisParams {
    fn default() -> Self {
        Self {
            pre_opener_ticks: 20,
            opener_min_secs: 20.0,  // BBA spam koruması: t=0'dan en az 20s bekle
            bsi_rev_threshold: 1.5,        // v4b: 2.0→1.5
            price_anchor_threshold: 0.20,  // |down_bid - up_bid| ≥ 0.20 → piyasa yönü (BSI sonrası)
            ofi_exhaustion_threshold: 0.4,
            cvd_exhaustion_threshold: 3.0,
            ofi_directional_threshold: 0.3, // v4b: 0.4→0.3
            dscore_strong_threshold: 1.5,   // v4b: 1.0→1.5
            score_neutral: 5.0,
            signal_flip_threshold: 5.0,
            signal_flip_max_count: 1,
            flip_freeze_opp_secs: 60.0,
            open_usdc_dom: 25.0,
            open_usdc_hedge: 12.0,
            order_usdc_dom: 15.0,
            order_usdc_hedge: 8.0,
            pyramid_usdc: 30.0,
            scoop_usdc: 50.0,
            requote_price_eps: 0.04,        // v4b: 0.02→0.04 (en kritik fix)
            requote_cooldown_secs: 3.0,
            avg_down_min_edge: 0.023,
            pyramid_ofi_min: 0.83,
            pyramid_score_persist_secs: 5.0,
            pyramid_cooldown_secs: 3.0,
            parity_min_gap_qty: 250.0,
            parity_cooldown_secs: 5.0,
            parity_opp_bid_min: 0.15,
            lock_avg_threshold: 0.97,
            hard_stop_dom_bid_min: 0.25, // DOM 0.25 altına düşünce alım durdur
            scoop_opp_bid_max: 0.05,
            scoop_min_remaining_secs: 35.0,
            scoop_cooldown_secs: 2.0,
            deadline_safety_secs: 8.0,
        }
    }
}

impl ElisParams {
    /// `StrategyParams`'tan opsiyonel override'ları uygular; eksik alanlar default kalır.
    pub fn from_strategy_params(p: &StrategyParams) -> Self {
        let d = Self::default();
        Self {
            pre_opener_ticks: p.elis_pre_opener_ticks.unwrap_or(d.pre_opener_ticks),
            opener_min_secs: p.elis_opener_min_secs.unwrap_or(d.opener_min_secs),
            bsi_rev_threshold: p.elis_bsi_rev_threshold.unwrap_or(d.bsi_rev_threshold),
            price_anchor_threshold: p
                .elis_price_anchor_threshold
                .unwrap_or(d.price_anchor_threshold),
            ofi_exhaustion_threshold: p
                .elis_ofi_exhaustion_threshold
                .unwrap_or(d.ofi_exhaustion_threshold),
            cvd_exhaustion_threshold: p
                .elis_cvd_exhaustion_threshold
                .unwrap_or(d.cvd_exhaustion_threshold),
            ofi_directional_threshold: p
                .elis_ofi_directional_threshold
                .unwrap_or(d.ofi_directional_threshold),
            dscore_strong_threshold: p
                .elis_dscore_strong_threshold
                .unwrap_or(d.dscore_strong_threshold),
            score_neutral: p.elis_score_neutral.unwrap_or(d.score_neutral),
            signal_flip_threshold: p
                .elis_signal_flip_threshold
                .unwrap_or(d.signal_flip_threshold),
            signal_flip_max_count: p
                .elis_signal_flip_max_count
                .unwrap_or(d.signal_flip_max_count),
            flip_freeze_opp_secs: p
                .elis_flip_freeze_opp_secs
                .unwrap_or(d.flip_freeze_opp_secs),
            open_usdc_dom: p.elis_open_usdc_dom.unwrap_or(d.open_usdc_dom),
            open_usdc_hedge: p.elis_open_usdc_hedge.unwrap_or(d.open_usdc_hedge),
            order_usdc_dom: p.elis_order_usdc_dom.unwrap_or(d.order_usdc_dom),
            order_usdc_hedge: p.elis_order_usdc_hedge.unwrap_or(d.order_usdc_hedge),
            pyramid_usdc: p.elis_pyramid_usdc.unwrap_or(d.pyramid_usdc),
            scoop_usdc: p.elis_scoop_usdc.unwrap_or(d.scoop_usdc),
            requote_price_eps: p.elis_requote_price_eps.unwrap_or(d.requote_price_eps),
            requote_cooldown_secs: p
                .elis_requote_cooldown_secs
                .unwrap_or(d.requote_cooldown_secs),
            avg_down_min_edge: p.elis_avg_down_min_edge.unwrap_or(d.avg_down_min_edge),
            pyramid_ofi_min: p.elis_pyramid_ofi_min.unwrap_or(d.pyramid_ofi_min),
            pyramid_score_persist_secs: p
                .elis_pyramid_score_persist_secs
                .unwrap_or(d.pyramid_score_persist_secs),
            pyramid_cooldown_secs: p
                .elis_pyramid_cooldown_secs
                .unwrap_or(d.pyramid_cooldown_secs),
            parity_min_gap_qty: p.elis_parity_min_gap_qty.unwrap_or(d.parity_min_gap_qty),
            parity_cooldown_secs: p
                .elis_parity_cooldown_secs
                .unwrap_or(d.parity_cooldown_secs),
            parity_opp_bid_min: p.elis_parity_opp_bid_min.unwrap_or(d.parity_opp_bid_min),
            lock_avg_threshold: p.elis_lock_avg_threshold.unwrap_or(d.lock_avg_threshold),
            hard_stop_dom_bid_min: p
                .elis_hard_stop_dom_bid_min
                .unwrap_or(d.hard_stop_dom_bid_min),
            scoop_opp_bid_max: p.elis_scoop_opp_bid_max.unwrap_or(d.scoop_opp_bid_max),
            scoop_min_remaining_secs: p
                .elis_scoop_min_remaining_secs
                .unwrap_or(d.scoop_min_remaining_secs),
            scoop_cooldown_secs: p
                .elis_scoop_cooldown_secs
                .unwrap_or(d.scoop_cooldown_secs),
            deadline_safety_secs: p
                .elis_deadline_safety_secs
                .unwrap_or(d.deadline_safety_secs),
        }
    }
}
