# Polymarket CLOB API — Tam Referans (REST + WebSocket)

> Kaynak: [docs.polymarket.com/api-reference](https://docs.polymarket.com/api-reference)
> Tarih: Nisan 2026 itibarıyla güncel resmi dokümantasyona göre derlenmiştir.
> Hedef: Rust ile Polymarket trading entegrasyonu yapacak geliştiriciler için referans.

---

## İçindekiler

1. [Genel Bakış](#genel-bakış)
2. [Base URLs](#base-urls)
3. [Authentication (L1 / L2)](#authentication-l1--l2)
4. [Signature Types & Funder](#signature-types--funder)
5. [Rate Limits](#rate-limits)
6. **REST — Public (Auth gerekmez)**
   - [Order Book](#order-book)
   - [Market Price](#market-price)
   - [Midpoint Price](#midpoint-price)
   - [Spread](#spread)
   - [Last Trade Price](#last-trade-price)
   - [Prices History](#prices-history)
   - [Tick Size](#tick-size)
   - [Fee Rate](#fee-rate)
   - [Server Time](#server-time)
   - [Simplified / Sampling Markets](#simplified--sampling-markets)
7. **REST — Authenticated (L2 gerekir)**
   - [Post Order](#post-order)
   - [Post Multiple Orders](#post-multiple-orders)
   - [Cancel Single Order](#cancel-single-order)
   - [Cancel Multiple Orders](#cancel-multiple-orders)
   - [Cancel All Orders](#cancel-all-orders)
   - [Cancel Orders for a Market](#cancel-orders-for-a-market)
   - [Get User Orders](#get-user-orders)
   - [Get Single Order by ID](#get-single-order-by-id)
   - [Get Order Scoring Status](#get-order-scoring-status)
   - [Send Heartbeat](#send-heartbeat)
   - [Get Trades](#get-trades)
   - [Get Builder Trades](#get-builder-trades)
8. **WebSocket**
   - [Bağlantı & Heartbeat](#bağlantı--heartbeat)
   - [Market Channel](#market-channel)
   - [User Channel](#user-channel)
   - [Sports Channel](#sports-channel)
9. [Order Tipleri (GTC / FOK / GTD / FAK)](#order-tipleri-gtc--fok--gtd--fak)
10. [Hata Kodları](#hata-kodları)
11. [Rust Entegrasyon İpuçları](#rust-entegrasyon-ipuçları)

---

## Genel Bakış

**CLOB (Central Limit Order Book) API**, Polymarket'in temel trading altyapısıdır:

- Polygon zincirinde Conditional Token Framework (CTF) üzerinde çalışır
- Yes/No outcome çiftleri matematiksel olarak `Yes + No = $1.00` ilişkisini korur
- Geleneksel borsa mekaniği (price + time priority order matching)
- USDC.e ile collateralize edilen pozisyonlar
- Public read endpointleri (orderbook, fiyatlar) ve authenticated trading endpointleri içerir

İki tür endpoint:
- **Public** — orderbook, fiyat, midpoint, spread, geçmiş, server time vs. (auth gerekmez)
- **Authenticated** — order yönetimi, trade history, heartbeat (L2 API key headers gerekir)

> **Önemli:** Trading öncesi USDC.e bakiyeniz olmalı ve EOA cüzdanlar için CTF kontratlarına allowance set edilmiş olmalı.

---

## Base URLs

| Tür | URL |
|---|---|
| **Production REST** | `https://clob.polymarket.com` |
| **Staging REST** | `https://clob-staging.polymarket.com` |
| **Production WebSocket** | `wss://ws-subscriptions-clob.polymarket.com/ws/<channel>` |
| **Sports WebSocket** | `wss://sports-api.polymarket.com/ws` |

WebSocket channel isimleri: `market`, `user` (→ `wss://ws-subscriptions-clob.polymarket.com/ws/market`)

---

## Authentication (L1 / L2)

CLOB API **iki seviyeli authentication** kullanır.

### L1 Authentication — Private Key

EIP-712 imza kullanarak cüzdanın sahipliğini ispatlar. Sadece API credential oluşturmak/almak için kullanılır.

**Kullanım alanları:**
- API credential oluşturma (`POST /auth/api-key`)
- Mevcut credential'ı türetme (`GET /auth/derive-api-key`)
- Order'ları lokal olarak imzalama

**Required L1 Headers:**

| Header | Açıklama |
|---|---|
| `POLY_ADDRESS` | Polygon signer adresi |
| `POLY_SIGNATURE` | CLOB EIP-712 imzası |
| `POLY_TIMESTAMP` | Mevcut UNIX timestamp |
| `POLY_NONCE` | Nonce (default: 0) |

**EIP-712 Sign Yapısı:**

```javascript
const domain = {
  name: "ClobAuthDomain",
  version: "1",
  chainId: 137 // Polygon
};

const types = {
  ClobAuth: [
    { name: "address",   type: "address" },
    { name: "timestamp", type: "string" },
    { name: "nonce",     type: "uint256" },
    { name: "message",   type: "string" }
  ]
};

const value = {
  address:   signingAddress,
  timestamp: ts,
  nonce:     nonce,
  message:   "This message attests that I control the given wallet"
};
```

#### `POST /auth/api-key`
Yeni API credential oluşturur.

#### `GET /auth/derive-api-key`
Aynı nonce ile mevcut credential'ı türetir (recovery için).

**Response (her iki endpoint için):**
```json
{
  "apiKey":     "550e8400-e29b-41d4-a716-446655440000",
  "secret":     "base64EncodedSecretString",
  "passphrase": "randomPassphraseString"
}
```

> ⚠️ Bu üç değer **L2 authentication için kesinlikle gereklidir**. Güvenli bir şekilde saklayın. Kayıp ederseniz geri alınamaz — yeni nonce ile yeniden oluşturmanız gerekir.

### L2 Authentication — API Key (HMAC-SHA256)

Trade endpointleri için kullanılır. Her isteğe HMAC-SHA256 imzası eklenir.

**Required L2 Headers (5 header):**

| Header | Açıklama |
|---|---|
| `POLY_ADDRESS` | Polygon signer adresi |
| `POLY_SIGNATURE` | İstek için HMAC-SHA256 imza |
| `POLY_TIMESTAMP` | Mevcut UNIX timestamp |
| `POLY_API_KEY` | Yukarıdaki `apiKey` değeri |
| `POLY_PASSPHRASE` | Yukarıdaki `passphrase` değeri |

**HMAC İmza Hesaplaması (pseudocode):**

```
message = timestamp + method.upper() + requestPath + body
signature = base64(hmac_sha256(secret, message))
```

> Reference implementations:
> - TypeScript: https://github.com/Polymarket/clob-client/blob/main/src/signing/hmac.ts
> - Python: https://github.com/Polymarket/py-clob-client/blob/main/py_clob_client/signing/hmac.py
> - Rust (resmi SDK): https://github.com/Polymarket/rs-clob-client/blob/main/src/auth.rs — `URL_SAFE` ile secret decode ve imza encode **aynı alfabe** (Python `urlsafe_b64decode` / `urlsafe_b64encode` ile uyumlu).

> **Not:** L2 header'ları olsa bile, order **creation** yapan metodlar order payload'unu ayrıca cüzdan private key'i ile **imzalamayı** gerektirir.

---

## Signature Types & Funder

Order yaratırken `signatureType` ve `funder` belirtmek gerekir:

| Signature Type | Value | Açıklama |
|---|---|---|
| `EOA` | `0` | Standart Ethereum cüzdanı (MetaMask). Funder = EOA adresi. POL gas gerektirir. |
| `POLY_PROXY` | `1` | Magic Link/Email proxy wallet (Polymarket.com'a email/Google ile giriş yapılmışsa). PK Polymarket.com'dan export edilmeli. |
| `GNOSIS_SAFE` | `2` | Gnosis Safe multisig proxy (en yaygın). Yeni veya geri dönen kullanıcılar için kullanılır. |

**Funder address:**
- Fonların gerçekten tutulduğu adres
- Polymarket.com'da görünen wallet = proxy wallet = funder olarak kullanılmalı
- Proxy wallet'lar deterministik olarak türetilebilir (CREATE2)
- Polymarket.com'a ilk girişte otomatik deploy edilir

---

## Rate Limits

Cloudflare throttling kullanılır. Limit aşılırsa istekler **gecikmeyle** servis edilir.

### Genel
| Endpoint | Limit |
|---|---|
| Genel | 9,000 req / 10s |
| `GET` balance/allowance | 200 req / 10s |
| `UPDATE` balance/allowance | 50 req / 10s |

### Market Data
| Endpoint | Limit |
|---|---|
| `/book` | 1,500 req / 10s |
| `/books` | 500 req / 10s |
| `/price` | 1,500 req / 10s |
| `/prices` | 500 req / 10s |
| `/midpoint` | 1,500 req / 10s |
| `/midpoints` | 500 req / 10s |
| `/prices-history` | 1,000 req / 10s |
| Market tick size | 200 req / 10s |

### Ledger
| Endpoint | Limit |
|---|---|
| `/trades`, `/orders`, `/notifications`, `/order` | 900 req / 10s |
| `/data/orders` | 500 req / 10s |
| `/data/trades` | 500 req / 10s |
| `/notifications` | 125 req / 10s |

### Authentication
| Endpoint | Limit |
|---|---|
| API key endpoints | 100 req / 10s |

### Trading (Burst + Sustained)

| Endpoint | Burst | Sustained |
|---|---|---|
| `POST /order` | 3,500 req / 10s | 36,000 req / 10 min |
| `DELETE /order` | 3,000 req / 10s | 30,000 req / 10 min |
| `POST /orders` | 1,000 req / 10s | 15,000 req / 10 min |
| `DELETE /orders` | 1,000 req / 10s | 15,000 req / 10 min |
| `DELETE /cancel-all` | 250 req / 10s | 6,000 req / 10 min |
| `DELETE /cancel-market-orders` | 1,000 req / 10s | 1,500 req / 10 min |

### WebSocket
- IP başına maks **5 concurrent connection**
- Ping/Pong her 10 saniyede bir gönderilmeli

---

## REST — Public Endpoints (Auth gerekmez)

### Order Book

#### `GET /book`
Belirli bir token için orderbook snapshot'ı döndürür (bids + asks + market detayları).

**Query Parameters:**
- `token_id` (string, required) — Token (asset) ID

**Örnek İstek:**
```bash
curl "https://clob.polymarket.com/book?token_id=71321045679252212594626385532706912750332728571942532289631379312455583992563"
```

**Response (200):**
```json
{
  "market":    "0x1234567890123456789012345678901234567890",
  "asset_id":  "0xabc123def456...",
  "timestamp": "1234567890",
  "hash":      "a1b2c3d4e5f6...",
  "bids": [
    { "price": "0.45", "size": "100" },
    { "price": "0.44", "size": "200" }
  ],
  "asks": [
    { "price": "0.46", "size": "150" },
    { "price": "0.47", "size": "250" }
  ],
  "min_order_size":   "1",
  "tick_size":        "0.01",
  "neg_risk":         false,
  "last_trade_price": "0.45"
}
```

**Alanlar:**
- `bids` — Fiyat azalan sırada
- `asks` — Fiyat artan sırada
- `hash` — Snapshot integrity hash
- `neg_risk` — Negative Risk marketi mi
- `min_order_size` — Bu market için min order
- `tick_size` — Min fiyat artımı

#### `POST /books`
Birden fazla token için orderbook çeker (batch).

**Body:**
```json
[
  { "token_id": "71321045679..." },
  { "token_id": "52114319501..." }
]
```

**Response:** `[OrderBook, OrderBook, ...]`

---

### Market Price

#### `GET /price`
Belirli bir token ve side (BUY/SELL) için **best fiyatı** döndürür.

**Query Parameters:**
- `token_id` (string, required)
- `side` (enum: `BUY` | `SELL`, required) — `BUY` → best bid, `SELL` → best ask

```bash
curl "https://clob.polymarket.com/price?token_id=71321...&side=BUY"
```

**Response:**
```json
{ "price": 0.45 }
```

#### `GET /prices` (query parameters)
Birden fazla token için fiyat çeker (query params üzerinden).

#### `POST /prices` (request body)
Birden fazla token için fiyat çeker (body üzerinden).

**Body:**
```json
[
  { "token_id": "71321...", "side": "BUY" },
  { "token_id": "52114...", "side": "SELL" }
]
```

**Response:**
```json
{
  "71321...": { "BUY": "0.45" },
  "52114...": { "SELL": "0.55" }
}
```

---

### Midpoint Price

Best bid ve best ask'in ortalaması.

#### `GET /midpoint`
**Query Parameters:**
- `token_id` (string, required)

```bash
curl "https://clob.polymarket.com/midpoint?token_id=71321..."
```

**Response:**
```json
{ "mid_price": "0.45" }
```

#### `GET /midpoints` (query parameters)
Birden fazla token için midpoint (query üzerinden).

#### `POST /midpoints` (request body)
Birden fazla token için midpoint (body üzerinden).

**Body:**
```json
[
  { "token_id": "71321..." },
  { "token_id": "52114..." }
]
```

---

### Spread

Best ask − best bid farkı.

#### `GET /spread`
**Query Parameters:**
- `token_id` (string, required)

```bash
curl "https://clob.polymarket.com/spread?token_id=71321..."
```

**Response:**
```json
{ "spread": "0.02" }
```

#### `POST /spreads`
Birden fazla token için spread (body):
```json
[ { "token_id": "..." }, { "token_id": "..." } ]
```

---

### Last Trade Price

#### `GET /last-trade-price`
Son gerçekleşen trade fiyatını döndürür.

**Query Parameters:**
- `token_id` (string, required)

**Response:**
```json
{ "price": "0.45" }
```

#### `GET /last-trades-prices` (query parameters)
Birden fazla token için.

#### `POST /last-trades-prices` (request body)
Body üzerinden batch.

---

### Prices History

#### `GET /prices-history`
Bir market için tarihsel fiyat verisi (charting için).

**Query Parameters:**

| Parameter | Type | Açıklama |
|---|---|---|
| `market` | string | **(required)** Token (asset) ID |
| `startTs` | number | Başlangıç UNIX timestamp |
| `endTs` | number | Bitiş UNIX timestamp |
| `interval` | enum | `max`, `all`, `1m`, `1w`, `1d`, `6h`, `1h` |
| `fidelity` | integer | Veri çözünürlüğü (dakika cinsinden, default 1) |

**Örnek İstek:**
```bash
curl "https://clob.polymarket.com/prices-history?market=71321...&interval=1d&fidelity=60"
```

**Response:**
```json
{
  "history": [
    { "t": 1700000000, "p": 0.42 },
    { "t": 1700001000, "p": 0.43 },
    { "t": 1700002000, "p": 0.45 }
  ]
}
```

- `t` — UNIX timestamp (saniye)
- `p` — Fiyat (0-1 arası)

#### `POST /batch-prices-history`
Birden fazla market için tarihsel fiyat çekme (batch).

---

### Tick Size

#### `GET /tick-size`
Bir token'ın minimum fiyat artımını döndürür.

**Query Parameters:**
- `token_id` (string)

```bash
curl "https://clob.polymarket.com/tick-size?token_id=71321..."
```

**Response:**
```json
{ "minimum_tick_size": 0.01 }
```

#### `GET /tick-size/{token_id}`
Path parameter versiyonu (alternatif).

---

### Fee Rate

#### `GET /fee-rate`
Bir token için fee rate'ini döndürür.

**Query Parameters:**
- `token_id` (string)

#### `GET /fee-rate/{token_id}`
Path parameter versiyonu.

---

### Server Time

#### `GET /time`
Server'ın UNIX timestamp'ini döndürür. Lokal saat senkronizasyonu için kritik (HMAC imzaları timestamp'e dayanır).

```bash
curl "https://clob.polymarket.com/time"
```

**Response:**
```
1234567890
```

> ⚠️ Tek raw integer döner, JSON wrap yok.

---

### Simplified / Sampling Markets

CLOB API üzerinden, on-chain'de var olan tüm marketleri minimum metadata ile listeler.

#### `GET /simplified-markets`
**Query Parameters:**
- `next_cursor` (string) — Sayfa cursor'u

```bash
curl "https://clob.polymarket.com/simplified-markets"
```

**Response:**
```json
{
  "limit": 100,
  "next_cursor": "MTAw",
  "count": 50,
  "data": [
    {
      "condition_id": "0xabc...",
      "rewards": {
        "rates": [
          { "asset_address": "0x...", "rewards_daily_rate": 100 }
        ],
        "min_size": 50,
        "max_spread": 0.03
      },
      "tokens": [
        { "token_id": "71321...", "outcome": "Yes", "price": 0.65, "winner": false },
        { "token_id": "52114...", "outcome": "No",  "price": 0.35, "winner": false }
      ],
      "active": true,
      "closed": false,
      "archived": false,
      "accepting_orders": true
    }
  ]
}
```

#### `GET /sampling-markets`
Sadece sampling reward marketlerini listeler (likidite reward'larına katılan marketler).

#### `GET /sampling-simplified-markets`
İkisinin birleşimi — basitleştirilmiş + sampling.

---

## REST — Authenticated Endpoints (L2 gerektirir)

Tüm bu endpointler için 5 L2 header gönderilmesi şart:
`POLY_ADDRESS`, `POLY_SIGNATURE`, `POLY_TIMESTAMP`, `POLY_API_KEY`, `POLY_PASSPHRASE`

### Post Order

#### `POST /order`
Tek bir order'ı orderbook'a gönderir.

**Body:**
```json
{
  "order": {
    "maker":         "0x1234567890123456789012345678901234567890",
    "signer":        "0x1234567890123456789012345678901234567890",
    "taker":         "0x0000000000000000000000000000000000000000",
    "tokenId":       "0xabc123def456...",
    "makerAmount":   "100000000",
    "takerAmount":   "200000000",
    "side":          "BUY",
    "expiration":    "1735689600",
    "nonce":         "0",
    "feeRateBps":    "30",
    "signature":     "0x1234abcd...",
    "salt":          1234567890,
    "signatureType": 0
  },
  "owner":     "f4f247b7-4ac7-ff29-a152-04fda0a8755a",
  "orderType": "GTC",
  "deferExec": false
}
```

**Order alanları:**
- `maker` — Order veren cüzdan (proxy wallet için funder address)
- `signer` — İmzalayan adres (signature_type'a göre maker'la aynı veya farklı)
- `taker` — Taker (genellikle `0x000...0` = herkes)
- `tokenId` — Yes/No token id'si (Gamma'dan `clobTokenIds` ile alınır)
- `makerAmount` / `takerAmount` — fixed-math, 6 decimal (USDC)
- `side` — `BUY` veya `SELL`
- `expiration` — UNIX timestamp (GTD ise gerekli)
- `nonce` — Order nonce
- `feeRateBps` — Fee rate (basis points)
- `signature` — EIP-712 ile imzalanmış order
- `salt` — Random uniqueness salt
- `signatureType` — `0` (EOA), `1` (POLY_PROXY), `2` (GNOSIS_SAFE)
- `owner` — API key UUID
- `orderType` — `GTC`, `FOK`, `GTD`, `FAK` (default `GTC`)
- `deferExec` — Match'leme'yi geciktir

**Response (200):**
```json
{
  "success":      true,
  "orderID":      "0xabcdef1234567890abcdef1234567890abcdef12",
  "status":       "live",
  "makingAmount": "100000000",
  "takingAmount": "200000000",
  "transactionsHashes": ["0x..."],
  "tradeIDs":     ["trade-123"],
  "errorMsg":     ""
}
```

**Status values:**
- `live` — Orderbook'ta kalan emir
- `matched` — Tamamen eşleşti (transactionsHashes ve tradeIDs döner)
- `delayed` — Geçici olarak ertelendi (rate limit, vs.)

---

### Post Multiple Orders

#### `POST /orders`
Birden fazla order'ı paralel olarak gönderir. **Maks 15 order/request.**

**Body:** `[ <order1>, <order2>, ... ]` (her biri yukarıdaki şemayla)

**Response:** `[ <result1>, <result2>, ... ]` — Her order için ayrı sonuç. Bazıları başarılı, bazıları hata olabilir:

```json
[
  {
    "success": true,
    "orderID": "0xabcdef...",
    "status":  "live",
    "errorMsg": ""
  },
  {
    "success": true,
    "orderID": "0xfedcba...",
    "status":  "matched",
    "transactionsHashes": ["0x..."],
    "tradeIDs": ["trade-123"]
  },
  {
    "success": false,
    "orderID": "",
    "status":  "delayed",
    "errorMsg": "Rate limit exceeded for tokenId: 0xdef..."
  }
]
```

---

### Cancel Single Order

#### `DELETE /order`
Bir order'ı iptal eder. Cancel-only mode'da bile çalışır.

**Body:**
```json
{ "orderID": "0xabcdef1234567890abcdef1234567890abcdef12" }
```

**Response:**
```json
{
  "canceled": ["0xabcdef..."],
  "not_canceled": {}
}
```

`not_canceled` doluysa içinde error mesajı bulunur:
```json
{
  "canceled": [],
  "not_canceled": {
    "0xabcdef...": "Order not found or already canceled"
  }
}
```

---

### Cancel Multiple Orders

#### `DELETE /orders`
Birden fazla order'ı iptal eder. **Maks 3000 order/request.** Duplicate ID'ler otomatik ayıklanır.

**Body:**
```json
[
  "0xabcdef1234567890abcdef1234567890abcdef12",
  "0xfedcba0987654321fedcba0987654321fedcba09",
  "0x1234567890abcdef1234567890abcdef12345678"
]
```

**Response:**
```json
{
  "canceled": [
    "0xabcdef...",
    "0xfedcba...",
    "0x123456..."
  ],
  "not_canceled": {}
}
```

---

### Cancel All Orders

#### `DELETE /cancel-all`
Authenticated user'ın **tüm** açık orderlarını iptal eder. Cancel-only mode'da bile çalışır.

```bash
curl -X DELETE "https://clob.polymarket.com/cancel-all" \
  -H "POLY_ADDRESS: 0x..." \
  -H "POLY_API_KEY: ..." \
  -H "POLY_PASSPHRASE: ..." \
  -H "POLY_SIGNATURE: ..." \
  -H "POLY_TIMESTAMP: ..."
```

**Response:**
```json
{
  "canceled": ["0xabc...", "0xdef..."],
  "not_canceled": {}
}
```

---

### Cancel Orders for a Market

#### `DELETE /cancel-market-orders`
Belirli bir market + asset için tüm orderları iptal eder.

**Body:**
```json
{
  "market":   "0x0000000000000000000000000000000000000000000000000000000000000001",
  "asset_id": "0xabc123def456..."
}
```

**Response:** Yukarıdakiyle aynı format.

---

### Get User Orders

#### `GET /orders`
Authenticated user'ın açık orderlarını paginated döndürür.

**Query Parameters:**
- `id` (string) — Tek order'a filtre
- `market` (string) — Market (condition ID) filtresi
- `asset_id` (string) — Token id filtresi
- `next_cursor` (string) — Sayfa cursor'u (base64 offset)

```bash
curl "https://clob.polymarket.com/orders?market=0x..." \
  -H "POLY_ADDRESS: ..." -H "POLY_API_KEY: ..." \
  -H "POLY_PASSPHRASE: ..." -H "POLY_SIGNATURE: ..." -H "POLY_TIMESTAMP: ..."
```

**Response:**
```json
{
  "limit":       100,
  "next_cursor": "MTAw",
  "count":       2,
  "data": [
    {
      "id":              "0xabcdef1234567890abcdef1234567890abcdef12",
      "status":          "ORDER_STATUS_LIVE",
      "owner":           "f4f247b7-4ac7-ff29-a152-04fda0a8755a",
      "maker_address":   "0x1234567890123456789012345678901234567890",
      "market":          "0x0000...0001",
      "asset_id":        "0xabc123def456...",
      "side":            "BUY",
      "original_size":   "100000000",
      "size_matched":    "0",
      "price":           "0.5",
      "outcome":         "YES",
      "expiration":      "1735689600",
      "order_type":      "GTC",
      "associate_trades": [],
      "created_at":      1700000000
    },
    {
      "id":              "0xfedcba...",
      "status":          "ORDER_STATUS_LIVE",
      "owner":           "f4f247b7-...",
      "maker_address":   "0x1234...",
      "market":          "0x0000...0002",
      "asset_id":        "0xdef456abc789...",
      "side":            "SELL",
      "original_size":   "200000000",
      "size_matched":    "50000000",
      "price":           "0.75",
      "outcome":         "NO",
      "expiration":      "1735689600",
      "order_type":      "GTC",
      "associate_trades": ["trade-123"],
      "created_at":      1700000001
    }
  ]
}
```

`next_cursor` boş string veya `LTE=` döndüğünde sayfa kalmamış demektir.

---

### Get Single Order by ID

#### `GET /data/orders/{order_id}`
Tek bir order'ı id ile getirir.

```bash
curl "https://clob.polymarket.com/data/orders/0xabcdef..." \
  -H "POLY_ADDRESS: ..." -H "POLY_API_KEY: ..." \
  -H "POLY_PASSPHRASE: ..." -H "POLY_SIGNATURE: ..." -H "POLY_TIMESTAMP: ..."
```

**Response:** Tek bir order objesi (yukarıdaki `data[]` öğesiyle aynı format).

---

### Get Order Scoring Status

#### `GET /order-scoring`
Bir order'ın market making rewards için skoru olup olmadığını döndürür.

**Query Parameters:**
- `order_id` (string)

**Response:**
```json
{ "scoring": true }
```

---

### Send Heartbeat

#### `POST /heartbeat`
Connection'ı canlı tutar (özellikle long-running session için).

```bash
curl -X POST "https://clob.polymarket.com/heartbeat" \
  -H "POLY_ADDRESS: ..." -H "POLY_API_KEY: ..." \
  -H "POLY_PASSPHRASE: ..." -H "POLY_SIGNATURE: ..." -H "POLY_TIMESTAMP: ..."
```

---

### Get Trades

#### `GET /trades`
Authenticated user'ın trade'lerini paginated döndürür.

**Query Parameters:**
- `id` (string) — Tek trade ID'si
- `maker_address` (string, **required**) — Maker adresi filtresi
- `market` (string) — Market (condition ID)
- `asset_id` (string) — Token id
- `before` (string) — UNIX timestamp öncesi
- `after` (string) — UNIX timestamp sonrası
- `next_cursor` (string) — Pagination cursor

```bash
curl "https://clob.polymarket.com/trades?maker_address=0x..." \
  -H "POLY_ADDRESS: ..." -H "POLY_API_KEY: ..." \
  -H "POLY_PASSPHRASE: ..." -H "POLY_SIGNATURE: ..." -H "POLY_TIMESTAMP: ..."
```

**Response:**
```json
{
  "limit":       100,
  "next_cursor": "MTAw",
  "count":       2,
  "data": [
    {
      "id":             "trade-123",
      "taker_order_id": "0xabcdef...",
      "market":         "0x0000...0001",
      "asset_id":       "15871154585880608648...",
      "side":           "BUY",
      "size":           "100000000",
      "fee_rate_bps":   "30",
      "price":          "0.5",
      "status":         "TRADE_STATUS_CONFIRMED",
      "match_time":     "1700000000",
      "last_update":    "1700000000",
      "outcome":        "YES",
      "bucket_index":   0,
      "owner":          "f4f247b7-...",
      "maker_address":  "0x1234...",
      "transaction_hash": "0x1234...abcdef",
      "trader_side":    "TAKER",
      "maker_orders":   []
    }
  ]
}
```

**Trade status değerleri:**
- `TRADE_STATUS_MATCHED` — Eşleşti, henüz on-chain confirm değil
- `TRADE_STATUS_MINED` — On-chain mined
- `TRADE_STATUS_CONFIRMED` — Tamamlandı
- `TRADE_STATUS_FAILED` — Hata
- `TRADE_STATUS_RETRYING` — Yeniden deneniyor

---

### Get Builder Trades

#### `GET /builder-trades`
Builder Program kapsamında attribute edilen trade'leri döndürür.

**Query Parameters:** `/trades` ile benzer + builder filtreleri.

---

## WebSocket

Polymarket 3 WebSocket channel sunar.

### Bağlantı & Heartbeat

**Base URL:** `wss://ws-subscriptions-clob.polymarket.com/ws/`

| Channel | Path |
|---|---|
| Market | `/ws/market` |
| User | `/ws/user` |
| Sports | `wss://sports-api.polymarket.com/ws` (farklı host) |

**Heartbeat:**
- Client her **10 saniyede bir** `PING` mesajı (boş JSON `{}`) gönderir
- Server `PONG` (boş JSON `{}`) ile cevap verir
- IP başına maks **5 concurrent connection**

**Dinamik Subscription:**
- Subscription değişiklikleri için reconnect gerekmiyor
- `operation: "subscribe"` veya `operation: "unsubscribe"` ile asset/market eklenebilir/çıkarılabilir

---

### Market Channel

**URL:** `wss://ws-subscriptions-clob.polymarket.com/ws/market`

**Public** — auth gerekmez. Real-time orderbook, fiyat ve market lifecycle event'leri.

#### Subscription Request (bağlantıdan sonra ilk mesaj)

```json
{
  "assets_ids": [
    "65818619657568813474341868652308942079804919287380422192892211131408793125422",
    "52114319501245915516055106046884209969926127482827954674443846427813813222426"
  ],
  "type": "market"
}
```

#### Subscription Update (mevcut bağlantıya asset ekle/çıkar)

```json
{
  "operation": "subscribe",
  "assets_ids": [
    "71321045679252212594626385532706912750332728571942532289631379312455583992563"
  ]
}
```

```json
{
  "operation": "unsubscribe",
  "assets_ids": [ "..." ]
}
```

#### Receive — Event Tipleri

##### `book` — Tam orderbook snapshot
```json
{
  "event_type": "book",
  "asset_id":   "65818619657568813474341868652308942079804919287380422192892211131408793125422",
  "market":     "0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af",
  "bids": [
    { "price": "0.48", "size": "30" },
    { "price": "0.49", "size": "20" },
    { "price": "0.50", "size": "15" }
  ],
  "asks": [
    { "price": "0.52", "size": "25" },
    { "price": "0.53", "size": "60" },
    { "price": "0.54", "size": "10" }
  ],
  "timestamp": "1757908892351",
  "hash":      "0xabc123..."
}
```

##### `price_change` — Orderbook level delta
```json
{
  "event_type": "price_change",
  "market":     "0x5f65177b394277fd294cd75650044e32ba009a95022d88a0c1d565897d72f8f1",
  "price_changes": [
    {
      "asset_id": "71321045679...",
      "price":    "0.5",
      "size":     "200",
      "side":     "BUY",
      "hash":     "56621a121a47ed9333273e21c83b660cff37ae50",
      "best_bid": "0.5",
      "best_ask": "1"
    }
  ],
  "timestamp": "1757908892351"
}
```

##### `last_trade_price` — Trade gerçekleşti
```json
{
  "event_type":       "last_trade_price",
  "asset_id":         "114122071509644379678018727908709560226618148003371446110114509806601493071694",
  "market":           "0x6a67b9d828d53862160e470329ffea5246f338ecfffdf2cab45211ec578b0347",
  "price":            "0.456",
  "size":             "219.217767",
  "fee_rate_bps":     "0",
  "side":             "BUY",
  "timestamp":        "1750428146322",
  "transaction_hash": "0xeeefffggghhh"
}
```

##### `tick_size_change` — Tick size güncellendi
```json
{
  "event_type":    "tick_size_change",
  "asset_id":      "65818619657568813474341868652308942079804919287380422192892211131408793125422",
  "market":        "0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af",
  "old_tick_size": "0.01",
  "new_tick_size": "0.001",
  "timestamp":     "1757908892351"
}
```

##### `best_bid_ask` — En iyi bid/ask güncellendi
```json
{
  "event_type": "best_bid_ask",
  "market":     "0x0005c0d312de0be897668695bae9f32b624b4a1ae8b140c49f08447fcc74f442",
  "asset_id":   "85354956062430465315924116860125388538595433819574542752031640332592237464430",
  "best_bid":   "0.73",
  "best_ask":   "0.77",
  "spread":     "0.04",
  "timestamp":  "1766789469958"
}
```

> Not: `custom_feature_enabled: true` gerektirir.

##### `new_market` — Yeni market deploy edildi
```json
{
  "event_type": "new_market",
  "id":         "1031769",
  "question":   "Will NVIDIA (NVDA) close above $240 end of January?",
  "market":     "0x311d0c4b6671ab54af4970c06fcf58662516f5168997bdda209ec3db5aa6b0c1",
  "slug":       "nvda-above-240-on-january-30-2026",
  "assets_ids": [
    "76043073756653678226373981964075571318267289248134717369284518995922789326425",
    "31690934263385727664202099278545688007799199447969475608906331829650099442770"
  ],
  "outcomes":   ["Yes", "No"],
  "timestamp":  "1766790415550",
  "tags":       ["stocks", "indices"]
}
```

##### `market_resolved` — Market çözümlendi
```json
{
  "event_type":         "market_resolved",
  "id":                 "1031769",
  "market":             "0x311d0c4b6671ab54af4970c06fcf58662516f5168997bdda209ec3db5aa6b0c1",
  "assets_ids": [
    "76043073756653678226373981964075571318267289248134717369284518995922789326425",
    "31690934263385727664202099278545688007799199447969475608906331829650099442770"
  ],
  "winning_asset_id":   "76043073756653678226373981964075571318267289248134717369284518995922789326425",
  "winning_outcome":    "Yes",
  "timestamp":          "1766790415550",
  "tags":               ["stocks"]
}
```

---

### User Channel

**URL:** `wss://ws-subscriptions-clob.polymarket.com/ws/user`

**Authenticated** — L2 credentials gerektirir. User'ın order ve trade event'lerini real-time alır.

#### Subscription Request

```json
{
  "auth": {
    "apiKey":     "your-api-key-uuid",
    "secret":     "your-api-secret",
    "passphrase": "your-passphrase"
  },
  "type": "user"
}
```

> Opsiyonel olarak filter için `markets: [...]` veya `assets_ids: [...]` eklenebilir.

#### Subscription Update

```json
{
  "operation": "subscribe",
  "markets": [
    "0x5f65177b394277fd294cd75650044e32ba009a95022d88a0c1d565897d72f8f1"
  ]
}
```

#### Receive — Event Tipleri

##### `order` — Order placement / update / cancellation
```json
{
  "event_type":     "order",
  "id":             "0xff354cd7ca7539dfa9c28d90943ab5779a4eac34b9b37a757d7b32bdfb11790b",
  "owner":          "9180014b-33c8-9240-a14b-bdca11c0a465",
  "market":         "0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af",
  "asset_id":       "52114319501245915516055106046884209969926127482827954674443846427813813222426",
  "side":           "SELL",
  "order_owner":    "9180014b-33c8-9240-a14b-bdca11c0a465",
  "original_size":  "10",
  "size_matched":   "0",
  "price":          "0.57",
  "associate_trades": null,
  "outcome":        "YES",
  "type":           "PLACEMENT",
  "created_at":     "1672290687",
  "expiration":     "1234567",
  "order_type":     "GTD",
  "status":         "LIVE",
  "maker_address":  "0x1234...",
  "timestamp":      "1672290687"
}
```

**Order event tipleri (`type` alanı):**
- `PLACEMENT` — Yeni order yerleştirildi
- `UPDATE` — Mevcut order güncellendi (size matched değişti)
- `CANCELLATION` — Cancel edildi

##### `trade` — Trade match veya status değişikliği
```json
{
  "event_type":     "trade",
  "type":           "TRADE",
  "id":             "28c4d2eb-bbea-40e7-a9f0-b2fdb56b2c2e",
  "taker_order_id": "0x06bc63e346ed4ceddce9efd6b3af37c8f8f440c92fe7da6b2d0f9e4ccbc50c42",
  "market":         "0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af",
  "asset_id":       "52114319501245915516055106046884209969926127482827954674443846427813813222426",
  "side":           "BUY",
  "size":           "10",
  "price":          "0.57",
  "fee_rate_bps":   "0",
  "status":         "MATCHED",
  "matchtime":      "1672290701",
  "last_update":    "1672290701",
  "outcome":        "YES",
  "owner":          "9180014b-33c8-9240-a14b-bdca11c0a465",
  "trade_owner":    "9180014b-33c8-9240-a14b-bdca11c0a465",
  "maker_address":  "0x1234...",
  "transaction_hash": "",
  "bucket_index":   0,
  "maker_orders": [
    {
      "order_id":      "0xff354cd7ca7539dfa9c28d90943ab5779a4eac34b9b37a757d7b32bdfb11790b",
      "owner":         "9180014b-33c8-9240-a14b-bdca11c0a465",
      "maker_address": "0x5678...",
      "matched_amount": "10",
      "price":         "0.57",
      "fee_rate_bps":  "0",
      "asset_id":      "52114319501...",
      "outcome":       "YES",
      "side":          "SELL"
    }
  ],
  "trader_side": "TAKER",
  "timestamp":   "1672290701"
}
```

---

### Sports Channel

**URL:** `wss://sports-api.polymarket.com/ws`

**Public** — auth gerekmez. Real-time spor maç sonuçları.

> ⚠️ Server PING gönderir (her 5 sn), client 10 sn içinde PONG cevaplamalı (yukarıdaki Market/User channel'larından farklı yön).

#### Receive

##### `Sports Result Update`
```json
{
  "slug":        "mci-liv-2025-02-03",
  "live":        true,
  "ended":       false,
  "score":       "1-0",
  "period":      "1H",
  "elapsed":     "32:15",
  "last_update": "2025-02-03T19:50:16.939Z"
}
```

---

## Order Tipleri (GTC / FOK / GTD / FAK)

| Tip | Açıklama |
|---|---|
| **GTC** (Good Till Cancelled) | İptal edilene kadar orderbook'ta kalır. Default. Standart limit order. |
| **FOK** (Fill or Kill) | Tamamen ve hemen fill edilemezse iptal olur. Market order olarak kullanılır. |
| **GTD** (Good Till Date) | `expiration` timestamp'ine kadar geçerli. Ondan sonra otomatik iptal. |
| **FAK** (Fill and Kill) | Olabildiği kadar fill et, kalan kısmı iptal et. Immediate-or-Cancel. |

**Market order pattern (FOK):**
```rust
// FOK BUY için makerAmount = USDC harcanacak,
// price (0.65) → makerAmount × (1/price) = takerAmount
// Yani: 25 USDC ile $0.65 fiyatından maks 38.46 share alınır
let mo = MarketOrderArgs {
    token_id: "...",
    amount:   25.0,        // USDC
    side:     Side::Buy,
    order_type: OrderType::FOK,
};
```

---

## Hata Kodları

| Error | Sebep | Çözüm |
|---|---|---|
| `INVALID_SIGNATURE` | Yanlış private key veya format | PK'nin `0x` ile başlayan geçerli hex olduğundan emin ol |
| `NONCE_ALREADY_USED` | Aynı nonce ile API key oluşturulmuş | `deriveApiKey()` ile mevcut credential'ı al, veya farklı nonce kullan |
| `Invalid Funder Address` | Funder adresi yanlış | `polymarket.com/settings`'ten doğrula. Yoksa proxy wallet deploy et |
| `not enough balance / allowance` | USDC.e bakiye/allowance yetersiz | EOA'ysan `set_allowances.py` script'ini çalıştır |
| `429 / throttled` | Rate limit aşıldı | Exponential backoff ile retry et |

---

## Rust Entegrasyon İpuçları

### Önerilen Crate'ler

```toml
[dependencies]
# HTTP & async
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
tokio   = { version = "1", features = ["full"] }
futures = "0.3"

# Serialization
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"

# Crypto / signing
sha2       = "0.10"
hmac       = "0.12"
base64     = "0.22"
ethers     = "2"           # EIP-712, signing, address handling
# alternatif: alloy
hex        = "0.4"

# WebSocket
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }

# Yardımcı
uuid       = { version = "1", features = ["v4", "serde"] }
chrono     = { version = "0.4", features = ["serde"] }
anyhow     = "1"
thiserror  = "1"
url        = "2"

# Resmi Rust SDK varsa:
# polymarket-client-sdk = "..."
```

> **Not:** Polymarket'in resmi `polymarket-client-sdk` Rust crate'i mevcuttur. Auth, signing, order construction'ı handle eder. Direkt kendi implementasyonunu yapmak istemiyorsan bunu kullan.

### L2 HMAC İmza Üretimi

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{engine::general_purpose::URL_SAFE, Engine as _};

type HmacSha256 = Hmac<Sha256>;

pub fn build_l2_signature(
    secret_b64: &str,
    timestamp: &str,
    method: &str,         // "GET", "POST", "DELETE"
    request_path: &str,   // "/order", "/orders", ...
    body: &str,           // JSON body veya boş string (HMAC mesajında Python ile aynı olması için tek tırnak → çift tırnak normalize edilmeli — bkz. rs-clob-client `body_to_string`)
) -> anyhow::Result<String> {
    // API secret: resmi Python `urlsafe_b64decode`, Rust SDK `URL_SAFE.decode` — STANDARD decode kullanma.
    let secret = URL_SAFE.decode(secret_b64)?;
    let message = format!("{}{}{}{}", timestamp, method.to_uppercase(), request_path, body);

    let mut mac = HmacSha256::new_from_slice(&secret)?;
    mac.update(message.as_bytes());
    let result = mac.finalize().into_bytes();

    Ok(URL_SAFE.encode(result))
}
```

### Authenticated Request Pattern

```rust
use reqwest::Client;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ClobAuth {
    pub address:    String,
    pub api_key:    String,
    pub secret:     String,
    pub passphrase: String,
}

pub async fn auth_post<T, B>(
    http: &Client,
    base: &str,
    path: &str,
    auth: &ClobAuth,
    body: &B,
) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
    B: Serialize,
{
    let body_str = serde_json::to_string(body)?;
    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs().to_string();
    let sig = build_l2_signature(&auth.secret, &ts, "POST", path, &body_str)?;

    let resp = http
        .post(format!("{}{}", base, path))
        .header("POLY_ADDRESS",    &auth.address)
        .header("POLY_API_KEY",    &auth.api_key)
        .header("POLY_PASSPHRASE", &auth.passphrase)
        .header("POLY_TIMESTAMP",  &ts)
        .header("POLY_SIGNATURE",  &sig)
        .header("Content-Type",    "application/json")
        .body(body_str)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?;

    Ok(resp)
}
```

### EIP-712 Order İmzası (ethers ile)

```rust
use ethers::{
    types::{Address, U256, transaction::eip712::Eip712},
    signers::{LocalWallet, Signer},
};

#[derive(Eip712, Clone)]
#[eip712(
    name = "Polymarket CTF Exchange",
    version = "1",
    chain_id = 137,
    verifying_contract = "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E"
)]
pub struct Order {
    pub salt:           U256,
    pub maker:          Address,
    pub signer:         Address,
    pub taker:          Address,
    pub token_id:       U256,
    pub maker_amount:   U256,
    pub taker_amount:   U256,
    pub expiration:     U256,
    pub nonce:          U256,
    pub fee_rate_bps:   U256,
    pub side:           u8,    // 0 = BUY, 1 = SELL
    pub signature_type: u8,    // 0 = EOA, 1 = POLY_PROXY, 2 = GNOSIS_SAFE
}

pub async fn sign_order(wallet: &LocalWallet, order: &Order) -> anyhow::Result<String> {
    let signature = wallet.sign_typed_data(order).await?;
    Ok(format!("0x{}", hex::encode(signature.to_vec())))
}
```

> **NegRisk Exchange için verifying contract farklıdır:** `0xC5d563A36AE78145C45a50134d48A1215220f80a`
> Doğru contract adreslerini Polymarket repo'sundan kontrol et.

### WebSocket — Market Channel

```rust
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct MarketSubscribe<'a> {
    assets_ids: &'a [&'a str],
    #[serde(rename = "type")]
    sub_type:   &'a str,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "event_type", rename_all = "snake_case")]
enum MarketEvent {
    Book {
        asset_id: String,
        market:   String,
        bids:     Vec<Level>,
        asks:     Vec<Level>,
        timestamp: String,
        hash:     String,
    },
    PriceChange {
        market:    String,
        price_changes: Vec<PriceChange>,
        timestamp: String,
    },
    LastTradePrice {
        asset_id: String,
        market:   String,
        price:    String,
        size:     String,
        side:     String,
        timestamp: String,
        transaction_hash: String,
        fee_rate_bps: String,
    },
    BestBidAsk {
        market:    String,
        asset_id:  String,
        best_bid:  String,
        best_ask:  String,
        spread:    String,
        timestamp: String,
    },
    TickSizeChange {
        asset_id:      String,
        market:        String,
        old_tick_size: String,
        new_tick_size: String,
        timestamp:     String,
    },
    NewMarket {
        id:         String,
        question:   String,
        market:     String,
        slug:       String,
        assets_ids: Vec<String>,
        outcomes:   Vec<String>,
        timestamp:  String,
        tags:       Vec<String>,
    },
    MarketResolved {
        id:               String,
        market:           String,
        assets_ids:       Vec<String>,
        winning_asset_id: String,
        winning_outcome:  String,
        timestamp:        String,
        tags:             Vec<String>,
    },
}

#[derive(Deserialize, Debug)]
struct Level { price: String, size: String }

#[derive(Deserialize, Debug)]
struct PriceChange {
    asset_id: String,
    price:    String,
    size:     String,
    side:     String,
    hash:     String,
    best_bid: String,
    best_ask: String,
}

pub async fn run_market_stream(asset_ids: Vec<String>) -> anyhow::Result<()> {
    let url = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
    let (mut ws, _) = connect_async(url).await?;

    // İlk mesaj: subscribe
    let ids: Vec<&str> = asset_ids.iter().map(String::as_str).collect();
    let sub = MarketSubscribe { assets_ids: &ids, sub_type: "market" };
    ws.send(Message::Text(serde_json::to_string(&sub)?)).await?;

    // Her 10 saniyede ping atan task
    let (mut writer, mut reader) = ws.split();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            if writer.send(Message::Text("{}".into())).await.is_err() {
                break;
            }
        }
    });

    // Mesajları oku
    while let Some(msg) = reader.next().await {
        let msg = msg?;
        if let Message::Text(txt) = msg {
            // Pong skip
            if txt == "{}" || txt.is_empty() { continue; }
            match serde_json::from_str::<MarketEvent>(&txt) {
                Ok(event) => println!("{:?}", event),
                Err(e) => eprintln!("parse error: {e} — raw: {txt}"),
            }
        }
    }
    Ok(())
}
```

### Genel Mimari Önerileri

1. **Tek `reqwest::Client` instance** — Connection pooling için.
2. **Server time sync** — HMAC imzaları timestamp'e dayanır. `GET /time` ile lokal saat sapmasını ölç.
3. **Backoff** — 429 için `tokio::time::sleep` + exponential.
4. **WebSocket reconnect** — `tokio_tungstenite` connection drop olursa eksponansiyel backoff ile reconnect, son subscription state'i restore et.
5. **Order book replay** — WSS bağlandığında REST'ten `GET /book` ile snapshot al, sonra WSS delta'larını apply et.
6. **Rate limit tracking (istemci)** — Polymarket tarafı limitlere takılmamak için çıkan istekleri sınırla: **[governor](https://crates.io/crates/governor)** gibi token bucket yaygın seçimdir. `tower::limit::RateLimit` bir `Service` katmanıdır; Axum **sunucu** yanında, kendi HTTP istemcinizi `Service` olarak modellediğinizde de kullanılabilir — “yalnızca sunucu” değildir; fakat yalın `reqwest` ile **governor** genelde daha doğrudan uygulanır.
7. **Signature type:** Yeni kullanıcılar için `GNOSIS_SAFE` (2). EOA (0) sadece doğrudan Metamask trader'lar için.
8. **`outcome_prices`, `clob_token_ids` parsing** — String-encoded array, `serde_json::from_str` ile parse et.
9. **6 ondalık sabit matematik (USDC.e + share):** Zincir / EIP-712 order struct’ında tutarlar genelde **1e6** ölçeklidir. Tipik binary limit order (SDK’sız elle kuruyorsanız — **neg_risk / farklı exchange** için resmi şema ve `polymarket-client-sdk` kaynak koduna bakın): **BUY** için USDC ödeyen taraf ≈ `size * price * 1e6`, outcome alan taraf ≈ `size * 1e6`; **SELL** için verilen share ≈ `size * 1e6`, alınan USDC ≈ `size * price * 1e6`. Yanlış eşleme çoğu zaman zincirde reddedilir veya beklenmeyen ekonomi üretir; üretimde **SDK order builder** tercih edilir.
10. **Heartbeat:** Long-running session için `POST /heartbeat` periyodik gönder (özellikle market making için kritik).

### Tipik Trade Workflow

```rust
// 1. Gamma'dan market bul ve clobTokenIds al
let market = gamma.market_by_slug("will-bitcoin-reach-100k-by-2025").await?;
let yes_token_id = &market.clob_token_ids[0];

// 2. CLOB'tan tick size & order book al
let tick = clob.tick_size(yes_token_id).await?;
let book = clob.order_book(yes_token_id).await?;

// 3. Order yarat ve imzala
let order = OrderBuilder::new()
    .token_id(yes_token_id)
    .side(Side::Buy)
    .price(0.65)
    .size(100.0)
    .signature_type(SignatureType::GnosisSafe)
    .funder(&proxy_wallet)
    .build();
let signed = sign_order(&wallet, &order).await?;

// 4. POST et
let resp = clob.post_order(signed, OrderType::Gtc).await?;
println!("Order ID: {}", resp.order_id);

// 5. WSS user channel'a bağlan, fill event'lerini izle
tokio::spawn(run_user_stream(auth.clone()));

// 6. Pozisyon dolduğunda Data API'den positions çek (data-api.polymarket.com)
```

---

## İlgili Kaynaklar

- **Resmi REST Docs:** https://docs.polymarket.com/api-reference
- **Auth detayları:** https://docs.polymarket.com/api-reference/authentication
- **Rate limits:** https://docs.polymarket.com/api-reference/rate-limits
- **WebSocket:** https://docs.polymarket.com/api-reference/wss/market
- **Python CLOB Client (referans):** https://github.com/Polymarket/py-clob-client
- **TypeScript CLOB Client (referans):** https://github.com/Polymarket/clob-client
- **Gamma API (market keşfi):** Bkz. `polymarket-gamma-api.md`
- **Discord:** https://discord.gg/polymarket
- **Status:** https://status.polymarket.com