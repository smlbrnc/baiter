# Bot platformu — mimari, akış ve veri modeli

Bu doküman, **frontend** ile **API (Rust)** arasındaki sorumlulukları, bot yaşam döngüsünü, **Gamma/CLOB/WebSocket** ile uyumlu **SQLite** kayıtlarını ve **trade durumlarını** sade bir dille özetler.

### Resmi API kaynağı (öncelik)

**Tek doğruluk kaynağı:** [docs.polymarket.com](https://docs.polymarket.com/) (Gamma REST, CLOB REST, WebSocket kanalları, şema ve alan adları).

Repo içi kopyalar yalnızca **hızlı offline referans**dır: [polymarket-clob.md](api/polymarket-clob.md), [polymarket-gamma.md](api/polymarket-gamma.md). **Çelişki halinde** her zaman resmi site ve güncel endpoint şeması geçerlidir.

### Kullanılan taban URL ve endpointler (doğrulama)

Aşağıdaki tablo [polymarket-clob.md](api/polymarket-clob.md) ve [polymarket-gamma.md](api/polymarket-gamma.md) ile uyumludur; resmi sitede yol güncellenirse önce [docs.polymarket.com](https://docs.polymarket.com/) doğrulanır.

| Sistem | Taban URL | Bu mimaride kullanım |
|--------|-----------|----------------------|
| **Gamma REST** | `https://gamma-api.polymarket.com` | **Yalnız bu taban** ile keşif: slug, `clobTokenIds`, `startDate` / `endDate` (ör. `GET /markets/slug/{slug}`; diğerleri bkz. gamma dokümanı). Market keşfi / listeleme için **CLOB REST üzerinden ayrı metadata uçları kullanılmaz**. |
| **CLOB REST** | `https://clob.polymarket.com` | `POST /order`; `DELETE /order` veya `DELETE /orders`; T−15 hazırlığında orderbook/fiyat okumaları (`/book`, `/price` vb.) |
| **CLOB REST** | — | **Kullanılmaz:** `GET /trades` — kalıcı trade yalnız User WS `trade`; **`GET /orders`** — açık emirler yalnız User WS `order` |
| **Market WS** | `wss://ws-subscriptions-clob.polymarket.com/ws/market` | `book`, `price_change`, `tick_size_change`, `last_trade_price`, `market_resolved`, …; `best_bid_ask` / `new_market` / `market_resolved` için abonelikte `custom_feature_enabled: true` (resmi tablo: [WebSocket overview](https://docs.polymarket.com/market-data/websocket/overview)); `type: "market"`, `assets_ids` |
| **User WS** | `wss://ws-subscriptions-clob.polymarket.com/ws/user` | `order`, `trade` (L2 kimlik bilgisi ile) |

**Yanıt uyumu (CLOB REST — repo özeti = resmi şema):** `POST /order` → `success`, `orderID`, `status` (`live` / `matched` / `delayed` / `unmatched` — sonuncusu marketable ama gecikme başarısızlığı senaryosu; bkz. resmi [Orders overview — Insert Statuses](https://docs.polymarket.com/trading/orders/overview)), `errorMsg`; eşleşmede `tradeIDs`, `transactionsHashes`. `DELETE /order` → `canceled`, `not_canceled`. **REST heartbeat** (CLOB emir güvenliği — WebSocket `PING`/`PONG` ile ayrı): bkz. **§4.1** ve resmi [Orders overview — Heartbeat](https://docs.polymarket.com/trading/orders/overview). Bu alanlar §5 örnekleri ve §5.5 tablolarıyla örtüşür.

**Polymarket iletişim terimleri (özet):** Abonelik ve payload alan adları **sunucu şemasıyla bire bir** kullanılır — ör. Market WS aboneliğinde `type`, `assets_ids`, `custom_feature_enabled`; olaylarda `event_type` (`book`, `market_resolved`, …). [User channel](https://docs.polymarket.com/market-data/websocket/user-channel): **`trade`** için zincir durumu `status` (`MATCHED` … `CONFIRMED`); **`order`** için resmi alan adı **`type`** (`PLACEMENT` / `UPDATE` / `CANCELLATION`). Kalıcı kayıt ve yapısal JSON bu adlarla uyumludur; metin logda `type=`. **Açık emirler** yalnız `order` akışı ile takip edilir; REST ile açık emir sorgusu yok (bkz. tablo “Kullanılmaz”). Gamma yanıtındaki market alanları için bkz. [polymarket-gamma.md](api/polymarket-gamma.md) (ör. `clobTokenIds`).

### Kalıcılık ve sonradan sorgu (ürün kuralı)

User WebSocket **`order`** ve **`trade`** ile **`POST /order`** / **`DELETE /order`** (gerekiyorsa **`DELETE /orders`**) yanıtları işlendikçe **SQLite’a** yazılır. Sonradan “açık emir”, “son işlemler”, strateji motoru girdi özeti gibi ihtiyaçlar **bu veritabanından** sorgulanır; canlı akıştan gelen kayıt ile sorgu **tutarlıdır** (aynı olay hem WS hem REST cevabında geldiyse tek mantıksal emir/fill kimliğiyle birleştirilir). **Önkoşul:** süreç, güvenilir sorgu için ilgili mesajları **persist etmeden** “yalnız bellek” varsayımı yapmaz.

### Tahmin ve API dışı ikame yok

- Polymarket’tan gelmeyen bir sonucu **tahmin eden**, **Gamma ile “tamamlayan”** veya **uydurma alanlarla ikame eden** akışlar tanımlanmaz (REST/WS **alternatifi üretmez**).
- **Kimlik çözümleme (ürün):** Önce bot ayarı, yoksa `.env` — yalnızca **kimlik** içindir; **API yanıtına** ikinci bir kaynak gibi davranmaz.

### `market_resolved` gecikmesi ve yeniden deneme (5+5+10)

Resmi WebSocket olayı [UMA çözümleme](https://docs.polymarket.com/concepts/resolution) nedeniyle pencere bitiminden örneğin **~5 dk veya ~15 dk** sonra gelebilir; bu **tahmin değil**, zincir/oracle gecikmesidir.

- **Sentetik `market_resolved` veya Gamma’dan “türetilmiş kazanan” yazılmaz.**
- Olay henüz gelmediyse uygulama **üç aşamalı bekleme** ile aynı **resmi Market WS** akışını dinlemeye / aboneliği doğrulamaya devam eder: **5 dk** → **5 dk** → **10 dk** (`5+5+10`). **Yalnızca sunucudan gelen gerçek `market_resolved` payload’ı** SQLite’a işlenir.
- Bu süreç sonunda hâlâ olay yoksa, sonuç alanı **İngilizce** `not resolved` olarak kaydedilir (tahminî kazanan üretilmez).

### Ürün kuralları (API’de sabit değil)

**T−15**, **1 sn frontend polling**, **stop_before_end_ms**, strateji adları ve pencere seçimi bu repoya özgü **iş kurallarıdır**; Polymarket dokümanında ayrı bir “T−15 endpoint”i yoktur. Zamanlama, Gamma’daki market **`startDate` / `endDate`** ([örnek alanlar](api/polymarket-gamma.md)) ile uyumlu seçilir.

**Desteklenen market kısıtı (kripto varlık zorunluluğu):** Bu platform **yalnızca** aşağıdaki slug şemasına uyan Polymarket marketleri üzerinde işlem yapabilir:

```
{asset}-updown-{interval}-{unix_timestamp_saniye}
```

| Varlık (`asset`) | Binance Futures sembolü | Desteklenen aralıklar (`interval`) | Aralık (sn) |
|---|---|---|---|
| `btc` | `btcusdt` | `5m`, `15m`, `1h`, `4h` | 300, 900, 3600, 14400 |
| `eth` | `ethusdt` | `5m`, `15m`, `1h`, `4h` | 300, 900, 3600, 14400 |
| `sol` | `solusdt` | `5m`, `15m`, `1h`, `4h` | 300, 900, 3600, 14400 |
| `xrp` | `xrpusdt` | `5m`, `15m`, `1h`, `4h` | 300, 900, 3600, 14400 |

Slug örnekleri (resmi Polymarket URL’lerinden doğrulandı):

```
btc-updown-5m-1776500700    → BTC 5 dk pencere
eth-updown-15m-1776427200   → ETH 15 dk pencere
sol-updown-1h-1776427200    → SOL 1 saat pencere
xrp-updown-4h-1776427200    → XRP 4 saat pencere
```

Timestamp hesabı: `ts = (unix_now_saniye // interval_saniye) * interval_saniye`.

**Eşleşmeyen slug → bot başlatma reddi** (ürün hatası; konfigürasyon aşamasında yakalanr). Politika, seçim, spor gibi diğer kategoriler desteklenmez.

**Çözümleme kaynağı notu:** Bu marketler Chainlink `{asset}/USD` akışıyla çözümlenir — Binance spot veya futures fiyatıyla değil. Binance Futures aggTrade verileri **yalnızca dahili harici sinyal** (`binance_signal`) olarak kullanılır; çözümleme kaynağıyla karıştırılmaz (bkz. §14).

**Aktif market keşfi:** `GET https://gamma-api.polymarket.com/markets?active=true&closed=false` yanıtı slug öneki ile filtrelenir (ör. `btc-updown-5m-`). Doğrudan slug tahmini yerine liste filtresi tercih edilir; bkz. [Fetching markets](https://docs.polymarket.com/market-data/fetching-markets).

---

## ⚡ Temel Mimari Kural: Minimum Gecikme ve Anlık Emir

> **Bu kural tüm mimari kararların üzerindedir. Projedeki her bileşen bu kurala göre tasarlanır ve değerlendirilir.**

Polymarket binary marketlerinde fiyatlar hızlı hareket eder ve orderbook derinliği sınırlıdır. **Emir geciktiği her milisaniye potansiyel fill fiyatını kötüleştirir veya fırsatı tamamen kaçırır.** Bu nedenle aşağıdaki kurallar projenin tamamında değiştirilemez temel kabul edilir.

### Kural 1 — Emir Yolu (Critical Path) Sıfır Blok

Bir strateji kararı verildikten sonra **emir gönderimi (POST /order veya POST /orders) hiçbir şeyle bloke edilemez:**

| Yasak | Neden |
|---|---|
| DB yazımını beklemek | SQLite I/O bloke eder |
| Log flush'ı beklemek | Disk I/O bloke eder |
| Metrik hesabını beklemek | CPU işi bloke eder |
| Frontend bildirimini beklemek | Network I/O bloke eder |

**Doğru sıra:**
```
[Event geldi] → [Strateji kararı] → [POST /order] → (paralel/arka plan)
                                                     ├── DB yaz
                                                     ├── Log yaz
                                                     └── Frontend'e push
```

### Kural 2 — State Önceden Hazır Olmalı

Strateji motoru bir `best_bid_ask` veya `trade MATCHED` event'i geldiğinde **tüm kararı anında verebilmelidir:**

- `StrategyMetrics` (imbalance, avg_*, signal_score, MarketZone, MarketPnL) her event sonrasında güncellenir ve **hafızada** tutulur.
- Emir kararı anında bu hazır state'i okur; **yeniden hesap yapmaz**, **WS veya REST'ten bilgi beklemez**.
- `binance_signal` (§14) ve `MarketZone` (§15) ayrı görevlerde sürekli güncellenir; strateji motoru **lock-free read** (`Arc<RwLock<>>` okuma kilidi) ile anında erişir.

### Kural 3 — Async, Non-Blocking, Connection Pooling

```rust
// Her bot için tek paylaşımlı reqwest::Client (connection pool dahil).
// Emir başına yeni connection açılmaz.
let client = reqwest::Client::builder()
    .tcp_nodelay(true)          // Nagle algoritması kapalı — küçük paket gecikme yok
    .pool_max_idle_per_host(4)  // Bağlantı havuzu; auth endpoint dahil
    .build()?;
```

- Tüm HTTP çağrıları (`POST /order`, `DELETE /orders`, heartbeat) `tokio::spawn` veya `async/await` zinciriyle non-blocking gönderilir.
- `tokio::sync::broadcast` veya `mpsc` channel'ları: WS okuyucu → strateji motoru → emir gönderici arasındaki veri yolu sıfır kopyayla aktarılır.
- **Signing** (EIP-712, `alloy`) senkron CPU işidir; order struct hazırlığı event loop'u bloke etmemek için `tokio::task::spawn_blocking` içinde veya önceden (market açılışında) yapılır.

### Kural 4 — DB ve Log = Fire-and-Forget

Emir gönderildikten hemen sonra DB ve log yazımı **ayrı bir tokio task'ında** yapılır; emir gönderen coroutine tamamlanmayı **beklemez:**

```rust
// Emir gönder
let resp = clob_client.post_order(signed_order).await?;

// DB + log: fire-and-forget
tokio::spawn(async move {
    db.upsert_order(&resp).await;
    logger.write_order_line(&resp).await;
});

// Frontend push: hemen
frontend_tx.send(FrontendEvent::OrderPlaced(resp));
```

### Kural 5 — Frontend'e Anlık Push

Frontend 1 sn polling ile özet verileri okur (§2); ancak **kritik olaylar (emir gönderimi, fill, PnL değişimi) bu polling'i beklemez:**

- Bot, emir gönderdikten hemen sonra kritik olayı **stdout'a structured JSON satırı** olarak yazar.
- Supervisor log-tail task'ı bu satırı parse eder ve process-local `broadcast::channel`'a iletir.
- API katmanı SSE (Server-Sent Events) üzerinden frontend'e **anında iletir**.
- 1 sn polling **sadece** özet/durum ekranı için; **kritik olay akışı push** ile taşınır.

#### 5.1 IPC Mekanizması — stdout JSON Satırı

**Neden stdout?** §1 IPC bölümü Unix Domain Socket'i ve direkt stdin boru hattını yasaklar; `tokio::sync::broadcast` yalnız process-local çalışır (ayrı PID'ler arasında geçersiz). Mevcut log pipe (`ChildStdout`) zaten supervisor tarafından okunuyor — kritik event'ler aynı kanalda **`[[EVENT]]` prefix'li** satırlar olarak taşınır.

**Bot tarafı (emit):**

```
[[EVENT]] {"kind":"OrderPlaced","bot_id":7,"order_id":"0xff35...","order_type":"GTC","side":"SELL","price":0.57,"size":10,"ts_ms":1766789469958}
```

- Prefix (`[[EVENT]] `) regular log satırlarından ayırmak için zorunlu.
- JSON tek satır (newline-delimited); büyük payload yok.
- Bot bu satırı `tokio::spawn` ile arka plan işi değil, **inline `println!` ile emir yanıtından hemen sonra** yazar.

**Supervisor tarafı (parse + broadcast):**

```
┌─────────────────────────────────────────────────────────────────┐
│  bot PID                                                        │
│    strategy decision → POST /order → yanıt alındı               │
│         │                                                       │
│         ├── tokio::spawn(db.upsert_order(...))       (Kural 4)  │
│         └── println!("[[EVENT]] {\"kind\":\"OrderPlaced\",...")  │
│                             │                                   │
└─────────────────────────────┼───────────────────────────────────┘
                              │ stdout pipe (ChildStdout)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  supervisor PID — log_tail task                                 │
│    satır in: `[[EVENT]] {...}` prefix ise                       │
│      → parse FrontendEvent                                      │
│      → app_state.event_tx.send(ev)                              │
│        (tokio::sync::broadcast, process-local)                  │
│    değilse → SQLite logs tablosuna yaz                          │
└─────────────────────────────┬───────────────────────────────────┘
                              │ broadcast subscribe
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  API SSE handler (GET /api/events)                              │
│    event_rx.recv() → sse Event::data(json) → frontend           │
└─────────────────────────────┬───────────────────────────────────┘
                              │ EventSource
                              ▼
                          frontend useSSE hook
```

**Toplam gecikme hedefi:** < 5 ms (bot println → frontend EventSource onmessage).

#### 5.2 `FrontendEvent` Varyantları

Supervisor'un `broadcast` kanalında taşıdığı ve SSE ile frontend'e ilettiği event türleri:

| Varyant | Tetiklenme | Alanlar |
|---|---|---|
| `OrderPlaced` | POST /order yanıtı alındıktan hemen sonra | `bot_id, order_id, order_type, side, price, size, ts_ms` |
| `Fill` | User WS `trade` event'i (MATCHED + sonraki status güncellemeleri) | `bot_id, trade_id, size, price, outcome, status, ts_ms` |
| `PnLUpdate` | Yalnız MATCHED trade sonrası (cost_basis/shares değişince) | `bot_id, session_id, pnl_if_up, pnl_if_down, ts_ms` |
| `BotStateChanged` | Supervisor spawn/crash/stop | `bot_id, state (RUNNING/STOPPED/FAILED)` |
| `BestBidAsk` | Market WS `best_bid_ask` (opsiyonel, chart için) | `bot_id, side (YES/NO), best_bid, best_ask, spread, ts_ms` |

**Push edilmeyenler:**
- `mtm_pnl` değişimleri → §17 kuralına göre 1 sn polling ile okunur (her `best_bid_ask` event'inde push frontend'i boğar).
- Orderbook `book` snapshot'ları → frontend göstermediği için push edilmez; yalnız DB'ye yazılır.

### Kural 6 — WS Okuyucu Önceliği

Market WS (`best_bid_ask`, `book`) ve User WS (`trade MATCHED`) event'leri **diğer tüm işlemler üzerinde önceliklidir:**

- WS okuyucu task'ı hiçbir zaman bloke olmaz; gelen mesajı `mpsc::Sender` ile strateji motoruna **anında** iletir.
- Strateji motoru kanaldan mesajı alır almaz state'i günceller ve emir kararını verir.
- Heartbeat (§4.1) ayrı bir `tokio::interval` task'ında çalışır; strateji motoruyla aynı thread'i paylaşmaz.

### Gecikme Hedefi (Referans)

| Aşama | Hedef gecikme |
|---|---|
| WS event → strateji kararı | < 1 ms |
| Strateji kararı → POST /order gönderildi | < 1 ms |
| POST /order → yanıt alındı | network RTT (~50–200 ms) |
| Fill (MATCHED) → Frontend push | < 50 ms |
| Fill → DB yazımı | < 500 ms (arka plan) |

> **Not:** Network RTT Polymarket CLOB sunucularına olan bağlantı kalitesine bağlıdır ve kontrol dışıdır. Kontrol altındaki tüm aşamalar (karar, gönderim, push) milisaniye hedefinde tutulur.

---

## 1. Genel mantık

| Katman | Rol |
|--------|-----|
| **Frontend** | Salt **okuma**: bot listesi, ayarlar, başlat/durdur/sil, **logları** ve **slug/market** bazlı özetleri izleme; canlı **monitoring** (API’nin işlediği WS verileri üzerinden). |
| **API (Rust)** | Bot **durum makinesi**, Polymarket **Gamma** (keşif), **CLOB** (emir/trade), **Market/User WebSocket** (anlık derinlik ve kullanıcı olayları), strateji hesapları, **SQLite** kalıcılığı. |

Kullanıcı yeni bot oluştururken **hangi event/metin (ör. “BTC 15dk”)** ve **hangi pencerede** işlem yapılacağını seçer; bot ayarlarını girer ve **başlatır**. API, seçilen kurallara göre **aktif veya bir sonraki markete** bağlanır ve stratejiyi çalıştırır.

### Kimlik ve cüzdan

- **Bot başına:** Kayıtta **Polymarket kimlik bilgisi** (API key seti, adres, private key vb.) **tanımlıysa** yalnızca **o bot** için bu değerler kullanılır; emir ve trade bu kimlikle yapılır.
- **Tanımlı değilse** sunucu `.env` varsayılanı kullanılır.
- **Tek `.env` ile çoklu bot:** Aynı ortam dosyasından beslenen birden fazla bot oluşturulabilir; kimlik çakışması olmaması için **bot başına ayrı key** tanımlamak üretimde tercih edilir. Ayarlarda key verilmiş botlar **yalnızca config’teki** değerlerle trade eder.

#### Credential saklama politikası

Per-bot kimlik bilgileri **SQLite `bot_credentials` tablosunda plaintext** olarak tutulur (şema için bkz. §9a):

- Frontend bot oluşturma formundan girilen `poly_address`, `poly_api_key`, `poly_passphrase`, `poly_secret`, `polygon_private_key` bu tabloya yazılır.
- Bot başlatılırken öncelik sırası: (1) `bot_credentials` tablosundaki bot-spesifik kayıt; (2) `.env` fallback (POLY_ADDRESS, POLY_API_KEY, POLY_PASSPHRASE, POLY_SECRET, POLYGON_PRIVATE_KEY).
- Bir bot silindiğinde `ON DELETE CASCADE` ile credential kaydı da silinir.

**Güvenlik notu:** `polygon_private_key` **plaintext** saklanır — tehdit modeli lokal geliştirme / tek kullanıcılı sunucu senaryosudur. Üretimde ek önlemler gerekir:

- `chmod 600 data/baiter.db` — DB dosyası sadece sahibi okuyabilsin.
- Ayrı sistem kullanıcısı (`baiter`) ile supervisor süreci çalıştırılmalı; DB dizini o kullanıcıya ait olmalı.
- Full-disk encryption (LUKS, FileVault) öneriliyor.
- Tehdit modeli değişirse (paylaşımlı host, çoklu operatör) **AES-GCM alan-seviyesi şifreleme** eklenir; anahtar OS keyring veya KMS'den okunur. Bu genişletme şu an kapsam dışı.

### CLOB alanları ve UP/DOWN (resmi şema ile uyum)

- **Kaynak:** [docs.polymarket.com](https://docs.polymarket.com/) ve repo özeti [polymarket-clob.md](api/polymarket-clob.md). **SQLite**, yapısal log ve ham işleme: User WS `trade` / REST emir yanıtlarındaki **resmi alan adları ve değerler** (`asset_id`, `outcome`, `size`, `price`, `side`, `status`, …) — **uydurma alan yok**.
- **Kapsam:** Bu ürün **yalnızca iki outcome’lu** (binary) marketleri hedefler; çoklu outcome genişletmesi tanımlanmaz.
- **Ham string (gösterim ve kalıcı kayıt):** Resmi yanıtlardaki metinler **sunucunun döndüğü gibi** saklanır ve logda gösterilir — **normalize edilmiş kopya zorunlu değildir.** Repo özetindeki resmi örnekler (biçim farkı normaldir):
  - User WS **`trade`**: `"outcome": "YES"` ([polymarket-clob.md](api/polymarket-clob.md) User `trade` JSON).
  - User WS **`order`**: `"outcome": "YES"`.
  - Market WS **`market_resolved`**: `"winning_outcome": "Yes"`; `new_market` içinde `"outcomes": ["Yes", "No"]`.
- **İç mantık (UP/DOWN eşlemesi, karşılaştırma):** Strateji ve koşullar için `outcome` / `winning_outcome` değerleri gerektiğinde **trim + kanonik eşleme** (ör. büyük harfe çevirip `YES`/`NO` ile karşılaştırma) uygulanabilir; bu **türetilmiş** bir katmandır, **ham alanlar** API ile bire bir kalır.
- **UP / DOWN:** Polymarket API’sinde ayrı bir `UP` alanı yoktur; **ürün/strateji dilidir**. İki outcome’lu markette `UP` ↔ bir `asset_id` / outcome, `DOWN` ↔ diğeri **Gamma’dan gelen `clobTokenIds` sırası** veya sabit eşleme tablosuyla belirlenir. Strateji metrikleri (`imbalance`, `imb_cost_up`, …) bu eşleme üzerinden hesaplanır; **alt veri her zaman resmi `asset_id` + ham `outcome` ile izlenebilir olmalıdır**.

### Dağıtım ve süreç modeli

- **Tek makine** hedeflenir.
- **Maksimum performans** için her bot **ayrı işlem (ayrı PID)** olarak çalışır; denetleyici süreç botları başlatır, durdurur ve sağlığını izler.

### Süreç mimarisi (supervisor / denetleyici)

**Yönetim:** Supervisor, **systemd servis birimi değildir** — Rust içinde çalışan **ana süreçtir** (`main.rs`). Bot işlemleri `std::process::Command` (veya `tokio::process::Command`) ile alt süreç olarak başlatılır; supervisor `stdin`/`stdout` uçlarını veya Unix domain socket'ı tutmaz, yalnızca **PID** ve **çıkış kodunu** izler.

**Genel topoloji:**
```
Frontend (Axum HTTP API)
    │  HTTP (tek port — supervisor üzerinden)
    ▼
Supervisor süreci  ←─── SQLite (bot durumu, log, trade)
    │  spawn / kill
    ├── Bot-A (ayrı PID)
    ├── Bot-B (ayrı PID)
    └── Bot-C (ayrı PID)
```

**IPC / port:** Her bot **ayrı port açmaz**. Frontend, **yalnızca supervisor**'un HTTP API'sine bağlanır; supervisor bot kimliklerini uç yollar aracılığıyla yayar. Bot'tan supervisor'a veri akışı **stdout/stderr log satırları** (supervisor tarafından okunup SQLite'a yazılır) veya **paylaşımlı SQLite** üzerinden olur.

**Crash loop ve yeniden başlatma:**

| Durum | Kural |
|-------|-------|
| Bot beklenmedik çıkış (exit code ≠ 0) | Supervisor **exponential backoff** ile yeniden başlatır: `1s → 2s → 4s → 8s → …` |
| Maksimum deneme | Ürün sabitler (ör. **5 deneme** ya da **toplam 60 s backoff**); aşılırsa bot `FAILED` olarak işaretlenir, log satırı yazılır, el ile `başlat` komutu gerekir |
| Temiz durdurma (kullanıcı `durdur`) | Supervisor `SIGTERM` gönderir; bot açık emirleri temizler / heartbeat döngüsünü durdurur ve normal çıkar — crash loop sayacına **eklemez** |
| Market penceresi bitti, normal geçiş | Bot **çıkmaz** — iç döngü sonraki markete geçer; supervisor bu durumu izlemez (PID hâlâ aktif) |

**Sağlık (health) mekanizması:**

- **Heartbeat dosyası:** Her bot belirli aralıkta (ör. 5 s) **paylaşımlı dizindeki** `bots/<id>.heartbeat` dosyasını günceller (`mtime` yeterlidir). Supervisor `mtime` değerini okur; **belirli eşiği** (ör. 15 s) aşan bot sağlıksız kabul edilir ve crash loop kuralı tetiklenir.
- **Alternatif (SQLite):** Bot son aktif zamanını `bots` tablosuna yazar; supervisor periyodik sorgu ile kontrol eder. Bu mimari ek IPC gerektirmez.
- **HTTP health endpoint:** Bot kendi HTTP ucu açmaz — supervisor `/api/bots/{id}/status` ile bot'un `RUNNING / FAILED / STOPPED` durumunu SQLite'tan okur ve frontend'e döner.

**Komut → süreç akışı (frontend → bot):**

| Frontend komutu | Supervisor davranışı |
|-----------------|----------------------|
| `POST /api/bots/{id}/start` | PID yoksa `Command::spawn`; PID varsa hata |
| `POST /api/bots/{id}/stop` | PID'e `SIGTERM`; belirli süre sonra hâlâ çalışıyorsa `SIGKILL` |
| `DELETE /api/bots/{id}` | Önce `stop`, ardından kayıt ve log temizliği |

**Loglama akışı:** Bot `stdout`'u supervisor'un `tokio::process::ChildStdout` akışına bağlıdır; supervisor satırları okuyup SQLite `logs` tablosuna (bot_id + timestamp + satır) yazar — frontend bu tablodan sayfalı veya SSE akışıyla okur. Bot kendi başına log dosyası **açmaz**.

### Stratejiler (genişletilebilir)

Aşağıdaki üç strateji adı sabit; ileride yeni stratejiler eklenebilir.

| Kod | Ad |
|-----|-----|
| `dutch_book` | Dutch book |
| `harvest` | Harvest |
| `prism` | Prism |

### Ortak metrik kataloğu ve strateji bağlama

**İlke:** Aşağıdaki `imbalance`, `imbalance_cost`, `avg_*`, `sum_*`, `AVG SUM`, `POSITION Δ` tanımları **tek ortak katalog**tur; her strateji için ayrı ayrı aynı formülleri kopyalayan fonksiyonlar yazılmaz. Uygulama katmanı **bir kez** (aynı trade penceresi / aynı muhasebe kurallarıyla) bu metrikleri günceller; strateji kodu yalnızca **ihtiyaç duyduğu alt kümeyi okur** ve kendi **eşik / aksiyon** kurallarını uygular.

**Strateji → tüketilen metrik grubu (ürün matrisi — örnek):**

| Ortak metrik grubu | `dutch_book` | `harvest` | `prism` |
|--------------------|:------------:|:---------:|:-------:|
| Pay dengesizliği: `imbalance` (`UP − DOWN`, share) | ✓ | ✓ | — |
| Maliyet farkı: `imb_cost_up`, `imb_cost_down`, `imbalance_cost` | ✓ | ✓ | — |
| `avgsum`: `avg_up`, `avg_down`, `AVG SUM` | ✓ | ✓ | ✓ |
| `profit`: türetilen **profit %** (`AVG SUM` üzerinden) | ✓ | — | ✓ |
| Brüt hacim: `sum_up`, `sum_down` | ✓ | ✓ | ✓ |
| `POSITION Δ` (pay `imbalance` + brüt hacim oranı ve yön) | ✓ | ✓ | — |
| Harici sinyal: `binance_signal` (aggTrade CVD/BSI/OFI, 0–10 skor) | ✓ | ✓ | ✓ |

**Not:** `prism` ne pay `imbalance` ne maliyet `imbalance_cost` hattını kullanmaz; `harvest` **`profit`** hattını kullanmaz (`avgsum` ProfitLock koşulunda `avg_YES + avg_NO` için açıktır). Yeni strateji eklenirken `MetricMask` yalnızca aşağıdaki **geçerli** `(avgsum, profit)` çiftleriyle genişletilir: `(false,false)`, `(true,false)`, `(true,true)` — **`(false,true)` yasaktır** (`profit` açıkken `avgsum` kapalı olamaz). Matris **yapılandırma veya sabit enum** (`MetricSubscription` benzeri) ile genişletilir; metrik tanımı katalogda kalır.

#### Rust uygulama taslağı (`Strategy` ↔ metrik maskesi)

Motor her döngüde **`StrategyMetrics`** (veya eşdeğeri) yapısını **tüm katalog alanlarıyla** doldurabilir veya yalnız `required_metrics` ile işaretlenen dalları hesaplayarak CPU tasarrufu yapabilir. Strateji modülü **maskeden fazlasını okumamalı** (API kontratı).

`POSITION Δ`, matriste ayrı sütun olmasa da **`imbalance` (pay) + `sum_volume`** birlikte seçiliyse aynı `StrategyMetrics` üzerinden türetilir (`imbalance_cost` ayrı bayrak olsa da `POSITION Δ` formülünde doğrudan kullanılmaz).

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    DutchBook,
    Harvest,
    Prism,
}

/// Üstteki matrisle bire bir: hangi metrik hatları hesaplanacak / okunacak.
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricMask {
    /// Pay tarafı: `UP − DOWN` net share (`imbalance`).
    pub imbalance: bool,
    /// Maliyet tarafı: `imb_cost_up`, `imb_cost_down`, `imbalance_cost`.
    pub imbalance_cost: bool,
    /// `avg_up`, `avg_down`, `AVG SUM`
    pub avgsum: bool,
    /// Türetilen **profit %** (ör. `(1 − AVG SUM) × 100%` — ürün formülü sabitlenir)
    pub profit: bool,
    /// `sum_up`, `sum_down`
    pub sum_volume: bool,
    /// Binance USD-M Futures aggTrade sinyali (0–10 skor).
    /// Yalnızca desteklenen slug kalıbında (`{btc|eth|sol|xrp}-updown-{5m|15m|1h|4h}-{ts}`) true.
    pub binance_signal: bool,
}

/// Bot başlatma konfigürasyonu (frontend / .env'den gelir).
pub struct BotConfig {
    // slug, kimlik bilgileri vb. runtime alanları buraya eklenir.

    /// Çalışma modu — bkz. §16.
    pub run_mode: RunMode,

    /// Emir başına harcanacak USDC miktarı (tüm stratejiler için ortak).
    /// GTC size = max(⌈order_usdc / bid_price⌉, api_min_order_size)
    /// FAK size = imbalance (bu formül dışında).
    /// `api_min_order_size`: market init'te GET /book → min_order_size'dan okunur;
    /// BotConfig'de saklanmaz, market state'ine yazılır.
    pub order_usdc: f64,

    /// Binance aggTrade sinyal ağırlığı: 0 = sinyali tamamen yoksay (çarpan 1.0),
    /// 10 = maksimum etki. Varsayılan: 5. Aralık: [0, 10].
    pub signal_weight: u8,

    /// Strateji seçimine göre strateji-spesifik parametreler.
    /// Kolun `Strategy` enum değeriyle uyumlu olması zorunludur.
    pub strategy_params: StrategyConfig,
}

/// Strateji-spesifik parametre bloğu (tagged union).
/// Her kol yalnızca ilgili stratejiye özgü alanları taşır.
pub enum StrategyConfig {
    DutchBook(DutchBookParams),
    Harvest(HarvestParams),
    Prism(PrismParams),
}

pub struct HarvestParams {
    /// OpenDual YES (UP) tarafı GTC bid fiyatı (zorunlu, kullanıcı girer).
    pub up_bid: f64,
    /// OpenDual NO (DOWN) tarafı GTC bid fiyatı (zorunlu, kullanıcı girer).
    pub down_bid: f64,
    /// avg_YES + avg_NO ≤ avg_threshold → ProfitLock tetikler.
    pub avg_threshold: f64,       // varsayılan: 0.98
    /// İki averaging GTC arasındaki minimum bekleme (ms).
    pub cooldown_ms: u64,         // varsayılan: 30_000
    /// Tek tarafın toplam share limiti; aşılırsa yeni averaging yasak.
    pub max_position_size: f64,   // varsayılan: 100.0
}

// DutchBookParams ve PrismParams implementasyon sırasında tanımlanır.

impl Strategy {
    pub const fn required_metrics(self) -> MetricMask {
        match self {
            Self::DutchBook => MetricMask {
                imbalance: true,
                imbalance_cost: true,
                avgsum: true,
                profit: true,
                sum_volume: true,
                binance_signal: true,
            },
            Self::Harvest => MetricMask {
                imbalance: true,
                imbalance_cost: true,   // UI gösterimi için
                avgsum: true,           // avg_up/avg_down = avg_YES/avg_NO — ProfitLock koşulunda kullanılır
                profit: false,
                sum_volume: true,
                binance_signal: true,
            },
            Self::Prism => MetricMask {
                imbalance: false,
                imbalance_cost: false,
                avgsum: true,
                profit: true,
                sum_volume: true,
                binance_signal: true,
            },
        }
    }

    /// POSITION Δ: pay `imbalance` + brüt hacim (maskede `imbalance && sum_volume`).
    pub const fn wants_position_delta(self) -> bool {
        let m = self.required_metrics();
        m.imbalance && m.sum_volume
    }
}

impl MetricMask {
    /// Geçerlilik kuralları:
    /// 1. `profit == true` ⇒ `avgsum == true` (AVG SUM girdisi olmadan profit tanımsız).
    /// 2. `binance_signal == true` ⇒ market slug kalıbı {btc|eth|sol|xrp}-updown-* olmalı
    ///    (çalışma zamanı kontrolü BotConfig::validate'te yapılır; bu kural belgeseldir).
    pub const fn is_valid(self) -> bool {
        !self.profit || self.avgsum
    }
}
```

**Maske tutarlılığı:** `profit` üretilecekse **`avgsum`** da açık olmalıdır (`AVG SUM` girdisi olmadan `profit %` tanımsız); `avgsum: false` iken `profit: true` **ürün hatası** sayılır (`MetricMask::is_valid` false). `avgsum: true`, `profit: false` geçerlidir (ör. yalnız VWAP / `AVG SUM` gösterimi). `binance_signal: true` iken market slug kalbı desteklenen kripto şemasında (ör. `btc-updown-5m-*`) olmalı; desteklenmeyen slug’da `binance_signal` false olarak zorlanır — `signal_weight` değeri yoksayılır.

**Strateji katmanı ve pozisyon:** PnL, dengesizlik, `Position: UP=… DOWN=…` gibi özetler **yalnızca strateji motorunda** hesaplanır; Polymarket’te bunların tek başına bir **resmi “pozisyon endpoint”i yoktur**. Bu satırlar logda **strateji/uygulama** üretimi olarak işaretlenir; CLOB alanlarıyla karıştırılmaz.

**Ortak katalog — `imbalance` (dengesizlik):** **Share** bazlıdır: User WS `trade` (ve tutarlılık için strateji motorunun kullandığı emir özeti) üzerinden, ilgili outcome için **net pay (`size`)** birikir. **Pozisyon / `imbalance` / `imb_cost_*` / strateji özetleri:** aynı `trade.id` için User WS `trade` **`status=MATCHED`** olduğunda güncellenir (fill bir kez sayılır); **`MATCHED` öncesi** (`MINED` vb. yokken) bu metrikler **artmaz**. `MINED` / `CONFIRMED` yalnızca trade satırındaki zincir durumunu günceller, pay birikimini **tekrarlamaz**. **`MATCHED` hiç gelmeyen** trade kimliği için ortak katalogda birikim yapılmaz. `MATCHED` sonrası `FAILED` / `RETRYING` gibi uç durumlar için ek tutarlılık kuralları ürün olarak ayrı sabitlenir. `UP` ve `DOWN`, sırasıyla UP ve DOWN tarafındaki **toplam net share** (BUY ile artar, SELL ile azalır — strateji hesabının kurallarıyla).

`imbalance = UP − DOWN` → birimi **share** (CLOB `trade` / `size` ile aynı anlamda).

- **İşaret:** `imbalance > 0` → net **UP** share fazlası; `< 0` → net **DOWN** share fazlası; `= 0` → iki tarafta eşit net share (nötr).
- **Mutlak dengesizlik** (rapor/uyarı için): `|imbalance|` isteğe bağlı; logdaki `imbalance: +6` **işaretli** share farkıdır.

**Not:** USD veya fiyat ile çarpılmış tutar **imbalance** değil; pay sayısı (`size`) kullanılır. CLOB’da ayrı bir “resmi imbalance” alanı yoktur; bu tamamen strateji özetidir.

**Ortak katalog — `imbalance_cost`:** **Share × price** (işlem başına) ile UP ve DOWN tarafının **net maliyet** karşılaştırmasıdır. Birikim tetiki **pay `imbalance` ile aynıdır:** yalnız User WS `trade` **`status=MATCHED`**. Her `trade` için CLOB’daki **`size`** ve o işleme ait **`price`** kullanılır; strateji motoru outcome’a göre biriktirir:

- **`imb_cost_up`:** UP outcome için işaretli **`size × price`** toplamı (net maliyet).
- **`imb_cost_down`:** DOWN outcome için işaretli **`size × price`** toplamı (net maliyet).

(BUY/SELL ile netleştirme — `imbalance` ile aynı muhasebe kuralı.)

`imbalance_cost = imb_cost_up − imb_cost_down`

- **Anlam:** UP ile DOWN tarafı arasındaki **net USDC** (veya platformun quote birimi) farkı; `imbalance`’ın **para** karşılığı özetidir.
- **`imbalance` ile ilişki:** Aynı pozisyon için genelde `|imbalance_cost|` büyüdükçe taraf ayrımı güçlenir; fiyatlar değiştikçe aynı share dengesizliği farklı `imbalance_cost` verebilir (çünkü işlem fiyatları trade bazlıdır).

**Ortak katalog — `avg_up` / `avg_down` (taraf bazlı VWAP):** Aynı markette, kapsama giren **trade** listesi için (ör. “Recent Trades” penceresi — User WS `trade`):

- `avg_up = (Σ size_i × price_i)` **yalnız UP outcome satırlarında** `÷` **`Σ size_i`** (aynı satırlar).
- `avg_down = (Σ size_j × price_j)` **yalnız DOWN outcome satırlarında** `÷` **`Σ size_j`** (aynı satırlar).

Birim: **quote per share**. Net pozisyon `UP` / `DOWN` ve `imb_cost_*` ile aynı muhasebe kuralı (BUY/SELL netleştirmesi) kullanılırsa, bu ortalamalar **net pozisyona göre VWAP** ile de uyumludur; UI’da genelde **son işlemler listesindeki** taraf bazlı fill’lerden hesaplanır. **`avg_up` / `avg_down` → `AVG SUM` (`avgsum`):** referans fill listesine **yalnızca** ilgili User WS `trade` mesajında **`status=MATCHED`** olduğunda eklenir — `imbalance` ile **aynı tetik**; `MATCHED` öncesi güncelleme yok. **`profit`:** `avgsum` girdileri güncellendikten sonra aynı `MATCHED` anında veya hemen ardından ürün formülüyle türetilir; `profit` maskesi kapalıysa hesaplanmaz.

**Ortak katalog — `sum_up` / `sum_down` (brüt hacim):** Aynı trade kümesinde, taraf bazında **pay hacmi toplamı** (liste ekranındaki `×5.0` vb. değerlerin toplamı — o taraftaki tüm fill `size`’ları):

- `sum_up` = UP satırlarında `Σ size`
- `sum_down` = DOWN satırlarında `Σ size`

**Brüt hacim satırları** da ortak katalogta **`MATCHED`** anında güncellenir (`MATCHED` öncesi artış yok).

**Ortak katalog — `AVG SUM`:** İki ortalamanın toplamı — **pair cost** göstergesi (binary markette sıklıkla **~1.0** çevresi):

`AVG SUM = avg_up + avg_down`

Örnek: `AVG SUM = 1.0016` → **implied profit / edge** (paraya göre): **`(1 − AVG SUM) × 100%`** (ör. `1 − 1.0016 = −0.0016` → **−0.16%**). `1.00` teorik “par” noktası; ürün metninde bu yüzde **teorik** sapmayı gösterir.

**Ortak katalog — `POSITION Δ`:** İki parçalı gösterim (örnek UI ile uyumlu):

1. **Yüzde:** `|imbalance| / (sum_up + sum_down) × 100` — net dengesizliğin **brüt hacme** oranı (ör. **3.5%**).
2. **Yön + büyüklük:** İşaretli **`imbalance = UP − DOWN`** share cinsinden (ör. **+3.0 UP** = net 3 share UP fazlası; **−** ise DOWN tarafı baskın).

`sum_up + sum_down` sıfırsa yüzde tanımsızdır. Referans trade kümesi (tüm oturum / son N işlem / pencere) UI ve API’de **aynı** seçilmelidir; **ortak katalog** metrikleri bu kümeden türetilir — hangi stratejinin hangi alt kümeyi okuduğu matristeki **Strateji → metrik** satırlarıyla uyumlu olmalıdır (`harvest` için `profit` yok; `prism` için pay `imbalance`, `imbalance_cost` ve `POSITION Δ` yok).

---

## 2. Frontend API yüzeyi (read-only + komutlar)

Aşağıdakiler tipik bir ayrımdır; uç yollar implementasyonda netleşir.

- **Read-only:** bot listesi, bot detayı, ayar özeti, **akış logları** (metin veya sayfalı), **bota ait slug/market listesi**, **slug altında market bazlı loglar**.
- **Yazma (komut):** yeni bot, ayar güncelleme, **başlat**, **durdur**, **sil** — bunlar API’de iş kuyruğu veya durum bayrağı ile yürütülür; frontend yalnızca isteği tetikler.

**Canlı monitoring:** API, CLOB/WS üzerinden gelen **fiyat, emir, trade** bilgisini işler. Emir gönderimi **kritik yolun** ilk adımıdır (bkz. §⚡ Kural 1); log ve DB yazımı ardından **fire-and-forget** arka plan task'larında yapılır — emir göndericisini bloke etmez. **Kritik olaylar** (emir gönderimi, fill/MATCHED, PnL değişimi) SSE kanalı üzerinden frontend'e **anında push** edilir (bkz. §⚡ Kural 5); özet ekranlar polling ile güncellenir.

**Özet ekran yenileme:** Frontend, pano ve özet metrikleri (pozisyon, son işlemler, bot durumu) **yaklaşık 1 saniye aralıkla** API’den okuyarak günceller (kısa aralıklı **polling**). Ham WebSocket akışı API’de işlenir; istemci tarafında WS zorunlu değildir.

---

## 3. Market seçimi: “şu anki” ve “sonraki”

Kullanıcı örneğin **“BTC 15dk”** (veya 5dk vb.) gibi tekrarlayan bir **pencere** seçer.

| Seçenek | Davranış |
|---------|-----------|
| **Güncel (aktif) market** | Bot, **şu anda devam eden** pencereye ait marketten başlar (Gamma/slug ile çözülen güncel `market` / `clobTokenIds`). |
| **Sonraki market** | Bot, **sıradaki** markete odaklanır; o marketin **Gamma** kaydındaki **startDate** ve **endDate** pencere sınırları olarak kullanılır (geçerli zaman dilimine düşen market satırı). |

Bu seçim **yalnızca başlangıç noktasını** belirler. Bot çalışmaya başladıktan sonra her pencere kapandığında **otomatik olarak sıradaki markete** geçer — ayrı bir `başlat` komutu gerekmez. Bot, **kullanıcı `durdur` komutunu gönderene kadar** (veya kritik hata ile crash olana kadar) sürekli çalışır; supervisor bu döngüye müdahil olmaz.

---

## 4. Zaman çizelgesi: T−15 ve bağlantılar

**Aktif güncel market** veya **sonraki market** modu için:

- Hedef marketin **başlangıcından 15 saniye önce (T−15)** ilgili market için **ön veri hazırlanır** (Gamma’dan çözümlenmiş slug/market, `clobTokenIds`, **startDate / endDate**).
- **T−15** anında **CLOB REST** (kimlik, orderbook, hazırlık) ve **Market WebSocket** (+ gerekiyorsa **User WebSocket**) bu market için **hazır** olur; abonelikler ilgili `asset_id` listesiyle kurulur.
- **T−15 market init:** `GET /book` ile her token için **`api_min_order_size`** (share cinsinden minimum emir) ve **`tick_size`** (minimum fiyat artımı) okunur; her ikisi market state'ine yazılır. `tick_size`'a uymayan limit fiyatlar API tarafından `INVALID_ORDER_MIN_TICK_SIZE` hatasıyla reddedilir. `HarvestParams::up_bid` / `down_bid` bu aşamada `tick_size` ile doğrulanır; uyumsuzluk bot başlatmayı reddeder.
- Pencere boyunca seçilen **strateji** (`dutch_book` / `harvest` / `prism`) döngüsü çalışır; pencere sonuna doğru (ör. `stop_before_end_ms`) erken durdurma kuralları uygulanabilir.
- Pencere **endDate** (veya strateji kuralı) ile uyumlu şekilde sonlandırılır; log: market tamamlandı, sıradaki hedefe geçiş.

**Sonraki market** seçildiğinde: sıradaki marketin **startDate** / **endDate** değerleri tek kaynak olarak kullanılır; T−15 bu **startDate**’e göre hesaplanır.

### WebSocket işletimi (Market + User)

Resmi [WebSocket overview](https://docs.polymarket.com/market-data/websocket/overview): **Market** ve **User** kanallarında istemci yaklaşık **10 saniyede bir** düz metin **`PING`** gönderir; sunucu **`PONG`** döner — bağlantı kopmasını önlemek için zorunlu kabul edilir.

**Yeniden bağlanma:** Oturum koptuğunda veya bot market değiştirdiğinde **abonelik mesajı yeniden** gönderilir (`type`, `assets_ids` / `markets`, User için `auth`, Market için `custom_feature_enabled: true`); aksi halde olay akışı gelmez.

**Market kanalı — orderbook (yalnız resmi şema):** [Market Channel](https://docs.polymarket.com/market-data/websocket/market-channel) tanımına göre **`book`** olayı **bir `asset_id` için ilk abonelikte** ve **defteri etkileyen trade’de** yayınlanır; tam `bids` / `asks` ve **`hash`** payload’da taşınır. **`price_change`** yeni emir veya iptalde seviye güncellemelerini taşır; her öğede **`hash`** vardır; **`size` = `"0"`** seviyenin kaldırıldığını belirtir (resmi metin). Bağlantı koptuğunda [WebSocket overview](https://docs.polymarket.com/market-data/websocket/overview) ile abonelik yenilenir; lokal defter, **kopuk süreye ait varsayım taşınmaz** — güncel durum yalnızca bu kanaldan gelen **`book`** ve **`price_change`** ile güncellenir. Resmi dokümanda yer almayan yerel `hash` yeniden hesaplama veya REST ile paralel “ikinci baseline” mimaride tanımlanmaz; `hash` alanları kayıtta sunucunun döndüğü değerle saklanabilir. §6 SQLite orderbook satırları bu olaylarla uyumludur.

### 4.1 REST heartbeat (CLOB — resmi [Orders overview — Heartbeat](https://docs.polymarket.com/trading/orders/overview))

Aşağıdaki maddeler **doğrudan** resmi dokümandaki Heartbeat bölümüyle uyumludur (WebSocket `PING`/`PONG` değildir):

- **Amaç:** Oturum canlılığı — emir güvenliği.
- **Zamanlama:** Geçerli heartbeat **10 saniye** içinde gelmezse (**en fazla ~5 saniyelik tampon** ile), **tüm açık emirler iptal** edilir (resmi metin).
- **İstek sıklığı:** Resmi örneklerde döngü **`sleep(5)`** / **`setInterval(..., 5000)`** ile yaklaşık **5 saniyede bir** `postHeartbeat` çağrısı gösterilir — uygulama bu aralığı **resmi örnekle** hizalar; gerektiğinde daha sık gönderim tampon içinde kalır.
- **`heartbeat_id`:** İlk istekte **boş string**; her yanıtta dönen **`heartbeat_id`** bir sonraki istekte gönderilir. Geçersiz/expired id → sunucu **400 Bad Request** ve yanıtta **doğru `heartbeat_id`** döner; istemci güncelleyip tekrar dener (resmi madde).
- **SDK:** Rust örneği `post_heartbeat` / arka planda `start_heartbeats` — [Orders overview](https://docs.polymarket.com/trading/orders/overview) ile aynı akış.
- **Ürün kuralı:** Bot CLOB oturumu açıkken REST heartbeat döngüsü **kesintisiz çalışır** (açık emir olmasa da); resmi “10 saniye içinde geçerli heartbeat yoksa tüm açık emirler iptal” kuralına düşmemek için zorunlu kabul edilir.

---

## 5. Loglama

### 5.1 Biçim

- Her satır: `[HH:MM:SS.mmm] [bot_etiketi] mesaj` (ör. `[btc]`).
- **Bot etiketi** kullanıcı tanımlı kısa ad; **slug/market** ve **pencere** bilgisi ayrı satırlarda tekrarlanır.
- Metinler **stdout** (supervisor tarafından okunup SQLite'a yazılır; bkz. §1 Loglama akışı) ve API üzerinden **SSE log akışı** için aynı formatta üretilebilir. Bot kendi başına ayrı log dosyası **açmaz** (§1 kuralı).

### 5.2 Örnek: tek market penceresi (tam metin)

Aşağıdaki blok **örnek veridir**; zaman damgaları ve id’ler hayalidir.

```
[10:19:55.725] [btc] Target market: btc-updown-5m-1776420900
[10:19:55.725] [btc] Window: 2026-04-17 10:15:00 UTC - 2026-04-17 10:20:00 UTC
[10:19:55.725] [btc] 📡 Fetching market: btc-updown-5m-1776420900
[10:19:55.763] [btc] ✅ Found market: Bitcoin Up or Down - April 17, 6:15AM-6:20AM ET
[10:19:55.763] [btc]       UP:   10888309533765379088623246783892...
[10:19:55.763] [btc]       DOWN: 30504641493152850985876961001926...
[10:19:55.763] [btc] 🔐 Initializing trading client...
[10:19:55.763] [btc]    Address: 0xc09f3985a738A6c45a4b1294274790d7698c718a
[10:19:55.766] [btc]    Using Poly Proxy wallet (Magic Link)
[10:19:55.766] [btc]    Funder: 0xd1246EC6b187738aEEB54f038a3aE10958F39caf
[10:19:56.325] [btc]    ✅ Authenticated successfully
[10:19:56.325] [btc]    Deriving API credentials...
[10:19:56.372] [btc]    ✅ Derived existing API key
[10:19:56.372] [btc] 🚀 Starting trading loop (strategy: dutch_book)...
[10:19:56.372] [btc] 🧹 Cleaning up any previous open orders...
[10:19:56.372] [btc]    (REST `GET /orders` yok — iptal/yerel durum veya `DELETE` ile bilinen id’ler; açık emir takibi User WS `order`.)
[10:19:56.406] [btc] 🔌 Connecting to Market WebSocket...
[10:19:56.406] [btc] 🔌 Connecting to User WebSocket...
[10:19:56.508] [btc]    ✅ User WS connected (101 Switching Protocols)
[10:19:56.508] [btc]    ✅ Subscribed to order updates
[10:19:56.540] [btc]    ✅ Connected (101 Switching Protocols)
[10:19:56.540] [btc]    ✅ Subscribed to UP and DOWN assets
[10:19:56.553] [btc] 📚 [PRICE] UP | Bid: $0.99 | Ask: $0.00 | Spread: $-0.99
[10:19:56.553] [btc] 📚 [PRICE] DOWN | Bid: $0.00 | Ask: $0.01 | Spread: $0.01
[10:19:56.553] [btc] 📦 POST /order (249ms)
[10:19:56.553] [btc] ✅ orderType=GTC side=BUY outcome=YES | POST status=live orderID=0x8e9a3174e6429c...
```

**POST `/order` yanıtı `matched` ise** (deftere düşmeden eşleşme; `tradeIDs` dönebilir — **FOK** tam dolum, **FAK/GTC** ilk vuruşta kısmi olabilir; bkz. §5.5 `matched` satırı):

```
[10:19:56.600] [btc]  ✅ orderType=FOK side=BUY outcome=YES | POST status=matched orderID=0xabc... tradeIDs=[28c4d2eb-bbea-40e7-a9f0-b2fdb56b2c2e] tx=[0x...]
```

**User WebSocket `trade` olayı** — bu projede **trade logu ve SQLite trade kaydı yalnızca buradan**; **`GET /trades` kullanılmaz.**

```
[10:19:56.610] [btc]  📬 WS trade | id=28c4d2eb-bbea-40e7-a9f0-b2fdb56b2c2e status=MATCHED taker_order_id=0x06bc... side=BUY outcome=YES size=10 price=0.57 trader_side=TAKER
[10:19:58.200] [btc]  📬 WS trade | id=28c4d2eb-bbea-40e7-a9f0-b2fdb56b2c2e status=CONFIRMED last_update=1672290705
```

**Eşleme:** `POST /order` yanıtındaki **`tradeIDs`** içindeki kimlik, hemen ardından (veya kısa gecikmeyle) gelen User WS **`trade`** olayındaki **`id`** ile **aynıdır**; kısmi dolumda User WS **`order`** / **`UPDATE`** içindeki **`associate_trades`** de aynı trade kimliklerini içerir. **Her fill parçası** bir **`trade.id`** ile birleşir; aynı emir **birden fazla** `trade.id` üretebilir (SQLite ve §10 — upsert `trade.id` anahtarı).

**Fill özeti** (WS `trade` / ilgili order yanıtlarına dayalı özet) ve **pozisyon** (strateji hesabı — resmi endpoint yok):

```
[10:19:56.553] [btc] ✅ fill_summary outcome=YES (UP) size=6 price=0.68 order_type=GTC order_id=0x8e9a3174e6429c...
[10:19:56.553] [btc] 📊 [strategy] Position: UP=6, DOWN=0 (imbalance: +6)
```

**İptal (`DELETE /order` veya `/orders`)** — yanıt: `canceled` / `not_canceled` ([polymarket-clob.md](api/polymarket-clob.md)):

```
[10:20:27.133] [btc] 🚫 DELETE /order (1 id)
[10:20:27.159] [btc]      canceled=[0x9d056a3c627211...] not_canceled={}
```

**Kısmen başarısız iptal örneği:**

```
[10:20:27.159] [btc]      canceled=[] not_canceled={"0xabc...":"Order not found or already canceled"}
```

**Pencere sonu / erken durdurma / sıradaki market:**

```
[10:19:56.642] [btc] ⏰ Only 30s until window end, stopping early (stop_before_end_ms=30000)
[10:19:56.664] [btc] 🏁 Market window complete, transitioning to next market...
[10:19:56.664] [btc] ✅ Market #10 complete, moving to next...
```

### 5.3 Örnek: `market_resolved` (metin log)

Olay, **Market WebSocket** üzerinden `event_type: market_resolved` ile geldiğinde (bkz. §9) özet satır:

```
[10:25:12.100] [btc] 🏆 market_resolved | market=0x311d0c4b... | winning_outcome=Yes | winning_asset_id=76043073756653678226373981964075571318267289248134717369284518995922789326425 | ts=1766790415550
```

**5+5+10 sonrası olay yoksa** (SQLite ile aynı anlam): metin logda örneğin `resolution=not resolved | market=0x...` gibi **İngilizce** bir satır; kazanan alanı uydurulmaz.

### 5.4 API / DB ile hizalı yapısal satır (isteğe bağlı JSON)

Alan adları mümkün olduğunca **CLOB** ile aynı: `orderType`, `status` (POST yanıtı küçük harf `live`/`matched`/`delayed`/`unmatched`; **trade satırlarında** yalnız User WS `trade` — `MATCHED` / `MINED` / `CONFIRMED` / …), `orderID`, `tradeIDs`, `canceled`/`not_canceled`. User **`order`** ve **`trade`** yapısal örnekleri, [User channel](https://docs.polymarket.com/market-data/websocket/user-channel) resmi örnekleriyle aynı üst düzey alan adlarını kullanır (`type`, `event_type`, `associate_trades`, …).

```json
{"ts":"2026-04-17T10:19:56.553Z","bot":"btc","slug":"btc-updown-5m-1776420900","level":"info","event_type":"book","source":"market_ws","asset_id":"...","market":"0x...","bids":[{"price":"0.48","size":"30"}],"asks":[{"price":"0.52","size":"25"}],"timestamp":"1757908892351"}
```

```json
{"ts":"2026-04-17T10:19:56.553Z","bot":"btc","source":"rest","method":"POST","path":"/order","orderType":"GTC","side":"BUY","outcome":"YES","latency_ms":249,"success":true,"status":"live","orderID":"0x8e9a3174e6429c...","errorMsg":""}
```

```json
{"ts":"2026-04-17T10:19:56.553Z","bot":"btc","source":"user_ws","event_type":"order","type":"PLACEMENT","order_type":"GTC","id":"0xff35...","status":"LIVE","side":"SELL","price":"0.57","original_size":"10","size_matched":"0","asset_id":"52114319501245915516055106046884209969926127482827954674443846427813813222426","market":"0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af","associate_trades":null,"timestamp":"1672290687"}
```

```json
{"ts":"2026-04-17T10:19:56.553Z","bot":"btc","source":"user_ws","event_type":"order","type":"UPDATE","id":"0xff35...","size_matched":"5","associate_trades":["28c4d2eb-bbea-40e7-a9f0-b2fdb56b2c2e"]}
```

```json
{"ts":"2026-04-17T10:19:56.553Z","bot":"btc","source":"user_ws","event_type":"order","type":"CANCELLATION","id":"0xff35..."}
```

```json
{"ts":"2026-04-17T10:19:56.553Z","bot":"btc","source":"user_ws","event_type":"trade","type":"TRADE","id":"28c4d2eb-bbea-40e7-a9f0-b2fdb56b2c2e","status":"MATCHED","asset_id":"52114319501245915516055106046884209969926127482827954674443846427813813222426","market":"0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af","taker_order_id":"0x06bc63e346ed4ceddce9efd6b3af37c8f8f440c92fe7da6b2d0f9e4ccbc50c42","side":"BUY","size":"10","price":"0.57","outcome":"YES","trader_side":"TAKER","matchtime":"1672290701","timestamp":"1672290701","last_update":"1672290701","maker_orders":[{"order_id":"0xff354cd7ca7539dfa9c28d90943ab5779a4eac34b9b37a757d7b32bdfb11790b","matched_amount":"10","price":"0.57","outcome":"YES","asset_id":"52114319501245915516055106046884209969926127482827954674443846427813813222426","owner":"9180014b-33c8-9240-a14b-bdca11c0a465"}],"owner":"9180014b-33c8-9240-a14b-bdca11c0a465","trade_owner":"9180014b-33c8-9240-a14b-bdca11c0a465"}
```

```json
{"ts":"2026-04-17T10:20:27.159Z","bot":"btc","source":"rest","method":"DELETE","path":"/order","canceled":["0x9d056a3c627211..."],"not_canceled":{},"latency_ms":25}
```

```json
{"ts":"2026-04-17T10:25:12.100Z","bot":"btc","event_type":"market_resolved","market":"0x311d0c4b...","winning_outcome":"Yes","winning_asset_id":"76043073756653678226373981964075571318267289248134717369284518995922789326425","timestamp":"1766790415550"}
```

Bu JSON satırları **isteğe bağlı yapısal log** içindir; **User WS `trade`** satırları ayrıca §10’daki **trade tablosuna** (özellikle `id`) yazılır — çift tanım değil, aynı olayın iki biçimi.

### 5.5 Log planı — CLOB API terimleri ile uyum

Kaynak: resmi [docs.polymarket.com](https://docs.polymarket.com/) CLOB bölümü; repo özeti: [polymarket-clob.md](api/polymarket-clob.md) (`POST /order`, `DELETE /order`, User WS `order` / `trade`). **Kullanılmaz:** `GET /trades`, `GET /orders` (bkz. üst tablo).

#### Emir tipleri (`orderType` — istek gövdesi)

| `orderType` | Anlam (logda aynı string) |
|-------------|---------------------------|
| **GTC** | İptal edilene kadar defterde; varsayılan limit. |
| **FOK** | Tam ve anında fill yoksa iptal (Fill or Kill). |
| **GTD** | `expiration`’a kadar; sonra otomatik düşer. |
| **FAK** | Olabildiğince fill, kalan iptal (Fill and Kill / IOC). |

Logda her emir satırında **`orderType=`** ve mümkünse **`expiration=`** (GTD için) kullanılmalıdır.

**Platform emir fiyatı kuralı (ürün geneli):**

| Emir tipi | Fiyat tarafı | Rol | Açıklama |
|---|---|---|---|
| **GTC** (ve GTD, FOK) | **Bid** (alış teklifi) | Maker | Deftere yazılır; karşı taraf gelene kadar bekler. Fiyat `best_bid` veya altında belirlenir. |
| **FAK** | **Ask** (satış teklifi) | Taker | Spread’i geçer; mevcut `best_ask`’a çarpar ve anında dolar. |

Tüm stratejiler bu kurala uyar: **GTC = bid fiyatıyla maker emir; FAK = ask fiyatıyla taker emir.** Aksi belirtilmedikçe strateji dokümanlarındaki fiyat ifadeleri bu sözleşmeye göredir.

#### Emir tipi ve fill / kısmi dolum (resmi [Orders overview — Order Types](https://docs.polymarket.com/trading/orders/overview))

Resmi tabloya göre:

- **FOK:** Anında **tamamı** dolar veya **tüm emir iptal** — kısmi dolum yok (`FOK_ORDER_NOT_FILLED_ERROR` vb.).
- **FAK:** Anında mümkün olan kadar dolar, **kalan iptal** — girişte kısmi dolum beklenen tiptir.
- **GTC / GTD:** Limit emir; defterde kalır, **zaman içinde birden fazla eşleşme** ile kademeli dolum olabilir (aynı `orderID` için birden fazla `trade`).

REST **OpenOrder** alanları (`associate_trades`, `size_matched`, `original_size`) resmi dokümanda bu emirlerin **hangi trade kimliklerine** dağıldığını ve ne kadarının dolduğunu gösterir; User WS **`order` `UPDATE`** ile `size_matched` artabilir, `associate_trades` büyüyebilir. Ortak katalog / strateji birikimi: her **yeni** `trade.id` için **`MATCHED`** bir kez sayılır (bkz. üstteki ortak katalog — `imbalance` / `imbalance_cost` ve §10–§11); aynı emrin parçalı dolumu = **birden fazla trade satırı**, tek trade’de çift sayım yapılmaz.

**Strateji tanımı vs durum:** Çalışan **`Strategy` seçimi** ve **`MetricMask` / katalog formülleri** parçalı dolumla **değişmez**; güncellenen, her `MATCHED` trade ile birlikte **`StrategyMetrics`** (ve buna bağlı eşik / aksiyon kararları) ile motorun **anlık durumudur** — kısmi dolum, tam dolumla aynı olay modeliyle kademeli güncelleme üretir.

#### REST `POST /order` yanıtı (`status`)

| `status` | Ne zaman logla |
|----------|----------------|
| **live** | Emir defterde kaldı; `orderID` ile takip. |
| **matched** | Deftere düşmeden eşleşme oldu; `tradeIDs` / `transactionsHashes` olabilir. **FOK** için tam dolum şartıdır; **FAK** veya **GTC** ilk vuruşta kısmi olabilir — kalan miktar için **`live`** veya sonraki **`order` UPDATE** ile `size_matched` takibi gerekir (tek `matched` yanıtını “tam dolum” sanma). |
| **delayed** | Eşleşme ertelendi; `errorMsg` ile birlikte (ör. limit). |
| **unmatched** | Marketable ama gecikme başarısız; yerleştirme yine başarılı (resmi insert status tablosu). |

#### REST `GET /orders` — bu mimaride kullanılmaz

Polymarket dokümantasyonunda açık emirler için REST [Querying Orders](https://docs.polymarket.com/trading/orders/overview) anlatılır; bu projede çağrılmaz. Açık emir durumu yalnız User WS **`order`** ile kurulur.

#### User WebSocket — `order` olayı

| Alan | Logda |
|------|--------|
| `type` (User WS `order`) | Resmi: **`PLACEMENT`** / **`UPDATE`** / **`CANCELLATION`** — metin logda `type=` |
| `order_type` | **GTC** / **FOK** / **GTD** / **FAK** |
| `status` | Örnekte **LIVE**; kısmi dolumda `UPDATE` ile `size_matched` artar. |

#### User WebSocket — `trade` olayı (tek trade kaynağı)

| Alan | Logda |
|------|--------|
| `status` | WS payload — **duruma göre** `MATCHED` → `MINED` → `CONFIRMED` (veya `RETRYING` / `FAILED`); aynı `id` ile güncellenir; bkz. §11. |
| `trader_side` | **TAKER** / maker tarafı için `maker_orders` özetlenebilir. |

**Not:** CLOB REST `GET /trades` ve `TRADE_STATUS_*` değerleri resmi dokümanda vardır; **bu uygulama trade için REST kullanmaz** (§10, §11).

#### İptal — `DELETE /order` / `DELETE /orders`

| Yanıt alanı | Logda |
|-------------|--------|
| `canceled` | Başarılı iptal edilen `orderID` listesi. |
| `not_canceled` | `{ orderID: hata_metni }` — **“iptal ediliyor”** yerine önce istek, sonra bu yapı. |

#### Önerilen metin log şablonları (kısa)

```
… POST /order orderType=GTC … → status=live orderID=0x…
… POST /order orderType=FOK … → status=matched tradeIDs=[…]
… POST /order … → status=delayed errorMsg="…"
… POST /order … → status=unmatched …
… WS order type=PLACEMENT order_type=GTC status=LIVE id=0x…
… WS order type=UPDATE id=0x… size_matched=… associate_trades=[…]
… WS order type=CANCELLATION id=0x…
… WS trade id=… status=MATCHED taker_order_id=0x… trader_side=TAKER
… WS trade id=… status=CONFIRMED (aynı id, güncelleme)
… DELETE /order canceled=[0x…] not_canceled={}
… DELETE /orders (bulk) canceled=[0x…, 0x…] not_canceled={}
```

---

## 6. SQLite — WebSocket orderbook anlık görüntüsü

**Market** kanalından resmi **`book`** ve **`price_change`** olayları ([Market Channel — Message Types](https://docs.polymarket.com/market-data/websocket/market-channel)) API’de işlendikten sonra, **her kayıt** için en az şu alanlar saklanabilir — alan adları resmi örneklerle uyumludur:

| Alan | Anlamı |
|------|--------|
| `asset_id` | Outcome token kimliği |
| `market` | Market (condition) kimliği |
| `bids` / `asks` | Derinlik yapısı (JSON veya normalize tablo) |
| `hash` | Resmi `book` / `price_change` payload’ındaki değer (varsa) |
| `timestamp` | Olayın zaman damgası (kaynak: WS payload) |

Amaç: frontend’in “son bilinen orderbook”ı ve geçmiş kırılımını göstermesi; ham WS gövdesinin tamamını değil, **işlenmiş** anlık görüntüyü tutmak yeterlidir.

---

## 7. SQLite — bir sonraki market (ön kayıt) ve pencere başlangıcı

- **Yeni pencere başlamadan önce:** Bir sonraki marketin **kimlik/slug/pencere** bilgisi mümkün olduğunca **önceden** bir satır olarak kaydedilir (ör. “planlanan market”).
- **Pencere başladığında:** Aynı satır (veya ilişkili satır) **güncellenir**: token id’ler, bağlantı durumu, strateji parametreleri, ilk orderbook özeti vb. eklenir.

Böylece “gelecek market” ile “şu an işlenen market” tek tablo hiyerarşisi veya ilişkisel anahtarlarla takip edilir.

---

## 8. SQLite — emir kayıtları (bot × market)

Kalıcı emir izi **User WS `order`** ve **`POST /order`** / **`DELETE /order`** (veya **`DELETE /orders`**) yanıtlarından beslenir — **`GET /orders` yok** (üst tablo).

**Birincil anahtar:** **`order_id`** = User WS `order` **`id`** (REST **`orderID`** ile aynı kimlik).

| Alan | Açıklama |
|------|----------|
| `order_id` | Benzersiz emir kimliği (CLOB hash) |
| `bot_id`, `market_session_id` | Ürün FK (bot ve market oturumu / pencere) |
| `source` | `user_ws` (`order` olayı) \| `rest_post` (`POST /order`) \| `rest_delete` (`DELETE` …) |
| `lifecycle_type` | User WS `order` resmi **`type`**: `PLACEMENT` / `UPDATE` / `CANCELLATION`; REST-only satırda boş |
| `market`, `asset_id`, `side`, `price`, `outcome` | Payload / REST (ham string) |
| `order_type` | Emir tipi: GTC / FOK / GTD / FAK (payload `order_type`) |
| `original_size`, `size_matched` | [OpenOrder](https://docs.polymarket.com/trading/orders/overview) anlamıyla uyumlu |
| `expiration` | GTD vb. |
| `associate_trades` | JSON veya metin — trade `id` listesi |
| `post_status` | REST `POST /order` sonrası: `live` / `matched` / `delayed` / `unmatched` |
| `order_status` | User WS `order` içi `status` (ör. `LIVE`) — gelen değer bire bir |
| `delete_canceled`, `delete_not_canceled` | `DELETE` yanıtı alanları (`canceled`, `not_canceled`) |
| `ts` | Payload `timestamp` veya yerel alım zamanı |
| `raw_payload` | İsteğe bağlı — denetim / yeniden oynatma |

**Upsert:** Aynı `order_id` için `UPDATE` ve REST sonrası güncellemeler **aynı satırı** iter; `PLACEMENT` veya ilk REST kaydı **insert**.

---

## 9a. SQLite — `bot_credentials` (per-bot kimlik saklama)

Frontend'den girilen Polymarket credential'ları bot başına bu tabloda tutulur. §1 "Kimlik ve cüzdan" altındaki öncelik sırası: bu tablodaki kayıt → `.env` fallback.

```sql
CREATE TABLE bot_credentials (
    bot_id               INTEGER PRIMARY KEY REFERENCES bots(id) ON DELETE CASCADE,
    poly_address         TEXT,           -- Polymarket proxy / owner address
    poly_api_key         TEXT,           -- L2 API key UUID
    poly_passphrase      TEXT,           -- L2 passphrase
    poly_secret          TEXT,           -- L2 secret (HMAC için)
    polygon_private_key  TEXT,           -- L1 EIP-712 signing key (plaintext)
    poly_signature_type  INTEGER DEFAULT 0,   -- 0 = EOA, 1 = proxy, 2 = gnosis safe
    poly_funder          TEXT,           -- proxy/safe için funder adresi
    updated_at           INTEGER NOT NULL     -- unix ms
);
```

**Alan anlamları:**

| Alan | Kaynak | Kullanım |
|---|---|---|
| `bot_id` | `bots.id` FK | Bir bot silinirse credential de silinir (`ON DELETE CASCADE`) |
| `poly_address` | [Auth overview](https://docs.polymarket.com/trading/clob-api/authentication) | L1 header `POLY_ADDRESS` |
| `poly_api_key` | `POST /auth/api-key` veya `POST /auth/derive-api-key` | L2 header `POLY_API_KEY` |
| `poly_passphrase` | Aynı kaynak | L2 header `POLY_PASSPHRASE` |
| `poly_secret` | Aynı kaynak | HMAC-SHA256 base64 URL_SAFE anahtarı |
| `polygon_private_key` | Kullanıcı cüzdanı | EIP-712 order imzalama (plaintext; bkz. §1 güvenlik notu) |
| `poly_signature_type` | [Order signing](https://docs.polymarket.com/trading/orders/signing) | 0 = direct EOA; 1 = Polymarket proxy; 2 = Gnosis Safe |
| `poly_funder` | Proxy/Safe senaryosu | `signatureType ≠ 0` için zorunlu |

**Okuma:**

```sql
SELECT * FROM bot_credentials WHERE bot_id = ?;
```

Bot başlatılırken bu sorgu çalıştırılır; sonuç boşsa `.env` okunur.

**Güvenlik:** bkz. §1 "Credential saklama politikası" — plaintext saklama gerekçesi ve OS düzeyi koruma önerileri.

---

## 9. SQLite — `market_resolved` (tek kaynak: resmi WebSocket)

**Resmi dokümantasyon:** [Market Channel (WebSocket)](https://docs.polymarket.com/market-data/websocket/market-channel) — (`developers/CLOB/...` eski yolu yönlendirme ile açılabilir; kanonik sayfa `market-data` altındadır.)

**Abonelik şartı:** İlk subscription mesajında `"custom_feature_enabled": true` gönderilmelidir; aksi halde `market_resolved`, `new_market` ve `best_bid_ask` olayları **yayınlanmaz** (aynı sayfada belirtilir).

**Olay:** `event_type: "market_resolved"` — market **çözümlendiğinde** tetiklenir. Örnek payload alanları: `market`, `assets_ids`, `winning_asset_id`, `winning_outcome`, `timestamp`, `slug`, `question`, vb. (resmi örnekle uyumlu). **`winning_outcome`** SQLite ve logda **ham string** (ör. resmi örnekteki gibi `"Yes"`) — bkz. üst bölüm **«CLOB alanları ve UP/DOWN»**.

**Zincir / oracle bağlamı** (neden hemen olmayabilir): Çözümleme [UMA Optimistic Oracle](https://docs.polymarket.com/concepts/resolution) kurallarına tabidir; teklif, itiraz ve oylama süreleri zamanlamayı belirler — **tahminî süre** üretip buna göre **sentetik** `market_resolved` veya sahte zamanlama **kullanılmaz** (yalnızca gerçek WS olayı veya ürün kuralı `not resolved`).

**Uygulama kuralı:** `market_resolved` mesajı alındığında SQLite’a **doğrudan** işlenir. **Gamma REST ile kazanan uydurma** veya API dışı kanal üzerinden aynı bilginin **üretilmesi** tanımlanmaz.

**Gecikme ve 5+5+10:** Olay geç gelirse bkz. üst bölüm **«`market_resolved` gecikmesi ve yeniden deneme»** — yalnızca bekleme + aynı resmi kanalı tekrar; sonunda yoksa **`not resolved`**; veri uydurma yok.

Repo içi özet (Market WS): [polymarket-clob.md](api/polymarket-clob.md) — resmi şema ile çelişmezse kullanılır.

---

## 10. SQLite — trade kayıtları (bot × market)

Her **bot** ve her **market oturumu** için trade satırları **yalnızca User WebSocket `trade` olaylarından** loglanır ve yazılır — **REST `GET /trades` kullanılmaz**; içeride uydurulmuş trade yoktur.

**Her `trade` olayı:** İşlendiğinde strateji motoru önce state'i günceller (`imbalance`, `avg_*`, `MarketPnL`), ardından gerekiyorsa sonraki emir kararını verir. Log ve SQLite yazımı **fire-and-forget** arka plan task'larında yapılır; strateji motorunu veya emir göndericisini bloke etmez (bkz. §⚡ Kural 4). Yazım içeriği: **(1)** metin loga **en az bir** trade satırı, **(2)** SQLite’ta **`trade.id`** anahtarıyla kayıt: **ilk** mesaj (genelde `MATCHED`) **ekleme**, **aynı `id` ile sonraki** mesajlar (`MINED`, `CONFIRMED`, …) **aynı satırı günceller** — çift satır yok, durum ilerlemesi tek kayıtta tutulur. Strateji özeti (pozisyon, `imbalance`, vb.) **aynı `trade.id` için `MATCHED` ile bir kez** güncellenir; sonraki status’lar yalnız trade kaydındaki alanları günceller (bkz. üst bölüm **«imbalance»**).

**Kaynak:** [User channel / trade](api/polymarket-clob.md) — resmi şema [docs.polymarket.com](https://docs.polymarket.com/market-data/websocket/user-channel) ile uyumlu tutulur.

**Emir ↔ fill eşlemesi (trade `id`):** CLOB’da bir işlemin kalıcı kimliği User WS **`trade`** payload’ındaki **`id`** alanıdır (ör. UUID string — [polymarket-clob.md](api/polymarket-clob.md)). **Emir tarafı:** `POST /order` yanıtında **`tradeIDs`** veya User WS **`order`** olayında **`associate_trades`** içinde aynı kimlik geçer. **SQLite:** **`trade.id`** benzersiz; tekrarlayan mesajlar **upsert**. Emir ekranında “beklenen fill” ile “gelen fill” **aynı trade id** üzerinden birleştirilir.

Örnek alanlar (isimler dokümandaki ile uyumludur):

| Alan | Açıklama |
|------|----------|
| `id` | Trade kimliği — **benzersiz anahtar**; `tradeIDs` / `associate_trades` ile eşleşir |
| `taker_order_id` | Taker emir kimliği |
| `side` | `BUY` / `SELL` |
| `size`, `price` | İşlem büyüklüğü ve fiyat |
| `fee_rate_bps` | Ücret (baz puan) |
| `status` | §11 — User WS `trade` yaşam döngüsü (`MATCHED` → `MINED` → `CONFIRMED` veya `FAILED` / `RETRYING`) |
| `matchtime` / `last_update` | User WS `trade` payload (CLOB örnekleriyle uyumlu) |
| `outcome` | Ham string — resmi payload ile bire bir (ör. `"YES"`) |
| `trader_side` | `TAKER` / `MAKER` — User WS `trade` payload'ında gelir (resmi [Orders overview — Trade Object](https://docs.polymarket.com/trading/orders/overview) şemasında tanımlı; WS kanalında da aynı alan adı ve değerle iletilir) |
| `maker_orders` | Maker tarafında birden fazla parça varsa JSON; **maker** perspektifinde log üretilecekse bu dizi işlenir |

**Not:** Kullanıcı bazen **maker**, bazen **taker** olur; raporlama ve PnL için `trader_side` ve `maker_orders` ayrımı önemlidir.

---

## 11. Trade `status` (User WebSocket — bu proje)

Bu projede trade **yalnızca User WS `trade`** ile takip edilir; **`GET /trades` çağrılmaz.** Log ve SQLite’da **`status`** her zaman **gelen WS payload’daki** değerdir (duruma göre **güncellenir**). Kaynak: [User channel — trade](https://docs.polymarket.com/market-data/websocket/user-channel) — `MATCHED` → `MINED` → `CONFIRMED` ve hata yolları.

**Yaşam döngüsü (resmi özet):** Aynı **`trade.id`** için sunucu sırayla (veya atlayarak) **durum güncellemesi** gönderebilir; uygulama **her mesajı** loglar ve SQLite’ta **aynı satırı** yeni `status` ve `last_update` ile günceller.

| `status` | Terminal? | Anlam (özet) |
|----------|-----------|--------------|
| `MATCHED` | Hayır | Eşleşme operatöre iletildi; zincir kesinliği yok. |
| `MINED` | Hayır | İşlem zincirde görüldü; nihai kesinlik henüz yok. |
| `CONFIRMED` | Evet (başarı) | Güçlü olasılıksal kesinlik; başarılı kabul. |
| `RETRYING` | Hayır | TX başarısız/reorg; yeniden deneniyor (`MINED` ile döngü). |
| `FAILED` | Evet (hata) | Başarısız, tekrar yok. |

Tahminî veya REST’ten türetilmiş **synthetic** `status` yazılmaz; **yalnızca gelen** WS mesajlarındaki `status` kalıcıya işlenir.

---

## 12. Özet tablo: ne ne zaman yazılır?

| Olay | Ne zaman | Nereye |
|------|----------|--------|
| Metin log | Sürekli (fire-and-forget — bkz. §⚡ Kural 4) | stdout → supervisor → SQLite `logs` |
| **Kritik olay push** (emir gönderildi, fill/MATCHED, PnL değişimi) | **Anında** — emir göndericisi tamamlandıktan hemen sonra | SSE kanalı → Frontend (bkz. §⚡ Kural 5) |
| WS orderbook özeti | Olay geldikçe (fire-and-forget) | SQLite (işlenmiş `asset_id`, `market`, `bids`/`asks`, `timestamp`) |
| Sonraki market planı | Pencere öncesi | SQLite (ön kayıt) |
| Pencere başlangıcı | T=0 | Aynı satırın güncellenmesi |
| Trade satırları | Her User WS `trade` mesajı — state güncellemesi önce, log + SQLite upsert fire-and-forget | SQLite §10 + log (`GET /trades` yok) |
| Emir satırları | Emir gönderildikten sonra fire-and-forget (User WS `order` + REST `POST`/`DELETE`) | SQLite §8 (bkz. «Kalıcılık ve sonradan sorgu») |
| `market_resolved` | Market WS (`custom_feature_enabled: true`); bekleme **5+5+10**; yoksa **`not resolved`** | SQLite §9 (payload veya `not resolved`) |

---

## 14. Binance USD-M Futures aggTrade — harici sinyal katmanı

Bu bölüm, desteklenen kripto marketleri (`btc/eth/sol/xrp-updown-*`) için Binance Futures aggTrade akışından üretilen `binance_signal` metriğini tanımlar. Sinyal, emir öncesinde tüm stratejiler tarafından `signal_weight` parametresiyle ağırlıklı olarak tüketilir; Polymarket CLOB veya orderbook akışlarına dokunmaz.

### 14.1 WebSocket Bağlantısı

| Alan | Değer |
|---|---|
| Uç nokta | `wss://fstream.binance.com/ws/<symbol>@aggTrade` |
| Desteklenen semboller | `btcusdt`, `ethusdt`, `solusdt`, `xrpusdt` |
| Güncelleme hızı | gerçek zamanlı (100 ms agg. penceresi) |
| Protokol | Binance her 20 sn `PING` frame → bot `PONG` (WS protocol-level) |

**Slug → Binance sembol eşlemesi:**

| Market slug öneki | aggTrade sembolü |
|---|---|
| `btc-updown-*` | `btcusdt` |
| `eth-updown-*` | `ethusdt` |
| `sol-updown-*` | `solusdt` |
| `xrp-updown-*` | `xrpusdt` |

Sembol, Gamma `slug` alanından bot başlatıldığında bir kez çözümlenir ve ömür boyunca sabit kalır.

aggTrade payload (USD-M Futures):

```json
{
  "e": "aggTrade",
  "E": 1776500700123,
  "s": "BTCUSDT",
  "a": 5933014,
  "p": "84200.50",
  "q": "1.250",
  "f": 100,
  "l": 105,
  "T": 1776500700100,
  "m": false
}
```

Trade sınıflandırması:

| `m` alanı | Anlam | İşlem |
|---|---|---|
| `false` | Taker BUY (ask'e vurdu) | `buy_vol += q` |
| `true` | Taker SELL (bid'e vurdu) | `sell_vol += q` |

### 14.2 Türetilen Ham Metrikler

**CVD (Cumulative Volume Delta) — kayan pencere W:**

```
delta_i = if m==false { +q } else { -q }
CVD(t, W) = Σ_{t_i ∈ [t−W, t]} delta_i
```

Pencere `W` market aralığına göre otomatik seçilir:

| Market aralığı | CVD penceresi W |
|---|---|
| `5m` | 60 s |
| `15m` | 180 s |
| `1h` | 600 s |
| `4h` | 1800 s |

`CVD > 0` → net agresif alış baskısı; `CVD < 0` → net agresif satış baskısı.

**BSI (Buy-Sell Imbalance — Hawkes üstel bozunumu):**

```
BSI(t) = BSI(t−1) × e^(−κ × Δt_saniye) + delta_t
```

- `κ = 0.1` (yavaş bozunum); her market oturumu başında `BSI = 0`
- Kaynak: [tr8dr.github.io/BuySellImbalance](https://tr8dr.github.io/BuySellImbalance/)

**OFI (Order Flow Imbalance — sayı bazlı oran):**

```
OFI(t, W) = (N_buy − N_sell) / (N_buy + N_sell)    ∈ [−1, +1]
```

`N_buy`, `N_sell`: `[t−W, t]` içindeki agresif alış/satış trade sayısı.

### 14.3 Sinyal Skoru — 0–10 Ölçeği

Ham OFI, **rolling z-score** ile normalize edilip `[0, 10]` aralığına çevrilir:

```
z             = clamp((OFI_t − μ_OFI) / σ_OFI,  −3, +3)
signal_score  = round((z + 3) / 6 × 10,  1)       ∈ [0.0, 10.0]
```

- `μ_OFI`, `σ_OFI`: son `N = 300` OFI ölçümünün kayan istatistiği
- Warmup (`N < 300`): `signal_score = 5.0` (nötr)
- `5.0` = nötr; `> 5.0` = alış baskısı; `< 5.0` = satış baskısı

**Etkin skor (`signal_weight` uygulanmış):**

```
effective_score = 5.0 + (signal_score − 5.0) × (signal_weight / 10.0)
```

| `signal_weight` | Etki |
|---|---|
| `0` | `effective_score` her zaman `5.0`; sinyal devre dışı |
| `5` (varsayılan) | Yarım ağırlık |
| `10` | Tam ağırlık; `effective_score = signal_score` |

### 14.4 Strateji Bazlı Etki

| `effective_score` aralığı | Yorumlama | `dutch_book` | `harvest` | `prism` |
|---|---|---|---|---|
| `8–10` (güçlü alış) | Agresif UP baskısı | UP boyut `×1.5`; yön teyit zorunlu | Avg YES `×1.0` (zıt); Avg NO `×1.3` (teyit) | Giriş eşiği düşürülür (erken pozisyon) |
| `6–8` (hafif alış) | Hafif UP baskısı | UP boyut `×1.2` | Avg YES `×0.9`; Avg NO `×1.1` | Eşik normale yakın |
| `4–6` (nötr) | Baskı yok | `×1.0` (değişmez) | `×1.0` | Normal zamanlama |
| `2–4` (hafif satış) | Hafif DOWN baskısı | DOWN boyut `×1.2`; UP emirde `×0.8` | Avg YES `×1.1` (teyit); Avg NO `×0.9` | Giriş eşiği yükseltilir |
| `0–2` (güçlü satış) | Agresif DOWN baskısı | DOWN boyut `×1.5`; zıt yönde emir atlanır | Avg YES `×1.3` (teyit); Avg NO `×1.0` (zıt) | Giriş engellenir |

**`harvest` tablosu okuma kılavuzu:** Sinyal, averaging yapılan **tarafı teyit edip etmediğine** göre ölçekler. "Avg YES" = YES (UP) tarafı averaging; "Avg NO" = NO (DOWN) tarafı averaging. Güçlü alış sinyali, fiyatı yükselen YES tarafına averaging yapmayı teyit etmez (zıt); aksine düşen NO tarafı için averaging'i güçlendirir (teyit). Güçlü satış sinyali ise YES fiyatının düştüğünü gösterir, dolayısıyla YES averaging'i teyit eder. Yön filtresi yok; sinyal yalnızca **averaging GTC boyutunu** ölçekler.

**Strateji mekanizma özeti:**
- **`dutch_book`:** Sinyal hem pozisyon boyutunu hem yön doğrulamasını etkiler. `effective_score < 2` iken UP emir üretilmez.
- **`harvest`:** Sinyal yalnızca **averaging GTC boyutunu** ölçekler (teyit mantığı); yönü filtrelemez, her koşulda emir üretilir.
- **`prism`:** Sinyal zamanlama / giriş eşiğini etkiler. Güçlü satış → giriş ertelenir veya pencere içinde iptal edilir.

**`MarketZone` ile etkileşim (bkz. §15):** Sinyal hesabı (`CVD`, `BSI`, `OFI`, `signal_score`) her bölgede sürekli yapılır. Ancak her strateji, `ZoneSignalMap` aracılığıyla bazı bölgelerde sinyali **pasif** bırakabilir. Pasif bölgede `effective_score` zorla `5.0` (nötr) olarak uygulanır; pozisyon boyutu ve yön filtresi sinyal tarafından etkilenmez. Hangi bölgelerde sinyalin aktif olduğu her stratejinin bölge haritasında (`strategies.md §1–3`) tanımlanır. `StopTrade` bölgesinde sinyal durumundan bağımsız olarak yeni emir üretilmez (§15 zorunlu kuralı).

### 14.5 Rust Yapı Taslağı

```rust
/// Tek aggTrade olayı (Binance USD-M Futures şeması).
pub struct BinanceAggTrade {
    pub symbol:         String,
    pub price:          f64,
    pub qty:            f64,
    pub is_buyer_maker: bool,   // m alanı: false = taker BUY, true = taker SELL
    pub trade_time_ms:  u64,    // T alanı
}

/// Anlık sinyal durumu — Arc<RwLock<>> ile strateji katmanına açılır.
pub struct BinanceSignalState {
    pub cvd:           f64,   // kayan pencere CVD (base asset birimi)
    pub bsi:           f64,   // Hawkes BSI
    pub ofi:           f64,   // OFI ∈ [−1, +1]
    pub signal_score:  f64,   // normalize 0–10 (warmup: 5.0)
    pub updated_at_ms: u64,
    pub warmup:        bool,  // N < 300 → nötr skor kullan
}
```

**Görev mimarisi:**
- `binance_aggtrade_task`: `tokio-tungstenite` → `wss://fstream.binance.com`; PING/PONG işler; `mpsc::Sender<BinanceAggTrade>`
- `signal_processor_task`: `mpsc::Receiver` → rolling `VecDeque<(u64, f64)>`; `Arc<RwLock<BinanceSignalState>>` günceller
- Strateji katmanı: emir kararı üretmeden önce `signal_state.read()` → `effective_score` → pozisyon çarpanı

**Yeniden bağlanma:** Binance WS düşerse üstel backoff `1s → 2s → 4s → … → max 60s`; bağlantı kopuk süre boyunca `signal_score = 5.0` (nötr), emir üretimi durmaz.

**Bağımsızlık:** Binance aggTrade akışı tamamen bağımsız bir görevde çalışır; Polymarket CLOB, Market WS, User WS akışlarına ve heartbeat döngüsüne dokunmaz.

---

## 15. Market Bölge Sistemi (`MarketZone`)

Market penceresi `[startDate, endDate]` boyunca bot, anlık ilerleme yüzdesine göre 5 farklı **bölgede** çalışır. Her bölgenin emir davranışı ve `binance_signal` aktifliği strateji bazında ayrıca tanımlanır.

### 15.1 Bölge Tanımları

**İlerleme yüzdesi hesabı:**

```
zone_pct = (now − startDate) / (endDate − startDate) × 100.0
```

| Bölge | `zone_pct` aralığı | Özellik |
|---|---|---|
| `DeepTrade` | 0 – 10 % | Pencerenin en erken fazı; referans fiyat henüz oturmamış, spread geniş |
| `NormalTrade` | 10 – 50 % | Ana işlem penceresi; orderbook derinleşmiş, fiyat yavaş yavaş belirginleşir |
| `AggTrade` | 50 – 90 % | İkinci yarı; momentum netleşir, fiyat hareketi hızlanabilir |
| `FakTrade` | 90 – 97 % | Kapanış öncesi; likidite azalır, spread genişler, yanıltıcı hareketler artabilir |
| `StopTrade` | 97 – 100 % | Kapanışa yakın; **yeni emir üretimi durdurulur**, açık emirler yönetilir |

**Terminoloji notu:** `AggTrade` bölge adı, §14'teki Binance `aggTrade` WebSocket akışıyla aynı terim değildir. Bağlamdan ayırt edilmeli; doküman içinde `MarketZone::AggTrade` şeklinde tam nitelikli kullanım tercih edilir.

### 15.2 Rust Enum Taslağı

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketZone {
    DeepTrade,   //  0–10 %
    NormalTrade, // 10–50 %
    AggTrade,    // 50–90 %
    FakTrade,    // 90–97 %
    StopTrade,   // 97–100 %
}

impl MarketZone {
    /// `zone_pct` = (now − startDate) / (endDate − startDate) × 100.0
    /// Sınır değerler alt bölgeye dahildir (örn. 10.0 → NormalTrade).
    pub fn from_pct(zone_pct: f64) -> Self {
        match zone_pct {
            p if p < 10.0 => Self::DeepTrade,
            p if p < 50.0 => Self::NormalTrade,
            p if p < 90.0 => Self::AggTrade,
            p if p < 97.0 => Self::FakTrade,
            _              => Self::StopTrade,
        }
    }

    /// Bölge indeksi — ZoneSignalMap dizisinde konum olarak kullanılır.
    pub const fn index(self) -> usize {
        self as usize
    }
}
```

### 15.3 Per-Strateji Bölge × Sinyal Haritası (`ZoneSignalMap`)

Her strateji, her bölgede `binance_signal`'ın aktif mi pasif mi olduğunu `ZoneSignalMap` ile tanımlar.

```rust
/// Beş bölge için binance_signal aktifliği.
/// Sıra: [DeepTrade, NormalTrade, AggTrade, FakTrade, StopTrade]
pub struct ZoneSignalMap(pub [bool; 5]);

impl ZoneSignalMap {
    /// Verilen bölgede sinyal aktif mi?
    pub fn signal_active(&self, zone: MarketZone) -> bool {
        self.0[zone.index()]
    }
}

impl Strategy {
    /// Her strateji kendi zone → sinyal haritasını döndürür.
    /// Değerler strategies.md'deki bölge tablolarından gelir.
    pub fn zone_signal_map(self) -> ZoneSignalMap {
        match self {
            // Tam değerler strategies.md bölge tablosu doldurulunca buraya yazılır.
            Self::DutchBook | Self::Harvest | Self::Prism => ZoneSignalMap([false; 5]),
        }
    }
}
```

**Kullanım akışı (strateji motoru, her emir kararı öncesinde):**

```
zone     = MarketZone::from_pct(zone_pct)
sig_ok   = strategy.zone_signal_map().signal_active(zone)

effective_score = if sig_ok {
    // §14.3 formülü: 5.0 + (signal_score − 5.0) × (signal_weight / 10.0)
    5.0 + (signal_score - 5.0) * (signal_weight as f64 / 10.0)
} else {
    5.0  // nötr — sinyal bu bölgede pasif
}
```

`StopTrade` bölgesinde (`zone_pct ≥ 97 %`) sinyal değerinden bağımsız olarak **yeni emir üretilmez**; yalnızca açık emir yönetimi ve heartbeat döngüsü sürer. Bu kural `zone_signal_map` dışında, strateji motorunun üst katmanında zorunlu olarak uygulanır.

---

## 16. Çalışma Modu — `RunMode` (`live` / `dryrun`)

Her bot, **`live`** veya **`dryrun`** modunda başlatılır. Mod, frontend'deki bot oluşturma formunda seçilir ve `BotConfig::run_mode` alanında saklanır. **Mod, bot çalışırken değiştirilemez.**

### Mod Karşılaştırması

| Bileşen | `live` | `dryrun` |
|---|---|---|
| Market WS (orderbook, fiyat) | Bağlanır — gerçek veri | Bağlanır — gerçek veri (aynı) |
| User WS (order/trade event) | Bağlanır — gerçek emir olayları | **Bağlanmaz** |
| REST heartbeat | Gönderilir | **Gönderilmez** |
| `POST /order`, `POST /orders` | CLOB API'ye gönderilir | **Gönderilmez** — motor içinde anında fill simüle edilir |
| `DELETE /orders` | CLOB API'ye gönderilir | **Gönderilmez** — anında başarılı yanıt simüle edilir |
| CLOB auth header | Her istekte eklenir | **Eklenmez** |
| DB / log yazımı | `run_mode = live` ile | `run_mode = dryrun` ile (aynı şema) |

### Dryrun Simülasyon Kuralları

**GTC emir (OpenDual / Averaging):**
- `POST /order` veya `POST /orders` çağrısı engellenir.
- Bot, emir gönderildiği anda **anında fill** olmuş gibi davranır:
  - Fill fiyatı = emrin kendi bid fiyatı (`up_bid`, `down_bid`, `first_best_leg`)
  - Fill boyutu = hesaplanan `effective_size` (tam fill)
  - Simüle edilen order ID = `dryrun-<uuid-v4>`
- `avg_*`, `imbalance`, `imbalance_cost` ve diğer metrikler gerçek fill ile aynı şekilde güncellenir.
- User WS `trade` olayı yoktur; güncelleme doğrudan motor içinde tetiklenir.

**FAK emir (ProfitLock):**
- CLOB API'ye gönderilmez.
- Her zaman **tam fill** simüle edilir: `filled = imbalance`, `price = hedge_leg`.
- Kısmi fill senaryosu dryrun'da test edilmez.

**GTC iptal (`DELETE /orders`):**
- API çağrısı yapılmaz; bot iptal başarılı kabul eder ve lokal emir listesinden kaldırır.

### Rust Yapısı

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RunMode {
    /// Gerçek CLOB API — auth gerekli, emirler iletilir.
    Live,
    /// Simülasyon — Market WS gerçek, emirler lokal fill ile simüle edilir.
    DryRun,
}

impl RunMode {
    /// Live modda CLOB auth header eklenir ve emirler iletilir.
    pub fn is_live(self) -> bool { matches!(self, Self::Live) }
    /// Dryrun modda emir çağrısı engellenir; anında simüle fill üretilir.
    pub fn is_dry(self) -> bool { matches!(self, Self::DryRun) }
}
```

`BotConfig::run_mode: RunMode` — bot başlatılırken ayarlanır, runtime'da değiştirilemez.

### Loglama ve DB

- Her log satırında `"run_mode":"live"` veya `"run_mode":"dryrun"` alanı bulunur.
- SQLite tablolarında `run_mode TEXT NOT NULL` sütunu eklenir; tüm satırlara yazılır.
- Dryrun kayıtları live kayıtlardan ayrı sorgulanabilir (`WHERE run_mode = 'dryrun'`).
- Simüle fill kayıtlarında `order_id` `dryrun-` önekiyle başlar.

### Kısıtlamalar

- Dryrun'da **User WS bağlantısı yoktur** — gerçek emir/trade olayları alınamaz. Tüm durum güncellemesi motor içi simülasyondan gelir.
- Dryrun'da **REST heartbeat yoktur** — heartbeat zaman aşımı geçerliliği test edilemez.
- Dryrun sonuçları (kâr/zarar simülasyonu) gerçek piyasa likiditesini yansıtmaz; fill her zaman tam ve anlıktır.

---

## 17. Anlık PnL Hesabı (Per-Market, Per-Bot)

### Veri Kaynağı

Polymarket'te resmi bir "PnL endpoint" yoktur. Tüm hesap **User WS `trade`** olaylarında `status=MATCHED` olduğunda gelen `size`, `price`, `fee_rate_bps` alanlarından yapılır; market fiyatı ise Market WS **`best_bid_ask`** event'inden okunur. **`GET /trades` kullanılmaz** (bkz. §5.5 "Kullanılmaz").

**Settlement kuralı (binary market):** Market çözümlendiğinde kazanan outcome'un her share'i `$1.00 USDC`, kaybeden outcome'un share'i `$0.00` öder ([UMA Optimistic Oracle](https://docs.polymarket.com/concepts/resolution)). Bu, `pnl_if_up` / `pnl_if_down` hesabının temelidir.

### Temel Değişkenler

Her MATCHED fill event'inde aşağıdaki değişkenler biriktirilir:

| Değişken | Formül | Kaynak |
|---|---|---|
| `cost_basis` | `Σ(size_i × price_i + fee_i)` — tüm BUY filllerinin toplam USDC maliyeti | User WS `trade` `MATCHED` |
| `fee_total` | `Σ(size_i × price_i × fee_rate_bps_i / 10_000)` | `fee_rate_bps` alanı (her trade'de değişebilir) |
| `shares_YES` | `Σ size` (YES BUY) − `Σ size` (YES SELL) — net YES pozisyonu | `outcome` + `side` alanları |
| `shares_NO` | `Σ size` (NO BUY) − `Σ size` (NO SELL) — net NO pozisyonu | `outcome` + `side` alanları |

> **Strateji notu:** `harvest` dahil mevcut stratejiler yalnızca **BUY** emri gönderir (SELL olmaz); pozisyon market çözümüyle kapatılır. SELL muhasebesi ileride SELL emri olan stratejiler için hazır olacak şekilde tasarlanmıştır.

### Üç PnL Metriği

```
pnl_if_up   = shares_YES × 1.0 − cost_basis   // UP (YES) kazanırsa gerçekleşecek USDC kâr/zarar
pnl_if_down = shares_NO  × 1.0 − cost_basis   // DOWN (NO) kazanırsa gerçekleşecek USDC kâr/zarar
mtm_pnl     = shares_YES × best_bid_YES
            + shares_NO  × best_bid_NO
            − cost_basis                       // Anlık mark-to-market (bugün satılabilseydi)
```

| Metrik | Anlamı | Güncelleme tetikleri |
|---|---|---|
| `pnl_if_up` | YES kazanırsa hesaba girecek net USDC | Her MATCHED fill |
| `pnl_if_down` | NO kazanırsa hesaba girecek net USDC | Her MATCHED fill |
| `mtm_pnl` | Mevcut bid fiyatlarıyla teorik anlık değer | Her MATCHED fill + her `best_bid_ask` event |

**`harvest` için not:** Delta-nötr strateji ideal durumda `shares_YES ≈ shares_NO ≈ pair_count` sağlar; bu durumda `pnl_if_up ≈ pnl_if_down ≈ (1 − AVG_SUM) × pair_count` — ProfitLock'taki kâr formülüyle örtüşür.

**Ücret notu:** Maker (GTC) ücreti Polymarket'te genellikle 0 bps; Taker (FAK) ücreti tipik olarak ~100–200 bps. `fee_rate_bps` değeri her trade payload'ında gelir ve `cost_basis`'e dahil edilir.

### Rust Yapı Taslağı

```rust
/// Bir bot'un tek market oturumu boyunca anlık PnL durumu.
/// Her MATCHED trade ve best_bid_ask event'inde güncellenir.
#[derive(Debug, Clone, Default)]
pub struct MarketPnL {
    /// Toplam harcanan USDC (tüm BUY filllerinin notional + fee toplamı).
    pub cost_basis:   f64,
    /// Ödenen toplam ücret (USDC).
    pub fee_total:    f64,
    /// Net YES share pozisyonu.
    pub shares_yes:   f64,
    /// Net NO share pozisyonu.
    pub shares_no:    f64,
    /// YES (UP) kazanırsa gerçekleşecek PnL (USDC).
    pub pnl_if_up:    f64,
    /// NO (DOWN) kazanırsa gerçekleşecek PnL (USDC).
    pub pnl_if_down:  f64,
    /// Anlık mark-to-market PnL — best_bid fiyatlarıyla teorik değer (USDC).
    pub mtm_pnl:      f64,
}

impl MarketPnL {
    /// User WS `trade` `status=MATCHED` event'inde çağrılır.
    /// `outcome`: "YES" | "NO"  — ham payload değeri (trim + uppercase ile karşılaştırılır)
    /// `side`:    "BUY" | "SELL"
    pub fn on_trade_matched(
        &mut self,
        size:          f64,
        price:         f64,
        fee_rate_bps:  f64,
        outcome:       &str,
        side:          &str,
    ) {
        let notional = size * price;
        let fee      = notional * fee_rate_bps / 10_000.0;
        let sign     = if side == "BUY" { 1.0 } else { -1.0 };

        if side == "BUY" {
            self.cost_basis += notional + fee;
            self.fee_total  += fee;
        } else {
            // SELL: pozisyon azalır, maliyet tabanı satılan payın maliyeti kadar düşer
            self.cost_basis -= notional - fee;
        }

        match outcome.trim().to_uppercase().as_str() {
            "YES" => self.shares_yes += sign * size,
            "NO"  => self.shares_no  += sign * size,
            _     => {}
        }

        self.pnl_if_up   = self.shares_yes - self.cost_basis;
        self.pnl_if_down = self.shares_no  - self.cost_basis;
    }

    /// Market WS `best_bid_ask` event'inde çağrılır (mtm_pnl güncellenir).
    pub fn on_best_bid_ask(&mut self, best_bid_yes: f64, best_bid_no: f64) {
        self.mtm_pnl =
            self.shares_yes * best_bid_yes
            + self.shares_no * best_bid_no
            - self.cost_basis;
    }
}
```

### Frontend API Uç Noktası

`GET /api/bots/{id}/markets/{session_id}/pnl` — supervisor, bot'un in-memory `MarketPnL` durumunu SQLite'taki son fill ozeti ile birleştirerek döndürür:

```json
{
  "market_session_id": "...",
  "slug": "btc-updown-5m-1776500700",
  "cost_basis":   4.72,
  "fee_total":    0.01,
  "shares_yes":   10.0,
  "shares_no":    10.0,
  "pnl_if_up":    5.28,
  "pnl_if_down":  5.28,
  "mtm_pnl":      3.10,
  "run_mode":     "live"
}
```

**Güncelleme kanalı (§⚡ Kural 5 uyumu):**
- **MATCHED fill** (yani `pnl_if_up` / `pnl_if_down` değişimi) → SSE kanalı üzerinden frontend'e **anında push** edilir; polling beklenmez.
- **`mtm_pnl`** (`best_bid_ask` event'lerinde sık güncellenir) → frontend §2'deki **1 sn polling** ile okur; her WS event'inde push gerekmez.

`run_mode = "dryrun"` ise tüm değerler simüle fill'lerden hesaplanır (gerçek likiditeyi yansıtmaz).

### SQLite Kalıcılığı

`MarketPnL` alanları market oturumu tablosunda ayrı sütunlar olarak tutulabilir **veya** her MATCHED trade satırından türetilebilir (her iki yaklaşım da desteklenir; öneri: in-memory + periyodik snapshot). Market çözümü (`market_resolved`) geldiğinde nihai `pnl_if_up` ya da `pnl_if_down` (kazanan outcome'a göre) SQLite'a `realized_pnl` olarak yazılır.

---

## 18. Runtime Konfigürasyonu

Sistem davranışını değiştiren tüm ayar değerleri **çevre değişkenleri** üzerinden verilir; `.env` dosyası lokal geliştirme için fallback sağlar. Kod içinde hardcoded path, port veya URL yoktur.

### 18.1 Çevre Değişkenleri

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `PORT` | `3000` | Supervisor Axum HTTP sunucusunun dinlediği port |
| `RUST_LOG` | `info` | `tracing-subscriber` env-filter (ör. `info,baiter=debug`) |
| `DB_PATH` | `./data/baiter.db` | SQLite dosya yolu; dizin yoksa start-up'ta oluşturulur (WAL mode: `PRAGMA journal_mode=WAL`) |
| `BOT_BINARY` | Debug: `./target/debug/bot`; Release: `./target/release/bot` | Supervisor'un spawn edeceği bot binary yolu |
| `HEARTBEAT_DIR` | `./data/heartbeat` | `<bot_id>.heartbeat` dosyalarının dizini; dizin yoksa oluşturulur |
| `GAMMA_BASE_URL` | `https://gamma-api.polymarket.com` | Market keşif REST base URL |
| `CLOB_BASE_URL` | `https://clob.polymarket.com` | CLOB REST base URL; staging için `https://clob-staging.polymarket.com` |
| `POLYGON_CHAIN_ID` | `137` | EIP-712 domain chain ID (Polygon mainnet) |
| `POLY_ADDRESS` | — | Fallback L1 address (bot-spesifik yoksa) |
| `POLY_API_KEY` | — | Fallback L2 API key UUID |
| `POLY_PASSPHRASE` | — | Fallback L2 passphrase |
| `POLY_SECRET` | — | Fallback L2 secret (HMAC key) |
| `POLYGON_PRIVATE_KEY` | — | Fallback L1 signing private key |

**Kimlik çözümleme önceliği:** Bot başlatılırken önce SQLite `bot_credentials` tablosundan bot-spesifik kayıt okunur (§9a); yoksa yukarıdaki `POLY_*` / `POLYGON_PRIVATE_KEY` fallback değerleri kullanılır. Her iki kaynak da boşsa bot başlatılamaz (`AppError::MissingCredentials`).

**Runtime path override:** Docker, systemd veya farklı deployment senaryolarında `DB_PATH`, `BOT_BINARY`, `HEARTBEAT_DIR` override edilebilir; kod bu path'leri varsayılan yerine environment'tan okur.

### 18.2 Graceful Shutdown (SIGTERM)

Supervisor botu durdururken önce **SIGTERM** gönderir; ardından 10 sn timeout sonrası SIGKILL ile sonlandırır. Bot SIGTERM handler'ı aşağıdaki sırayı uygular:

1. **WS bağlantılarını kapat** — Market WS ve User WS subscribe task'larına shutdown sinyali; `ws.close()` çağrısı.
2. **Açık emirleri temizle** — `DELETE /orders` ile tüm açık GTC order'ları iptal et (§1 "temiz durdurma" kuralı). GTD ve FOK/FAK emirleri zaten kısa ömürlüdür; iptal gerekmez.
3. **REST heartbeat döngüsünü durdur** — CLOB heartbeat task'ını (§4.1) sonlandır; son heartbeat'i gönderip çık.
4. **Stdout flush + exit 0** — Tüm pending `[[EVENT]]` satırlarını flush et; exit code 0 ile çık (supervisor crash loop sayacına eklemez; kullanıcı kaynaklı stop olarak işaretlenir).

Exit code 0 olmayan çıkışlar (panic, `AppError`) supervisor tarafından crash olarak değerlendirilir ve exponential backoff ile yeniden başlatılır.

**Rust taslağı:**

```rust
#[tokio::main]
async fn main() -> Result<(), AppError> {
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())?;

    tokio::select! {
        result = run_bot() => result,
        _ = sigterm.recv() => {
            tracing::info!("SIGTERM received, graceful shutdown");
            shutdown().await?;
            Ok(())
        }
    }
}

async fn shutdown() -> Result<(), AppError> {
    // 1. WS aboneliklerini kapat
    WS_SHUTDOWN.store(true, Ordering::SeqCst);

    // 2. Açık GTC emirlerini temizle
    clob::delete_all_orders(&http_client, &auth).await?;

    // 3. Heartbeat döngüsü shutdown (drop Arc<AtomicBool>)
    HEARTBEAT_SHUTDOWN.store(true, Ordering::SeqCst);

    // 4. Stdout flush
    std::io::Write::flush(&mut std::io::stdout())?;
    Ok(())
}
```

---

## 13. İlgili dokümanlar

**Resmi (Polymarket):**

- [Market Channel — WebSocket](https://docs.polymarket.com/market-data/websocket/market-channel) — `custom_feature_enabled`, `market_resolved`
- [WebSocket overview](https://docs.polymarket.com/market-data/websocket/overview) — kanallar, `PING` / `PONG`
- [User channel](https://docs.polymarket.com/market-data/websocket/user-channel) — `order`, `trade`, trade yaşam döngüsü (`MATCHED` → …)
- [Orders overview](https://docs.polymarket.com/trading/orders/overview) — emir tipleri, tick size, **REST heartbeat** (emir güvenliği), insert `status`, allowances, hata kodları
- [Fetching markets (Gamma)](https://docs.polymarket.com/market-data/fetching-markets) — slug / etiket / etkin market keşfi (`gamma-api.polymarket.com`)
- [Resolution (concepts)](https://docs.polymarket.com/concepts/resolution) — UMA oracle süreci

**Repo içi:**

- [polymarket-clob.md](api/polymarket-clob.md) — REST, WS örnekleri, trade alanları
- [polymarket-gamma.md](api/polymarket-gamma.md) — Event/market keşfi, `startDate` / `endDate`, slug
- [rust-polymarket-kutuphaneler.md](rust-polymarket-kutuphaneler.md) — Rust bağımlılık önerileri

---

*Şema, HTTP rotaları ve strateji parametreleri implementasyon sırasında netleştirilir. Polymarket API’si için güncel davranış her zaman [docs.polymarket.com](https://docs.polymarket.com/) ile doğrulanmalıdır.*
