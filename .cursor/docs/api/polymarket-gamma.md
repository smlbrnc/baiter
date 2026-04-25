# Polymarket GAMMA API — Tam Referans

> Kaynak: [docs.polymarket.com/api-reference](https://docs.polymarket.com/api-reference)
> Tarih: Nisan 2026 itibarıyla güncel resmi dokümantasyona göre derlenmiştir.
> Hedef: Rust ile Polymarket entegrasyonu yapacak geliştiriciler için referans.

---

## İçindekiler

1. [Genel Bakış](#genel-bakış)
2. [Base URL](#base-url)
3. [Authentication](#authentication)
4. [Veri Modeli](#veri-modeli)
5. [Pagination & Filtreleme](#pagination--filtreleme)
6. [Rate Limits](#rate-limits)
7. **Endpoints**
   - [Status](#status)
   - [Events](#events)
   - [Markets](#markets)
   - [Tags](#tags)
   - [Series](#series)
   - [Comments](#comments)
   - [Sports](#sports)
   - [Search](#search)
   - [Public Profile](#public-profile)
8. [Rust Entegrasyon İpuçları](#rust-entegrasyon-ipuçları)

---

## Genel Bakış

**Gamma API**, Polymarket'in market metadata, event grupları, etiketler (tags), seriler, yorumlar, sporlar, arama ve public profil verilerini sunan **read-only REST servisidir**. Tüm market verileri zincir üstünde (Polygon + UMA Optimistic Oracle) bulunsa da Gamma bu veriyi indeksler ve ek metadata (kategorizasyon, indekslenmiş hacim, görseller, vs.) sağlar.

Gamma'nın temel kullanım amaçları:
- Aktif marketleri ve eventleri keşfetmek
- Kategori/etiket bazlı filtreleme
- Market metadata (görseller, açıklamalar, hacim, likidite, fiyat değişimleri)
- Outcome ve `clobTokenIds` bilgisini almak (CLOB ile trade için zorunlu)
- Profil ve yorum verisi

---

## Base URL

```
https://gamma-api.polymarket.com
```

> **Not:** Gamma'nın staging environment'ı yoktur. Production endpointleri public ve auth gerektirmez.

---

## Authentication

Gamma API tamamen **public**'tir — API key, JWT, signature gibi hiçbir authentication gerektirmez. Tüm endpointler doğrudan HTTP GET ile çağrılır.

> **Önemli:** Trading işlemleri için CLOB API kullanılmalı (auth gereklidir). Gamma sadece veri okuma içindir.

---

## Veri Modeli

Polymarket veriyi iki ana hiyerarşi etrafında organize eder:

### Market (en temel birim)
Tradable binary outcome'tır (Yes/No). Şu alanları içerir:
- `conditionId` — UMA condition ID (on-chain)
- `clobTokenIds` — Yes ve No outcome'larının token ID'leri (CLOB'da trade için kullanılır)
- `outcomes` — Outcome adları (`["Yes", "No"]`)
- `outcomePrices` — Outcome'ların güncel fiyatları (`["0.65", "0.35"]`)
- `enableOrderBook` — CLOB'da trade edilebilir mi?

### Event (1+ market'i gruplayan container)
- **Single Market Pair (SMP):** 1 market içerir (örn. "Will Bitcoin reach $100k by 2025?")
- **Grouped Market Pair (GMP):** 2+ market içerir (örn. "Who will win the election?" → her aday için ayrı market)
- Negative Risk eventler: tüm marketler birlikte tam olasılık uzayını kapsar (`enableNegRisk: true`)

### Series
Event'leri tekrarlanan bir tema altında gruplayan üst yapı (örn. "Fed Rate Decisions", "NBA Finals").

### Outcomes ve Prices
```json
{
  "outcomes": "[\"Yes\", \"No\"]",
  "outcomePrices": "[\"0.20\", \"0.80\"]"
}
// Index 0: "Yes" → 0.20 (20% olasılık)
// Index 1: "No"  → 0.80 (80% olasılık)
```

> ⚠️ `outcomes` ve `outcomePrices` **string olarak JSON encoded array** döner — Rust'ta `serde_json::from_str` ile tekrar parse etmek gerekir.

### İlişkiler
```
Series ─< Events ─< Markets
         │
         └── Tags, Categories, Collections
```

---

## Pagination & Filtreleme

Çoğu list endpoint'i `limit` + `offset` tabanlı paginatedir:

```bash
# Sayfa 1: ilk 50 sonuç
curl "https://gamma-api.polymarket.com/events?limit=50&offset=0&closed=false"

# Sayfa 2: sonraki 50
curl "https://gamma-api.polymarket.com/events?limit=50&offset=50&closed=false"
```

**Genel filtre parametreleri:**
- `limit` (integer, ≥0) — Sayfa başına sonuç
- `offset` (integer, ≥0) — Atlanacak sonuç sayısı
- `order` (string) — Sıralama alanı (örn. `id`, `volume`, `liquidity`, `startTime`)
- `ascending` (boolean) — Sıralama yönü
- `active` (boolean) — Aktif olanlar
- `closed` (boolean) — Kapanmış olanlar (default `false`)
- `archived` (boolean) — Arşivlenmiş olanlar

**Best practices:**
- Aktif marketler için her zaman `active=true&closed=false` kullan
- Tarihsel veri için `closed=true` ekle
- Tek market için slug method daha hızlı: `/markets/slug/{slug}`
- Kategori bazlı browse için `tag_id` parametresini kullan
- Tüm aktif marketleri çekmek için `/events` üzerinden iterate et (events markets içerir)

---

## Rate Limits

Cloudflare throttling sistemi kullanılır. Limit aşılırsa istekler **gecikmeyle/queue ile** servis edilir (HTTP 429 değil).

| Endpoint | Limit |
| --- | --- |
| Genel | 4,000 req / 10s |
| `/events` | 500 req / 10s |
| `/markets` | 300 req / 10s |
| `/markets` + `/events` (toplam) | 900 req / 10s |
| `/comments` | 200 req / 10s |
| `/tags` | 200 req / 10s |
| `/public-search` | 350 req / 10s |
| Health check (`/ok`) | 100 req / 10s |

---

## Endpoints

### Status

#### `GET /ok`
Servisin çalışıp çalışmadığını kontrol eder.

```bash
curl https://gamma-api.polymarket.com/ok
```

**Response:** `200 OK`

---

### Events

Polymarket'teki event objelerini listeler/getirir. Bir event 1 veya daha fazla market içerir.

#### `GET /events`
Tüm event'leri filtrelenebilir şekilde listeler.

**Query Parameters:**

| Parameter | Type | Açıklama |
|---|---|---|
| `limit` | integer | Sayfa başına sonuç |
| `offset` | integer | Atlanacak sonuç |
| `order` | string | Sıralama alanı (örn. `id`, `volume`, `startDate`) |
| `ascending` | boolean | Artan/azalan |
| `id` | integer[] | Event ID'lerine göre filtreleme |
| `slug` | string[] | Slug'lara göre filtreleme |
| `tag_id` | integer | Tag ID'sine göre filtreleme |
| `exclude_tag_id` | integer[] | Hariç tutulacak tag ID'leri |
| `related_tags` | boolean | İlgili tag'leri dahil et |
| `featured` | boolean | Featured event'ler |
| `cyom` | boolean | "Create Your Own Market" eventler |
| `include_chat` | boolean | Chat verisini dahil et |
| `include_template` | boolean | Template verisini dahil et |
| `recurrence` | string | Tekrarlanma tipi |
| `closed` | boolean | Kapanmış event'ler |
| `start_date_min` | date-time | Başlangıç tarihi alt sınır |
| `start_date_max` | date-time | Başlangıç tarihi üst sınır |
| `end_date_min` | date-time | Bitiş tarihi alt sınır |
| `end_date_max` | date-time | Bitiş tarihi üst sınır |

**Örnek İstek:**
```bash
curl "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=5"
```

**Örnek Response (200 - basitleştirilmiş):**
```json
[
  {
    "id": "123456",
    "ticker": "btc-100k-2025",
    "slug": "will-bitcoin-reach-100k-by-2025",
    "title": "Will Bitcoin reach $100k by 2025?",
    "description": "...",
    "startDate": "2025-01-01T00:00:00Z",
    "endDate": "2025-12-31T23:59:59Z",
    "image": "https://...",
    "icon": "https://...",
    "active": true,
    "closed": false,
    "archived": false,
    "featured": true,
    "liquidity": 1234567.89,
    "volume": 9876543.21,
    "openInterest": 100000,
    "volume24hr": 50000,
    "volume1wk": 300000,
    "volume1mo": 1200000,
    "enableOrderBook": true,
    "negRisk": false,
    "negRiskMarketID": null,
    "negRiskFeeBips": 0,
    "commentCount": 234,
    "markets": [
      {
        "id": "789",
        "question": "Will Bitcoin reach $100k by 2025?",
        "conditionId": "0xabc...",
        "slug": "will-bitcoin-reach-100k-by-2025",
        "clobTokenIds": "[\"71321...\", \"52114...\"]",
        "outcomes": "[\"Yes\", \"No\"]",
        "outcomePrices": "[\"0.65\", \"0.35\"]",
        "volume": "9876543.21",
        "liquidity": "1234567.89",
        "active": true,
        "closed": false,
        "enableOrderBook": true,
        "acceptingOrders": true,
        "orderPriceMinTickSize": 0.01,
        "orderMinSize": 5,
        "lastTradePrice": 0.65,
        "bestBid": 0.64,
        "bestAsk": 0.66,
        "spread": 0.02,
        "oneDayPriceChange": 0.05,
        "oneHourPriceChange": 0.001,
        "oneWeekPriceChange": 0.10,
        "rewardsMinSize": 50,
        "rewardsMaxSpread": 0.03
      }
    ],
    "tags": [
      { "id": "21", "label": "Crypto", "slug": "crypto" }
    ],
    "series": [],
    "categories": []
  }
]
```

**Önemli Event alanları:**
- `id`, `slug`, `ticker`, `title` — Tanımlayıcılar
- `liquidity`, `volume`, `volume24hr`, `volume1wk`, `volume1mo`, `volume1yr` — Hacim metrikleri
- `openInterest` — Açık pozisyon değeri
- `enableNegRisk`, `negRiskMarketID`, `negRiskFeeBips` — Negative Risk yapısı
- `markets[]` — Event'e ait tüm marketler (tüm market alanlarıyla)
- `series[]`, `tags[]`, `categories[]`, `collections[]` — İlişkili gruplama objeleri
- `live`, `ended`, `score`, `period`, `elapsed` — Spor eventleri için durum
- `eventCreators[]`, `chats[]`, `templates[]` — Yan veriler

#### `GET /events/{id}`
ID ile tek event getirir.

```bash
curl "https://gamma-api.polymarket.com/events/123456"
```

#### `GET /events/slug/{slug}`
Slug ile tek event getirir (önerilen yöntem — daha hızlı ve direkt).

**Path Parameters:**
- `slug` (string, required)

**Query Parameters:**
- `include_chat` (boolean)
- `include_template` (boolean)

```bash
curl "https://gamma-api.polymarket.com/events/slug/fed-decision-in-october"
```

**Response:** Tek bir event objesi (`/events` ile aynı şema, array değil tek obje).

#### `GET /events/{id}/tags`
Bir event'e ait tüm tag'leri döndürür.

```bash
curl "https://gamma-api.polymarket.com/events/123456/tags"
```

**Response:**
```json
[
  {
    "id": "21",
    "label": "Crypto",
    "slug": "crypto",
    "forceShow": false,
    "forceHide": false,
    "isCarousel": true,
    "publishedAt": "2024-01-01T00:00:00Z",
    "createdAt": "2023-11-07T05:31:56Z",
    "updatedAt": "2023-11-07T05:31:56Z"
  }
]
```

---

### Markets

Polymarket'teki tüm tradable market'leri listeler/getirir.

#### `GET /markets`
Marketleri filtrelenebilir şekilde listeler.

**Query Parameters:**

| Parameter | Type | Açıklama |
|---|---|---|
| `limit` | integer | Sayfa başına sonuç |
| `offset` | integer | Atlanacak sonuç |
| `order` | string | Sıralama alanı |
| `ascending` | boolean | Sıralama yönü |
| `id` | integer[] | Market ID'leri |
| `slug` | string[] | Slug'lar |
| `clob_token_ids` | string[] | CLOB token ID'lerine göre filtre |
| `condition_ids` | string[] | UMA condition ID'leri |
| `market_maker_address` | string[] | Market maker adresleri |
| `liquidity_num_min` | number | Minimum likidite |
| `liquidity_num_max` | number | Maximum likidite |
| `volume_num_min` | number | Minimum hacim |
| `volume_num_max` | number | Maximum hacim |
| `start_date_min` | date-time | Başlangıç alt sınır |
| `start_date_max` | date-time | Başlangıç üst sınır |
| `end_date_min` | date-time | Bitiş alt sınır |
| `end_date_max` | date-time | Bitiş üst sınır |
| `tag_id` | integer | Tag ID'si |
| `related_tags` | boolean | İlgili taglerle birlikte |
| `cyom` | boolean | "Create Your Own Market" |
| `uma_resolution_status` | string | UMA çözüm durumu |
| `game_id` | string | Spor maçı ID'si |
| `sports_market_types` | string[] | Spor market tipi |
| `rewards_min_size` | number | Minimum reward boyutu |
| `question_ids` | string[] | UMA question ID'leri |
| `include_tag` | boolean | Tag bilgilerini dahil et |
| `closed` | boolean | Kapanmış marketler (default `false`) |

**Örnek İstek:**
```bash
curl "https://gamma-api.polymarket.com/markets?closed=false&limit=10&order=volume&ascending=false"
```

**Örnek Response (200, basitleştirilmiş):**
```json
[
  {
    "id": "789",
    "question": "Will Bitcoin reach $100k by 2025?",
    "conditionId": "0xabc123def456...",
    "slug": "will-bitcoin-reach-100k-by-2025",
    "description": "...",
    "endDate": "2025-12-31T23:59:59Z",
    "startDate": "2025-01-01T00:00:00Z",
    "image": "https://...",
    "icon": "https://...",
    "outcomes": "[\"Yes\", \"No\"]",
    "outcomePrices": "[\"0.65\", \"0.35\"]",
    "volume": "9876543.21",
    "liquidity": "1234567.89",
    "volumeNum": 9876543.21,
    "liquidityNum": 1234567.89,
    "active": true,
    "closed": false,
    "archived": false,
    "marketType": "binary",
    "formatType": "yes_no",
    "marketMakerAddress": "0x...",
    "questionID": "0x...",
    "umaEndDate": "2025-12-31T23:59:59Z",
    "umaResolutionStatus": "open",
    "enableOrderBook": true,
    "acceptingOrders": true,
    "orderPriceMinTickSize": 0.01,
    "orderMinSize": 5,
    "clobTokenIds": "[\"71321...\", \"52114...\"]",
    "umaBond": "100000000",
    "umaReward": "1000000",
    "fpmmLive": false,
    "volume24hr": 50000,
    "volume1wk": 300000,
    "volume1mo": 1200000,
    "volume1yr": 9876543.21,
    "volumeAmm": 0,
    "volumeClob": 9876543.21,
    "liquidityAmm": 0,
    "liquidityClob": 1234567.89,
    "makerBaseFee": 0,
    "takerBaseFee": 30,
    "rewardsMinSize": 50,
    "rewardsMaxSpread": 0.03,
    "spread": 0.02,
    "lastTradePrice": 0.65,
    "bestBid": 0.64,
    "bestAsk": 0.66,
    "oneDayPriceChange": 0.05,
    "oneHourPriceChange": 0.001,
    "oneWeekPriceChange": 0.10,
    "oneMonthPriceChange": 0.20,
    "oneYearPriceChange": 0.50,
    "automaticallyResolved": true,
    "automaticallyActive": true,
    "ready": true,
    "funded": true,
    "rfqEnabled": false,
    "events": [ /* parent event objesi */ ],
    "tags": [ { "id": "21", "label": "Crypto", "slug": "crypto" } ],
    "categories": []
  }
]
```

**Trader için kritik alanlar:**
- `clobTokenIds` — CLOB üzerinden trade için kesinlikle gerekli (Yes ve No token id'leri)
- `conditionId` — On-chain identifier
- `enableOrderBook` — `false` ise CLOB'da trade edilemez
- `acceptingOrders` — `false` ise yeni order kabul etmiyor
- `orderPriceMinTickSize` — Minimum fiyat artımı (0.01, 0.001 vs.)
- `orderMinSize` — Minimum order boyutu
- `bestBid`, `bestAsk`, `spread`, `lastTradePrice`
- `outcomes` ve `outcomePrices` — JSON-encoded string array (`serde_json::from_str` ile parse edilmeli)

#### `GET /markets/{id}`
ID ile tek market getirir.

```bash
curl "https://gamma-api.polymarket.com/markets/789"
```

#### `GET /markets/slug/{slug}`
Slug ile tek market getirir.

```bash
curl "https://gamma-api.polymarket.com/markets/slug/will-bitcoin-reach-100k-by-2025"
```

#### `GET /markets/{id}/tags`
Bir market'e ait tag'leri getirir.

```bash
curl "https://gamma-api.polymarket.com/markets/789/tags"
```

---

### Tags

Marketleri kategorize etmek için kullanılan etiket sistemi.

#### `GET /tags`
Tüm tag'leri listeler.

**Query Parameters:**
- `limit`, `offset`, `order`, `ascending`
- `include_template` (boolean)
- `is_carousel` (boolean) — Sadece anasayfa carousel'inde gösterilenler

```bash
curl "https://gamma-api.polymarket.com/tags?limit=100"
```

**Response:**
```json
[
  {
    "id": "21",
    "label": "Crypto",
    "slug": "crypto",
    "forceShow": false,
    "forceHide": false,
    "isCarousel": true,
    "publishedAt": "2024-01-01T00:00:00Z",
    "createdBy": 1,
    "updatedBy": 1,
    "createdAt": "2023-11-07T05:31:56Z",
    "updatedAt": "2023-11-07T05:31:56Z"
  }
]
```

#### `GET /tags/{id}`
ID ile tek tag getirir.

#### `GET /tags/slug/{slug}`
Slug ile tek tag getirir.

#### `GET /tags/{id}/related-tags`
Bir tag ile ilişkili tag relationships'i (parent/child/sibling) döndürür (ID ile).

#### `GET /tags/slug/{slug}/related-tags`
Aynı, slug ile.

#### `GET /tags/{id}/related-tags/tags`
Bir tag ile ilişkili tag'lerin kendilerini döndürür.

#### `GET /tags/slug/{slug}/related-tags/tags`
Aynı, slug ile.

---

### Series

Tekrarlanan tema altında grupladığı event koleksiyonları (örn. NBA Finals, Fed Rate Decisions).

#### `GET /series`
Tüm serileri listeler.

**Query Parameters:**
- `limit`, `offset`, `order`, `ascending`
- `slug` (string[])
- `categories_ids` (integer[])
- `categories_labels` (string[])
- `closed` (boolean)
- `include_chat` (boolean)
- `recurrence` (string)

```bash
curl "https://gamma-api.polymarket.com/series?limit=10"
```

**Response (basitleştirilmiş):**
```json
[
  {
    "id": "10345",
    "ticker": "nba-finals-2025",
    "slug": "nba-finals-2025",
    "title": "NBA Finals 2025",
    "subtitle": "Championship Series",
    "seriesType": "sports",
    "recurrence": "annual",
    "description": "...",
    "image": "https://...",
    "icon": "https://...",
    "layout": "default",
    "active": true,
    "closed": false,
    "archived": false,
    "featured": true,
    "publishedAt": "2024-12-01T00:00:00Z",
    "createdAt": "2024-12-01T00:00:00Z",
    "updatedAt": "2024-12-01T00:00:00Z",
    "commentsEnabled": true,
    "competitive": "high",
    "volume": 1234567,
    "volume24hr": 50000,
    "liquidity": 234567,
    "startDate": "2025-06-01T00:00:00Z",
    "pythTokenID": null,
    "cgAssetName": null,
    "score": 100,
    "events": [ /* event array */ ],
    "collections": [],
    "categories": [],
    "tags": [],
    "commentCount": 56,
    "chats": []
  }
]
```

#### `GET /series/{id}`
ID ile tek seri getirir.

```bash
curl "https://gamma-api.polymarket.com/series/10345"
```

---

### Comments

Marketler/event'ler/seriler üzerine yapılan yorumlar.

#### `GET /comments`
Yorumları listeler.

**Query Parameters:**
- `limit`, `offset`, `order`, `ascending`
- `parent_entity_type` — `Event`, `Series`, `market`
- `parent_entity_id` (integer) — Parent objenin ID'si
- `get_positions` (boolean) — Yorum yapanın pozisyonlarını dahil et
- `holders_only` (boolean) — Sadece pozisyon sahiplerinin yorumları

```bash
curl "https://gamma-api.polymarket.com/comments?parent_entity_type=Event&parent_entity_id=123456&limit=20"
```

**Response (basitleştirilmiş):**
```json
[
  {
    "id": "comment-uuid",
    "body": "Yorumun metni",
    "parentEntityType": "Event",
    "parentEntityID": 123456,
    "parentCommentID": null,
    "userAddress": "0x1234...",
    "replyAddress": null,
    "createdAt": "2025-01-01T12:00:00Z",
    "updatedAt": "2025-01-01T12:00:00Z",
    "profile": {
      "name": "Trader42",
      "pseudonym": "Anonymous Trader",
      "displayUsernamePublic": true,
      "bio": "...",
      "isMod": false,
      "isCreator": false,
      "proxyWallet": "0x5678...",
      "baseAddress": "0x1234...",
      "profileImage": "https://...",
      "positions": [
        { "tokenId": "71321...", "positionSize": "100" }
      ]
    },
    "reactions": [
      {
        "id": "react-uuid",
        "commentID": 1,
        "reactionType": "👍",
        "icon": "thumbsup",
        "userAddress": "0xabcd...",
        "createdAt": "2025-01-01T12:30:00Z",
        "profile": { /* aynı şema */ }
      }
    ],
    "reportCount": 0,
    "reactionCount": 5
  }
]
```

#### `GET /comments/{id}`
ID ile tek yorum (varsa cevaplarıyla birlikte) getirir.

#### `GET /comments/user/{userAddress}`
Bir kullanıcının tüm yorumlarını getirir.

```bash
curl "https://gamma-api.polymarket.com/comments/user/0x1234567890abcdef..."
```

---

### Sports

Spor marketleri için metadata ve takım/lig bilgileri.

#### `GET /sports`
Otomatik desteklenen tüm sport leagues'i metadata ile listeler.

```bash
curl "https://gamma-api.polymarket.com/sports"
```

**Response:**
```json
[
  {
    "sport": "NBA",
    "image": "https://.../nba.svg",
    "resolution": "https://www.nba.com/...",
    "ordering": "home",
    "tags": "100639,100640",
    "series": "10345"
  },
  {
    "sport": "NFL",
    "image": "https://.../nfl.svg",
    "resolution": "https://www.nfl.com/...",
    "ordering": "away",
    "tags": "100700,100701",
    "series": "10350"
  }
]
```

> **Not:** UFC, Boxing, F1, Golf, Chess gibi otomatik olmayan sporlar bu listede gözükmez. Onlar için `/tags` üzerinden bul ve `/events?tag_id=...` ile filtrele.

#### `GET /sports/market-types`
Geçerli spor market tiplerini döndürür (örn. `moneyline`, `spread`, `totals`, `prop`).

```bash
curl "https://gamma-api.polymarket.com/sports/market-types"
```

#### `GET /teams`
Spor takımlarının listesini döndürür.

```bash
curl "https://gamma-api.polymarket.com/teams"
```

---

### Search

Tek endpoint ile event, tag ve profile araması yapar.

#### `GET /public-search`

**Query Parameters:**

| Parameter | Type | Açıklama |
|---|---|---|
| `q` | string | **(required)** Arama sorgusu |
| `cache` | boolean | Cache kullan |
| `events_status` | string | Event durumu filtresi |
| `limit_per_type` | integer | Tip başına maks sonuç |
| `page` | integer | Sayfa numarası |
| `events_tag` | string[] | Tag'lere göre filtre |
| `keep_closed_markets` | integer | Kapanmış marketleri dahil et |
| `sort` | string | Sıralama alanı |
| `ascending` | boolean | Sıralama yönü |
| `search_tags` | boolean | Tag'lerde de ara |
| `search_profiles` | boolean | Profillerde de ara |
| `recurrence` | string | Recurrence tipi |
| `exclude_tag_id` | integer[] | Hariç tut |
| `optimized` | boolean | Optimized response |

**Örnek İstek:**
```bash
curl "https://gamma-api.polymarket.com/public-search?q=bitcoin&limit_per_type=5&search_tags=true&search_profiles=true"
```

**Örnek Response:**
```json
{
  "events": [
    {
      "id": "123456",
      "slug": "will-bitcoin-reach-100k-by-2025",
      "title": "Will Bitcoin reach $100k by 2025?",
      /* ... event şeması ... */
    }
  ],
  "tags": [
    {
      "id": "21",
      "label": "Crypto",
      "slug": "crypto",
      "event_count": 145
    }
  ],
  "profiles": [
    {
      "id": "user-uuid",
      "name": "BitcoinMaxi",
      "user": 12345,
      "pseudonym": "Anonymous Trader",
      "displayUsernamePublic": true,
      "profileImage": "https://...",
      "bio": "...",
      "proxyWallet": "0x1234...",
      "isCloseOnly": false,
      "isCertReq": false
    }
  ],
  "pagination": {
    "hasMore": true,
    "totalResults": 234
  }
}
```

---

### Public Profile

Bir cüzdan adresine bağlı public profili getirir.

#### `GET /public-profile`

**Query Parameters:**
- `address` (string, required) — Cüzdan adresi (proxy wallet veya user address). Pattern: `^0x[a-fA-F0-9]{40}$`

```bash
curl "https://gamma-api.polymarket.com/public-profile?address=0x1234567890abcdef1234567890abcdef12345678"
```

**Response:**
```json
{
  "createdAt": "2024-01-15T10:30:00Z",
  "proxyWallet": "0x1234...",
  "profileImage": "https://...",
  "displayUsernamePublic": true,
  "bio": "Prediction market enthusiast",
  "pseudonym": "Anonymous Trader",
  "name": "TraderXYZ",
  "users": [
    {
      "id": "user-uuid",
      "creator": false,
      "mod": false
    }
  ],
  "xUsername": "traderxyz",
  "verifiedBadge": false
}
```

---

## Rust Entegrasyon İpuçları

### Önerilen Crate'ler
```toml
[dependencies]
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
url = "2"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
```

### Temel HTTP Client
```rust
use reqwest::Client;
use serde::Deserialize;

const GAMMA_BASE: &str = "https://gamma-api.polymarket.com";

pub struct GammaClient {
    http: Client,
    base: String,
}

impl GammaClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("client build"),
            base: GAMMA_BASE.to_string(),
        }
    }

    pub async fn list_events(
        &self,
        limit: u32,
        offset: u32,
        active: bool,
        closed: bool,
    ) -> anyhow::Result<Vec<Event>> {
        let resp = self.http
            .get(format!("{}/events", self.base))
            .query(&[
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
                ("active", active.to_string()),
                ("closed", closed.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Event>>()
            .await?;
        Ok(resp)
    }
}
```

### `outcomes` ve `outcomePrices` Parse Pattern

Bu alanlar string-encoded JSON array döner. Helper kullan:

```rust
use serde::{Deserialize, Deserializer};

fn parse_string_array<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    serde_json::from_str(&s).map_err(serde::de::Error::custom)
}

fn parse_string_f64_array<'de, D>(deserializer: D) -> Result<Vec<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    let raw: Vec<String> = serde_json::from_str(&s).map_err(serde::de::Error::custom)?;
    raw.iter()
        .map(|v| v.parse::<f64>().map_err(serde::de::Error::custom))
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct Market {
    pub id: String,
    pub question: Option<String>,
    #[serde(rename = "conditionId")]
    pub condition_id: String,
    pub slug: Option<String>,
    #[serde(deserialize_with = "parse_string_array", default)]
    pub outcomes: Vec<String>,
    #[serde(rename = "outcomePrices", deserialize_with = "parse_string_f64_array", default)]
    pub outcome_prices: Vec<f64>,
    #[serde(rename = "clobTokenIds", deserialize_with = "parse_string_array", default)]
    pub clob_token_ids: Vec<String>,
    #[serde(rename = "enableOrderBook", default)]
    pub enable_order_book: bool,
    #[serde(rename = "acceptingOrders", default)]
    pub accepting_orders: bool,
    #[serde(rename = "orderPriceMinTickSize", default)]
    pub order_price_min_tick_size: Option<f64>,
    #[serde(rename = "orderMinSize", default)]
    pub order_min_size: Option<f64>,
    #[serde(rename = "lastTradePrice", default)]
    pub last_trade_price: Option<f64>,
    #[serde(rename = "bestBid", default)]
    pub best_bid: Option<f64>,
    #[serde(rename = "bestAsk", default)]
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub volume: Option<String>,
    pub liquidity: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
}
```

### Performans/Mimari Önerileri

1. **Connection pooling:** Tek `reqwest::Client` instance'ı uygulama boyunca paylaş.
2. **Backoff:** Cloudflare 429 dönerse `tokio::time::sleep` ile exponential backoff uygula.
3. **Pagination iterator:** `futures::stream::unfold` ile limit/offset üzerinden async stream üret.
4. **Caching:** `moka` veya `cached` crate'i ile slug/id bazlı market lookup'larını cache'le.
5. **Slug-first:** Tek market/event için her zaman `/markets/slug/{slug}` veya `/events/slug/{slug}` kullan — `?slug=...` query parametresinden daha hızlı.
6. **Filter aggressively:** `closed=false&active=true` her zaman kullan — gereksiz veri trafiğini azaltır.

### Tipik Workflow

```rust
// 1. Aktif eventleri çek
let events = client.list_events(50, 0, true, false).await?;

// 2. İlk event'in ilk market'inin clobTokenIds'ini al
let first_market = &events[0].markets[0];
let yes_token = &first_market.clob_token_ids[0];
let no_token = &first_market.clob_token_ids[1];

// 3. Bu token_id'leri CLOB API'ye geçirerek order book / trade işlemlerini yap
//    (CLOB dokümanına bakın)
```

---

## İlgili Kaynaklar

- **Resmi Docs:** https://docs.polymarket.com/api-reference
- **CLOB API (trade işlemleri için):** https://docs.polymarket.com/api-reference/authentication
- **Discord:** https://discord.gg/polymarket
- **GitHub:** https://github.com/polymarket