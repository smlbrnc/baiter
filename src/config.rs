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
/// parse edilir.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyParams {
    #[serde(default)]
    pub rtds_enabled: Option<bool>,
    #[serde(default)]
    pub window_delta_weight: Option<f64>,
    #[serde(default)]
    pub signal_lookahead_secs: Option<f64>,

    // === Bonereaper parametreleri ===
    // Strateji: Polymarket "Bonereaper" wallet (0xeebde7a0...) davranış kopyası.
    // Order-book reactive martingale + late winner injection. Sinyal kullanmaz.

    // ── Genel akış ───────────────────────────────────────────────────────
    /// Ardışık BUY emirleri arası minimum bekleme (ms). 500–60_000 sınırlı.
    /// Default 2_000.
    #[serde(default)]
    pub bonereaper_buy_cooldown_ms: Option<u64>,
    /// avg_sum yumuşak cap. `new_avg + opp_avg > X` ise yeni alım yok
    /// (scalp/LW muaf). 0.50–2.00 sınırlı. Default 1.00.
    #[serde(default)]
    pub bonereaper_max_avg_sum: Option<f64>,
    /// İlk emir için minimum |up_bid - down_bid| spread eşiği. Bu eşik
    /// aşılana kadar BUY atılmaz; aşılınca ilk emir yüksek bid tarafına verilir.
    /// 0.0 = devre dışı. Default 0.02.
    #[serde(default)]
    pub bonereaper_first_spread_min: Option<f64>,
    /// |up_filled − down_filled| bu eşiği aşarsa zayıf yöne rebalance.
    /// 9999 = dinamik mod (N(t) × est_trade_size, zaman bazlı).
    #[serde(default)]
    pub bonereaper_imbalance_thr: Option<f64>,

    // ── Late winner injection ────────────────────────────────────────────
    /// LW penceresi (sn). T ≤ X anında winner tarafa taker BUY. 0 = KAPALI.
    /// Default 300.
    #[serde(default)]
    pub bonereaper_late_winner_secs: Option<u32>,
    /// LW için winner bid eşiği. 0.50–0.99 sınırlı. Default 0.85.
    #[serde(default)]
    pub bonereaper_late_winner_bid_thr: Option<f64>,
    /// LW USDC notional (arb_mult öncesi base). 0 = KAPALI.
    /// Default `1 × order_usdc`. arb_mult lineer 5×@lw_thr → 10×@0.99.
    #[serde(default)]
    pub bonereaper_late_winner_usdc: Option<f64>,
    /// Session başına maksimum LW injection. 0 = sınırsız. Default 30.
    #[serde(default)]
    pub bonereaper_lw_max_per_session: Option<u32>,
    /// LW shot'ları arası minimum bekleme (ms). 0 = normal buy_cooldown.
    /// Default 10_000.
    #[serde(default)]
    pub bonereaper_lw_cooldown_ms: Option<u64>,

    // ── Sizing ───────────────────────────────────────────────────────────
    /// Longshot anchor (bid ≤ 0.30, sabit) — piecewise lineer interp.
    /// 5m bot için ~10, 15m bot için ~3 (DB override).
    #[serde(default)]
    pub bonereaper_size_longshot_usdc: Option<f64>,
    /// Mid anchor (bid = 0.65) — lineer interp 0.30 → 0.65.
    /// Default `2.5 × order_usdc`. 5m bot ~25, 15m bot ~7.
    #[serde(default)]
    pub bonereaper_size_mid_usdc: Option<f64>,
    /// High anchor (bid = lw_thr) — lineer interp 0.65 → lw_thr.
    /// `bid ≥ lw_thr` ise LW akışı devralır (high fallback).
    /// Default `8 × order_usdc`. 5m bot ~80, 15m bot ~20.
    #[serde(default)]
    pub bonereaper_size_high_usdc: Option<f64>,
    /// Sabit share modu (15m bot için optimal). > 0 ise interp_usdc BYPASS,
    /// her normal alım `shares × ask` USDC harcar. 0 = interp aktif (5m bot).
    /// Gerçek bot 15m markette her trade'de 10 share atıyor.
    #[serde(default)]
    pub bonereaper_size_shares_const: Option<f64>,
    /// Spread-aware sizing — dar spread eşiği. `|up_bid − dn_bid| < X` iken
    /// `shares_lo` lot size kullanılır. 0 = devre dışı (sabit shares_const).
    /// Analiz (12k emir): spread<0.15 → gerçek bot P75=32sh.
    #[serde(default)]
    pub bonereaper_spread_lo_thr: Option<f64>,
    /// Spread-aware sizing — orta spread üst eşiği. `spread_lo ≤ spread < X`
    /// iken `shares_mid` kullanılır. Default 0.50.
    #[serde(default)]
    pub bonereaper_spread_hi_thr: Option<f64>,
    /// Dar spread (<spread_lo) için lot size. Default 40. 0 = devre dışı.
    /// Analiz: spread<0.15 markette P75=32sh, en yakın 40sh.
    #[serde(default)]
    pub bonereaper_size_shares_lo: Option<f64>,
    /// Orta spread (spread_lo–spread_hi) için lot size. Default 25.
    /// Analiz: spread 0.15-0.50 → P75=40sh; 25sh P50'ye yakın.
    #[serde(default)]
    pub bonereaper_size_shares_mid: Option<f64>,
    /// Yön seçim winner-bias eşiği. spread > X iken Δbid yerine her zaman
    /// winner yönüne taker (yüksek bidli taraf). 0 = devre dışı (Δbid mantığı).
    /// 6 market simülasyonu: 0.30 eşiği PnL korurken bant uyumunu artırıyor.
    #[serde(default)]
    pub bonereaper_winner_bias_spread_thr: Option<f64>,
    /// Loser fırsat alımı (çift emir) tetik eşiği. spread ≥ X iken her karar
    /// döngüsünde winner emrinin yanında loser yönüne ek küçük lot eklenir.
    /// 0 = devre dışı (tek emir). Gerçek bot 0.20-0.30 loser fırsatlarını
    /// kaçırmamak için.
    #[serde(default)]
    pub bonereaper_force_both_spread_thr: Option<f64>,
    /// Loser fırsat alımı için sabit lot size. Default 8sh. 0 = devre dışı.
    #[serde(default)]
    pub bonereaper_force_both_loser_shares: Option<f64>,

    // ── Loser scalp ──────────────────────────────────────────────────────
    /// Loser tarafı için minimum bid eşiği (cheap scalp). Default 0.01.
    #[serde(default)]
    pub bonereaper_loser_min_price: Option<f64>,
    /// Loser scalp USDC notional. Default `0.5 × order_usdc`. 0 = KAPALI.
    #[serde(default)]
    pub bonereaper_loser_scalp_usdc: Option<f64>,
    /// Loser scalp üst bid eşiği. Bid bu eşiğin altındaysa scalp boyutu
    /// uygulanır. 0.05–0.50 sınırlı. Default 0.30.
    #[serde(default)]
    pub bonereaper_loser_scalp_max_price: Option<f64>,
    /// Loser tarafta avg fiyatı bu eşiği aşarsa o yöne sadece minimal scalp.
    /// Pahalı martingale-down birikimini engeller. Default 0.50.
    #[serde(default)]
    pub bonereaper_avg_loser_max: Option<f64>,

    // ── Late pyramid (winner size factor) ────────────────────────────────
    /// T ≤ X sn'den itibaren winner tarafına size çarpanı. 0 = KAPALI.
    #[serde(default)]
    pub bonereaper_late_pyramid_secs: Option<u32>,
    /// Late pyramid penceresinde winner size çarpanı. 1.0–10.0 sınırlı.
    /// Default 2.0.
    #[serde(default)]
    pub bonereaper_winner_size_factor: Option<f64>,

    // === Gravie (Dual-Balance Accumulator) ===
    // Amaç: her markette UP shares == DOWN shares ve avg_up + avg_down < X.
    // Bu iki koşul sağlandığında hangi sonuç gelirse gelsin garantili kâr:
    //   profit = N × (1 − avg_sum). Yön sinyali GEREKMEZ.
    /// Ardışık BUY emirleri arası minimum bekleme (ms). Default: 2000.
    #[serde(default)]
    pub gravie_buy_cooldown_ms: Option<u64>,
    /// `avg_up + avg_down` üst sınırı. Yeni BUY bu eşiği geçerse atlanır.
    /// 1.0 altında her durumda pozitif kâr; default 0.95 (%5 brut marj).
    #[serde(default)]
    pub gravie_avg_sum_max: Option<f64>,
    /// Her iki taraf için max alım fiyatı (ask). Üstünde BUY açılmaz.
    /// Default: 0.99 (size_multiplier kademeli alımı için tüm bantlar açık).
    #[serde(default)]
    pub gravie_max_ask: Option<f64>,
    /// Kapanışa bu kadar sn kala yeni emir verilmez. Default: 30.
    #[serde(default)]
    pub gravie_t_cutoff_secs: Option<f64>,
    /// FAK başına maksimum share (düşük fiyatta size patlamasını sınırlar).
    /// 0 = sınırsız. Default: 50.
    #[serde(default)]
    pub gravie_max_fak_size: Option<f64>,
    /// Rebalance tetik eşiği. `|up_filled − down_filled| > X` ise zayıf
    /// tarafa zorunlu BUY (yön seçimi bypass). Default: 5 share.
    #[serde(default)]
    pub gravie_imb_thr: Option<f64>,
    /// İlk emir için minimum winner bid eşiği. `max(up_bid, dn_bid) >= X`
    /// koşulu sağlanana kadar BUY yapılmaz. İlk alım o yüksek-bid tarafına
    /// (winner momentum) yapılır. Default: 0.65.
    #[serde(default)]
    pub gravie_first_bid_min: Option<f64>,
    /// Loser-scalp bypass eşiği. `ask <= X` ise avg_sum gate atlanır.
    /// Ucuz taraftan share toplarken avg_sum artmaz → Bonereaper loser-scalp
    /// mantığı. Default: 0.30 (Bonereaper scalp band sınırı). 0.0 = bypass kapalı.
    #[serde(default)]
    pub gravie_loser_bypass_ask: Option<f64>,
    /// Late Winner injection tetikleme eşiği. `max(up_bid, dn_bid) >= X`
    /// olduğunda kazanan tarafa büyük taker BUY atılır. Default: 0.88
    /// (Bonereaper ile aynı).
    #[serde(default)]
    pub gravie_lw_bid_thr: Option<f64>,
    /// Late Winner USDC notional çarpanı (`order_usdc × X × lw_mult`).
    /// Default: 2.0 (Bonereaper `2× order_usdc` ile aynı).
    #[serde(default)]
    pub gravie_lw_usdc_factor: Option<f64>,
    /// Session başına maksimum Late Winner injection sayısı.
    /// Default: 30 (Bonereaper ile aynı). 0 = sınırsız.
    #[serde(default)]
    pub gravie_lw_max_per_session: Option<u32>,
    /// Loser tarafın `avg` fiyatı bu eşiği aşarsa o tarafa yeni BUY yapılmaz
    /// (pahalı martingale-down koruması). Default: 0.50 (Bonereaper ile aynı).
    #[serde(default)]
    pub gravie_avg_loser_max: Option<f64>,
    /// Loser-scalp boyut çarpanı. `ask <= loser_bypass_ask` durumunda
    /// `size = ceil(order_usdc × X / ask)` ile sabit küçük alım yapılır
    /// (size_multiplier yerine). Default: 0.5 (Bonereaper ile aynı).
    /// 0 = scalp KAPALI; bu durumda bypass aktif olsa bile size_multiplier
    /// kullanılır (eski davranış).
    #[serde(default)]
    pub gravie_loser_scalp_usdc_factor: Option<f64>,
}

