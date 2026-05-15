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
    /// Session başına maksimum late winner injection sayısı.
    /// 0 = sınırsız. Default: 0 (sınırsız).
    #[serde(default)]
    pub bonereaper_lw_max_per_session: Option<u32>,
    /// LW shot'ları arasındaki minimum bekleme (ms). Normal buy cooldown'dan
    /// bağımsız; sadece LW için geçerli. Gerçek bot analizi: 24 market, medyan
    /// gap 2s (burst), 90.pct gap 40s. 10s default → max ~18 shot/180s window
    /// (gerçek bot medyan 14 shot/market). 0 = devre dışı (normal CD kullanır).
    #[serde(default)]
    pub bonereaper_lw_cooldown_ms: Option<u64>,
    /// |up_filled − down_filled| bu eşiği aşarsa weaker side rebalance trade'i
    /// yapılır (ob_driven yön seçimi bypass edilir). Default 50 share — bot
    /// 101 örnek 4908'de imbalance 199 oldu, eski 200 eşik tetiklenmedi → SAF
    /// tam kayıp. Düşük eşik SAF riskini erken hedge ile keser.
    #[serde(default)]
    pub bonereaper_imbalance_thr: Option<f64>,
    /// avg_sum yumuşak cap. `new_avg + opp_avg > X` ise yeni alım yok (scalp/LW muaf).
    /// JSON'da null ise default **1.10** (gerçek bot peak avg_sum medyanı 1.10).
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
    /// mantığı. Default: 0.50. 0.0 = bypass kapalı.
    #[serde(default)]
    pub gravie_loser_bypass_ask: Option<f64>,
}

impl StrategyParams {
    /// RTDS Chainlink task'ını başlatmak için kontrol (sinyal hesabında kullanılmaz).
    pub fn rtds_enabled_or_default(&self) -> bool {
        self.rtds_enabled.unwrap_or(true)
    }

