# Sinyal Zenginleştirme Planı v2 — Hafif ve Doğru

> **v1 özeti ve bu sürümde ne değişti:** İlk plan **6 ayrı sinyal kaynağı** ve **5 fazlı büyük migrasyon** öneriyordu. Bu sürüm, gerçek dünya verileri ışığında **tek kritik sinyal** (RTDS Chainlink Window Delta) etrafında organize edildi. Çünkü:
>
> - **Jonathan Peterson'ın `oracle-lag-sniper`** (15-dk crypto marketleri) yalnızca 3 basit kural kullanır — backtest: **5 017 trade, %61.4 win rate**, BTC/ETH/XRP/SOL'de tutarlı. Window Delta ≥ 0.07% + time-left ≥ 5 dk + token fiyatı ≤ $0.62.
> - **Archetapp BTC 5-dk botu:** 7 indikatör kullanıyor ama sahibi açıkça yazıyor — *Window Delta weight 5-7 = baskın sinyal*; diğer 6 indikatör ek %2-5 marjinal kazanım.
> - **Polymarket'in kendi oracle'ı = Chainlink BTC/USD Data Streams.** RTDS relay'i (`crypto_prices_chainlink`, **auth-siz, public, saniyede 1 tick**) bu feed'i birebir yayınlıyor. Bu bizim için çok değerli — tahmin değil, **resolution kaynağının canlı yayını.**
> - Binance futures, Coinbase, Bybit, funding rate, liquidations — hepsi **farklı fiyatı** ölçer. Polymarket'in resolution'ı **onları değil** Chainlink BTC/USD'yi baz alır. Yani bu ek sinyaller ≠ target = **noise** eklerler.
>
> **Yeni felsefe:** *Doğru fiyatı oku. Binance aggTrade'i ikincil "momentum tazeliği" olarak koru. Gerisi çöp.*

---

## 0. Nihai Tasarım Özeti

| Sinyal | Durum | Rol |
|---|:---:|---|
| **RTDS Chainlink Window Delta** | 🆕 **PRIMARY** | Ana karar sinyali — resolution ile birebir veri |
| **Binance aggTrade OFI** | ✅ mevcut, küçültülmüş | "Son 30 sn hareket hızı" — tie-breaker |
| Multi-exchange OFI (Coinbase, Bybit) | ❌ | İptal — target asset Chainlink, korelasyon değil eşitlik lazım |
| Funding rate delta | ❌ | İptal — 5-dk ufka göre çok yavaş |
| Liquidation flow | ❌ | İptal — noisy, nadir, ekstra WS maliyeti |
| Orderbook depth L5 | ❌ | İptal — Binance L5, Polymarket resolution'ı tahmin etmez |
| Composite ensemble | 🆕 **sadeleştirildi** | Sadece 2 sinyalin ağırlıklı ortalaması |

**Sonuç:** 6 yeni WS bağlantısı yerine **1 yeni WS** (RTDS). CPU ve bellek maliyeti neredeyse sıfır.