impl StrategyParams {
    /// RTDS Chainlink task'ını başlatmak için kontrol (sinyal hesabında kullanılmaz).
    pub fn rtds_enabled_or_default(&self) -> bool {
        self.rtds_enabled.unwrap_or(true)
    }

    // === Bonereaper accessors ===

    /// Ardışık BUY arası min bekleme (ms); 500–60_000 sınırlı; default 2_000.
    pub fn bonereaper_buy_cooldown_ms(&self) -> u64 {
        self.bonereaper_buy_cooldown_ms
            .unwrap_or(2_000)
            .clamp(500, 60_000)
    }
    /// avg_sum yumuşak cap (`new_avg + opp_avg`); 0.50–2.00 sınırlı; default 1.00.
    pub fn bonereaper_max_avg_sum(&self) -> f64 {
        self.bonereaper_max_avg_sum
            .unwrap_or(1.00)
            .clamp(0.50, 2.00)
    }
    /// İlk emir spread eşiği; 0.00–0.20 sınırlı; default 0.02. 0 = devre dışı.
    pub fn bonereaper_first_spread_min(&self) -> f64 {
        self.bonereaper_first_spread_min
            .unwrap_or(0.02)
            .clamp(0.00, 0.20)
    }
    /// Imbalance eşiği (share); 0–10_000 sınırlı; default 9999 → dinamik mod
    /// (N(t) × est_trade_size). Param < 500 ise sabit override.
    pub fn bonereaper_imbalance_thr(&self) -> f64 {
        self.bonereaper_imbalance_thr
            .unwrap_or(9999.0)
            .clamp(0.0, 10_000.0)
    }

