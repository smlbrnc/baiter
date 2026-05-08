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
    /// P4 Improvement fail cooldown: improvement geçemeyince bu süre boyunca
    /// yeni emir verilmez (ms). Uzun bekleme mevcut maker emirlerin dolmasına
    /// fırsat verir. Simülasyon optimumu: 30_000. Default: 30000
    #[serde(default)]
    pub elis_imp_fail_cooldown_ms: Option<u64>,
    /// Inventory imbalance taker threshold: |up_filled - down_filled| bu eşiği
    /// aşarsa weaker side ASK fiyatından (taker) alınır → anında dengeleme.
    /// Avellaneda-Stoikov inventory skew + cascade exit hibrit yaklaşımı.
    /// Bot 67 simülasyonu: thr=100 → +%57 PnL, 0 zarar. Default: 100
    #[serde(default)]
    pub elis_imbalance_taker_threshold: Option<f64>,
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
    /// PURE FREEZE penceresi (sn). T-X anında UP_bid'den favori belirlenir,
    /// pencere içinde favori sınırı ters yöne geçerse bot yeni signal emir vermez
    /// (mevcut signal emirleri iptal edilir, hedge YOK). 0 = devre dışı.
    /// Default: 45 (akademik test sonucu sweet spot).
    #[serde(default)]
    pub bonereaper_freeze_window_secs: Option<u32>,
    /// PURE FREEZE eşiği — UP_bid'in geçişi flip sayar. Default: 0.5.
    #[serde(default)]
    pub bonereaper_freeze_threshold: Option<f64>,
    /// Signal emri fiyat tavanı [0.50, 0.99]. Bu değerin üzerindeki ask
    /// fiyatlarında sinyal emri verilmez. Aşırı pahalı dominant tarafta
    /// birikim (0.92 DOWN@0.97 gibi) engellenir. Default: 0.92.
    /// Not: Dutch Book emirleri bu filtreden muaftır (arbitraj garantilidir).
    #[serde(default)]
    pub bonereaper_signal_price_ceiling: Option<f64>,

    // === Gravie (Bot 66 davranış kopyası) ===
    /// Karar tick aralığı (sn). Bot 66 ortalama inter-arrival 4-5 sn.
    /// Default: 5.
    #[serde(default)]
    pub gravie_tick_interval_secs: Option<u64>,
    /// Ardışık BUY emirleri arası minimum bekleme (ms). Default: 4000.
    #[serde(default)]
    pub gravie_buy_cooldown_ms: Option<u64>,
    /// Yeni leg açma için ask fiyat tavanı. Bot 66 first entry medyan 0.50,
    /// p75 0.575 — sıkı kalibrasyon. Default: 0.65.
    #[serde(default)]
    pub gravie_entry_ask_ceiling: Option<f64>,
    /// Second-leg guard süresi (ms). İlk leg sonrası karşı tarafa
    /// otomatik geçiş için minimum bekleme. Bot 66 5m median 38 sn.
    /// Default: 38000.
    #[serde(default)]
    pub gravie_second_leg_guard_ms: Option<u64>,
    /// Second-leg karşı taraf fiyat tetikleyicisi — opp_ask bu eşiğin
    /// altına inerse guard beklenmeden flip. Bot 66 opp_first_px ~0.50.
    /// Default: 0.55.
    #[serde(default)]
    pub gravie_second_leg_opp_trigger: Option<f64>,
    /// Kapanışa bu kadar sn kala yeni emir verme. Bot 66 5m median T-78,
    /// %58 ≤ T-90. Default: 90.
    #[serde(default)]
    pub gravie_t_cutoff_secs: Option<f64>,
    /// Balance eşiği — `min/max` bunun altındaysa az tarafa zorunlu rebalance.
    /// Default: 0.30 (sim'de %42 rebalance trade idi; daralt).
    #[serde(default)]
    pub gravie_balance_rebalance: Option<f64>,
    /// Rebalance modunda entry ceiling multiplier (esneme). Default: 1.20.
    #[serde(default)]
    pub gravie_rebalance_ceiling_multiplier: Option<f64>,
    /// Sum-avg guard — `avg_up + avg_dn ≥ X` ise yeni emir verme.
    /// Default: 1.05 (sim'de 1.20 çok geç, sum_avg sürekli >1.0 oluyor).
    #[serde(default)]
    pub gravie_sum_avg_ceiling: Option<f64>,
    /// PATCH A — Lose-side ASK cap (asymmetric trend reversal guard).
    /// `max(up_ask, dn_ask) >= X` ise tüm yeni emirler durur. Bir tarafın
    /// fiyatı bu eşiğin üstüne çıktığında market o tarafın olasılığını
    /// `>= X` görüyor demektir; "ucuz" görünen karşı tarafa daha fazla
    /// pozisyon açmak collapse riskini büyütür. Default: 0.85.
    /// 1.0 = devre dışı.
    #[serde(default)]
    pub gravie_opp_ask_stop_threshold: Option<f64>,
    /// PATCH C — FAK emir başına maksimum share. Düşen fiyatlarda
    /// `ceil(usdc/price)` patlamasını önler (örn. price=0.05 → 200 share).
    /// 0 = sınırsız (devre dışı). Default: 50.
    #[serde(default)]
    pub gravie_max_fak_size: Option<f64>,
}

