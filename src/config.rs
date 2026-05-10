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
    // Strateji: Polymarket "Bonereaper" wallet (0xeebde7a0...) davranış kopyası.
    // Order-book reactive martingale + late winner injection. Sinyal kullanmaz.
    /// Ardışık BUY emirleri arası minimum bekleme (ms). Real bot 3 örnek
    /// session'da 1.5–4 sn aralık gözlendi (gerçek 17–40 trade/dk). Default
    /// 3000 ms (~20 trade/dk). Alt clamp 1000 ms (sub-sec spam koruması).
    #[serde(default)]
    pub bonereaper_buy_cooldown_ms: Option<u64>,
    /// Late winner injection penceresi (sn). T-X anında bid≥thr olan tarafa
    /// massive taker BUY. 0 = devre dışı. Default: 30.
    #[serde(default)]
    pub bonereaper_late_winner_secs: Option<u32>,
    /// Late winner için kazanan tarafın bid eşiği. Default: 0.85
    /// (real bot kapanışta bid 0.85+ olan tarafa giriyor).
    #[serde(default)]
    pub bonereaper_late_winner_bid_thr: Option<f64>,
    /// Late winner trade büyüklüğü (USDC notional). Real bot 3 log analizinde
    /// big-bet medyan ~$1000-1300 (size 1000-1340 @ 0.99). Default: $1000.
    /// 0 = kural KAPALI.
    #[serde(default)]
    pub bonereaper_late_winner_usdc: Option<f64>,
    /// Session başına maksimum late winner injection sayısı. Real bot 3-5
    /// market'te 1 big-bet yapıyor (sıklık ~0.2-0.33/market). Default: 1.
    /// 0 = sınırsız (eski davranış, **spam riski**).
    #[serde(default)]
    pub bonereaper_lw_max_per_session: Option<u32>,
    /// |up_filled − down_filled| bu eşiği aşarsa weaker side rebalance trade'i
    /// yapılır (ob_driven yön seçimi bypass edilir). Default 50 share — bot
    /// 101 örnek 4908'de imbalance 199 oldu, eski 200 eşik tetiklenmedi → SAF
    /// tam kayıp. Düşük eşik SAF riskini erken hedge ile keser.
    #[serde(default)]
    pub bonereaper_imbalance_thr: Option<f64>,
    /// avg_sum yumuşak cap. `new_avg + opp_avg > X` ise yeni alım yok.
    /// Default 1.30 (bot 107 karşılaştırması: 1.05 cap winner pyramid'i fren'liyor;
    /// gerçek Bonereaper 1.20'ye kadar trade ediyor).
    #[serde(default)]
    pub bonereaper_max_avg_sum: Option<f64>,
    /// İlk emir için minimum |up_bid - down_bid| spread eşiği. Bu eşik
    /// aşılana kadar BUY ATILMAZ; aşılınca ilk emir yüksek bid tarafına
    /// (winner momentum) verilir. Sonraki trade'ler mevcut akışla devam eder.
    /// Default: 0.02 (bot 101 backtest: ROI %1.41 → %2.56, 1st=DOWN+win=DOWN
    /// kategorisi −%5.45 → +%8.86). 0.0 = devre dışı (eski davranış).
    #[serde(default)]
    pub bonereaper_first_spread_min: Option<f64>,
    /// Long-shot bid bucket (bid ≤ 0.30) trade büyüklüğü (USDC notional).
    /// Real bot bu bantta avg ~$15-20. Default: $15.
    #[serde(default)]
    pub bonereaper_size_longshot_usdc: Option<f64>,
    /// Mid bid bucket (0.30 < bid ≤ 0.85) trade büyüklüğü (USDC). Real bot
    /// avg size ~45 share × 0.55 = $25; mid bant ana trade alanı. Default: $25.
    #[serde(default)]
    pub bonereaper_size_mid_usdc: Option<f64>,
    /// High-confidence bid bucket (bid > 0.85) trade büyüklüğü (USDC).
    /// Real bot kapanış öncesi $30-50 trade'ler; LW'den ayrı normal akış. Default: $30.
    #[serde(default)]
    pub bonereaper_size_high_usdc: Option<f64>,

    // === Bonereaper - Aşama 3 (loser long-shot scalp) ===
    /// Kaybeden taraf için minimum bid eşiği (1¢ scalp). Winner tarafı yine
    /// genel `min_price` ile sınırlı. Default 0.01 (real bot 0.01–0.05'te
    /// yüzlerce share bilet topluyor).
    #[serde(default)]
    pub bonereaper_loser_min_price: Option<f64>,
    /// Kaybeden taraf 1¢ scalp USDC notional. Default $1 (kuruşluk bilet).
    /// 0 = scalp KAPALI.
    #[serde(default)]
    pub bonereaper_loser_scalp_usdc: Option<f64>,
    /// Loser scalp üst eşiği (bid). Loser side bid bu eşiğin altında ise
    /// scalp boyutu (`loser_scalp_usdc`) uygulanır. Default 0.30 (real bot
    /// 0.10-0.30 bandında bilet topluyor; eski mantık sadece bid<min_price=0.10
    /// kullanıyordu, çoğu loser scalp tetiklenmiyordu).
    #[serde(default)]
    pub bonereaper_loser_scalp_max_price: Option<f64>,

    // === Bonereaper - Aşama 4 (winner pyramid scaling) ===
    /// T-X sn'den itibaren winner tarafa size çarpanı uygula. Default 60 sn
    /// (real bot T-145s..T-120s arası massive pyramid yapıyor; biz daha geç
    /// başlayıp daha agresif vurarak yetişiriz). 0 = scaling KAPALI.
    #[serde(default)]
    pub bonereaper_late_pyramid_secs: Option<u32>,
    /// Winner tarafı için size çarpanı (T < late_pyramid_secs olunca).
    /// Default 5.0 (real bot tek trade'de 78-136 share atıyor, biz 17 sh atıyorduk;
    /// 5× = 85 sh ile gerçek bot büyüklüğüne yaklaşır).
    #[serde(default)]
    pub bonereaper_winner_size_factor: Option<f64>,

    // === Bonereaper - Aşama 5 (multi-LW burst) ===
    /// Ek LW burst tetikleyici penceresi (sn). T-X sn kala 2. dalga LW.
    /// Default 12 (real bot T-12s civarında ek pyramid). 0 = burst KAPALI.
    #[serde(default)]
    pub bonereaper_lw_burst_secs: Option<u32>,
    /// Burst LW USDC notional. Default $200 (ana $500 LW'nin yarısı).
    #[serde(default)]
    pub bonereaper_lw_burst_usdc: Option<f64>,

    // === Bonereaper - Aşama 6 (martingale-down guard) ===
    /// Loser side avg fiyatı bu eşiği aşarsa o yöne sadece minimal scalp
    /// ($1) yapılır. Pahalı martingale-down birikimini engeller. Default 0.50.
    /// Real bot loser tarafa avg ~0.05'te ucuz alıyor; bizde avg 0.5+ olunca
    /// her yeni alım üst paritede pahalı kayıp.
    #[serde(default)]
    pub bonereaper_avg_loser_max: Option<f64>,

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
    /// PATCH D — SIGNAL GATE. `effective_score` ile yön filtresi. Açıkken
    /// `score > up_threshold` ise UP'a, `< down_threshold` ise DOWN'a izin
    /// verilir; karşı yöndeki open/accum BLOKLANIR. Bot 91 analizi: gate
    /// kapalıyken WR %32, accum trade'lerinin %68'i kaybeden tarafa yığılıyor.
    /// Bonereaper aynı sinyalle WR %76+. Default: true.
    #[serde(default)]
    pub gravie_signal_gate_enabled: Option<bool>,
    /// PATCH D — Signal UP eşiği. `effective_score > X` ise UP yönü zorunlu.
    /// Bonereaper Triple Gate ile aynı eşik. Default: 5.5.
    #[serde(default)]
    pub gravie_signal_up_threshold: Option<f64>,
    /// PATCH D — Signal DOWN eşiği. `effective_score < X` ise DOWN yönü zorunlu.
    /// Bonereaper Triple Gate ile aynı eşik. Default: 4.5.
    #[serde(default)]
    pub gravie_signal_down_threshold: Option<f64>,

    // === Gravie V3 (ASYM) — yeni mantık alanları ============================
    /// V3: Winner side (signal yönü) BUY emir başına USDC. Default: 15.
    /// Hedge'den 3× büyük (asimetrik size: kazanan tarafta daha çok share).
    #[serde(default)]
    pub gravie_winner_order_usdc: Option<f64>,
    /// V3: Hedge side (signal karşıtı) BUY emir başına USDC. Default: 5.
    /// Winner'ın 1/3'ü; sadece avg_sum<X arbitraj koşulu sağlanırsa açılır.
    #[serde(default)]
    pub gravie_hedge_order_usdc: Option<f64>,
    /// V3: Hedge BUY ile winner BUY arası ayrı cooldown. Default: 10000.
    /// Winner cd = `gravie_buy_cooldown_ms`.
    #[serde(default)]
    pub gravie_hedge_cooldown_ms: Option<u64>,
    /// V3: Winner side için maksimum ask fiyatı. Default: 0.99 (rahat tavan,
    /// avg_sum kontrolü zaten sınırlıyor). Daha sıkı tavan istenirse 0.55-0.65.
    #[serde(default)]
    pub gravie_winner_max_price: Option<f64>,
    /// V3: Hedge side için maksimum ask fiyatı. Default: 0.99. Daha sıkı arb
    /// için 0.40-0.45.
    #[serde(default)]
    pub gravie_hedge_max_price: Option<f64>,
    /// V3: Pair açıkken `avg_up + avg_down < X` koşulu. Default: 0.80.
    /// Her dual pair'de min %20 brut kar marjı garantisi (1.0 - X).
    #[serde(default)]
    pub gravie_avg_sum_max: Option<f64>,
    /// V3: Stability filter penceresi (son N tick'in smoothed signal'i).
    /// 0 = filtre kapalı. Default: 3.
    #[serde(default)]
    pub gravie_stability_window: Option<u32>,
    /// V3: Stability filter — son N tick'in std'si bu eşikten büyükse trade
    /// atlanır (kararsız market). Default: 0.5.
    #[serde(default)]
    pub gravie_stability_max_std: Option<f64>,
    /// V3: Signal smoothing EMA alpha. Default: 0.3 (yumuşak smooth).
    /// 1.0 = smoothing yok (raw signal).
    #[serde(default)]
    pub gravie_ema_alpha: Option<f64>,
    /// V3: Tek tarafta maksimum kümülatif share (sermaye koruma cap).
    /// 0 = sınırsız (default).
    #[serde(default)]
    pub gravie_max_size_per_side: Option<f64>,
    /// V3 — LATE-WINDOW: Kapanışa `to_end ≤ X sn` kala WINNER BUY engellenir
    /// (hedge BUY serbest kalır). Bot 91 backtest: late-flip kayıplarının
    /// %63'ü son %20 pencerede gerçekleşiyor; winner emir kapatılınca worst
    /// loss -$281 → -$238 (%15 düşüş), ROI +9.70% → +11.10%.
    /// Default: 90 (5dk market'te son 60sn winner pasif). 0 = devre dışı.
    #[serde(default)]
    pub gravie_late_winner_pasif_secs: Option<f64>,
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

    // === Bonereaper accessors (G_lw_only — backtest optimum: ROI +%2.86) ===
    // Felsefe: minimal scalp (normal trade'ler küçük) + tek büyük late winner
    // inject. 3-bot cross-validation (468 session): tüm botlarda pozitif ROI.
    /// Ardışık BUY arası min bekleme (ms); 1000–60000 sınırlı; default 3000.
    /// Real bot 3 örnek session'da ~1.5–4 sn aralık yaptı. Eski 15s default
    /// 14× daha az trade üretiyordu (bot 101: 30 trade vs gerçek ~600).
    pub fn bonereaper_buy_cooldown_ms(&self) -> u64 {
        self.bonereaper_buy_cooldown_ms
            .unwrap_or(3_000)
            .clamp(1_000, 60_000)
    }
    /// Late winner penceresi (sn); 0–300 sınırlı; default 30. 0 = kural KAPALI.
    pub fn bonereaper_late_winner_secs(&self) -> u32 {
        self.bonereaper_late_winner_secs.unwrap_or(30).min(300)
    }
    /// Late winner bid eşiği; 0.50–0.99 sınırlı; default 0.90
    /// (Polymarket extreme'lerde mükemmel kalibre — LessWrong 7661 markets).
    pub fn bonereaper_late_winner_bid_thr(&self) -> f64 {
        self.bonereaper_late_winner_bid_thr
            .unwrap_or(0.90)
            .clamp(0.50, 0.99)
    }
    /// Late winner USDC notional; 0–10000 sınırlı; default 2000
    /// (G_lw_only: tüm para tek büyük inject'e — ROI maximizer). 0 = KAPALI.
    pub fn bonereaper_late_winner_usdc(&self) -> f64 {
        self.bonereaper_late_winner_usdc
            .unwrap_or(2000.0)
            .clamp(0.0, 10_000.0)
    }
    /// Session başına max LW injection; 0–20 sınırlı; default 5 (multi-LW
    /// için: ana T-30s + burst T-12s + 3 emniyet). 0 = sınırsız.
    pub fn bonereaper_lw_max_per_session(&self) -> u32 {
        self.bonereaper_lw_max_per_session.unwrap_or(5).min(20)
    }
    /// Imbalance threshold (share); 0–10000 sınırlı; default 50.
    /// Düşük eşik = SAF kayıp riskini erken hedge ile keser. Bot 101 örnek
    /// 4908'de imb 199 oldu, eski 200 eşik tetiklenmedi → tam loss.
    pub fn bonereaper_imbalance_thr(&self) -> f64 {
        self.bonereaper_imbalance_thr
            .unwrap_or(50.0)
            .clamp(0.0, 10_000.0)
    }
    /// avg_sum yumuşak cap; 0.50–2.00 sınırlı; default 1.30.
    /// Bot 107 vs gerçek Bonereaper karşılaştırması: 1.05 cap winner pyramid'i
    /// erken donduruyor (avg_up=0.687 + avg_dn=0.36 = 1.047 > 1.05 → reject).
    /// Gerçek bot 1.20'ye kadar trade görüyor; 1.30 default güvenli üst sınır.
    pub fn bonereaper_max_avg_sum(&self) -> f64 {
        self.bonereaper_max_avg_sum
            .unwrap_or(1.30)
            .clamp(0.50, 2.00)
    }
    /// İlk emir spread eşiği; 0.00–0.20 sınırlı; default 0.02.
    /// 0.0 → eski davranış (ilk tick'ten emir vermeye çalış).
    pub fn bonereaper_first_spread_min(&self) -> f64 {
        self.bonereaper_first_spread_min
            .unwrap_or(0.02)
            .clamp(0.00, 0.20)
    }
    /// Long-shot bucket USDC; 0–10000 sınırlı; default 5
    /// (minimal scalp; LW'e yer aç).
    pub fn bonereaper_size_longshot_usdc(&self) -> f64 {
        self.bonereaper_size_longshot_usdc
            .unwrap_or(5.0)
            .clamp(0.0, 10_000.0)
    }
    /// Mid bucket USDC; 0–10000 sınırlı; default 10 (minimal scalp).
    pub fn bonereaper_size_mid_usdc(&self) -> f64 {
        self.bonereaper_size_mid_usdc
            .unwrap_or(10.0)
            .clamp(0.0, 10_000.0)
    }
    /// High-conf bucket USDC; 0–10000 sınırlı; default 15 (minimal scalp).
    pub fn bonereaper_size_high_usdc(&self) -> f64 {
        self.bonereaper_size_high_usdc
            .unwrap_or(15.0)
            .clamp(0.0, 10_000.0)
    }
    /// Loser side min bid eşiği; 0.001–0.10 sınırlı; default 0.01 (1¢ scalp).
    /// Real bot 0.01–0.05 fiyatlarında bilet topluyor.
    pub fn bonereaper_loser_min_price(&self) -> f64 {
        self.bonereaper_loser_min_price
            .unwrap_or(0.01)
            .clamp(0.001, 0.10)
    }
    /// Loser side scalp USDC; 0–10 sınırlı; default $1 (kuruşluk bilet).
    /// 0 = scalp KAPALI.
    pub fn bonereaper_loser_scalp_usdc(&self) -> f64 {
        self.bonereaper_loser_scalp_usdc
            .unwrap_or(1.0)
            .clamp(0.0, 10.0)
    }
    /// Loser scalp üst bid eşiği; 0.05–0.50 sınırlı; default 0.30. Loser side
    /// bid bu eşiğin altındaysa scalp boyutu uygulanır (longshot bucket yerine).
    pub fn bonereaper_loser_scalp_max_price(&self) -> f64 {
        self.bonereaper_loser_scalp_max_price
            .unwrap_or(0.30)
            .clamp(0.05, 0.50)
    }
    /// Late pyramid penceresi (sn); 0–300 sınırlı; default 60.
    pub fn bonereaper_late_pyramid_secs(&self) -> u32 {
        self.bonereaper_late_pyramid_secs.unwrap_or(60).min(300)
    }
    /// Winner pyramid size çarpanı; 1.0–10.0 sınırlı; default 5.0.
    pub fn bonereaper_winner_size_factor(&self) -> f64 {
        self.bonereaper_winner_size_factor
            .unwrap_or(5.0)
            .clamp(1.0, 10.0)
    }
    /// LW burst pencere (sn); 0–60 sınırlı; default 12. T-X kala 2. dalga LW.
    /// 0 = burst KAPALI. Ana LW (`late_winner_secs`) > burst > 0 olmalı.
    pub fn bonereaper_lw_burst_secs(&self) -> u32 {
        self.bonereaper_lw_burst_secs.unwrap_or(12).min(60)
    }
    /// LW burst USDC; 0–10000 sınırlı; default 200.
    pub fn bonereaper_lw_burst_usdc(&self) -> f64 {
        self.bonereaper_lw_burst_usdc
            .unwrap_or(200.0)
            .clamp(0.0, 10_000.0)
    }
    /// Loser avg fiyat üst sınırı (martingale-down guard); 0.10–0.95 sınırlı;
    /// default 0.50. Loser side avg bu eşiği aşarsa o yöne sadece minimal
    /// scalp ($1). Pahalı down-pyramid birikimini engeller.
    pub fn bonereaper_avg_loser_max(&self) -> f64 {
        self.bonereaper_avg_loser_max
            .unwrap_or(0.50)
            .clamp(0.10, 0.95)
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

/// Gravie V3 (ASYM) strateji parametreleri — `StrategyParams`'tan resolve edilir.
///
/// Default'lar Bot 91 4-günlük backtest şampiyonu (`ASYM W$15/H$5 avg<0.80`):
/// PnL +$2468, ROI +9.70%, WR %61, Worst loss -$281, Dual market %49.
#[derive(Debug, Clone, Copy)]
pub struct GravieParams {
    /// Karar tick aralığı (sn). Default: 5.
    pub tick_interval_secs: u64,
    /// Winner side BUY arası min bekleme (ms). Default: 10000.
    pub buy_cooldown_ms: u64,
    /// Hedge side BUY arası min bekleme (ms). Default: 10000.
    pub hedge_cooldown_ms: u64,
    /// Kapanışa bu kadar sn kala yeni emir verme. Default: 30.
    pub t_cutoff_secs: f64,
    /// FAK emir başına maksimum share. 0 = sınırsız. Default: 50.
    pub max_fak_size: f64,
    /// Winner side BUY emir başına USDC. Default: 15 (3× hedge).
    pub winner_order_usdc: f64,
    /// Hedge side BUY emir başına USDC. Default: 5.
    pub hedge_order_usdc: f64,
    /// Winner side için maksimum ask fiyatı. Default: 0.99 (rahat).
    pub winner_max_price: f64,
    /// Hedge side için maksimum ask fiyatı. Default: 0.99.
    pub hedge_max_price: f64,
    /// `avg_up + avg_down < X` koşulu. Default: 0.80 (min %20 brut marj).
    pub avg_sum_max: f64,
    /// Smoothed signal > X → UP winner. Default: 5.0 (no neutral).
    pub signal_up_threshold: f64,
    /// Smoothed signal < X → DOWN winner. Default: 5.0.
    pub signal_down_threshold: f64,
    /// Stability filter penceresi. 0 = kapalı. Default: 3.
    pub stability_window: u32,
    /// Stability filter — std bu eşikten büyükse trade atla. Default: 0.5.
    pub stability_max_std: f64,
    /// Signal smoothing EMA alpha. Default: 0.3.
    pub ema_alpha: f64,
    /// Tek tarafta maksimum kümülatif share. 0 = sınırsız. Default: 0.
    pub max_size_per_side: f64,
    /// LATE-WINDOW: `to_end ≤ X sn` kala winner BUY engellenir (hedge serbest).
    /// Default: 90 (late-flip kayıp koruması). 0 = devre dışı.
    pub late_winner_pasif_secs: f64,
}

impl Default for GravieParams {
    fn default() -> Self {
        Self {
            tick_interval_secs: 5,
            buy_cooldown_ms: 10_000,
            hedge_cooldown_ms: 10_000,
            t_cutoff_secs: 30.0,
            max_fak_size: 50.0,
            winner_order_usdc: 15.0,
            hedge_order_usdc: 5.0,
            winner_max_price: 0.99,
            hedge_max_price: 0.99,
            avg_sum_max: 0.80,
            signal_up_threshold: 5.0,
            signal_down_threshold: 5.0,
            stability_window: 3,
            stability_max_std: 0.5,
            ema_alpha: 0.3,
            max_size_per_side: 0.0,
            late_winner_pasif_secs: 90.0,
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
                .min(600_000),
            hedge_cooldown_ms: p
                .gravie_hedge_cooldown_ms
                .unwrap_or(d.hedge_cooldown_ms)
                .min(600_000),
            t_cutoff_secs: p
                .gravie_t_cutoff_secs
                .unwrap_or(d.t_cutoff_secs)
                .clamp(0.0, 600.0),
            max_fak_size: p
                .gravie_max_fak_size
                .unwrap_or(d.max_fak_size)
                .clamp(0.0, 10_000.0),
            winner_order_usdc: p
                .gravie_winner_order_usdc
                .unwrap_or(d.winner_order_usdc)
                .clamp(1.0, 10_000.0),
            hedge_order_usdc: p
                .gravie_hedge_order_usdc
                .unwrap_or(d.hedge_order_usdc)
                .clamp(1.0, 10_000.0),
            winner_max_price: p
                .gravie_winner_max_price
                .unwrap_or(d.winner_max_price)
                .clamp(0.10, 0.99),
            hedge_max_price: p
                .gravie_hedge_max_price
                .unwrap_or(d.hedge_max_price)
                .clamp(0.10, 0.99),
            avg_sum_max: p
                .gravie_avg_sum_max
                .unwrap_or(d.avg_sum_max)
                .clamp(0.50, 1.50),
            signal_up_threshold: p
                .gravie_signal_up_threshold
                .unwrap_or(d.signal_up_threshold)
                .clamp(0.0, 10.0),
            signal_down_threshold: p
                .gravie_signal_down_threshold
                .unwrap_or(d.signal_down_threshold)
                .clamp(0.0, 10.0),
            stability_window: p
                .gravie_stability_window
                .unwrap_or(d.stability_window)
                .min(50),
            stability_max_std: p
                .gravie_stability_max_std
                .unwrap_or(d.stability_max_std)
                .clamp(0.0, 5.0),
            ema_alpha: p
                .gravie_ema_alpha
                .unwrap_or(d.ema_alpha)
                .clamp(0.01, 1.0),
            max_size_per_side: p
                .gravie_max_size_per_side
                .unwrap_or(d.max_size_per_side)
                .clamp(0.0, 1_000_000.0),
            late_winner_pasif_secs: p
                .gravie_late_winner_pasif_secs
                .unwrap_or(d.late_winner_pasif_secs)
                .clamp(0.0, 600.0),
        }
    }
}