    /// LW penceresi (sn); 0–300 sınırlı; default 300. 0 = KAPALI.
    pub fn bonereaper_late_winner_secs(&self) -> u32 {
        self.bonereaper_late_winner_secs.unwrap_or(300).min(300)
    }
    /// LW winner bid eşiği; 0.50–0.99 sınırlı; default 0.85.
    /// arb_mult lineer: lw_thr'de 5×, 0.99'da 10×.
    pub fn bonereaper_late_winner_bid_thr(&self) -> f64 {
        self.bonereaper_late_winner_bid_thr
            .unwrap_or(0.85)
            .clamp(0.50, 0.99)
    }
    /// LW USDC notional (base, arb_mult öncesi); 0–10_000 sınırlı.
    /// Default: `1 × order_usdc`. 0 = KAPALI.
    pub fn bonereaper_late_winner_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_late_winner_usdc
            .unwrap_or(order_usdc)
            .clamp(0.0, 10_000.0)
    }
    /// Session başına max LW injection; 0–50 sınırlı; default 30. 0 = sınırsız.
    pub fn bonereaper_lw_max_per_session(&self) -> u32 {
        self.bonereaper_lw_max_per_session.unwrap_or(30).min(50)
    }
    /// LW shot'ları arası min bekleme (ms); 0–60_000 sınırlı; default 10_000.
    /// 0 = normal buy_cooldown_ms kullanılır.
    pub fn bonereaper_lw_cooldown_ms(&self) -> u64 {
        self.bonereaper_lw_cooldown_ms.unwrap_or(10_000).min(60_000)
    }

    /// Longshot anchor (bid ≤ 0.30, sabit); 0–10_000 sınırlı; default 10.
    pub fn bonereaper_size_longshot_usdc(&self) -> f64 {
        self.bonereaper_size_longshot_usdc
            .unwrap_or(10.0)
            .clamp(0.0, 10_000.0)
    }
    /// Mid anchor (bid = 0.65); 0–10_000 sınırlı.
    /// Default: `2.5 × order_usdc` (=$25 @ order=10).
    pub fn bonereaper_size_mid_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_size_mid_usdc
            .unwrap_or(2.5 * order_usdc)
            .clamp(0.0, 10_000.0)
    }
    /// High anchor (bid = lw_thr); 0–10_000 sınırlı.
    /// Default: `8 × order_usdc` (=$80 @ order=10). `bid ≥ lw_thr` ise LW devralır.
    pub fn bonereaper_size_high_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_size_high_usdc
            .unwrap_or(8.0 * order_usdc)
            .clamp(0.0, 10_000.0)
    }
    /// Piecewise lineer USDC sizing — anchor: longshot@0.30, mid@0.65, high@lw_thr.
    /// `bid ≤ 0.30` → longshot; `0.30→0.65` lineer; `0.65→lw_thr` lineer;
    /// `bid ≥ lw_thr` → high (LW akışı kontrol eder).
    pub fn bonereaper_interp_usdc(&self, bid: f64, order_usdc: f64) -> f64 {
        let longshot = self.bonereaper_size_longshot_usdc();
        let mid = self.bonereaper_size_mid_usdc(order_usdc);
        let high = self.bonereaper_size_high_usdc(order_usdc);
        let lw_thr = self.bonereaper_late_winner_bid_thr();
        if bid <= 0.30 {
            longshot
        } else if bid <= 0.65 {
            let t = ((bid - 0.30) / 0.35).clamp(0.0, 1.0);
            longshot + (mid - longshot) * t
        } else if bid < lw_thr {
            let span = (lw_thr - 0.65).max(0.01);
            let t = ((bid - 0.65) / span).clamp(0.0, 1.0);
            mid + (high - mid) * t
        } else {
            high
        }
    }
    /// Sabit share modu (15m bot için optimal). > 0 ise interp_usdc BYPASS,
    /// her normal alım `shares × ask` USDC harcar. 0 = interp aktif (5m bot).
    pub fn bonereaper_size_shares_const(&self) -> f64 {
        self.bonereaper_size_shares_const
            .unwrap_or(0.0)
            .clamp(0.0, 10_000.0)
    }
    /// Spread-aware sizing: dar spread eşiği; 0.0–0.50; default 0.0 (devre dışı).
    pub fn bonereaper_spread_lo_thr(&self) -> f64 {
        self.bonereaper_spread_lo_thr
            .unwrap_or(0.0)
            .clamp(0.0, 0.50)
    }
    /// Spread-aware sizing: orta spread üst eşiği; 0.0–1.0; default 0.50.
    pub fn bonereaper_spread_hi_thr(&self) -> f64 {
        self.bonereaper_spread_hi_thr
            .unwrap_or(0.50)
            .clamp(0.0, 1.0)
    }
    /// Dar spread (<spread_lo_thr) lot size; 0–10_000; default 40. 0 = devre dışı.
    pub fn bonereaper_size_shares_lo(&self) -> f64 {
        self.bonereaper_size_shares_lo
            .unwrap_or(40.0)
            .clamp(0.0, 10_000.0)
    }
    /// Orta spread (spread_lo..spread_hi) lot size; 0–10_000; default 25.
    pub fn bonereaper_size_shares_mid(&self) -> f64 {
        self.bonereaper_size_shares_mid
            .unwrap_or(25.0)
            .clamp(0.0, 10_000.0)
    }
    /// Yön seçim winner-bias eşiği; 0.0–0.99; default 0 (devre dışı).
    pub fn bonereaper_winner_bias_spread_thr(&self) -> f64 {
        self.bonereaper_winner_bias_spread_thr
            .unwrap_or(0.0)
            .clamp(0.0, 0.99)
    }
    /// Loser fırsat (çift emir) tetik eşiği; 0.0–0.99; default 0 (devre dışı).
    pub fn bonereaper_force_both_spread_thr(&self) -> f64 {
        self.bonereaper_force_both_spread_thr
            .unwrap_or(0.0)
            .clamp(0.0, 0.99)
    }
    /// Loser fırsat lot size; 0–10_000; default 0 (devre dışı). Önerilen: 8sh.
    pub fn bonereaper_force_both_loser_shares(&self) -> f64 {
        self.bonereaper_force_both_loser_shares
            .unwrap_or(0.0)
            .clamp(0.0, 10_000.0)
    }
    /// Spread-aware resolve: shares_const > 0 iken mevcut spread'e göre lot size döner.
    /// spread_lo_thr = 0 ise spread-aware devre dışı → shares_const sabit.
    pub fn bonereaper_spread_shares(&self, spread: f64) -> f64 {
        let const_sh = self.bonereaper_size_shares_const();
        if const_sh <= 0.0 { return 0.0; }
        let lo_thr = self.bonereaper_spread_lo_thr();
        if lo_thr <= 0.0 { return const_sh; }
        let hi_thr = self.bonereaper_spread_hi_thr();
        let sh_lo  = self.bonereaper_size_shares_lo();
        let sh_mid = self.bonereaper_size_shares_mid();
        if spread < lo_thr {
            if sh_lo > 0.0 { sh_lo } else { const_sh }
        } else if spread < hi_thr {
            if sh_mid > 0.0 { sh_mid } else { const_sh }
        } else {
            const_sh
        }
    }

    /// Loser tarafı min bid eşiği; 0.001–0.10 sınırlı; default 0.01.
    pub fn bonereaper_loser_min_price(&self) -> f64 {
        self.bonereaper_loser_min_price
            .unwrap_or(0.01)
            .clamp(0.001, 0.10)
    }
    /// Loser scalp USDC; 0–500 sınırlı; default `0.5 × order_usdc`. 0 = KAPALI.
    pub fn bonereaper_loser_scalp_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_loser_scalp_usdc
            .unwrap_or(order_usdc * 0.5)
            .clamp(0.0, 500.0)
    }
    /// Loser scalp üst bid eşiği; 0.05–0.50 sınırlı; default 0.30.
    pub fn bonereaper_loser_scalp_max_price(&self) -> f64 {
        self.bonereaper_loser_scalp_max_price
            .unwrap_or(0.30)
            .clamp(0.05, 0.50)
    }
    /// Loser avg üst sınırı (martingale-down guard); 0.10–0.95 sınırlı;
    /// default 0.50. Aşılırsa o yöne sadece minimal scalp.
    pub fn bonereaper_avg_loser_max(&self) -> f64 {
        self.bonereaper_avg_loser_max
            .unwrap_or(0.50)
            .clamp(0.10, 0.95)
    }

    /// Late pyramid penceresi (sn); 0–300 sınırlı; default 150. 0 = KAPALI.
    pub fn bonereaper_late_pyramid_secs(&self) -> u32 {
        self.bonereaper_late_pyramid_secs.unwrap_or(150).min(300)
    }
    /// Late pyramid winner size çarpanı; 1.0–10.0 sınırlı; default 2.0.
    pub fn bonereaper_winner_size_factor(&self) -> f64 {
        self.bonereaper_winner_size_factor
            .unwrap_or(2.0)
            .clamp(1.0, 10.0)
    }
}

