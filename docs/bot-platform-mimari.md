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
- **Maksimum performans** için her bot **ayrı işlem (ayrı PID)** olarak çalışabilir; denetleyici süreç botları başlatır, durdurur ve sağlığını izler.

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
| `avgsum`: `avg_up`, `avg_down`, `AVG SUM` | ✓ | — | ✓ |
| `profit`: türetilen **profit %** (`AVG SUM` üzerinden) | ✓ | — | ✓ |
| Brüt hacim: `sum_up`, `sum_down` | ✓ | ✓ | ✓ |
| `POSITION Δ` (pay `imbalance` + brüt hacim oranı ve yön) | ✓ | ✓ | — |

**Not:** `prism` ne pay `imbalance` ne maliyet `imbalance_cost` hattını kullanmaz; `harvest` **`avgsum`** ve **`profit`** hatlarını kullanmaz. Yeni strateji eklenirken `MetricMask` yalnızca aşağıdaki **geçerli** `(avgsum, profit)` çiftleriyle genişletilir: `(false,false)`, `(true,false)`, `(true,true)` — **`(false,true)` yasaktır** (`profit` açıkken `avgsum` kapalı olamaz). Matris **yapılandırma veya sabit enum** (`MetricSubscription` benzeri) ile genişletilir; metrik tanımı katalogda kalır.

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
}