**Beklenen doğruluk (backtest referansı):**
- T-30s itibariyle 5-dk markette yön tahmini: %60–65 (oracle-lag-sniper'ın 15-dk sonucu %61.4 ile tutarlı)
- 15-dk / 1h / 4h marketlerde Window Delta eşiği artırılarak daha yüksek güvenli aralık

---

## 1. Neden Sadeleştirdik — Veri Destekli Analiz

### 1.1 Gerçek "bulunan edge"

Jonathan Peterson'ın [oracle-lag-sniper repo'sunda](https://github.com/JonathanPetersonn/oracle-lag-sniper) belgelediği edge:

> Chainlink oracle güncellemesi ~1 sn içinde gelir.
> Polymarket orderbook orta ~55 saniye geç tepki verir.
> Bu ~55 saniyelik pencerede bir taraf açıkça yanlış fiyatlıdır.

**3 kuralı:**
1. `|window_delta| ≥ 0.07%` (price moved enough)
2. `time_left ≥ 5 min` (yeterli zaman var)
3. `token_price ≤ $0.62` (henüz pahalıya kaçmamış)

Backtest: **5 017 trade, %61.4 win rate**, 7 falsification testi, 60/40 in-sample/out-of-sample split, 4 asset'te tutarlı.

Bu **tek başına** çalışan bir edge. Ek sinyallere gerek yok.

### 1.2 Archetapp BTC 5-dk Botu — Öğrenilen Ders

[Archetapp gist'inin](https://gist.github.com/Archetapp/7680adabc48f812a561ca79d73cbac69) kendi açıklaması:

> **Window Delta (weight 5-7) — THE dominant signal. This is the most important indicator by far.**

| Delta | Yorum | Weight |
|---|---|---|
| \> 0.10% | Decisive (nearly certain) | 7 |
| \> 0.02% | Strong | 5 |
| \> 0.005% | Moderate | 3 |

Diğer 6 indikatör toplamda weight ≈ 10 — yani yarısı bile değil. **80/20 kuralı açık.**

### 1.3 İptal Edilen Sinyallerin Gerekçesi

**Multi-exchange OFI (Coinbase, Bybit):**
Polymarket 5-dk marketleri **Chainlink BTC/USD Data Streams**'i resolution kaynağı olarak kullanır ([Polymarket docs — Oracles](https://polymarketguide.gitbook.io/polymarketguide/resolution/oracles)). Chainlink BTC/USD feed'i kendi içinde Binance + Coinbase + Bitstamp + Kraken gibi exchange'lerden **toplu medyan** alır. Biz RTDS'ten aynı feed'i çekerken, **ayrıca Coinbase/Bybit WS açmak** bu medyanı bir kez daha oy hakkı olmadan tekrarlamaya çalışmak olur. Noise ekler, sinyal eklemez.

**Funding rate delta:**
Funding 8 saatte bir ayarlanır; anlık değişimi 5-dk ufuk için **çok yavaş** bir metrik. Yüksek funding "kalabalık long" sinyali versin demek, bu **saatler içinde** reversal olabilir — 5 dakikada anlamsız. 15-dk için de marjinal. 1h / 4h için değerli olabilir ama **mevcut edge zaten yeterli**; ek karmaşıklık ROI vermez.

**Liquidations:**
Binance `@forceOrder` nadiren ateşler (saniyelerde 0–5 event); çoğu zaman boş. Window içinde cascade olursa bunu zaten **Window Delta ani sıçraması olarak** zaten yakalarız. Ayrı bir task maliyeti = duplicate veri.

**Orderbook depth L5 (Binance):**
Binance orderbook dengesizliği **Binance spot BTC fiyatı** için tahmincidir. Polymarket resolution'ı **Chainlink BTC/USD** feed'ini kullanır. Bu iki fiyat tipik olarak <0.02% farklıdır — bizim 0.07% eşiğimizden küçük. Yani orderbook imbalance bize Chainlink'in **zaten verdiği sinyali** gecikmeli olarak verir. Net ek bilgi = ~0.

**Composite ensemble (v1'deki 6-kanal):**
6 kanal ağırlıklandırması **overfitting risk** yaratır. Kalibrasyon script'i, logistic regression, A/B test framework — bunların hepsi bakım yükü. 2 sinyal için basit ağırlıklı ortalama yeterli.

---

## 2. Mimari Kısıt — Aynen Korunur

Mevcut mimari `bot-platform-mimari.md` §14 (Binance aggTrade) pattern'ini zaten kurmuş. Yeni RTDS modülü aynı pattern'e uyar:

| Kural | Uygulama |
|---|---|
| Critical path zero-block | RTDS okuma async, emir göndericiyi bloklamaz |
| State hazırda | `Arc<RwLock<RtdsState>>` |
| Async non-blocking | Ayrı `tokio::spawn` |
| DB fire-and-forget | Snapshot periodic, emir yolunda yok |
| Tahmin yok, API dışı ikame yok | RTDS = **aynı Chainlink feed**, resolution'ı değil sinyali besler |

**Önemli:** `market_resolved` event'i yine **sadece** Market WS'ten gelir. RTDS asla `winning_outcome` yazmaz. RTDS sadece strateji **kararı** için okunur.

---

## 3. Tek Yeni Modül: `src/rtds.rs`

### 3.1 Resmi kaynak

- **WS endpoint:** `wss://ws-live-data.polymarket.com`
- **Topic:** `crypto_prices_chainlink`
- **Filter:** `{"symbol": "btc/usd"}` (slash formatı)
- **Auth:** yok
- **Heartbeat:** her 5 sn **metin `PING`** (WS protocol ping DEĞİL — düz string)
- **Güncelleme hızı:** normalde saniyede 1 tick ([dev.to yazısı](https://dev.to/jonathanpetersonn/))
- **Bilinen quirk:** ~10-20 sn arada tick atlanabilir ([issue #31](https://github.com/Polymarket/real-time-data-client/issues/31)) — `last_tick_ts` ile stale detection şart

### 3.2 Payload

```json
{
  "topic": "crypto_prices_chainlink",
  "type": "update",
  "timestamp": 1753314088421,
  "payload": {
    "symbol": "btc/usd",
    "timestamp": 1753314088395,
    "value": 67234.50
  }
}
```

### 3.3 Slug eşlemesi

`src/slug.rs` halihazırda `{btc|eth|sol|xrp}-updown-*` tanıyor. RTDS sembolü:

| Slug öneki | RTDS sembolü |
|---|---|
| `btc-updown-*` | `btc/usd` |
| `eth-updown-*` | `eth/usd` |
| `sol-updown-*` | `sol/usd` |
| `xrp-updown-*` | `xrp/usd` |

### 3.4 State

```rust
#[derive(Debug, Clone, Default)]
pub struct RtdsState {
    /// Son Chainlink tick fiyatı (USD).
    pub current_price: f64,
    /// Pencere açılışında kaydedilen ilk tick.
    /// None → henüz pencere boundary'de tick gelmedi.
    pub window_open_price: Option<f64>,
    /// (current − open) / open × 10_000 (bps).
    pub window_delta_bps: f64,
    /// Son tick unix-ms. Stale detection için.
    pub last_tick_ms: u64,
    /// WS bağlı mı.
    pub connected: bool,
}

pub type SharedRtdsState = Arc<RwLock<RtdsState>>;
```

### 3.5 Task

```rust
/// Tek RTDS WS bağlantısı, pencere döngüsü süresince açık kalır.
/// Bağlantı kopunca exponential backoff (1s → 60s max) ile reconnect.
///
/// `zombie connection` koruması: 30 sn son tick'ten geçmişse,
/// WS ping-pong sağlıklı olsa bile force-reconnect.
pub async fn rtds_task(
    ws_url: String,
    symbol: String,                // "btc/usd"
    window_start_ts_ms: u64,       // pencere boundary
    state: SharedRtdsState,
) -> ! {
    loop {
        match connect_and_stream(&ws_url, &symbol, window_start_ts_ms, &state).await {
            Ok(()) => {
                // Normal disconnect → hızlı retry
                sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                tracing::warn!(?e, "rtds disconnected, backoff");
                // backoff logic
            }
        }
    }
}
```

**3 önemli nokta:**

1. **Metin PING her 5 sn:**
```rust
tokio::spawn(async move {
    loop {
        sleep(Duration::from_secs(5)).await;
        if ws_send("PING").await.is_err() { break; }
    }
});
```

2. **Zombie connection detection:** WS ping-pong canlı gösterse bile tick akmıyorsa fark et (gerçek hayatta yaşanan bug):
```rust
if now_ms().saturating_sub(state.read().await.last_tick_ms) > 30_000 {
    // force reconnect
    break;
}
```

3. **Pencere açılış fiyatı:** İlk tick `timestamp >= window_start_ts_ms` olduğunda yakalanır:
```rust
let ts_ms = payload.timestamp;
let mut s = state.write().await;
if s.window_open_price.is_none() && ts_ms >= window_start_ts_ms {
    s.window_open_price = Some(payload.value);
}
s.current_price = payload.value;
s.last_tick_ms = now_ms();
if let Some(open) = s.window_open_price {
    s.window_delta_bps = (s.current_price - open) / open * 10_000.0;
}
```

---

## 4. Sinyal Skorlaması — Basit ve Doğru

### 4.1 `window_delta_bps` → skor

**Tek fonksiyon**, ek bir katman yok:

```rust
/// `window_delta_bps` → [0, 10] skoru (5.0 = nötr).
///
/// 5-dk market için eşikler. Daha uzun pencere = daha büyük
/// hareket beklenir, eşikler `interval_scale` ile genişletilir.
///
/// Archetapp ve oracle-lag-sniper verileri ışığında kalibre edildi.
pub fn window_delta_score(bps: f64, interval_scale: f64) -> f64 {
    let x = bps / interval_scale;                       // 5-dk eş-değeri
    // Piecewise linear — sigmoid karmaşıklığı gerekmez
    let score_delta = match x.abs() {
        d if d < 0.5  => x * 0.4,                       // ±0.005%  → ±0.2
        d if d < 2.0  => x.signum() * (0.2 + (d - 0.5) * 0.8),  // ±0.02%  → ±1.4
        d if d < 7.0  => x.signum() * (1.4 + (d - 2.0) * 0.5),  // ±0.07%  → ±3.9 (EDGE EŞİĞİ)
        d if d < 15.0 => x.signum() * (3.9 + (d - 7.0) * 0.1375),  // ±0.15% → ±5.0
        _             => x.signum() * 5.0,              // clip
    };
    (5.0 + score_delta).clamp(0.0, 10.0)
}

/// Pencere süresine göre eşik ölçeği. 5-dk için 1.0 (baseline).
/// Volatilite √T ölçeklenir (GBM için standart).
pub const fn interval_scale(interval_secs: u64) -> f64 {
    match interval_secs {
        300   => 1.0,    // 5 dk
        900   => 1.73,   // 15 dk = √3 × 5dk
        3600  => 3.46,   // 1 saat = √12 × 5dk
        14400 => 6.93,   // 4 saat = √48 × 5dk
        _     => 1.0,    // fallback
    }
}
```

**Neden piecewise linear, sigmoid değil:** Sigmoid hesabı `exp()` çağırır (ms-altı olsa da). Piecewise karşılaştırma + çarpım = sıfıra yakın CPU. Kaliteli bir trading botta **deterministic** ve **branch-predictable** kod tercih edilir.

**Neden √T ölçeklenir:** BTC fiyat hareketi GBM (geometric Brownian motion) yaklaşımıyla modellenir → volatilite `√t` büyür. 15 dk'da beklenen hareket 5-dk'nınkinin √3 katı. Bu ölçek kendi kendini kalibre eder — ek parametre yok.

### 4.2 Binance aggTrade sinyali — rol değişti

Mevcut `binance_signal` (0–10, `src/binance.rs`) korunur ama rolü daralır:

| Sinyal | Eski rolü (v1) | Yeni rolü (v2) |
|---|---|---|
| `binance_signal` | Ana karar | "Son 30 sn momentum tazeliği" — tie-breaker |
| `window_delta_score` | Yeni ana karar | Primary |

Window Delta hareketi zaten gerçekleşmişse (örn. `+0.10%`) yönü belli. Binance `signal_score` sadece **teyit etmek** için kullanılır:

```
eğer window_delta güçlü (|score−5| > 2):
    → binance_signal kontrol: yön uyumlu mu?
      uyumluysa → eş güvenle geç
      tersseyse → pozisyonu küçült veya atla  (momentum reversing)
eğer window_delta zayıf (|score−5| ≤ 1):
    → binance_signal'i daha çok dinle
```

### 4.3 Composite — 2 sinyal, basit formül

```rust
pub fn composite_score(
    window_delta_score: f64,    // 0–10, PRIMARY
    binance_score: f64,         // 0–10, TIE-BREAKER
) -> f64 {
    // RTDS bağlı değilse window_delta_score = 5.0 (nötr) döner;
    // composite otomatik olarak binance'e ağırlık verir.
    let w_window = 0.70;
    let w_binance = 0.30;
    (w_window * window_delta_score + w_binance * binance_score).clamp(0.0, 10.0)
}
```

**Yalnızca iki ağırlık. Kalibrasyon gerekmez.** Sebepleri:

- **70/30 oranı:** Archetapp'ın 7-weight sistemini indirgersek window delta 5/~15 ≈ %33'ten fazla geliyor, %70 bizde daha da agresif çünkü sadece 2 sinyal var
- **Graceful degradation:** RTDS kopar → window_delta_score = 5.0 → composite = 0.7×5 + 0.3×binance = 3.5 + 0.3×binance → binance yine yön verir
- **A/B test yok:** Canlıda 1-2 hafta dryrun ile oran istenirse ayarlanır. Başlangıç değerleri sağlam.

### 4.4 `signal_weight` entegrasyonu (mevcut)

Mevcut `BotConfig.signal_weight` (0-10) aynı işlevini sürdürür:

```rust
pub fn effective_composite(
    composite: f64,     // composite_score çıktısı, 0-10
    signal_weight: u8,  // BotConfig alanı, 0-10
) -> f64 {
    5.0 + (composite - 5.0) * (signal_weight as f64 / 10.0)
}
```

- `signal_weight = 0` → her iki sinyal de devre dışı; nötr davranış (mevcut uyum)
- `signal_weight = 10` → composite tam uygulanır

---

## 5. Strateji Entegrasyonu

### 5.1 `harvest::dual_prices` — minimum değişiklik

Mevcut `dual_prices(effective_score, yes_bid, yes_ask, no_bid, no_ask, tick_size, min_price, max_price)` fonksiyonu bozulmaz. Sadece **`effective_score` kaynağı** değişir:

**Önce (v1, mevcut):**
```rust
let effective_score = effective_score_from_binance(binance_state, signal_weight);
let (up, down) = harvest::dual_prices(effective_score, ...);
```

**Sonra (v2):**
```rust
let window_score = window_delta_score(rtds.window_delta_bps, scale);
let composite = composite_score(window_score, binance.signal_score);
let effective_score = effective_composite(composite, signal_weight);
let (up, down) = harvest::dual_prices(effective_score, ...);
```

Strateji dokümanlarında (`strategies.md`) **hiçbir formül değişmez**. Sadece "`effective_score` nereden geliyor" değişir.

### 5.2 `dutch_book` ve `prism` — aynen

Mevcut matris (`MetricMask::binance_signal`) genişletilir:

```rust
// src/strategy.rs (mevcut MetricMask'a ek)
pub struct MetricMask {
    // ... mevcut alanlar ...
    pub binance_signal: bool,      // mevcut
    pub window_delta: bool,        // 🆕 — RTDS kullanılır mı
}

impl Strategy {
    pub const fn required_metrics(self) -> MetricMask {
        match self {
            Self::DutchBook | Self::Harvest | Self::Prism => MetricMask {
                // ... mevcut true'lar ...
                binance_signal: true,
                window_delta: true,   // Tüm stratejiler kullanır
            },
        }
    }
}
```

`window_delta: false` ayarlanırsa (özel test senaryosu), RTDS task başlatılmaz; mevcut `binance_signal` tek başına devam eder — geriye dönük uyum.

### 5.3 Bölge haritası

Window Delta her bölgede aktif. `StopTrade`'de zaten emir yok — sinyalin önemi kalmaz.

| Bölge | `window_delta` aktif | `binance_signal` aktif |
|---|:---:|:---:|
| `DeepTrade` (0–10%) | ✓ | ✓ |
| `NormalTrade` (10–50%) | ✓ | ✓ |
| `AggTrade` (50–90%) | ✓ | ✓ |
| `FakTrade` (90–97%) | ✓ | ✓ |
| `StopTrade` (97–100%) | — | — |

Mevcut `ZoneSignalMap` yapısı genişletilir veya ayrı `window_delta_zone_map` eklenir. **Basit yaklaşım:** `ZoneSignalMap` ikisini de kontrol eder; ayrı map gerek yok.

---

## 6. Data Model ve Persistance

### 6.1 SQLite — Minimal ek

`v1`'deki **14 kolonlu `signal_snapshots` tablosu gereksiz**. Mevcut tablolar yeterli; sadece 2 alan eklenir:

```sql
-- migrations/add_rtds_to_market_sessions.sql
ALTER TABLE market_sessions ADD COLUMN rtds_window_open_price REAL;
ALTER TABLE market_sessions ADD COLUMN rtds_window_open_ts_ms INTEGER;
```

Bu iki alan pencere açılışında ilk RTDS tick geldiğinde **bir kez** yazılır. Sonraki tick'ler kaydedilmez — DB spam yok.

**Neden `signal_snapshots` tablosu yok:** 1 sn cadence × 4 asset × N bot = SQLite yazma yükü. Gerek yok, çünkü:
- Anlık değerler frontend'e SSE ile gider (kalıcılık gerekmez)
- Market kapanınca sonuç ve açılış fiyatı zaten `market_sessions` + `market_resolutions`'ta

### 6.2 Loglama — tek satır

```
[10:15:00.150] [25] 🌐 RTDS connecting symbol=btc/usd
[10:15:00.451] [25] 🌐 RTDS connected
[10:15:00.502] [25] 🌐 RTDS window_open=67234.50 (ts=1766789700451)
[10:15:03.005] [25] 🌐 RTDS delta=+8.90bps price=67294.35 last_tick=2504ms
```

Emir satırında `signal=X(eff Y)`'nin yanına ek:

```
[10:15:58] [25] ✅ orderType=GTC side=BUY outcome=UP size=10 price=0.54 | reason=harvest:open_dual:yes | signal=6.80(eff 6.80) | window_delta=+8.90bps
```

---

## 7. IPC ve Frontend

### 7.1 `FrontendEvent` — yeni varyant

```rust
// src/ipc.rs
pub enum FrontendEvent {
    // ... mevcut ...

    /// 🆕 Her 1 sn frontend_timer'da bot pushes.
    RtdsUpdate {
        bot_id: i64,
        current_price: f64,
        window_open_price: Option<f64>,
        window_delta_bps: f64,
        ts_ms: u64,
    },
}
```

### 7.2 UI — tek panel

Pre-market (T-15 → T+0) VE in-market (T+0 → T+end) aynı panel:

```
┌─ Oracle Delta — btc-updown-5m-1776789700 ────┐
│                                                │
│  Chainlink live:   67,294.35 USD              │
│  Window open:      67,234.50                  │
│  Delta:            +59.85 (+8.90 bps ↑)       │
│                                                │
│  Window score:     7.45  (UP bias)            │
│  Binance (mom):    6.80                       │
│  Composite:        7.25                       │
│                                                │
│  Time left:        3m 12s                     │
└────────────────────────────────────────────────┘
```

Bu panel aynı zamanda "zombie feed" tespiti yapar: `last_tick_ms` 10 sn'den eski ise "⚠ feed stale" uyarısı.

---

## 8. Konfigürasyon

### 8.1 `BotConfig` — 2 yeni alan

```rust
pub struct BotConfig {
    // ... mevcut ...

    /// 🆕 RTDS aktif mi. Default: true (desteklenen kripto slug'lar için).
    pub rtds_enabled: bool,

    /// 🆕 Window delta ve binance arasındaki ağırlık.
    /// Default: 0.70 (window daha dominant).
    /// Aralık: [0.0, 1.0].
    pub window_delta_weight: f64,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            // ...
            rtds_enabled: true,
            window_delta_weight: 0.70,
        }
    }
}
```

### 8.2 Env

```bash
# .env.example (ek)
RTDS_WS_URL=wss://ws-live-data.polymarket.com
RTDS_STALE_THRESHOLD_MS=30000     # 30 sn tick gelmezse reconnect
RTDS_RECONNECT_MAX_BACKOFF_MS=60000
```

---

## 9. Mimari Etkisi — Sıfıra Yakın

### 9.1 Bağlantı ve CPU

| Bileşen | Mevcut (bot başı) | v1 önerisi (reddedildi) | v2 (bu plan) |
|---|---|---|---|
| WS bağlantıları | 3 (Market, User, Binance) | 8 (+ RTDS, Coinbase, Bybit, Funding, Liq, Depth) | **4** (+ RTDS) |
| RAM (ek) | — | ~250 KB | **~5 KB** |
| CPU (ek) | — | ~3-5% (Depth L5 100ms) | **~0.01%** (1 tick/sn) |
| DB yazma | mevcut | +N tablo, 1Hz yazım | **pencere başı bir kez** |

### 9.2 Kritik yol üstünde değil

RTDS okuma `Arc<RwLock<RtdsState>>.read().await` — O(1), `std::sync::atomic` benzeri. Emir göndericisi asla bloklanmaz.

---

## 10. Uygulama — Tek Faz

v1'deki 5-fazlı 24-35 günlük plan **çok büyük**. v2:

### Tek Faz — 2-4 iş günü

| Gün | İş |
|---|---|
| 1 | `src/rtds.rs` — WS task + state + metin PING + zombie detection |
| 1-2 | `window_delta_score`, `composite_score`, `effective_composite` fonksiyonları + unit testler |
| 2 | `src/bot/ctx.rs`, `window.rs`, `tasks.rs` — RTDS task spawn + pencere boundary reset |
| 2 | `harvest::dual_prices` çağrı yerinde `effective_score` kaynağı değişimi |
| 3 | SQLite migration, IPC event, UI paneli |
| 3-4 | Dryrun 24-48 saat; canlı log doğrulama; RTDS disconnect simülasyonu |

### Kabul kriterleri

- [ ] Unit testler: `window_delta_score` eşik matrisinde sapma < %1
- [ ] Entegrasyon: RTDS offline iken composite = 0.7×5 + 0.3×binance (graceful degrade)
- [ ] RTDS tick 30 sn atladığında otomatik reconnect loga düşer
- [ ] `window_open_price` pencere boundary'sinde tek satırda kaydedilir
- [ ] Mevcut bot davranışı değişmez (`rtds_enabled = false` ile → tıpkı şu an gibi çalışır)
- [ ] 24 saat dryrun sonunda Python `oracle-lag-sniper` ile fiyat eşleşme farkı < 1 tick

---

## 11. Terk Edilen Sinyallerin Kaydı — Neden Atmadık

Bu sinyalleri gelecekte ekleme olasılığı yok değil; ama **şu an** veri desteği yok.

### Gelecek adımlar için tetikler

| Sinyal | Ne zaman yeniden düşünülür |
|---|---|
| Multi-exchange OFI | Eğer Polymarket resolution kaynağını değiştirir veya farklı asset eklerse |
| Funding delta | 4-saatlik market aktifleşirse — ufuk yeterli uzun olur |
| Liquidations | Cascade olaylarının window delta'dan **önce** geldiği kanıtlanırsa |
| Orderbook depth | Binance orderbook'u hedef asset oluyorsa (bu platformda olmayacak) |

Ekleme kriteri: "Sharpe ratio artışı ≥ %10 ya da win rate artışı ≥ %2, 1000+ trade backtest ile doğrulandı."

---

## 12. Referanslar (doğrulanmış)

### Resmi

- Polymarket RTDS: https://docs.polymarket.com/market-data/websocket/rtds
- Polymarket Oracles rehberi: https://polymarketguide.gitbook.io/polymarketguide/resolution/oracles
- Chainlink Data Streams: https://docs.chain.link/data-streams

### Gerçek bot implementasyonları (birincil referans)

- **Jonathan Peterson — `oracle-lag-sniper`** (MIT): https://github.com/JonathanPetersonn/oracle-lag-sniper
  - 5 017 backtest trade, %61.4 win rate, 7 falsification testi
  - 3 basit kural: delta ≥ 0.07%, time ≥ 5m, price ≤ $0.62
  - Yazı 1: [dev.to oracle latency bot yazısı](https://dev.to/jonathanpetersonn/building-a-real-time-oracle-latency-bot-for-polymarket-with-python-and-asyncio-3gpg)
  - Yazı 2: [55 sn oracle lag anatomy](https://dev.to/jonathanpetersonn/i-tapped-into-a-public-websocket-feed-and-found-a-consistent-pricing-gap-on-polymarket-hiding-in-5h0k)

- **Archetapp — Polymarket BTC 5-dk bot** (gist): https://gist.github.com/Archetapp/7680adabc48f812a561ca79d73cbac69
  - 7 indikatör; yazarın itirafı "Window Delta = THE dominant signal"
  - Delta-based pricing model (`$0.50 flat token fake doğruluğunu` önler)

### Akademik

- Cartea, Jaimungal, Ricci (2018) — "Algorithmic Trading, Stochastic Control, and Mutually Exciting Processes" (oracle lag ve mispricing konsepti)
- Markwick, D. (2022) — "Order Flow Imbalance — A High Frequency Trading Signal" https://dm13450.github.io/2022/02/02/Order-Flow-Imbalance.html

### İç

- [`docs/bot-platform-mimari.md`](./bot-platform-mimari.md)
- [`docs/strategies.md`](./strategies.md)

---

## 13. Özet — Bir Bakışta

**v1'de teklif edilen:** 6 yeni WS kaynağı, 14-kolonlu yeni tablo, 5 faz 24-35 gün, kalibrasyon pipeline'ı, A/B framework.

**v2'de seçilen:** 1 yeni WS (RTDS), 2 kolon ALTER, 2-4 gün tek faz, 2-sinyal basit ağırlıklı ortalama.

**Argüman:** Gerçek çalışan bot (`oracle-lag-sniper`) tek sinyalle %61.4 win rate. Polymarket'in resolution'ı Chainlink BTC/USD ise, **Chainlink'i okumak başka bir şey okumaktan daha değerli değil, tamamen farklı bir kategoride bilgidir.** Ek sinyaller ek fiyatı **gürültü** ekleyerek sulandırır.

**Risk:** RTDS feed'i kopabilir → bu durumda composite `binance_signal` ağırlığına düşer; mevcut davranışla aynıdır. Sistem **zarar görmez**, sadece edge küçülür.

**İlk teslimat sonrası:** 2 hafta canlı dryrun, win rate ve Sharpe ölçümü. Eğer < 55% win rate → `window_delta_weight` parametresini indir (0.5'e kadar) → yeniden test. Hedef: **oracle-lag-sniper seviyesi (~%60) veya daha iyi** çünkü bizim harvest stratejisi delta-neutral pair trade yapar (win rate'den bağımsız kâr mümkün).