/// Gravie (Dual-Balance Accumulator) parametreleri — `StrategyParams`'tan resolve edilir.
///
/// Mantık: her iki tarafta eşit share + `avg_up + avg_down < avg_sum_max`
/// ⇒ herhangi bir sonuçta garantili kâr (`N × (1 − avg_sum)`).
#[derive(Debug, Clone, Copy)]
pub struct GravieParams {
    /// Ardışık BUY arası minimum bekleme (ms). Default: 2000.
    pub buy_cooldown_ms: u64,
    /// `avg_up + avg_down` üst sınırı. Default: 1.00.
    pub avg_sum_max: f64,
    /// Her iki taraf için max alım fiyatı (ask). Default: 0.99.
    pub max_ask: f64,
    /// Kapanışa bu kadar sn kala yeni emir verme. Default: 30.
    pub t_cutoff_secs: f64,
    /// FAK başına max share. 0 = sınırsız. Default: 50.
    pub max_fak_size: f64,
    /// Rebalance tetik eşiği (share farkı). Default: 5.
    pub imb_thr: f64,
    /// İlk alım için minimum winner bid eşiği. Default: 0.65.
    pub first_bid_min: f64,
    /// Loser-scalp bypass: ask bu eşik altındaysa avg_sum gate atlanır.
    /// Default: 0.30. 0.0 = kapalı.
    pub loser_bypass_ask: f64,
    /// Late Winner tetik eşiği (winner bid). Default: 0.88.
    pub lw_bid_thr: f64,
    /// Late Winner USDC çarpanı (`order_usdc × X × lw_mult`). Default: 2.0.
    pub lw_usdc_factor: f64,
    /// Session başına max LW shot. Default: 30. 0 = sınırsız.
    pub lw_max_per_session: u32,
    /// Loser tarafta avg üst sınırı (pahalı birikim guard). Default: 0.50.
    pub avg_loser_max: f64,
    /// Loser-scalp USDC çarpanı. `ask ≤ loser_bypass_ask` iken
    /// `size = ceil(order_usdc × X / ask)`. Default: 0.5. 0 = scalp KAPALI.
    pub loser_scalp_usdc_factor: f64,
}