impl StrategyParams {
    pub fn avg_threshold(&self) -> f64 {
        self.profit_lock_pct.map(|p| 1.0 - p.abs()).unwrap_or(0.98)
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
        self.bonereaper_profit_lock_imbalance
            .unwrap_or(50.0)
            .clamp(1.0, 200.0)
    }
    pub fn bonereaper_signal_persistence_k(&self) -> u32 {
        self.bonereaper_signal_persistence_k
            .unwrap_or(1)
            .clamp(1, 20)
    }
    pub fn bonereaper_signal_w_market(&self) -> f64 {
        self.bonereaper_signal_w_market
            .unwrap_or(0.7)
            .clamp(0.0, 1.0)
    }
    pub fn bonereaper_signal_ema_alpha(&self) -> f64 {
        self.bonereaper_signal_ema_alpha
            .unwrap_or(1.0)
            .clamp(0.01, 1.0)
    }
    pub fn bonereaper_profit_lock(&self) -> bool {
        self.bonereaper_profit_lock.unwrap_or(true)
    }
    /// 0 = devre dışı; 1..=300 sınırlı. Default 45 sn.
    pub fn bonereaper_freeze_window_secs(&self) -> u32 {
        self.bonereaper_freeze_window_secs.unwrap_or(45).min(300)
    }
    /// 0.10..0.90 sınırlı; default 0.50.
    pub fn bonereaper_freeze_threshold(&self) -> f64 {
        self.bonereaper_freeze_threshold
            .unwrap_or(0.5)
            .clamp(0.10, 0.90)
    }
    /// 0.50..0.99 sınırlı; default 0.92.
    pub fn bonereaper_signal_price_ceiling(&self) -> f64 {
        self.bonereaper_signal_price_ceiling
            .unwrap_or(0.92)
            .clamp(0.50, 0.99)
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
    /// P4 Improvement fail cooldown: improvement geçemeyince bu süre NoOp (ms).
    /// Mevcut maker emirlere dolma fırsatı verir. Sim optimumu: 30_000.
    /// Default: 30_000
    pub imp_fail_cooldown_ms: u64,
    /// Inventory imbalance taker threshold: |q| > threshold ise weaker side
    /// ASK'tan alınır (anında dengeleme). 0 = kapalı. Default: 100.0
    pub imbalance_taker_threshold: f64,
}

impl Default for ElisParams {
    fn default() -> Self {
        Self {
            max_buy_order_size: 20.0,
            trade_cooldown_ms: 4000,
            stop_before_end_secs: 30.0,
            min_improvement: 0.005,
            vol_threshold: 0.05,
            bsi_filter_threshold: 0.50,
            lock_threshold: 0.98,
            max_order_age_ms: 30_000,
            imp_fail_cooldown_ms: 30_000,
            imbalance_taker_threshold: 100.0,
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
            stop_before_end_secs: p
                .elis_stop_before_end_secs
                .unwrap_or(d.stop_before_end_secs),
            min_improvement: p.elis_min_improvement.unwrap_or(d.min_improvement),
            vol_threshold: p.elis_vol_threshold.unwrap_or(d.vol_threshold),
            bsi_filter_threshold: p
                .elis_bsi_filter_threshold
                .unwrap_or(d.bsi_filter_threshold),
            lock_threshold: p.elis_lock_threshold.unwrap_or(d.lock_threshold),
            max_order_age_ms: p.elis_max_order_age_ms.unwrap_or(d.max_order_age_ms),
            imp_fail_cooldown_ms: p
                .elis_imp_fail_cooldown_ms
                .unwrap_or(d.imp_fail_cooldown_ms),
            imbalance_taker_threshold: p
                .elis_imbalance_taker_threshold
                .unwrap_or(d.imbalance_taker_threshold),
        }
    }
}

/// Gravie stratejisi parametreleri — `StrategyParams`'tan resolve edilir.
/// Bot 66 (`Lively-Authenticity`) davranış kalibrasyonu; default'lar
/// mikro davranış sondajından (data/bot66_micro_analysis.json) türetilmiştir.
#[derive(Debug, Clone, Copy)]
pub struct GravieParams {
    /// Karar tick aralığı (sn). Bot 66 ortalama inter-arrival 4-5 sn.
    pub tick_interval_secs: u64,
    /// Ardışık BUY emirleri arası minimum bekleme (ms).
    pub buy_cooldown_ms: u64,
    /// Yeni leg açma için ask fiyat tavanı.
    pub entry_ask_ceiling: f64,
    /// Second-leg guard süresi (ms).
    pub second_leg_guard_ms: u64,
    /// Second-leg karşı taraf fiyat tetikleyicisi.
    pub second_leg_opp_trigger: f64,
    /// Kapanışa bu kadar sn kala yeni emir verme.
    pub t_cutoff_secs: f64,
    /// Balance eşiği — bunun altında rebalance.
    pub balance_rebalance: f64,
    /// Rebalance modunda entry ceiling multiplier.
    pub rebalance_ceiling_multiplier: f64,
    /// Sum-avg guard — bu eşiğin üstünde yeni emir verme.
    pub sum_avg_ceiling: f64,
    /// PATCH A — Lose-side ASK cap. max(up_ask, dn_ask) >= X ise yeni emir yok.
    pub opp_ask_stop_threshold: f64,
    /// PATCH C — FAK emir başına max share. 0 = devre dışı.
    pub max_fak_size: f64,
}

impl Default for GravieParams {
    fn default() -> Self {
        Self {
            tick_interval_secs: 5,
            buy_cooldown_ms: 4_000,
            entry_ask_ceiling: 0.65,
            second_leg_guard_ms: 38_000,
            second_leg_opp_trigger: 0.55,
            t_cutoff_secs: 90.0,
            balance_rebalance: 0.30,
            rebalance_ceiling_multiplier: 1.20,
            sum_avg_ceiling: 1.05,
            opp_ask_stop_threshold: 0.85,
            max_fak_size: 50.0,
        }
    }
}

impl GravieParams {
    /// `StrategyParams`'tan opsiyonel override'ları uygular; eksik alanlar default kalır.
    #[inline(always)]
    pub fn from_strategy_params(p: &StrategyParams) -> Self {
        let d = Self::default();
        Self {
            tick_interval_secs: p
                .gravie_tick_interval_secs
                .unwrap_or(d.tick_interval_secs)
                .clamp(1, 60),
            buy_cooldown_ms: p
                .gravie_buy_cooldown_ms
                .unwrap_or(d.buy_cooldown_ms)
                .clamp(500, 60_000),
            entry_ask_ceiling: p
                .gravie_entry_ask_ceiling
                .unwrap_or(d.entry_ask_ceiling)
                .clamp(0.10, 0.99),
            second_leg_guard_ms: p
                .gravie_second_leg_guard_ms
                .unwrap_or(d.second_leg_guard_ms)
                .clamp(0, 600_000),
            second_leg_opp_trigger: p
                .gravie_second_leg_opp_trigger
                .unwrap_or(d.second_leg_opp_trigger)
                .clamp(0.10, 0.95),
            t_cutoff_secs: p
                .gravie_t_cutoff_secs
                .unwrap_or(d.t_cutoff_secs)
                .clamp(0.0, 600.0),
            balance_rebalance: p
                .gravie_balance_rebalance
                .unwrap_or(d.balance_rebalance)
                .clamp(0.0, 1.0),
            rebalance_ceiling_multiplier: p
                .gravie_rebalance_ceiling_multiplier
                .unwrap_or(d.rebalance_ceiling_multiplier)
                .clamp(1.0, 2.0),
            sum_avg_ceiling: p
                .gravie_sum_avg_ceiling
                .unwrap_or(d.sum_avg_ceiling)
                .clamp(0.80, 1.50),
            opp_ask_stop_threshold: p
                .gravie_opp_ask_stop_threshold
                .unwrap_or(d.opp_ask_stop_threshold)
                .clamp(0.50, 1.00),
            max_fak_size: p
                .gravie_max_fak_size
                .unwrap_or(d.max_fak_size)
                .clamp(0.0, 10_000.0),
        }
    }
}