    // === Bonereaper accessors (G_lw_only — backtest optimum: ROI +%2.86) ===
    // Felsefe: minimal scalp (normal trade'ler küçük) + tek büyük late winner
    // inject. 3-bot cross-validation (468 session): tüm botlarda pozitif ROI.
    /// Ardışık BUY arası min bekleme (ms); 500–60000 sınırlı; default 2000.
    /// 47-market log analizi (bonereaper6/7/8): yön-değişimi gap dağılımı:
    ///   min=2s (en küçük bağımsız karar aralığı), %12 = 2s, median=5s.
    ///   0s/1s = aynı Polygon bloğunda birden fazla fill (partial, bot kararı değil).
    /// Trading frekansını sınırlayan ASIL mekanizma avg_sum=1.00 kentidir:
    ///   Piyasa 50/50'den uzaklaşınca avg_sum > 1.00 → uzun doğal bekleme (30-100s).
    ///   Cooldown=2s + avg_sum=1.00 → gerçek bot ile aynı 10-18 trade/market profili.
    pub fn bonereaper_buy_cooldown_ms(&self) -> u64 {
        self.bonereaper_buy_cooldown_ms
            .unwrap_or(2_000)
            .clamp(500, 60_000)
    }
    /// Late winner penceresi (sn); 0–300 sınırlı; default 180.
    /// 25-market doğrulaması (12 + 13 yeni log): T-180 sonrası LW oranı %98.6
    /// (207/210 shot), T-180 öncesinde sadece 3 LW gözlemlendi (1.4%). Eski
    /// 300sn defaultu erken LW'ye izin veriyordu; 180 gerçek bot davranışıyla
    /// birebir uyumlu. 0 = kural KAPALI.
    pub fn bonereaper_late_winner_secs(&self) -> u32 {
        self.bonereaper_late_winner_secs.unwrap_or(300).min(300)
    }
    /// Late winner bid eşiği; 0.50–0.99 sınırlı; default 0.88
    /// (Canlı Bonereaper analizi [1:50-1:55 ET]: gerçek bot UP $0.92 bid'de
    /// [T] $115 + [T] $111 = $226 büyük shot attı → LW bid_thr=0.98 ile
    /// bu tetiklenmiyordu. 0.88 = loser ask ~$0.11, winner ~$0.89 — real bot
    /// "$0.07-$0.13 arası DOWN loser" gözlemlenince UP'a büyük LW yapıyor).
    pub fn bonereaper_late_winner_bid_thr(&self) -> f64 {
        self.bonereaper_late_winner_bid_thr
            .unwrap_or(0.88)
            .clamp(0.50, 0.99)
    }
    /// Late winner USDC notional; 0–10000 sınırlı; default 100.
    /// 47-market analizi (timestamp grup bazlı LW shot büyüklükleri):
    ///   - $0.85-0.95 bant medyanı $91/shot (117 shot) → $100 default uygun.
    ///   - $0.95+ bant medyanı $198/shot, ort $589/shot, max $4953/shot.
    ///     Bu bantta dinamik arb_mult ölçekleme bonereaper.rs'de uygulanır
    ///     ($0.95 → 1x, $0.97 → 2x, $0.99 → 4x).
    ///   - Sonuç: $100 × max 4x = $400/shot, max_per_session=20 ile $8k tavan.
    /// 0 = KAPALI.
    /// `order_usdc` verilirse formül: `3 × order_usdc`. DB'de override varsa onu kullan.
    pub fn bonereaper_late_winner_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_late_winner_usdc
            .unwrap_or(2.0 * order_usdc)
            .clamp(0.0, 10_000.0)
    }
    /// Session başına max LW injection; 0–50 sınırlı; default 30.
    /// 0 = sınırsız.
    pub fn bonereaper_lw_max_per_session(&self) -> u32 {
        self.bonereaper_lw_max_per_session.unwrap_or(30).min(50)
    }
    /// LW shot'ları arasındaki minimum bekleme (ms); 0–60000 sınırlı.
    /// Default: 10_000 (10s). 0 = devre dışı (normal buy_cooldown_ms kullanılır).
    pub fn bonereaper_lw_cooldown_ms(&self) -> u64 {
        self.bonereaper_lw_cooldown_ms.unwrap_or(10_000).min(60_000)
    }
    /// Imbalance threshold (share); 0–10000 sınırlı.
    /// Default: `10 × order_usdc` — bu eşik aşılınca weaker side rebalance.
    /// Analiz: thr=100 (10×$10) ile ~5 directional trade sonra rebalance başlar,
    /// DOWN kazanırsa mid-phase net ≈ +$11 vs thr=300'de -$85 (session analizi).
    /// 9999 → dynamic mod aktif (N=3/6/9/12 × est_trade_size, zaman bazlı).
    /// DB'de override varsa kullanılır.
    pub fn bonereaper_imbalance_thr(&self, order_usdc: f64) -> f64 {
        let _ = order_usdc;
        self.bonereaper_imbalance_thr
            .unwrap_or(9999.0)
            .clamp(0.0, 10_000.0)
    }
    /// avg_sum yumuşak cap; 0.50–2.00 sınırlı; default 1.00.
    /// avg_sum = avg_up + avg_dn. Bu değer >1.00 olunca hangi taraf kazanırsa
    /// kazansın zarar edilir (eğer share sayıları eşitse).
    /// 36 market analizi: LW check `order_price + opp_avg > max_avg_sum` ile
    /// max_avg_sum=1.00 → 444/446 LW fill bloke (%99.6). Gerçek bot (bonereaper8)
    /// 3 markette 0 LW shot bu mekanizma sayesinde. Normal buy'lar da daha hızlı
    /// durur → avg_sum=1.00 = gerçek botun doğru parametresi.
    pub fn bonereaper_max_avg_sum(&self) -> f64 {
        self.bonereaper_max_avg_sum
            .unwrap_or(1.00)
            .clamp(0.50, 2.00)
    }
    /// İlk emir spread eşiği; 0.00–0.20 sınırlı; default 0.02.
    /// 0.0 → eski davranış (ilk tick'ten emir vermeye çalış).
    pub fn bonereaper_first_spread_min(&self) -> f64 {
        self.bonereaper_first_spread_min
            .unwrap_or(0.02)
            .clamp(0.00, 0.20)
    }
    /// Long-shot bucket USDC (bid ≤ 0.30); 0–10000 sınırlı; default 15.
    /// 25-market doğrulaması (12 + 13 yeni log): real bot $0.20-0.40 bandında
    /// medyan $14.70-$15.34/shot. Eski $8 default real botun %50'si kadardı,
    /// $15 birebir uyumlu.
    pub fn bonereaper_size_longshot_usdc(&self) -> f64 {
        self.bonereaper_size_longshot_usdc
            .unwrap_or(15.0)
            .clamp(0.0, 10_000.0)
    }
    /// Mid bucket USDC (0.30 < bid ≤ 0.65); 0–10000 sınırlı.
    /// Default: `1.5 × order_usdc` — 66-market gerçek bot analizi:
    /// normal per-shot medyan=$9.8, ort=$13.1. order=10 → $15 ideal eşleşme.
    /// Eski 4× ($40) gerçek botun 3× üstündeydi → trade sıklığı ve bakiye oranı bozuktu.
    /// DB'de override varsa kullanılır.
    pub fn bonereaper_size_mid_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_size_mid_usdc
            .unwrap_or(1.5 * order_usdc)
            .clamp(0.0, 10_000.0)
    }
    /// High bucket USDC (bid > 0.65); 0–10000 sınırlı.
    /// Default: `2 × order_usdc` — 66-market gerçek bot analizi:
    /// high band ort. ~$20/shot (order=10). Eski 6× ($60) gerçek botun 3× üstündeydi.
    /// DB'de override varsa kullanılır.
    pub fn bonereaper_size_high_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_size_high_usdc
            .unwrap_or(2.0 * order_usdc)
            .clamp(0.0, 10_000.0)
    }
    /// Loser side min bid eşiği; 0.001–0.10 sınırlı; default 0.01 (1¢ scalp).
    /// Real bot 0.01–0.05 fiyatlarında bilet topluyor.
    pub fn bonereaper_loser_min_price(&self) -> f64 {
        self.bonereaper_loser_min_price
            .unwrap_or(0.01)
            .clamp(0.001, 0.10)
    }
    /// Loser side scalp USDC; 0–500 sınırlı.
    /// Default: `order_usdc × 0.5` — order_usdc=10 → $5 default (bot 332 optimum).
    /// 0 = scalp KAPALI. DB'de override varsa onu kullan.
    pub fn bonereaper_loser_scalp_usdc(&self, order_usdc: f64) -> f64 {
        self.bonereaper_loser_scalp_usdc
            .unwrap_or(order_usdc * 0.5)
            .clamp(0.0, 500.0)
    }
    /// Loser scalp üst bid eşiği; 0.05–0.50 sınırlı; default 0.30. Loser side
    /// bid bu eşiğin altındaysa scalp boyutu uygulanır (longshot bucket yerine).
    pub fn bonereaper_loser_scalp_max_price(&self) -> f64 {
        self.bonereaper_loser_scalp_max_price
            .unwrap_or(0.30)
            .clamp(0.05, 0.50)
    }
    /// Late pyramid penceresi (sn); 0–300 sınırlı; default 150.
    /// Gerçek bot t=122-177s'de (to_end=123-178s) winner'a $0.80-$0.87'den
    /// büyük lot alımları yapıyor (71, 83, 74 sh). Bu erken accumulation fazı
    /// T-150s'den itibaren başlıyor; eski 60s default bu pencereyi kaçırıyordu.
    pub fn bonereaper_late_pyramid_secs(&self) -> u32 {
        self.bonereaper_late_pyramid_secs.unwrap_or(150).min(300)
    }
    /// Winner pyramid size çarpanı; 1.0–10.0 sınırlı; default 2.0.
    /// size_high_usdc=$40 ile: $40×2=$80 → bid $0.90'da ~89sh ≈ real bot 87sh ✓.
    /// 941-trade analizi: $0.85-0.95 avg 85sh. ceil(80/0.90)=89 ← tam uyum.
    pub fn bonereaper_winner_size_factor(&self) -> f64 {
        self.bonereaper_winner_size_factor
            .unwrap_or(1.0)
            .clamp(1.0, 10.0)
    }
    /// LW burst pencere (sn); 0–60 sınırlı; default 0 (KAPALI).
    /// Gerçek bot analizi: ayrı burst wave yok, tüm $0.99 alımlar tek
    /// mekanizmadan geliyor. Ana LW secs=300 + bid_thr=0.98 ile aynı
    /// davranış sağlanıyor; burst ek karmaşıklık katıyor.
    pub fn bonereaper_lw_burst_secs(&self) -> u32 {
        self.bonereaper_lw_burst_secs.unwrap_or(0).min(60)
    }
    /// LW burst USDC; 0–10000 sınırlı; default 0 (KAPALI).
    pub fn bonereaper_lw_burst_usdc(&self) -> f64 {
        self.bonereaper_lw_burst_usdc
            .unwrap_or(0.0)
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

/// Gravie (Dual-Balance Accumulator) parametreleri — `StrategyParams`'tan resolve edilir.
///
/// Mantık: her iki tarafta eşit share + `avg_up + avg_down < avg_sum_max`
/// ⇒ herhangi bir sonuçta garantili kâr (`N × (1 − avg_sum)`).
#[derive(Debug, Clone, Copy)]
pub struct GravieParams {
    /// Ardışık BUY arası minimum bekleme (ms). Default: 2000.
    pub buy_cooldown_ms: u64,
    /// `avg_up + avg_down` üst sınırı. Default: 0.95.
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
    /// Default: 0.50. 0.0 = kapalı.
    pub loser_bypass_ask: f64,
}

impl Default for GravieParams {
    fn default() -> Self {
        Self {
            buy_cooldown_ms: 2_000,
            avg_sum_max: 0.95,
            max_ask: 0.99,
            t_cutoff_secs: 30.0,
            max_fak_size: 50.0,
            imb_thr: 5.0,
            first_bid_min: 0.65,
            loser_bypass_ask: 0.50,
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
        }
    }
}