impl Default for GravieParams {
    fn default() -> Self {
        Self {
            buy_cooldown_ms: 2_000,
            avg_sum_max: 1.00,
            max_ask: 0.99,
            t_cutoff_secs: 30.0,
            max_fak_size: 50.0,
            imb_thr: 5.0,
            first_bid_min: 0.65,
            loser_bypass_ask: 0.30,
            lw_bid_thr: 0.88,
            lw_usdc_factor: 2.0,
            lw_max_per_session: 30,
            avg_loser_max: 0.50,
            loser_scalp_usdc_factor: 0.5,
        }
    }
}

impl GravieParams {
    /// `StrategyParams`'tan opsiyonel override'ları uygular; eksik alanlar default kalır.
    #[inline(always)]
    pub fn from_strategy_params(p: &StrategyParams) -> Self {
        let d = Self::default();
        Self {
            buy_cooldown_ms: p
                .gravie_buy_cooldown_ms
                .unwrap_or(d.buy_cooldown_ms)
                .clamp(100, 600_000),
            avg_sum_max: p
                .gravie_avg_sum_max
                .unwrap_or(d.avg_sum_max)
                .clamp(0.10, 1.50),
            max_ask: p
                .gravie_max_ask
                .unwrap_or(d.max_ask)
                .clamp(0.05, 0.99),
            t_cutoff_secs: p
                .gravie_t_cutoff_secs
                .unwrap_or(d.t_cutoff_secs)
                .clamp(0.0, 600.0),
            max_fak_size: p
                .gravie_max_fak_size
                .unwrap_or(d.max_fak_size)
                .clamp(0.0, 10_000.0),
            imb_thr: p
                .gravie_imb_thr
                .unwrap_or(d.imb_thr)
                .clamp(0.0, 10_000.0),
            first_bid_min: p
                .gravie_first_bid_min
                .unwrap_or(d.first_bid_min)
                .clamp(0.50, 0.99),
            loser_bypass_ask: p
                .gravie_loser_bypass_ask
                .unwrap_or(d.loser_bypass_ask)
                .clamp(0.0, 0.99),
            lw_bid_thr: p
                .gravie_lw_bid_thr
                .unwrap_or(d.lw_bid_thr)
                .clamp(0.50, 0.99),
            lw_usdc_factor: p
                .gravie_lw_usdc_factor
                .unwrap_or(d.lw_usdc_factor)
                .clamp(0.0, 20.0),
            lw_max_per_session: p
                .gravie_lw_max_per_session
                .unwrap_or(d.lw_max_per_session)
                .min(200),
            avg_loser_max: p
                .gravie_avg_loser_max
                .unwrap_or(d.avg_loser_max)
                .clamp(0.10, 0.95),
            loser_scalp_usdc_factor: p
                .gravie_loser_scalp_usdc_factor
                .unwrap_or(d.loser_scalp_usdc_factor)
                .clamp(0.0, 5.0),
        }
    }
}

