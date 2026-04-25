# Sinyal Zenginleştirme Planı — Implementation Reference

> **Bu doküman**, mevcut `binance_signal` katmanının üzerine inşa edilecek yeni sinyal kaynaklarının detaylı uygulama planıdır. Referans mimari: [`docs/bot-platform-mimari.md`](./bot-platform-mimari.md) §14 (Binance aggTrade sinyali) ve §15 (`MarketZone`).
>
> **Hedef:** `btc/eth/sol/xrp-updown-{5m,15m,1h,4h}` marketlerinde emir kararı öncesi sinyal gücünü artırmak. Mevcut tek-kaynaklı (Binance aggTrade) sinyali **4–6 kaynağa çıkararak** Window Delta, multi-exchange OFI, funding delta, likidasyon akışı ve derin orderbook dengesizliği ile zenginleştirmek.

---

## 0. Özet

| Maddee | Durum | Açıklama |
|---|:---:|---|
| Mevcut `binance_signal` (aggTrade, 0–10) | ✅ | `src/binance.rs`'de çalışıyor |
| Polymarket RTDS (Chainlink canlı fiyat) | 🆕 | **Faz 1 (MVP)** — en yüksek ROI |
| Window Delta (`current − window_open`) | 🆕 | RTDS'den türetilir — **dominant sinyal** |
| Multi-exchange OFI (Coinbase, Bybit) | 🆕 | Faz 2 |
| Funding rate delta (Binance markPrice) | 🆕 | Faz 3 |
| Liquidation flow (Binance forceOrder) | 🆕 | Faz 3 |
| Orderbook depth L5 dengesizliği | 🆕 | Faz 4 |
| Composite ensemble skor | 🆕 | Faz 5 — ağırlıklandırma + kalibrasyon |