impl Strategy {
    pub const fn required_metrics(self) -> MetricMask {
        match self {
            Self::DutchBook => MetricMask {
                imbalance: true,
                imbalance_cost: true,
                avgsum: true,
                profit: true,
                sum_volume: true,
            },
            Self::Harvest => MetricMask {
                imbalance: true,
                imbalance_cost: true,
                avgsum: false,
                profit: false,
                sum_volume: true,
            },
            Self::Prism => MetricMask {
                imbalance: false,
                imbalance_cost: false,
                avgsum: true,
                profit: true,
                sum_volume: true,
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
    /// `profit == true` ⇒ `avgsum == true` olmalı; aksi halde maske geçersiz.
    pub const fn is_valid(self) -> bool {
        !self.profit || self.avgsum
    }
}
```

**Maske tutarlılığı:** `profit` üretilecekse **`avgsum`** da açık olmalıdır (`AVG SUM` girdisi olmadan `profit %` tanımsız); `avgsum: false` iken `profit: true` **ürün hatası** sayılır (`MetricMask::is_valid` false). `avgsum: true`, `profit: false` geçerlidir (ör. yalnız VWAP / `AVG SUM` gösterimi).

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

`sum_up + sum_down` sıfırsa yüzde tanımsızdır. Referans trade kümesi (tüm oturum / son N işlem / pencere) UI ve API’de **aynı** seçilmelidir; **ortak katalog** metrikleri bu kümeden türetilir — hangi stratejinin hangi alt kümeyi okuduğu matristeki **Strateji → metrik** satırlarıyla uyumlu olmalıdır (`harvest` için `avgsum` / `profit` yok; `prism` için pay `imbalance`, `imbalance_cost` ve `POSITION Δ` yok).

---

## 2. Frontend API yüzeyi (read-only + komutlar)

Aşağıdakiler tipik bir ayrımdır; uç yollar implementasyonda netleşir.

- **Read-only:** bot listesi, bot detayı, ayar özeti, **akış logları** (metin veya sayfalı), **bota ait slug/market listesi**, **slug altında market bazlı loglar**.
- **Yazma (komut):** yeni bot, ayar güncelleme, **başlat**, **durdur**, **sil** — bunlar API’de iş kuyruğu veya durum bayrağı ile yürütülür; frontend yalnızca isteği tetikler.

**Canlı monitoring:** API, CLOB/WS üzerinden gelen **fiyat, emir, trade** bilgisini işledikten sonra hem **log kanalına** yazar hem de (aşağıdaki gibi) **veritabanına** düşer.

**Özet ekran yenileme:** Frontend, pano ve özet metrikleri (pozisyon, son işlemler, bot durumu) **yaklaşık 1 saniye aralıkla** API’den okuyarak günceller (kısa aralıklı **polling**). Ham WebSocket akışı API’de işlenir; istemci tarafında WS zorunlu değildir.

---

## 3. Market seçimi: “şu anki” ve “sonraki”

Kullanıcı örneğin **“BTC 15dk”** (veya 5dk vb.) gibi tekrarlayan bir **pencere** seçer.

| Seçenek | Davranış |
|---------|-----------|
| **Güncel (aktif) market** | Bot, **şu anda devam eden** pencereye ait marketten başlar (Gamma/slug ile çözülen güncel `market` / `clobTokenIds`). |
| **Sonraki market** | Bot, **sıradaki** markete odaklanır; o marketin **Gamma** kaydındaki **startDate** ve **endDate** pencere sınırları olarak kullanılır (geçerli zaman dilimine düşen market satırı). |

“Şu anki güncel market” ile “bir sonraki periyot marketi” ayrımı böyle netleşir.

---

## 4. Zaman çizelgesi: T−15 ve bağlantılar

**Aktif güncel market** veya **sonraki market** modu için:

- Hedef marketin **başlangıcından 15 saniye önce (T−15)** ilgili market için **ön veri hazırlanır** (Gamma’dan çözümlenmiş slug/market, `clobTokenIds`, **startDate / endDate**).
- **T−15** anında **CLOB REST** (kimlik, orderbook, hazırlık) ve **Market WebSocket** (+ gerekiyorsa **User WebSocket**) bu market için **hazır** olur; abonelikler ilgili `asset_id` listesiyle kurulur.
- Pencere boyunca seçilen **strateji** (`dutch_book` / `harvest` / `prism`) döngüsü çalışır; pencere sonuna doğru (ör. `stop_before_end_ms`) erken durdurma kuralları uygulanabilir.
- Pencere **endDate** (veya strateji kuralı) ile uyumlu şekilde sonlandırılır; log: market tamamlandı, sıradaki hedefe geçiş.

**Sonraki market** seçildiğinde: sıradaki marketin **startDate** / **endDate** değerleri tek kaynak olarak kullanılır; T−15 bu **startDate**’e göre hesaplanır.

### WebSocket işletimi (Market + User)

Resmi [WebSocket overview](https://docs.polymarket.com/market-data/websocket/overview): **Market** ve **User** kanallarında istemci yaklaşık **10 saniyede bir** düz metin **`PING`** gönderir; sunucu **`PONG`** döner — bağlantı kopmasını önlemek için zorunlu kabul edilir.

**Yeniden bağlanma:** Oturum koptuğunda veya bot market değiştirdiğinde **abonelik mesajı yeniden** gönderilir (`type`, `assets_ids` / `markets`, User için `auth`, Market için `custom_feature_enabled: true`); aksi halde olay akışı gelmez.

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
- Metinler **stdout**, **dosya** ve API üzerinden **SSE/WebSocket log akışı** için aynı formatta üretilebilir.

### 5.2 Örnek: tek market penceresi (tam metin)

Aşağıdaki blok **örnek veridir**; zaman damgaları ve id’ler hayalidir.

```
[10:19:55.725] [btc] Target market: btc-updown-5m-1776420900
[10:19:55.725] [btc] Window: 2026-04-17 10:15:00 UTC - 2026-04-17 10:20:00 UTC
[10:19:55.725] [btc] 📡 Fetching market: btc-updown-5m-1776420900
[10:19:55.763] [btc]    ✅ Found market: Bitcoin Up or Down - April 17, 6:15AM-6:20AM ET
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
[10:19:56.553] [btc]  📦 POST /order (249ms)
[10:19:56.553] [btc]  ✅ orderType=GTC side=BUY outcome=YES | POST status=live orderID=0x8e9a3174e6429c...
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
[10:19:56.642] [btc] ⏰ Only 4s until window end, stopping early (stop_before_end_ms=30000)
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
{"ts":"2026-04-17T10:19:56.553Z","bot":"btc","source":"user_ws","event_type":"order","type":"UPDATE","id":"0xff35...","size_matched":"50000000","associate_trades":["28c4d2eb-bbea-40e7-a9f0-b2fdb56b2c2e"]}
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
```

---

## 6. SQLite — WebSocket orderbook anlık görüntüsü

**Market** kanalından gelen `book` (veya eşdeğer) olayları API’de işlendikten sonra, **her kayıt** için en az şu alanlar saklanabilir:

| Alan | Anlamı |
|------|--------|
| `asset_id` | Outcome token kimliği |
| `market` | Market (condition) kimliği |
| `bids` / `asks` | Derinlik yapısı (JSON veya normalize tablo) |
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

**Her `trade` olayı:** İşlendiğinde **(1)** metin loga **en az bir** trade satırı, **(2)** SQLite’ta **`trade.id`** anahtarıyla kayıt: **ilk** mesaj (genelde `MATCHED`) **ekleme**, **aynı `id` ile sonraki** mesajlar (`MINED`, `CONFIRMED`, …) **aynı satırı günceller** — çift satır yok, durum ilerlemesi tek kayıtta tutulur. Strateji özeti (pozisyon, `imbalance`, vb.) **aynı `trade.id` için `MATCHED` ile bir kez** güncellenir; sonraki status’lar yalnız trade kaydındaki alanları günceller (bkz. üst bölüm **«imbalance»**).

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
| `trader_side` | Örn. `TAKER` — kullanıcı **taker** mı **maker** mı anlamı için |
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
| Metin log | Sürekli | Log dosyası / API stream |
| WS orderbook özeti | Olay geldikçe | SQLite (işlenmiş `asset_id`, `market`, `bids`/`asks`, `timestamp`) |
| Sonraki market planı | Pencere öncesi | SQLite (ön kayıt) |
| Pencere başlangıcı | T=0 | Aynı satırın güncellenmesi |
| Trade satırları | Her User WS `trade` mesajı (aynı `id`’de güncelleme); log + SQLite upsert | SQLite §10 + log (`GET /trades` yok) |
| Emir satırları | User WS `order` + REST `POST`/`DELETE` emir yanıtları işlendiğinde | SQLite §8 (bkz. «Kalıcılık ve sonradan sorgu») |
| `market_resolved` | Market WS (`custom_feature_enabled: true`); bekleme **5+5+10**; yoksa **`not resolved`** | SQLite §9 (payload veya `not resolved`) |

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
