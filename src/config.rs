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
/// parse edilir, tüm stratejiler (Bonereaper / Gravie / Arbitrage) buradan okur.
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

    // === Arbitrage (Pure cross-leg FAK BID arbitrage) ===
    // Strateji: avg_sum<1 garantili sentetik dolar minting.
    // Bot 108 backtest (16 200 kombinasyon): cost<0.95 + mt=5 + $100 → WR %100,
    // ROI %4.35, NET +$994/12.4h.
    /// Tick check interval (ms). Sub-second için 1000 ideal. Default 1000.
    #[serde(default)]
    pub arbitrage_tick_interval_ms: Option<u64>,
    /// Maks `winner_bid + loser_bid` (= avg_sum). Bu değerin altındaysa fırsat.
    /// Default 0.95 (sıkı: yalnızca 5¢+ marj). 0.97 = orta, 0.99 = gevşek (fee yiyor).
    #[serde(default)]
    pub arbitrage_cost_max: Option<f64>,
    /// Her trade USDC notional (winner ve loser leg ayrı ayrı bu büyüklükte).
    /// Default $20. Linear scaling — $100 ile +$994/12.4h.
    #[serde(default)]
    pub arbitrage_order_usdc: Option<f64>,
    /// Session başına maksimum trade sayısı. Default 5.
    #[serde(default)]
    pub arbitrage_max_trades_per_session: Option<u32>,
    /// Trade'ler arası minimum bekleme (ms). Default 5000.
    #[serde(default)]
    pub arbitrage_cooldown_ms: Option<u64>,
    /// Pencere kapanmasına bu kadar saniye kala kadar fırsat ara (T-X..T-0).
    /// Default 300 (tüm pencere). 60 = sadece son 1dk.
    #[serde(default)]
    pub arbitrage_entry_window_secs: Option<u32>,

    // === Binance Latency Arbitrage parametreleri =============================
    // Strateji: Polymarket BTC 5dk markete karşı Binance Spot BTC/USDT lag'ini
    // sömürür. Session start_ts'de BTC mid snapshot, her tick delta hesabı;
    // |delta| ≥ sig_thr → BUY (delta>0 UP, <0 DOWN).
    // Bot 91 backtest (665 session, 64h):
    //   - sig=$50 mt=10 cd=3s → WR %89, NET +$8323, ROI +%4.80, yıllık ~$1.14M
    //   - sig=$80 mt=3 cd=3s → WR %93, NET +$2222, ROI +%9.11 (max ROI)
    //   - sig=$50 mt=50 cd=3s → WR %91, NET +$12808, ROI +%3.16 (max NET)
    /// Sinyal eşiği (USD). Default 50.
    #[serde(default)]
    pub binance_latency_sig_thr_usd: Option<f64>,
    /// Trade'ler arası min bekleme (ms). Default 3000.
    #[serde(default)]
    pub binance_latency_cooldown_ms: Option<u64>,
    /// Session başına max trade. Default 10.
    #[serde(default)]
    pub binance_latency_max_trades_per_session: Option<u32>,
    /// Order USDC notional. Default 100.
    #[serde(default)]
    pub binance_latency_order_usdc: Option<f64>,
    /// Entry penceresi (T-X..T-0 arası ara). Default 300 (tüm pencere).
    #[serde(default)]
    pub binance_latency_entry_window_secs: Option<u32>,
    /// Hedge leg notional (USDC). 0 = hedge KAPALI (default; backtest
    /// matematik aleyhine: hedge=$1 → NET -$375, hedge=$5 → NET -$2628).
    /// Sadece tek-yön risk azaltmak isteyenler için opt-in.
    #[serde(default)]
    pub binance_latency_hedge_usdc: Option<f64>,
    /// Hedge için karşı tarafın bid üst sınırı. Bid bu eşiğin altındaysa
    /// FAK BID hedge alınır. Default 0.30. Düşük = sıkı (daha az hedge),
    /// yüksek = gevşek (daha çok hedge → daha çok kayıp).
    #[serde(default)]
    pub binance_latency_hedge_max_bid: Option<f64>,

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
    /// RTDS Chainlink task'ını başlatmak için kontrol (sinyal hesabında kullanılmaz).
    pub fn rtds_enabled_or_default(&self) -> bool {
        self.rtds_enabled.unwrap_or(true)
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
    /// Late winner penceresi (sn); 0–300 sınırlı; default 180.
    /// 25-market doğrulaması (12 + 13 yeni log): T-180 sonrası LW oranı %98.6
    /// (207/210 shot), T-180 öncesinde sadece 3 LW gözlemlendi (1.4%). Eski
    /// 300sn defaultu erken LW'ye izin veriyordu; 180 gerçek bot davranışıyla
    /// birebir uyumlu. 0 = kural KAPALI.
    pub fn bonereaper_late_winner_secs(&self) -> u32 {
        self.bonereaper_late_winner_secs.unwrap_or(180).min(300)
    }
    /// Late winner bid eşiği; 0.50–0.99 sınırlı; default 0.88
    /// (Canlı Bonereaper analizi [1:50-1:55 ET]: gerçek bot UP $0.92 bid'de
    /// [T] $115 + [T] $111 = $226 büyük shot attı → LW bid_thr=0.98 ile
    /// bu tetiklenmiyordu. 0.88 = loser ask ~$0.11, winner ~$0.89 — real bot
    /// "$0.07-$0.13 arası DOWN loser" gözlemlenince UP'a büyük LW yapıyor).
    pub fn bonereaper_late_winner_bid_thr(&self) -> f64 {
        self.bonereaper_late_winner_bid_thr
            .unwrap_or(0.90)
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
    pub fn bonereaper_late_winner_usdc(&self) -> f64 {
        self.bonereaper_late_winner_usdc
            .unwrap_or(100.0)
            .clamp(0.0, 10_000.0)
    }
    /// Session başına max LW injection; 0–50 sınırlı; default 20.
    /// 47-market analizi: gerçek bot bir markette 23'e kadar LW shot atıyor
    /// (1778010000: 23 shot, 1778027100: 17 shot, 1778586300: 11 shot).
    /// quota 20 + lw_usdc=$100 + arb_mult max 4x = $8k tavan, gerçek botun
    /// max $7,147 LW örneğine ($1778568000) yakın. 0 = sınırsız.
    pub fn bonereaper_lw_max_per_session(&self) -> u32 {
        self.bonereaper_lw_max_per_session.unwrap_or(20).min(50)
    }
    /// Imbalance threshold (share); 0–10000 sınırlı; default 1000.
    /// 4-market analizi (bot 117): imbalance_thr=200 ile T=0-60s arası
    /// 14 yön değişimi (salınım) → her iki taraf $0.50 avg → avg_sum=1.00
    /// → HER TARAF KAZANSA KAYBEDER. Simülasyon: thr=1000 → 0 salınım.
    /// Doğal OB switch (bid değişimi) yön geçişini yönetir; imbalance
    /// sadece aşırı birikim (1000sh fark) için tetiklenir.
    pub fn bonereaper_imbalance_thr(&self) -> f64 {
        self.bonereaper_imbalance_thr
            .unwrap_or(1000.0)
            .clamp(0.0, 10_000.0)
    }
    /// avg_sum yumuşak cap; 0.50–2.00 sınırlı; default 1.05.
    /// avg_sum = avg_up + avg_dn. Bu değer >1.00 olunca hangi taraf kazanırsa
    /// kazansın zarar edilir (eğer share sayıları eşitse). Gerçek bot yüksek
    /// avg_sum'a (1.16-1.42) sahip olsa da winner/loser share oranı 5-40:1
    /// olduğu için zarar etmiyor. Bizim botumuz imbalance'dan dolayı eşit
    /// share biriktirir → 1.05 hard cap: winner avg=$0.90 + loser avg=$0.15
    /// = 1.05 → ikinci tarafı $0.15 üstü normal emirle alamaz, sadece loser
    /// scalp (exempt) ile ucuza toplar. LW ve loser scalp bu captan muaf.
    pub fn bonereaper_max_avg_sum(&self) -> f64 {
        // Default 1.05: belirsiz (50/50) piyasada her iki tarafı mid-range'den
        // almanın önüne geçer (avg_up≈0.63 + avg_dn≈0.53 = 1.16 → bloke).
        // Gerçek bot: winner avg≈0.90 + loser avg≈0.10 = 1.00 → cap etkilemiyor.
        // 2.0 default denenince her iki taraf serbest doldu → her iki pnl negatif.
        self.bonereaper_max_avg_sum
            .unwrap_or(1.05)
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
    /// Mid bucket USDC (0.30 < bid ≤ 0.65); 0–10000 sınırlı; default 23.
    /// 25-market doğrulaması: $0.40-0.65 bandında medyan $23.12-$23.31/shot.
    /// Eski $10 default real botun ~%43'üydü → bot büyük accumulation
    /// kaçırıyordu, $23 birebir uyumlu.
    pub fn bonereaper_size_mid_usdc(&self) -> f64 {
        self.bonereaper_size_mid_usdc
            .unwrap_or(23.0)
            .clamp(0.0, 10_000.0)
    }
    /// High bucket USDC (bid > 0.65); 0–10000 sınırlı; default 37.
    /// 25-market doğrulaması: $0.65-0.85 bandında medyan $37.19-$43.12/shot.
    /// Eski $20 default real botun ~%54'üydü, $37 medyana oturur.
    pub fn bonereaper_size_high_usdc(&self) -> f64 {
        self.bonereaper_size_high_usdc
            .unwrap_or(37.0)
            .clamp(0.0, 10_000.0)
    }
    /// Loser side min bid eşiği; 0.001–0.10 sınırlı; default 0.01 (1¢ scalp).
    /// Real bot 0.01–0.05 fiyatlarında bilet topluyor.
    pub fn bonereaper_loser_min_price(&self) -> f64 {
        self.bonereaper_loser_min_price
            .unwrap_or(0.01)
            .clamp(0.001, 0.10)
    }
    /// Loser side scalp USDC; 0–50 sınırlı; default $10.
    /// 47-market analizi (238 scoop shot, timestamp grup bazlı):
    ///   medyan $9.90/shot, ort $11.37/shot, dağılım %32 <$5, %18 $5-10,
    ///   %24 $10-15, %8 $15-20, %18 $20+. $10 default medyana oturuyor.
    /// 0 = scalp KAPALI.
    pub fn bonereaper_loser_scalp_usdc(&self) -> f64 {
        self.bonereaper_loser_scalp_usdc
            .unwrap_or(10.0)
            .clamp(0.0, 50.0)
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

    // === Arbitrage accessors (cost<0.95 + mt=5 + $100 backtest optimum) ===
    /// Tick interval (ms); 100–10000 sınırlı; default 1000 (saniyede 1 check).
    pub fn arbitrage_tick_interval_ms(&self) -> u64 {
        self.arbitrage_tick_interval_ms
            .unwrap_or(1_000)
            .clamp(100, 10_000)
    }
    /// avg_sum cap (winner_bid + loser_bid); 0.50–0.999 sınırlı; default 0.95.
    pub fn arbitrage_cost_max(&self) -> f64 {
        self.arbitrage_cost_max
            .unwrap_or(0.95)
            .clamp(0.50, 0.999)
    }
    /// Order USDC notional; 0–10000 sınırlı; default 20.
    pub fn arbitrage_order_usdc(&self) -> f64 {
        self.arbitrage_order_usdc
            .unwrap_or(20.0)
            .clamp(0.0, 10_000.0)
    }
    /// Max trades per session; 0=sınırsız, 1–20 sınırlı; default 5.
    pub fn arbitrage_max_trades_per_session(&self) -> u32 {
        self.arbitrage_max_trades_per_session.unwrap_or(5).min(20)
    }
    /// Cooldown ms; 1000–60000 sınırlı; default 5000.
    pub fn arbitrage_cooldown_ms(&self) -> u64 {
        self.arbitrage_cooldown_ms
            .unwrap_or(5_000)
            .clamp(1_000, 60_000)
    }
    /// Entry window sn (T-X..T-0 arası ara); 30–600 sınırlı; default 300.
    pub fn arbitrage_entry_window_secs(&self) -> u32 {
        self.arbitrage_entry_window_secs
            .unwrap_or(300)
            .clamp(30, 600)
    }

    // === Binance Latency accessors ============================================
    /// Sinyal eşiği (USD); BTC delta `|now − open|` bunu aşmazsa NoOp.
    /// 1–500 sınırlı; default 50 (Bot 91 backtest sweet spot: WR %89, ROI %4.80).
    /// $80+ → WR %93 ama düşük frekans; $30 → WR %83 yüksek frekans.
    pub fn binance_latency_sig_thr_usd(&self) -> f64 {
        self.binance_latency_sig_thr_usd
            .unwrap_or(50.0)
            .clamp(1.0, 500.0)
    }
    /// Trade'ler arası min bekleme (ms); 1000–60000 sınırlı; default 3000.
    pub fn binance_latency_cooldown_ms(&self) -> u64 {
        self.binance_latency_cooldown_ms
            .unwrap_or(3_000)
            .clamp(1_000, 60_000)
    }
    /// Session başına max trade; 1–100 sınırlı; default 10.
    /// Backtest: mt=10 → NET +$8323, mt=50 → NET +$12808 (max NET ama düşük ROI).
    pub fn binance_latency_max_trades_per_session(&self) -> u32 {
        self.binance_latency_max_trades_per_session
            .unwrap_or(10)
            .clamp(1, 100)
    }
    /// Order USDC notional; 5–10000 sınırlı; default 100.
    pub fn binance_latency_order_usdc(&self) -> f64 {
        self.binance_latency_order_usdc
            .unwrap_or(100.0)
            .clamp(5.0, 10_000.0)
    }
    /// Entry window sn (T-X..T-0 arası ara); 15–600 sınırlı; default 300
    /// (tüm 5dk pencere).
    pub fn binance_latency_entry_window_secs(&self) -> u32 {
        self.binance_latency_entry_window_secs
            .unwrap_or(300)
            .clamp(15, 600)
    }
    /// Hedge leg notional (USDC); 0–100 sınırlı; default 0 (kapalı).
    /// Pure directional optimum; hedge ekleme NET'i azaltır (backtest).
    pub fn binance_latency_hedge_usdc(&self) -> f64 {
        self.binance_latency_hedge_usdc
            .unwrap_or(0.0)
            .clamp(0.0, 100.0)
    }
    /// Hedge için karşı taraf bid üst sınırı; 0.05–0.50 sınırlı; default 0.30.
    pub fn binance_latency_hedge_max_bid(&self) -> f64 {
        self.binance_latency_hedge_max_bid
            .unwrap_or(0.30)
            .clamp(0.05, 0.50)
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