**ROI tahmini (Archetapp Polymarket gist'inden ve akademik referanslardan):**
- Window Delta **tek başına** 5-dk marketlerde %70–85 yön tahmini doğruluğu (T-10s itibariyle)
- Diğer sinyaller eklendiğinde marginal gain %5–10, ama **ince farklara** yardımcı (flat markette tipping sinyali)

---

## 1. Mimari Kısıtlar ve Prensipler

### 1.1 Uyulması Zorunlu Kurallar

Yeni sinyal katmanı ekleme, mevcut mimariyi **bozmaz**. Aşağıdaki kurallar her yeni modül için geçerli:

| Kural | Referans | Uygulama |
|---|---|---|
| **Critical path zero-block** | `mimari.md` §⚡ Kural 1 | Yeni sinyal okuma emir göndericisini bloke etmemeli |
| **State önceden hazır** | §⚡ Kural 2 | Tüm yeni state `Arc<RwLock<…>>` ile hafızada tutulur |
| **Async, non-blocking** | §⚡ Kural 3 | Her yeni WS bağlantısı ayrı `tokio::spawn` |
| **DB fire-and-forget** | §⚡ Kural 4 | Sinyal persistance `tokio::spawn`'da arka planda |
| **WS reader önceliği** | §⚡ Kural 6 | Yeni sinyal WS'leri mevcut Market/User WS'i yavaşlatmaz |
| **Tahmin ve API-dışı ikame yok** | "Resmi API kaynağı" bölümü | Yeni sinyaller `market_resolved`'ü etkilemez; yalnız auxiliary metric |

### 1.2 RTDS Özel Notu — Prediction Değil, Oracle'ın Canlı Okuması

Mimaride **§"Tahmin ve API dışı ikame yok"** kuralı **resolution** içindir. Polymarket RTDS `crypto_prices_chainlink` topic'i aynı **Chainlink BTC/USD** feed'ini yayınlar — yani Polymarket'in kendi resolution kaynağı ile **birebir aynı** veri. Bu nedenle:

- ✅ RTDS okuması **tahmin değil**, resolution kaynağının canlı mirror'ı
- ✅ Log'da "Binance-based estimate" değil, **"Chainlink live delta"** etiketi kullanılır
- ✅ `winning_outcome` yine **sadece** Market WS `market_resolved` olayından yazılır — RTDS'den değil
- ❌ RTDS verisiyle sentetik `market_resolved` üretmek **yasaktır** (zaten gerekmez — WS olayı beklenir)

### 1.3 Sinyal vs. Resolution Ayrımı

Mevcut `binance_signal` gibi, yeni sinyaller de **auxiliary metric**'tir. Şu kurallara uyulur:

```
┌─────────────────────────────────────────────────────────┐
│  AUXILIARY SIGNAL  (strateji motoru karar için okur)    │
│  ─────────────────                                      │
│  • binance_signal (aggTrade OFI)                        │
│  • rtds_window_delta  ← YENİ                            │
│  • multi_exchange_ofi ← YENİ                            │
│  • funding_delta      ← YENİ                            │
│  • liquidation_pressure ← YENİ                          │
│  • orderbook_depth_imb_l5 ← YENİ                        │
│                                                         │
│  Tümü hafızada `Arc<RwLock<…>>` — emir öncesi okunur    │
└─────────────────────────────────────────────────────────┘
                          ↑
                   strateji motoru
                          ↓
┌─────────────────────────────────────────────────────────┐
│  RESOLUTION (tek doğruluk kaynağı: Polymarket Market WS)│
│  ─────────                                              │
│  • event_type: "market_resolved"                        │
│  • winning_outcome                                      │
│  • winning_asset_id                                     │
│                                                         │
│  Sinyal kaynakları BURAYA DOKUNAMAZ                     │
└─────────────────────────────────────────────────────────┘
```

---

## 2. Mevcut Durum Özeti

### 2.1 Çalışan Sinyal Pipeline

```
wss://fstream.binance.com/ws/<symbol>@aggTrade
         ↓
  src/binance.rs::binance_aggtrade_task
         ↓
  SignalComputer (CVD/BSI/OFI)
         ↓
  Arc<RwLock<BinanceSignalState>>
         ↓
  strategy::harvest::dual_prices(effective_score, …)
         ↓
  OpenDual emir fiyatı + averaging boyut çarpanı
```

### 2.2 Mevcut State Struct'ı

```rust
// src/binance.rs (mevcut)
pub struct BinanceSignalState {
    pub cvd: f64,
    pub bsi: f64,
    pub ofi: f64,
    pub signal_score: f64,      // 0–10
    pub warmup: bool,
    pub updated_at_ms: u64,
    pub connected: bool,
}
```

### 2.3 Boşluklar

- ❌ Window Delta yok — market açıldığında "şu an fiyat referansın üstünde mi altında mı?" bilgisi yok
- ❌ Sadece Binance; Coinbase/Bybit baskıları görünmez
- ❌ Funding rate gözlemlenmiyor (sentiment shift sinyali kaçırılıyor)
- ❌ Likidasyon kaskadı bilinmiyor (volatility spike erken uyarı kaybı)
- ❌ Derin orderbook (L5) okunmuyor; sadece trade akışı var

---

## 3. Yeni Sinyal Kaynakları

### 3.1 Polymarket RTDS — Chainlink Canlı Fiyat (Faz 1, MVP)

**Amaç:** Market başlangıç fiyatını (`window_open_price`) ve anlık Chainlink fiyatını (`current_chainlink_price`) izleyerek **Window Delta** türetmek.

#### Kaynak

| Alan | Değer |
|---|---|
| Endpoint | `wss://ws-live-data.polymarket.com` (RTDS) |
| Topic | `crypto_prices_chainlink` |
| Filter örneği | `{"asset": "btc/usd"}` (JSON filter) veya string |
| Auth | **Gerekmez** |
| Protokol | JSON WebSocket, üye mesaj biçimi resmi dokümanda |
| Resmi doküman | [docs.polymarket.com/developers/RTDS/RTDS-crypto-prices](https://docs.polymarket.com/developers/RTDS/RTDS-crypto-prices) |

**Not:** RTDS WS base URL'i implementasyon sırasında resmi dokümandan **güncel** okunmalıdır. `docs.polymarket.com` tek doğruluk kaynağıdır; repo içi sabit string kullanılmaz.

#### Abone Mesajı (Taslak)

```json
{
  "action": "subscribe",
  "subscriptions": [
    {
      "topic": "crypto_prices_chainlink",
      "type": "update",
      "filters": "btc/usd"
    }
  ]
}
```

#### Olay Payload'ı

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

#### Slug → Sembol Eşlemesi

Mevcut `src/slug.rs` ile uyumlu genişletme:

| Market slug öneki | RTDS sembolü |
|---|---|
| `btc-updown-*` | `btc/usd` |
| `eth-updown-*` | `eth/usd` |
| `sol-updown-*` | `sol/usd` |
| `xrp-updown-*` | `xrp/usd` |

#### Pencere Açılış Fiyatı Tespiti

Polymarket dokümanı ve topluluk bulgularına göre pencere açılış fiyatı = **window boundary timestamp'inde veya hemen sonrasındaki ilk RTDS tick**.

```rust
// Pseudocode
if rtds_tick.timestamp >= session.start_ts * 1000 && session.window_open_price.is_none() {
    session.window_open_price = Some(rtds_tick.value);
    // Bu değer "Price to Beat" ile birebir eşleşir.
}
```

#### Türetilen Metrik: `window_delta_bps`

```
window_delta_bps = (current_price − window_open_price) / window_open_price × 10_000
```

| `window_delta_bps` | Yorumlama (5-dk market için) | Archetapp skoru |
|---|---|:---:|
| `≥ +10.0` (≥ +0.10%) | "Up" neredeyse kesin | +7 |
| `+2.0 … +10.0` | Güçlü Up | +5 |
| `+0.5 … +2.0` | Orta Up | +3 |
| `+0.1 … +0.5` | Hafif Up | +1 |
| `−0.1 … +0.1` | Flat (coin flip) | 0 |
| `−0.5 … −0.1` | Hafif Down | −1 |
| `−2.0 … −0.5` | Orta Down | −3 |
| `−10.0 … −2.0` | Güçlü Down | −5 |
| `≤ −10.0` (≤ −0.10%) | "Down" neredeyse kesin | −7 |

**Not:** Eşikler **pencere süresine** göre ölçeklenir — 15-dk'da aynı eşikler `× 1.7`, 1-saatte `× 3.5`, 4-saatte `× 7` (volatilite karesi). Kalibre değer akademik literatür ve backtestden gelir; başlangıç değerleri `BotConfig`'e parametre olarak konulabilir.

#### Rust Modül Taslağı

**Yeni dosya:** `src/rtds.rs`

```rust
//! Polymarket RTDS (Real-Time Data Socket) — Chainlink live price stream.
//!
//! §3.1 Sinyal Zenginleştirme Planı.
//!
//! Her bot kendi sembolü için ayrı bir task başlatır; state `Arc<RwLock<…>>`
//! ile strateji katmanına açılır. Bağlantı koptuğunda exponential backoff
//! ile yeniden dener; bu süre boyunca `window_delta_bps = 0.0` (nötr).

use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RtdsState {
    /// Chainlink BTC/USD (veya eşlenen sembol) son fiyatı.
    pub current_price: f64,
    /// Pencere açılışında kaydedilen ilk tick (Price to Beat).
    /// `None` → henüz pencere başlamadı veya RTDS tick gelmedi.
    pub window_open_price: Option<f64>,
    /// (current − open) / open × 10_000  (bps).
    pub window_delta_bps: f64,
    pub last_update_ms: u64,
    pub connected: bool,
}

impl Default for RtdsState {
    fn default() -> Self {
        Self {
            current_price: 0.0,
            window_open_price: None,
            window_delta_bps: 0.0,
            last_update_ms: 0,
            connected: false,
        }
    }
}

pub type SharedRtdsState = Arc<RwLock<RtdsState>>;

pub fn new_shared_state() -> SharedRtdsState {
    Arc::new(RwLock::new(RtdsState::default()))
}

/// RTDS WS bağlantı task'ı.
///
/// - `symbol`: "btc/usd" | "eth/usd" | "sol/usd" | "xrp/usd"
/// - `window_start_ts_ms`: pencere açılış zamanı; RTDS tick bu zamanda veya
///   sonrasında geldiyse `window_open_price` olarak kaydedilir.
/// - Bağlantı koptuğunda exponential backoff (1s → 2s → 4s → … → 60s).
pub async fn rtds_task(
    symbol: String,
    window_start_ts_ms: u64,
    state: SharedRtdsState,
) -> ! {
    // Resmi endpoint URL'i runtime'da config'den gelir.
    // Subscribe mesajı: crypto_prices_chainlink, filter=symbol
    // Her update için:
    //   1. state.current_price = tick.value
    //   2. window_open_price yoksa ve tick.timestamp >= window_start_ts_ms
    //      ise window_open_price = tick.value
    //   3. window_delta_bps = ((current − open) / open) × 10_000
    //   4. state.last_update_ms = now_ms()
    todo!("implementation in Phase 1")
}

/// `window_delta_bps` değerini strateji motorunun okuyabileceği
/// standart `[0, 10]` skor aralığına çevirir.
///
/// Varsayılan eşik tablosu 5-dk market içindir; pencere süresi
/// `interval_scale` ile ölçeklenir (15m=1.7, 1h=3.5, 4h=7.0).
pub fn window_delta_score(window_delta_bps: f64, interval_scale: f64) -> f64 {
    let scaled = window_delta_bps / interval_scale;
    // Sigmoid benzeri mapping: +10 bps → ~7.5; -10 bps → ~2.5; 0 → 5.0
    let clamped = scaled.clamp(-15.0, 15.0);
    5.0 + (clamped / 15.0) * 5.0
}
```

#### Log Örnekleri

```
[10:15:00.123] [25] 🌐 RTDS connecting (symbol=btc/usd)...
[10:15:00.451] [25] 🌐 RTDS connected — subscribing crypto_prices_chainlink btc/usd
[10:15:00.502] [25] 🌐 RTDS window_open=67234.50 (ts=1766789700)
[10:15:00.951] [25] 🌐 RTDS delta=+1.23 bps price=67242.76
[10:15:03.005] [25] 🌐 RTDS delta=+8.90 bps price=67294.35
...
[10:15:58.000] [25] ✅ orderType=GTC side=BUY outcome=UP size=10 price=0.5 | reason=harvest:open_dual:yes | signal=6.50(eff 6.50) | rtds_delta=+8.90bps rtds_score=7.97
```

#### IPC Event

Mevcut `FrontendEvent` enum'una eklenir:

```rust
// src/ipc.rs (mevcut enum'a ek)
RtdsUpdate {
    bot_id: i64,
    symbol: String,
    current_price: f64,
    window_open_price: Option<f64>,
    window_delta_bps: f64,
    ts_ms: u64,
},
```

Frontend polling ile değil **SSE push** alır; 1 sn cadence yeterli (aşırı dense değil).

---

### 3.2 Multi-Exchange OFI (Faz 2)

**Amaç:** Binance'in tek kaynak olmasından doğan bias'ı gidermek. Coinbase ve Bybit'te farklı trader tabanları var — konsensus sinyal daha güvenilir.

#### Kaynaklar

| Exchange | Endpoint | Sembol formatı | Notlar |
|---|---|---|---|
| Binance USD-M Futures | `wss://fstream.binance.com/ws/<sym>@aggTrade` | `btcusdt` | ✅ Mevcut |
| Coinbase Advanced | `wss://advanced-trade-ws.coinbase.com` | `BTC-USD` | US institutional baskısı önemli |
| Bybit Perpetual | `wss://stream.bybit.com/v5/public/linear` | `BTCUSDT` | Asya / retail baskısı |

#### Sembol Eşlemesi

```rust
impl SlugAsset {
    pub fn binance_sym(&self) -> &'static str { /* btcusdt, ethusdt, ... */ }
    pub fn coinbase_sym(&self) -> &'static str { /* BTC-USD, ETH-USD, ... */ }
    pub fn bybit_sym(&self) -> &'static str { /* BTCUSDT, ETHUSDT, ... */ }
    pub fn rtds_sym(&self) -> &'static str { /* btc/usd, eth/usd, ... */ }
}
```

#### Konsolide OFI

Her borsanın kendi OFI'sini hesapla (§14.2'deki formülle aynı), sonra **hacim-ağırlıklı ortalama**:

```
total_volume = vol_binance + vol_coinbase + vol_bybit
weight_binance = vol_binance / total_volume
...

consolidated_ofi = w_b × ofi_binance + w_c × ofi_coinbase + w_y × ofi_bybit
```

**Neden hacim-ağırlıklı?** Bybit gibi düşük hacimli borsaların noise'u dominant olmasın diye. Binance BTCUSDT hacmi Coinbase'in ~5–10 katı; ağırlık da o oranda.

#### Rust Taslağı

**Yeni dosya(lar):** `src/signals/coinbase.rs`, `src/signals/bybit.rs`, `src/signals/ensemble_ofi.rs`

```rust
// src/signals/mod.rs
pub mod binance;      // mevcut src/binance.rs'den taşınır
pub mod coinbase;     // yeni
pub mod bybit;        // yeni
pub mod ensemble_ofi; // yeni

pub struct ConsolidatedOfiState {
    pub ofi_binance: f64,
    pub ofi_coinbase: f64,
    pub ofi_bybit: f64,
    pub vol_binance: f64,
    pub vol_coinbase: f64,
    pub vol_bybit: f64,
    pub consolidated_score: f64,  // 0–10, z-score'lu
    pub last_update_ms: u64,
}
```

#### Uyarı

- **Coinbase** ve **Bybit** WS'leri Binance'den farklı payload şemasına sahiptir — her biri için ayrı parser gerekir.
- Bağlantı kopunca o borsanın ağırlığı `0` olur; diğerleri devam eder (graceful degradation).
- **Geri uyumluluk:** `binance_signal` alanı korunur; `consolidated_score` paralel olarak eklenir. Strateji motoru hangisini kullanacağına `BotConfig.signal_source` parametresiyle karar verir.

---

### 3.3 Funding Rate Delta (Faz 3)

**Amaç:** Perpetual futures'ta funding rate değişimi **sentiment shift** sinyalidir. Artan pozitif funding → long kalabalığı (ters hareket riski); azalan funding → kalabalık dağılıyor (trend teyidi).

#### Kaynak

| Alan | Değer |
|---|---|
| Endpoint | `wss://fstream.binance.com/ws/!markPrice@arr@1s` |
| Payload | Tüm sembollerin mark price + funding rate snapshot'ı (1 sn cadence) |
| Alternatif | `<symbol>@markPrice@1s` (tek sembol) |

#### Türetilen Metrikler

```rust
pub struct FundingState {
    /// Anlık funding rate (saatlik, %).
    pub current_funding_rate: f64,
    /// Son 5 dk ortalaması.
    pub ma_5min: f64,
    /// Son 30 sn içinde değişim (delta).
    pub delta_30s: f64,
    /// Mark price — spot ile farkı premium sinyali.
    pub mark_price: f64,
    pub index_price: f64,
    pub premium: f64,  // (mark − index) / index × 10_000
    pub last_update_ms: u64,
}
```

#### Yorumlama

| Koşul | Sinyal |
|---|---|
| `delta_30s > 0` + `premium > +5 bps` | Long kalabalık artıyor → ters hareket (Down) riski ↑ |
| `delta_30s < 0` + `premium < -5 bps` | Short kalabalık artıyor → ters hareket (Up) riski ↑ |
| `|delta_30s| < 0.01` | Nötr, sinyal yok |

#### Ürün Notu

Funding rate 5-dk market için **ikincil** sinyaldir (funding değişimi genelde saatler içinde anlamlı olur). Ama kısa vadede **extreme funding** reversal'ı tetikler — `abs(current_funding_rate) > 0.1%` durumlarında sinyal ağırlığı artırılır.

---

### 3.4 Liquidation Flow (Faz 3)

**Amaç:** Cascade liquidation'ları erken tespit etmek. Long liquidation dalgası = aşağı yönlü accelerating move; short liquidation = yukarı.

#### Kaynak

| Alan | Değer |
|---|---|
| Endpoint | `wss://fstream.binance.com/ws/<symbol>@forceOrder` |
| Payload | Her likidasyon olayı tek mesaj |

#### Payload Örneği

```json
{
  "e": "forceOrder",
  "o": {
    "s": "BTCUSDT",
    "S": "SELL",           // SELL = long liquidation; BUY = short liquidation
    "o": "LIMIT",
    "q": "0.500",          // miktar
    "p": "67200.00",
    "ap": "67150.00",      // average price
    "X": "FILLED",
    "T": 1766789700000
  }
}
```

#### Türetilen Metrikler

```rust
pub struct LiquidationState {
    /// Son 30 sn'de long likidasyonu USD toplamı.
    pub long_liq_30s: f64,
    /// Son 30 sn'de short likidasyonu USD toplamı.
    pub short_liq_30s: f64,
    /// Son 5 dk kümülatif.
    pub long_liq_5m: f64,
    pub short_liq_5m: f64,
    /// Oran: long_liq / (long_liq + short_liq) — 0.5 nötr.
    pub liq_ratio_30s: f64,
    pub liq_ratio_5m: f64,
    pub last_update_ms: u64,
}
```

#### Yorumlama

| `liq_ratio_30s` | Anlam | Sinyal |
|---|---|---|
| `> 0.7` | Long cascade devam ediyor | Down pressure ↑ |
| `0.5–0.7` | Hafif long baskı | Nötr-Down |
| `0.3–0.5` | Hafif short baskı | Nötr-Up |
| `< 0.3` | Short cascade | Up pressure ↑ |

#### Eşikler

- `total_liq_30s < $100_000` → veri zayıf, ratio ignore edilir
- `total_liq_30s > $1_000_000` → güçlü cascade, sinyal ağırlığı 2×

---

### 3.5 Orderbook Depth L5 Dengesizliği (Faz 4)

**Amaç:** Akademik bulgular ([Towards Data Science — Order Book Imbalance](https://towardsdatascience.com/price-impact-of-order-book-imbalance-in-cryptocurrency-markets-bf39695246f6/)): **L5 imbalance L1'den daha iyi kısa-vadeli fiyat tahmincisi.**

#### Kaynak

| Alan | Değer |
|---|---|
| Endpoint | `wss://fstream.binance.com/ws/<symbol>@depth20@100ms` |
| Güncelleme | 100 ms diff-depth |
| REST snapshot | `GET /fapi/v1/depth?symbol=<sym>&limit=20` (ilk sync için) |

#### Formül

```
bid_volume_L5 = Σ bids[0..5].qty
ask_volume_L5 = Σ asks[0..5].qty

imbalance_L5 = (bid_volume_L5 − ask_volume_L5) / (bid_volume_L5 + ask_volume_L5)
```

Aralık: `[-1, +1]`. `> 0` = bid baskısı (Up eğilimi), `< 0` = ask baskısı (Down eğilimi).

#### Rust Taslağı

```rust
pub struct DepthImbalanceState {
    pub bid_volume_l5: f64,
    pub ask_volume_l5: f64,
    pub imbalance_l5: f64,       // [-1, +1]
    /// L1 kıyas için (mevcut).
    pub imbalance_l1: f64,
    /// Kayan ortalamaya dayalı skor (0–10).
    pub depth_score: f64,
    pub last_update_ms: u64,
}
```

#### Uyarı

- Orderbook state senkronizasyonu **karmaşıktır**. Binance'in diff-depth protokolü:
  1. REST `GET /depth` ile snapshot al
  2. WS'den gelen `U` / `u` alanlarıyla diff uygula
  3. Sequence number gap olursa baştan başla
- Mevcut `src/polymarket.rs`'deki Polymarket book handling'in **benzer** deseni ile yapılır ama Binance şeması farklı.
- Bu en **yüksek CPU maliyetli** sinyal — 100 ms cadence × 4 sembol = saniyede 40 güncelleme.

---

### 3.6 Open Interest Momentum (Opsiyonel)

**Amaç:** OI artışı + fiyat artışı = güçlü trend teyidi; OI düşüşü + fiyat artışı = short squeeze (sürdürülebilir değil).

#### Kaynak

| Alan | Değer |
|---|---|
| Binance REST | `GET /fapi/v1/openInterest?symbol=<sym>` |
| Cadence | Binance'te WS yok; 15 sn polling uygun |
| Alternatif | Coinglass API (ücretli — $99/ay) multi-exchange aggregate OI |

#### Ürün Kararı

Bu sinyal **opsiyonel**. 5-dk market için yüksek ROI **değildir** (OI tipik olarak 1+ saatlik hareketlerde anlamlı). 1h / 4h marketlerde değerli. Faz 5'te değerlendirilir.

---

## 4. Composite Ensemble Skor

Tüm sinyalleri **tek bir skor** (`composite_score` ∈ [0, 10]) altında birleştirir. Mevcut `effective_score` yerine (veya paralel olarak) bu skor strateji motoruna verilir.

### 4.1 Ağırlıklı Ortalama

```rust
pub struct CompositeWeights {
    pub window_delta: f64,      // default 0.40 — dominant
    pub binance_aggtrade: f64,  // default 0.20
    pub multi_exchange_ofi: f64,// default 0.15
    pub funding_delta: f64,     // default 0.05
    pub liquidation: f64,       // default 0.05
    pub depth_l5: f64,          // default 0.15
}

impl Default for CompositeWeights {
    fn default() -> Self {
        Self {
            window_delta: 0.40,
            binance_aggtrade: 0.20,
            multi_exchange_ofi: 0.15,
            funding_delta: 0.05,
            liquidation: 0.05,
            depth_l5: 0.15,
        }
    }
}

pub fn composite_score(
    window_delta_score: f64,    // 0–10
    binance_score: f64,         // 0–10 (mevcut)
    multi_ofi_score: f64,       // 0–10 (veya None → binance'den düş)
    funding_score: f64,         // 0–10 (veya None)
    liq_score: f64,             // 0–10 (veya None)
    depth_score: f64,           // 0–10 (veya None)
    weights: &CompositeWeights,
) -> f64 {
    // Kayıp sinyaller için graceful degradation:
    // Her sinyalin `Option<f64>` olduğunu varsayıyoruz; None ise
    // ağırlığı diğerlerine redistribüte et (veya nötr 5.0 kullan).
    // ...
    todo!("implementation with missing-signal handling")
}
```

### 4.2 Kalibrasyon

Başlangıç ağırlıkları heuristic (Archetapp gist + akademik literatür). Uygulamada:

1. **Her pencere sonunda**: `(composite_score_t_minus_10, actual_outcome)` çifti SQLite'a kaydedilir
2. **Haftalık batch job**: Logistic regression ile ağırlıklar yeniden fit edilir
3. **A/B test**: `BotConfig.composite_weights = BacktestCalibrated | Heuristic`

### 4.3 `signal_weight` ile İlişki

Mevcut `signal_weight` (0–10) composite skora **aynı şekilde** uygulanır:

```
effective_composite = 5.0 + (composite_score − 5.0) × (signal_weight / 10.0)
```

`signal_weight = 0` → tüm sinyal katmanı devre dışı; nötr davranış.

### 4.4 Bölge Haritası Uyumu

`MarketZone` × sinyal aktifliği matrisi genişletilir:

| Bölge | Window Delta | Binance | Multi-OFI | Funding | Liq | Depth |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| `DeepTrade` (0–10%) | ✓ | ✓ | ✓ | — | — | ✓ |
| `NormalTrade` (10–50%) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `AggTrade` (50–90%) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `FakTrade` (90–97%) | ✓ | ✓ | ✓ | — | ✓ | ✓ |
| `StopTrade` (97–100%) | — | — | — | — | — | — |

**Not:** `DeepTrade` ve `FakTrade`'de funding sinyali **pasif** — bu bölgelerde funding trendi kısa zaman penceresi için anlamsız.

---

## 5. Modül Yapısı

### 5.1 Dizin Planı

```
src/
├── binance.rs              # (mevcut — olduğu gibi kalır veya signals/binance.rs'e taşınır)
├── rtds.rs                 # 🆕 Faz 1: Polymarket RTDS
├── signals/                # 🆕 Faz 2+
│   ├── mod.rs              # Ortak trait'ler + shared state
│   ├── binance.rs          # binance.rs'den taşınır (opsiyonel)
│   ├── coinbase.rs         # 🆕 Faz 2
│   ├── bybit.rs            # 🆕 Faz 2
│   ├── ensemble_ofi.rs     # 🆕 Faz 2
│   ├── funding.rs          # 🆕 Faz 3
│   ├── liquidations.rs     # 🆕 Faz 3
│   ├── orderbook_depth.rs  # 🆕 Faz 4
│   └── composite.rs        # 🆕 Faz 5: ensemble skoru
├── bot/
│   ├── ctx.rs              # 🔧 yeni state Arc'larını ekle
│   ├── tasks.rs            # 🔧 yeni task'ları spawn et
│   └── window.rs           # 🔧 sinyal state'lerini pencere başlangıcında sıfırla
└── ipc.rs                  # 🔧 yeni FrontendEvent varyantları
```

### 5.2 Ortak Trait (`signals/mod.rs`)

```rust
//! Her sinyal kaynağı bu trait'i implement eder; ensemble layer
//! için uniform erişim sağlar.

use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait::async_trait]
pub trait SignalSource: Send + Sync {
    /// Sinyal adı (log/metric için).
    fn name(&self) -> &'static str;

    /// Anlık skor [0, 10]. None = sinyal yok / warmup / disconnected.
    async fn current_score(&self) -> Option<f64>;

    /// Sinyal son güncelleme zamanı (ms).
    async fn last_update_ms(&self) -> u64;

    /// Bağlantı durumu (WS açık mı).
    async fn is_connected(&self) -> bool;
}
```

Her sinyal modülü (`rtds.rs`, `coinbase.rs`, vb.) `SignalSource` implement eder. `composite.rs` bir `Vec<Box<dyn SignalSource>>` üzerinden ensemble skoru hesaplar.

### 5.3 Bot Context Genişletmesi

```rust
// src/bot/ctx.rs (mevcut Ctx struct'ına ek)
pub struct Ctx {
    // ... mevcut alanlar ...

    // 🆕 Sinyal state'leri:
    pub rtds_state: SharedRtdsState,
    pub binance_signal_state: SharedBinanceSignalState,       // mevcut
    pub coinbase_signal_state: Option<SharedSignalState>,     // Faz 2
    pub bybit_signal_state: Option<SharedSignalState>,        // Faz 2
    pub funding_state: Option<SharedFundingState>,            // Faz 3
    pub liquidation_state: Option<SharedLiquidationState>,    // Faz 3
    pub depth_state: Option<SharedDepthState>,                // Faz 4
    pub composite_weights: CompositeWeights,                  // Faz 5
}
```

---

## 6. Konfigürasyon

### 6.1 `BotConfig` Genişletmesi

```rust
// src/strategy.rs veya src/config.rs
pub struct BotConfig {
    // ... mevcut alanlar ...

    /// 🆕 Hangi sinyal kaynakları aktif?
    pub signal_sources: SignalSourceFlags,

    /// 🆕 Composite skor ağırlıkları (None → default).
    pub composite_weights: Option<CompositeWeights>,

    /// 🆕 Window Delta eşik ölçeği (interval'a göre otomatik).
    /// Kullanıcı override etmek isterse.
    pub window_delta_scale_override: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SignalSourceFlags {
    pub rtds: bool,              // default: true (Faz 1'den sonra)
    pub binance_aggtrade: bool,  // default: true (mevcut)
    pub coinbase: bool,          // default: false (Faz 2)
    pub bybit: bool,             // default: false (Faz 2)
    pub funding: bool,           // default: false (Faz 3)
    pub liquidations: bool,      // default: false (Faz 3)
    pub depth_l5: bool,          // default: false (Faz 4)
}
```

### 6.2 Environment Variables

```bash
# .env.example (mevcut listeye ek)

# Faz 1: RTDS
RTDS_WS_URL=wss://ws-live-data.polymarket.com     # runtime'da resmi dokümanla doğrula
RTDS_RECONNECT_MAX_BACKOFF_MS=60000

# Faz 2: Multi-exchange
COINBASE_WS_URL=wss://advanced-trade-ws.coinbase.com
BYBIT_WS_URL=wss://stream.bybit.com/v5/public/linear

# Faz 3: Binance alt akışlar (aynı base URL)
# - !markPrice@arr@1s (funding)
# - <sym>@forceOrder (liquidations)

# Sinyal zaman aşımı (hepsi için ortak)
SIGNAL_STALE_THRESHOLD_MS=5000   # 5 sn eski sinyal nötrleştirilir
```

### 6.3 Frontend Form Genişletmesi

Bot oluşturma / ayar formuna yeni alanlar:

```
☐ RTDS (Chainlink live delta)       [Faz 1 — default ON]
☐ Binance aggTrade                   [default ON]
☐ Coinbase advanced-trade            [Faz 2]
☐ Bybit perpetual                    [Faz 2]
☐ Binance funding rate               [Faz 3]
☐ Binance liquidations               [Faz 3]
☐ Binance orderbook depth L5         [Faz 4]

Composite weights (advanced): [ ... numeric sliders ... ]
```

---

## 7. SQLite Şema Genişletmeleri

### 7.1 Yeni Tablo: `signal_snapshots`

```sql
CREATE TABLE signal_snapshots (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id               INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id    INTEGER NOT NULL REFERENCES market_sessions(id),
    ts_ms                INTEGER NOT NULL,

    -- Her sinyalden anlık skor / ham değer
    window_delta_bps     REAL,
    window_delta_score   REAL,
    binance_signal_score REAL,       -- mevcut
    coinbase_score       REAL,
    bybit_score          REAL,
    consolidated_ofi     REAL,
    funding_rate         REAL,
    funding_delta_30s    REAL,
    long_liq_30s_usd     REAL,
    short_liq_30s_usd    REAL,
    liq_ratio_30s        REAL,
    depth_imbalance_l5   REAL,
    depth_score          REAL,

    -- Ensemble
    composite_score      REAL,
    effective_composite  REAL,       -- signal_weight uygulanmış

    -- Hangi sinyaller aktif (bitfield)
    active_sources       INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_signal_snapshots_session_ts
    ON signal_snapshots(market_session_id, ts_ms);
```

### 7.2 `window_open_price` Kayıt

`market_sessions` tablosuna ek kolon:

```sql
ALTER TABLE market_sessions ADD COLUMN rtds_window_open_price REAL;
ALTER TABLE market_sessions ADD COLUMN rtds_window_open_ts_ms INTEGER;
```

Bu iki alan pencere açılışında ilk RTDS tick geldiğinde **bir kez** yazılır.

### 7.3 Snapshot Cadence

`window.rs::run_trading_loop` içinde mevcut `frontend_timer` (1 sn) döngüsüne eklenir:

```rust
_ = frontend_timer.tick() => {
    // ... mevcut snapshot'lar ...
    persist::snapshot_signals(&ctx.pool, &sess, &ctx);  // 🆕
}
```

---

## 8. Frontend / IPC Eventleri

### 8.1 Yeni `FrontendEvent` Varyantları

```rust
// src/ipc.rs
pub enum FrontendEvent {
    // ... mevcut varyantlar ...

    /// 🆕 RTDS tick — pencere boyunca sürekli akar.
    RtdsUpdate {
        bot_id: i64,
        symbol: String,
        current_price: f64,
        window_open_price: Option<f64>,
        window_delta_bps: f64,
        ts_ms: u64,
    },

    /// 🆕 Composite skor güncellemesi — 1 sn cadence.
    CompositeSignal {
        bot_id: i64,
        composite_score: f64,
        effective_composite: f64,
        breakdown: SignalBreakdown,  // her kaynağın skoru
        ts_ms: u64,
    },
}

pub struct SignalBreakdown {
    pub rtds: Option<f64>,
    pub binance: Option<f64>,
    pub coinbase: Option<f64>,
    pub bybit: Option<f64>,
    pub funding: Option<f64>,
    pub liquidation: Option<f64>,
    pub depth_l5: Option<f64>,
}
```

### 8.2 UI Önerisi — "Pre-market Indicator" Paneli

T-15 ile T+0 arası (market henüz açılmadı):

```
┌─ Pre-market Signals ─ btc-updown-5m-1776789700 ──┐
│                                                    │
│  Chainlink live:   67,234.50 USD                  │
│  Window opens in:  12 sec                         │
│                                                    │
│  Binance OFI:      ▓▓▓▓▓▓▓░░░  6.8  (light buy)  │
│  Consolidated OFI: ▓▓▓▓▓▓░░░░  6.2              │
│  Funding Δ:        +0.001%/8h  (neutral)         │
│                                                    │
│  Projected direction: UP (weak)                   │
│  ⚠ Pre-market signals only — not resolution       │
└────────────────────────────────────────────────────┘
```

T+0 sonrası:

```
┌─ Live Signals ─ btc-updown-5m-1776789700 ──┐
│                                              │
│  Window open:   67,234.50                   │
│  Current:       67,298.20  (+9.45 bps ↑)    │
│  Time left:     3m 12s                      │
│                                              │
│  Composite:     ▓▓▓▓▓▓▓▓░░  7.8  UP        │
│    Window Δ:    7.97  (w=0.40)              │
│    Binance:     6.80  (w=0.20)              │
│    Multi-OFI:   7.20  (w=0.15)              │
│    Depth L5:    6.50  (w=0.15)              │
│    Funding:     5.10  (w=0.05)              │
│    Liq:         5.00  (w=0.05)              │
└──────────────────────────────────────────────┘
```

---

## 9. Faz Faz Uygulama Planı

### Faz 1 — RTDS + Window Delta (MVP, en yüksek ROI)

**Süre tahmini:** 3–5 gün

- [ ] `src/rtds.rs` — RTDS WS task + `RtdsState`
- [ ] `src/slug.rs` — `rtds_sym()` mapping
- [ ] `src/bot/ctx.rs` — `rtds_state: SharedRtdsState` ekle
- [ ] `src/bot/tasks.rs` — `rtds_task` spawn
- [ ] `src/bot/window.rs` — pencere başlangıcında state reset
- [ ] `src/strategy/harvest.rs` — `effective_score` → `window_delta_score` eklentisi (ilk ağırlıklı ortalama)
- [ ] `src/db/signals.rs` — `signal_snapshots` tablosu + migration
- [ ] `src/ipc.rs` — `RtdsUpdate` varyantı
- [ ] Frontend — pre-market indicator paneli (basit versiyon)
- [ ] Birim testler: `window_delta_score` mapping, state reset logic
- [ ] Dryrun testi: 24 saat boyunca 5-dk marketlerde score doğruluğu ölç

### Faz 2 — Multi-Exchange OFI

**Süre tahmini:** 5–7 gün

- [ ] `src/signals/mod.rs` — `SignalSource` trait
- [ ] `src/signals/coinbase.rs` — Coinbase advanced-trade parser
- [ ] `src/signals/bybit.rs` — Bybit perpetual parser
- [ ] `src/signals/ensemble_ofi.rs` — hacim-ağırlıklı konsolidasyon
- [ ] Mevcut `src/binance.rs` → `src/signals/binance.rs` (opsiyonel refactor)
- [ ] `BotConfig.signal_sources.coinbase/bybit` flag
- [ ] Test: 3 borsa arasında korelasyon ölç, ağırlık kalibre et

### Faz 3 — Funding + Liquidations

**Süre tahmini:** 4–6 gün

- [ ] `src/signals/funding.rs` — `!markPrice@arr@1s` parser + state
- [ ] `src/signals/liquidations.rs` — `<sym>@forceOrder` parser + rolling window
- [ ] `CompositeWeights` güncellemesi
- [ ] Extreme funding (`|rate| > 0.1%`) erken uyarı log'u

### Faz 4 — Orderbook Depth L5

**Süre tahmini:** 7–10 gün (en karmaşık)

- [ ] `src/signals/orderbook_depth.rs` — Binance diff-depth sync (REST snapshot + WS diff)
- [ ] Sequence gap recovery mantığı
- [ ] `imbalance_l5` + `imbalance_l1` paralel hesap
- [ ] CPU profiling: 4 sembol × 100 ms cadence yük testi

### Faz 5 — Composite Ensemble + Kalibrasyon

**Süre tahmini:** 5–7 gün

- [ ] `src/signals/composite.rs` — ağırlıklı ortalama + graceful degradation
- [ ] Kalibrasyon script'i: SQLite'tan (composite_t-10, outcome) çiftleri çek, logistic regression
- [ ] A/B test framework (BotConfig `composite_weights: HeuristicV1 | CalibratedV2`)
- [ ] Dashboard: her sinyal kaynağının **marjinal katkısı** (feature importance)

---

## 10. Test Stratejisi

### 10.1 Birim Testler

Her yeni modül için:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn window_delta_score_mapping() {
        assert!((window_delta_score(0.0, 1.0) - 5.0).abs() < 0.01);
        assert!((window_delta_score(10.0, 1.0) - 8.33).abs() < 0.1);
        assert!((window_delta_score(-10.0, 1.0) - 1.67).abs() < 0.1);
    }

    #[test]
    fn composite_score_missing_signal_degrades_gracefully() {
        // Sadece window_delta (0.40 weight) mevcut; diğerleri None
        // → skor = window_delta (ağırlık redistribusyonu)
    }

    #[test]
    fn rtds_window_open_captured_once() {
        // İlk tick window_open'a yazılır; sonraki tick'ler yazmaz.
    }
}
```

### 10.2 Integration Test

Mock WS server kurulur; sentetik tick akışı gönderilir:

```
tests/
├── rtds_mock.rs         # sahte RTDS server
├── multi_exchange.rs    # sahte Coinbase + Bybit + Binance paralel
└── signal_ensemble.rs   # composite skor end-to-end
```

### 10.3 Dryrun Benchmark

`run_mode = dryrun` ile 7–14 gün kesintisiz çalıştır; her 5-dk market için:

```sql
SELECT
    s.slug,
    ss.composite_score,           -- T-10s snapshot
    m.winning_outcome,
    CASE
        WHEN ss.composite_score > 5.0 AND m.winning_outcome = 'Up'   THEN 1
        WHEN ss.composite_score < 5.0 AND m.winning_outcome = 'Down' THEN 1
        ELSE 0
    END AS correct
FROM market_sessions s
JOIN market_resolutions m ON m.slug = s.slug
JOIN signal_snapshots ss ON ss.market_session_id = s.id
    AND ss.ts_ms BETWEEN (s.end_ts - 15) * 1000 AND (s.end_ts - 5) * 1000;
```

**Hedef doğruluk:**

- Window Delta alone: ≥ 70%
- Composite v1 (heuristic weights): ≥ 75%
- Composite v2 (kalibre edilmiş): ≥ 80%

---

## 11. Backward Compatibility ve Migration

### 11.1 Mevcut Çalışan Botlar

Yeni sinyal sistemi **opt-in**. Mevcut botlar `signal_sources.rtds = false` (veya config'de bu alan yoksa default false — Faz 1 tamamlanana kadar) olarak çalışmaya devam eder.

Faz 1 deploy olduktan sonra:

1. Mevcut botlar **otomatik olarak RTDS'e bağlanmaz** — kullanıcı UI'dan tek tek aktif etmeli
2. Yeni botlar default olarak RTDS aktif
3. Deprecation yok — `binance_signal` tek başına hala kullanılabilir

### 11.2 SQLite Migration

```sql
-- migrations/2026_phase1_rtds.sql
ALTER TABLE market_sessions ADD COLUMN rtds_window_open_price REAL;
ALTER TABLE market_sessions ADD COLUMN rtds_window_open_ts_ms INTEGER;

CREATE TABLE IF NOT EXISTS signal_snapshots (
    -- ... §7.1'deki şema ...
);

CREATE INDEX IF NOT EXISTS idx_signal_snapshots_session_ts
    ON signal_snapshots(market_session_id, ts_ms);
```

Migration idempotent — `IF NOT EXISTS` ile birden fazla kez çalıştırılabilir.

### 11.3 Rollback Planı

Her faz ayrı git branch'te geliştirilir (`feature/signal-phase-1-rtds`, `...phase-2-multi-exchange`, ...). Sorun çıkarsa:

1. Supervisor stop
2. İlgili commit revert
3. Migration geri alma script'i (`migrations/rollback_phase_N.sql`)
4. Supervisor restart

SQLite data için `bot_credentials` ve `bots` tabloları **dokunulmaz** — yalnız sinyal-spesifik tablolar drop edilir.

---

## 12. Performans ve Kaynak Kullanımı

### 12.1 Bağlantı Sayısı (Tek Bot)

| Sinyal | WS bağlantı | REST istek | Bellek etkisi |
|---|:---:|---|---|
| RTDS | 1 | — | ~5 KB |
| Binance aggTrade (mevcut) | 1 | — | ~20 KB (300 tick buffer) |
| Coinbase | 1 | — | ~20 KB |
| Bybit | 1 | — | ~20 KB |
| Funding | 1 (paylaşımlı — tüm semboller) | — | ~2 KB/sembol |
| Liquidations | 1/sembol | — | ~30 KB (rolling 5 dk) |
| Depth L5 | 1/sembol | 1 (snapshot) | ~100 KB |

**Toplam bot başına (tüm sinyaller açık):** ~6 WS, ~250 KB RAM.

### 12.2 CPU Profili

- Binance aggTrade + RTDS + Coinbase + Bybit: <1% CPU (modern x86)
- Depth L5 (4 sembol @ 100 ms): ~3–5% CPU
- Composite skor hesabı: <0.1% (1 sn cadence, `async read`)

### 12.3 Rate Limit

- **Binance** Futures WS: 200 mesaj/sn — aggTrade + funding + forceOrder + depth toplu 4 WS = limit içinde
- **Coinbase**: 750 mesaj/sn — rahat
- **Bybit**: 10 abonelik/bağlantı, 500 mesaj/sn — rahat
- **Polymarket RTDS**: doküman belirtmiyor; genelde rate limit yok (public feed)

Çoklu bot senaryosu (örn. 10 bot × 4 sembol × 6 WS = 240 bağlantı):
- Her bot **ayrı PID** olduğu için ayrı network connection
- Exchange tarafı IP bazlı rate limit uygulayabilir → **shared connection pool** (supervisor seviyesi) değerlendirilebilir (Faz 6+)

---

## 13. Güvenlik ve Risk

### 13.1 Veri Doğrulama

- RTDS'den gelen `value` **alt sınır / üst sınır** kontrolü: 1000 < BTC < 1_000_000; aksi halde tick reddedilir (corrupt data koruması)
- Binance markPrice ve spot arasında **sanity check**: `|mark − spot| / spot > 0.05` → alarm (oracle manipulation veya exchange sorunu sinyali)
- WS `PING`/`PONG` timeout (10 sn) → reconnect

### 13.2 Graceful Degradation

Hiçbir sinyal **zorunlu değil**. Herhangi biri kopsa:

1. `is_connected() == false` state'e düşer
2. Composite layer ağırlığı diğerlerine redistribüte eder
3. Log: `⚠ Signal <name> disconnected, redistributing weight`
4. 1 sn'de bir reconnect denenir (exponential backoff max 60 sn)

Kritik senaryolar:

| Kayıp | Sonuç |
|---|---|
| RTDS | Window Delta yok → composite skor ağırlığı Binance + diğerlerine geçer |
| Binance aggTrade | Composite skor **hala çalışır** (sadece diğer 5 sinyal) |
| Tüm sinyaller | `effective_composite = 5.0` (nötr) → strateji OpenDual fiyatı = ask (taker) |

### 13.3 "Kör Mod" Güvenliği

Tüm sinyaller kopmuşsa ve `effective_composite = 5.0` nötr olduğunda strateji motoru:

- `harvest` → OpenDual ikisi de `ask` fiyatında (taker eşiği); yine de çalışır
- `dutch_book` → yön filtresi devre dışı; `×1.0` çarpan
- `prism` → eşik default

Yani hiçbir sinyalin olmaması **botu durdurmaz** — mevcut `signal_weight = 0` davranışıyla aynı.

---

## 14. Referanslar

### 14.1 Resmi Dokümantasyon

- Polymarket RTDS: https://docs.polymarket.com/developers/RTDS/RTDS-crypto-prices
- Polymarket Market Channel: https://docs.polymarket.com/market-data/websocket/market-channel
- Binance Futures WS: https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams
- Coinbase Advanced Trade WS: https://docs.cdp.coinbase.com/advanced-trade/docs/ws-overview
- Bybit V5 WS: https://bybit-exchange.github.io/docs/v5/websocket/public/trade

### 14.2 Akademik Kaynaklar

- Kolm, P. N., Turiel, J., & Westray, N. (2023). **Deep Order Flow Imbalance: Extracting Alpha at Multiple Horizons from the Limit Order Book.** Mathematical Finance.
- Zhang, Z., Zohren, S., & Roberts, S. (2019). **DeepLOB: Deep Convolutional Neural Networks for Limit Order Books.** IEEE Transactions on Signal Processing.
- Cartea, Á., Donnelly, R., & Jaimungal, S. (2018). **Enhancing Trading Strategies with Order Book Signals.** Applied Mathematical Finance.
- Markwick, D. (2022). **Order Flow Imbalance — A High Frequency Trading Signal.** https://dm13450.github.io/2022/02/02/Order-Flow-Imbalance.html

### 14.3 İlgili Açık Kaynak Projeler

- `Archetapp/polymarket-btc-bot` (gist) — 7-indicator skorlama, Window Delta dominant sinyal olarak doğrulanmış
- `FrondEnt/PolymarketBTC15mAssistant` — Polygon RPC + Chainlink feed fallback implementasyonu
- `haredoggy/Prediction-Markets-Trading-Bot-Toolkits` — Rust, 10 strateji, orderbook imbalance içerir
- `ThinkEnigmatic/polymarket-bot-arena` — Bayesian öğrenme + adaptive weight kalibrasyonu örneği
- `toma-x/exploring-order-book-predictability` — CNN ile L5 orderbook tahmini (JAX/FLAX)
- `nkaz001/hftbacktest` — Queue position + latency simülasyonlu backtest framework'ü

### 14.4 İç Dokümanlar

- [`docs/bot-platform-mimari.md`](./bot-platform-mimari.md) — ana mimari referansı
- [`docs/strategies.md`](./strategies.md) — strateji implementasyon detayları
- [`docs/rust-polymarket-kutuphaneler.md`](./rust-polymarket-kutuphaneler.md) — Rust kütüphane önerileri

---

## 15. Kabul Kriterleri (Her Faz İçin)

Her faz **aşağıdaki kriterler** sağlanana kadar `main` branch'e merge edilmez:

- [ ] Yeni modül birim test kapsamı ≥ %80
- [ ] Integration test en az 1 uçtan uca senaryo başarılı
- [ ] Dryrun 24 saat kesintisiz çalıştırılmış; crash yok, memory leak yok
- [ ] Log formatı mevcut §5.2 ve §5.4 ile uyumlu (`[[EVENT]]` prefix vb.)
- [ ] SQLite migration idempotent ve rollback script var
- [ ] `BotConfig` validation: geçersiz kombinasyonlar (`rtds: true` ama desteklenmeyen slug) reddedilir
- [ ] Frontend UI: yeni sinyal toggle'ları çalışıyor; pre-market paneli görüntüleniyor
- [ ] Doküman: `bot-platform-mimari.md` ilgili bölümü güncellendi

---

*Bu plan **canlı dokümandır**. Faz tamamlandıkça ilgili bölüm "Tamamlandı ✅" olarak işaretlenir; öğrenilen dersler Ek A bölümüne eklenir.*

---

## Ek A — Öğrenilen Dersler (Faz Tamamlandıkça Doldurulacak)

### Faz 1 — RTDS
*(henüz deploy edilmedi)*

### Faz 2 — Multi-Exchange OFI
*(henüz deploy edilmedi)*

### Faz 3 — Funding + Liquidations
*(henüz deploy edilmedi)*

### Faz 4 — Orderbook Depth L5
*(henüz deploy edilmedi)*

### Faz 5 — Composite Ensemble
*(henüz deploy edilmedi)*