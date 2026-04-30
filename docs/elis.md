# Dutch Book Strateji — Kapsamlı Teknik Doküman

**Proje:** Arbigab  
**Bot ID:** `7f7da476-7ecf-4c9f-9729-bafb86f97bff`  
**Sunucu:** AWS EC2 `54.170.6.194` (eu-west-1, Dublin)  
**Analiz Tarihi:** 30 Nisan 2026  
**Kaynak:** Canlı log analizi + binary incelemesi + gerçek zamanlı market izleme

---

## İçindekiler

1. [Strateji Özeti](#1-strateji-özeti)
2. [Dutch Book Teorisi](#2-dutch-book-teorisi)
3. [Sistem Mimarisi](#3-sistem-mimarisi)
4. [API Bağlantıları](#4-api-bağlantıları)
5. [Konfigürasyon Parametreleri](#5-konfigürasyon-parametreleri)
6. [Trade Döngüsü Akışı](#6-trade-döngüsü-akışı)
7. [Balance Factor Mekanizması](#7-balance-factor-mekanizması)
8. [Spread Tespiti ve Fiyatlama](#8-spread-tespiti-ve-fiyatlama)
9. [22:55–23:00 Canlı Market Analizi](#9-225523-00-canlı-market-analizi)
10. [Fiyat Hareketi Anatomisi](#10-fiyat-hareketi-anatomisi)
11. [Kritik Sorunlar ve Riskler](#11-kritik-sorunlar-ve-riskler)
12. [Performans Metrikleri](#12-performans-metrikleri)
13. [Geliştirme Önerileri](#13-geliştirme-önerileri)

---

## 1. Strateji Özeti

Arbigab Dutch Book botu, Polymarket'teki **BTC 5 dakikalık UP/DOWN binary piyasalarında** spread capture (arbitraj yakalama) stratejisi uygular.

### Temel Fikir

Her 5 dakikada bir yeni bir piyasa açılır: "Bitcoin 5 dakika içinde şu anki fiyatın üstünde mi kalacak, yoksa altına mı inecek?" Bu piyasada iki token vardır:

- **UP token**: BTC yükselirse $1.00 öder
- **DOWN token**: BTC düşerse $1.00 öder

Piyasanın her zaman `UP_fiyat + DOWN_fiyat = $1.00` olması gerekir. Ancak gerçekte piyasa katılımcıları arasındaki talep dengesizliği nedeniyle toplam bazen **$1.00'ın altına** düşer. Bu fark, teorik arbitraj kârıdır.

```
Arbitraj Marjı = $1.00 − (UP_ask + DOWN_ask)
```

---

## 2. Dutch Book Teorisi

### Matematiksel Temel

Bir Dutch Book, bir ya da birden fazla bahsin veya işlemin, sonuçtan bağımsız olarak garantili kâr ya da zarar üreteceği şekilde yapılandırılmasıdır.

Polymarket örneğinde:

| Senaryo | Ödeme |
|---|---|
| UP @ $0.37 + DOWN @ $0.61 al | Toplam maliyet: $0.98 |
| BTC yükselirse | UP $1.00 öder → Net kâr: $0.02 |
| BTC düşerse | DOWN $1.00 öder → Net kâr: $0.02 |
| **Her iki durumda** | **$0.02 garantili kâr** |

Bu yalnızca **her iki emir de aynı fiyattan fill olursa** geçerlidir.

### Pratik Zorluklar

1. **Likidite**: Her iki tarafın da yeterli satıcısı olmalı
2. **Fill Hızı**: İki emir arasındaki sürede fiyat değişebilir
3. **Slippage**: Büyük emirler fiyatı olumsuz etkileyebilir
4. **Tek Taraf Fill Riski**: Sadece bir taraf dolduğunda yönsel pozisyon oluşur

---

## 3. Sistem Mimarisi

```
┌─────────────────────────────────────────────────────────┐
│             AWS EC2 eu-west-1 (54.170.6.194)            │
│                                                         │
│  ┌──────────────────────────────────────────────────┐   │
│  │         Docker Container: arbigab-dashboard       │   │
│  │                                                  │   │
│  │  ┌─────────────────┐  ┌──────────────────────┐  │   │
│  │  │  Next.js Web    │  │   arbitrage_bot       │  │   │
│  │  │  (Node.js)      │  │   (Rust Binary)       │  │   │
│  │  │  Port: 8080     │  │                       │  │   │
│  │  │                 │  │  Strategy: Dutch Book  │  │   │
│  │  │  Dashboard UI   │  │  Symbol: BTC           │  │   │
│  │  │  Bot Yönetimi   │  │  Interval: 5dk         │  │   │
│  │  └────────┬────────┘  └──────────┬────────────┘  │   │
│  │           │                      │               │   │
│  │           └──────────────────────┘               │   │
│  │                    │                             │   │
│  │           SQLite DB (gabagool.db, 107MB)         │   │
│  └────────────────────┼─────────────────────────────┘   │
│                       │                                  │
└───────────────────────┼──────────────────────────────────┘
                        │
         ┌──────────────┼──────────────┐
         ▼              ▼              ▼
  Polymarket CLOB  Polymarket WS  gabagool22.com
  (HTTPS REST)     (WebSocket)    (Auth/Config)
  104.18.34.205    104.18.34.205  80.78.28.229
```

### Bileşenler

| Bileşen | Teknoloji | Görev |
|---|---|---|
| `arbitrage_bot` | Rust (binary) | Trade mantığı, emir gönderme |
| Web Dashboard | Next.js + Prisma | UI, bot yönetimi, log görüntüleme |
| `gabagool.db` | SQLite | Trade geçmişi, konfigürasyonlar |
| `docker-entrypoint.sh` | Shell | Container başlatma, DB migrasyonu |

---

## 4. API Bağlantıları

Bot çalışırken aşağıdaki API'lere bağlanır:

### Polymarket Gamma API
```
GET https://gamma-api.polymarket.com/markets/slug/{slug}
GET https://gamma-api.polymarket.com/events?slug={event_slug}
```
**Amaç**: Her piyasa penceresinin başında market metadata (token ID'leri, başlangıç/bitiş zamanları) çekilir.  
**Sıklık**: Her 5 dakikada bir (her yeni pencere başlangıcında).

### Polymarket CLOB API
```
POST https://clob.polymarket.com/orders          # Emir gönderme
DELETE https://clob.polymarket.com/orders        # Emir iptal
GET  https://clob.polymarket.com/orders          # Açık emirleri listele
POST https://clob.polymarket.com/auth/derive-api-key  # Kimlik doğrulama
```
**Amaç**: Gerçek emir işlemleri.  
**Kimlik Doğrulama**: EIP-712 imzası (Ethereum private key ile).  
**IP**: `104.18.34.205` (Cloudflare, Dublin PoP'una yönleniyor)

### Polymarket CLOB WebSocket
```
wss://ws-subscriptions-clob.polymarket.com/ws/market
wss://ws-subscriptions-clob.polymarket.com/ws/user
```
**Amaç**:
- `market` kanalı: Orderbook değişikliklerini anlık alır (bid/ask güncellemeleri)
- `user` kanalı: Kendi emirlerinin fill durumunu izler

**Mesaj Formatı** (gerçek örnekten):
```json
{
  "slug": "btc-updown-5m-1777578900",
  "ticks": [
    {"down_ask": 0.61, "down_bid": 0.60, "ts": 1777579099045, "up_ask": 0.40, "up_bid": 0.38}
  ],
  "ts": 1777579099936,
  "type": "price_snapshots"
}
```

### gabagool22.com (Sadece Dashboard)
```
POST https://gabagool22.com/api/auth              # Giriş doğrulama
GET  https://gabagool22.com/api/official-configs  # Hazır konfigürasyonlar
```
**Not**: `arbitrage_bot` binary'si bu API'yi **hiç kullanmaz**. Yalnızca Next.js dashboard kullanır.  
**Sıklık**: Kullanıcı login olduğunda ve "Refresh Official Configs" butonuna bastığında.

---

## 5. Konfigürasyon Parametreleri

```json
{
  "strategy": "dutch_book",
  "symbol": "btc",
  "interval_minutes": 5,
  "min_price": 0.15,
  "max_price": 0.89,
  "dry_run": true,
  "log_price": false,
  "cancel_orders_on_start": true,
  "stop_before_end_ms": 60000,
  "max_loss": 0,
  "enable_gamble": false,
  "current_market": false,
  "spread_threshold": 0.02,
  "max_buy_order_size": 20,
  "trade_cooldown": 5000,
  "balance_factor": 0.7
}
```

### Parametre Açıklamaları

| Parametre | Değer | Açıklama |
|---|---|---|
| `strategy` | `dutch_book` | Kullanılan strateji tipi |
| `symbol` | `btc` | İşlem yapılan kripto para |
| `interval_minutes` | `5` | Market pencere uzunluğu (dk) |
| `min_price` | `0.15` | Bu fiyatın altındaki emirler gönderilmez |
| `max_price` | `0.89` | Bu fiyatın üzerindeki emirler gönderilmez |
| `dry_run` | `true` | **Gerçek emir yok** — simülasyon modu |
| `cancel_orders_on_start` | `true` | Başlangıçta tüm eski emirleri temizle |
| `stop_before_end_ms` | `60000` | Pencere bitmeden 60s önce dur |
| `spread_threshold` | `0.02` | Min spread ($0.02) — bunun altında işlem yapma |
| `max_buy_order_size` | `20` | Her tarafa max 20 share emir |
| `trade_cooldown` | `5000` | Emir gönderdikten 5 saniye sonra iptal et |
| `balance_factor` | `0.7` | Pozisyon dengeleme agresifliği (0=pasif, 1=maksimum) |
| `current_market` | `false` | Mevcut değil, bir sonraki pencereyi bekle |

---

## 6. Trade Döngüsü Akışı

Her market penceresinde aşağıdaki döngü çalışır:

```
BOT BAŞLATMA
    │
    ▼
Gamma API → Market slug'ını al (btc-updown-5m-{timestamp})
    │
    ▼
CLOB API → Eski açık emirleri temizle (cancel_orders_on_start=true)
    │
    ▼
WebSocket → Market ve User kanallarına abone ol
    │
    ▼
Pencere başlamasını bekle (Window: XX:XX:00 UTC)
    │
    ▼
┌─────────────────────────────────────────────┐
│            ANA TRADE DÖNGÜSÜ                │
│                                             │
│  WebSocket orderbook güncellemesi geldi?    │
│         │                                   │
│         ▼ EVET                              │
│  Spread hesapla: UP_ask + DOWN_ask          │
│         │                                   │
│         ▼                                   │
│  Spread >= 0.02? ──── HAYIR ──→ Bekle       │
│         │                                   │
│         ▼ EVET                              │
│  Pozisyon dengesizliğini ölç (imbalance)    │
│         │                                   │
│         ▼                                   │
│  Balance factor ile emir boyutu hesapla     │
│         │                                   │
│         ▼                                   │
│  Fiyat min_price–max_price aralığında mı?   │
│         │                                   │
│         ▼ EVET                              │
│  Batch BUY emri gönder (UP + DOWN birlikte) │
│         │                                   │
│         ▼                                   │
│  5000ms bekle (trade_cooldown)              │
│         │                                   │
│         ▼                                   │
│  Her iki emri iptal et (cancel)             │
│         │                                   │
│         └──────────────────────────────┐    │
│                                        ▼    │
│              Pencere bitimine 60s kaldı?    │
│                    │                        │
│                    ▼ EVET                   │
│              Döngüden çık                   │
└─────────────────────────────────────────────┘
    │
    ▼
Market #N tamamlandı → Bir sonraki pencereyi bekle
```

### Zaman Çizelgesi (22:55–23:00 örneği)

```
22:55:00.000  Market penceresi başlıyor
22:55:00.648  Bot market'i keşfetti (Gamma API: 73ms)
22:55:00.721  Token ID'leri alındı (UP: 884637..., DOWN: 682114...)
22:55:00.863  WebSocket bağlantısı kuruldu (142ms)
22:55:00.863  UP ve DOWN asset'lerine abone olundu
22:55:00.964  Trading aktif!
22:55:01.469  İlk spread tespiti: UP $0.37 / DOWN $0.61
22:55:01.694  Batch emir gönderildi (225ms yanıt)
22:55:06.520  5 saniye sonra iptal
...           [23 döngü devam eder]...
22:59:00.053  Pencere bitimine 60 saniye kaldı → DUR
22:59:00.056  Market #131 tamamlandı
23:00:00.000  Market penceresi kapanır
```

---

## 7. Balance Factor Mekanizması

Balance factor (0.7), botun pozisyon dengesizliğini düzeltmek için emir boyutlarını nasıl ayarladığını kontrol eder.

### Formül

```
imbalance  = |UP_pozisyon − DOWN_pozisyon|
adjustment = round(imbalance × balance_factor × 0.5)

geride_kalan_taraf_emir = max_buy_order_size + adjustment
dominant_taraf_emir     = max_buy_order_size − adjustment
```

### Örnek — Döngü #9 (19:56:53)

```
UP  pozisyon: 54 share
DOWN pozisyon: 78 share
→ DOWN dominant (fazla var)
→ imbalance: 78 - 54 = 24

adjustment = round(24 × 0.7 × 0.5) = round(8.4) = 8

UP  emir boyutu: 20 + 8 = 28  (geride kalan, daha fazla al)
DOWN emir boyutu: 20 - 8 = 12  (dominant, az al)
```

### Balance Factor Değerlerinin Etkisi

| Değer | Davranış |
|---|---|
| `0.0` | Her zaman eşit boyut (20+20) — dengeleme yok |
| `0.5` | Orta agresiflik |
| `0.7` | **Mevcut ayar** — agresif dengeleme |
| `1.0` | Maksimum agresiflik — tüm fazlalığı kapatmaya çalışır |

### Döngü Boyunca Pozisyon Dengesi

```
Döngü  1: UP=0,   DOWN=11   → imbalance=11 (DOWN fazla)
Döngü  5: UP=45,  DOWN=45   → imbalance=0  (Mükemmel denge)
Döngü  9: UP=54,  DOWN=78   → imbalance=24 (DOWN fazla)
Döngü 11: UP=90,  DOWN=96   → imbalance=6  (İyileşiyor)
Döngü 14: UP=117, DOWN=126  → imbalance=9
Döngü 20: UP=181, DOWN=182  → imbalance=1  (Neredeyse tam denge)
FINAL:     UP=212, DOWN=211  → imbalance=1  ✅
```

---

## 8. Spread Tespiti ve Fiyatlama

### Spread Nasıl Hesaplanır?

Bot WebSocket'ten gelen `price_change` veya `price_snapshots` event'lerini dinler. Her güncelleme geldiğinde:

```
UP_spread   = UP_ask − UP_bid
DOWN_spread = DOWN_ask − DOWN_bid

Koşul: UP_spread >= spread_threshold (0.02)
       AND DOWN_spread >= spread_threshold (0.02)
```

Emir, **ask fiyatına** yerleştirilir (taker pozisyonu):
```
BUY UP   @ UP_ask
BUY DOWN @ DOWN_ask
```

### Tick Verisi Örneği (Gerçek WebSocket Datası)

22:55–23:00 penceresi sırasında yakalanan raw tick verisi (19:58:19 UTC):

```
Timestamp      UP Bid  UP Ask  DOWN Bid  DOWN Ask
1777579098938  $0.40   $0.41   $0.59     $0.60    spread: $0.01/$0.01
1777579099045  $0.38   $0.40   $0.60     $0.62    spread: $0.02/$0.02 ✓
1777579099079  $0.39   $0.41   $0.59     $0.61    spread: $0.02/$0.02 ✓
1777579099227  $0.38   $0.39   $0.61     $0.62    spread: $0.01/$0.01
1777579099294  $0.37   $0.39   $0.61     $0.63    spread: $0.02/$0.02 ✓
```

**Tick aralığı**: 40–100ms arası (çok hızlı orderbook değişiklikleri)

### Fiyat Aralığı Koruması

```
min_price = 0.15
max_price = 0.89
```

Bu aralığın dışındaki fiyatlarda emir gönderilmez. Döngü #14'te UP $0.78'e çıktığında:
- $0.78 < $0.89 → İzin verildi
- DOWN $0.20 > $0.15 → İzin verildi

Eğer UP $0.90'ı geçseydi bot o döngüde emir göndermeyecekti.

---

## 9. 22:55–23:00 Canlı Market Analizi

**Market:** `btc-updown-5m-1777578900`  
**Açıklama:** Bitcoin Up or Down — April 30, 3:55PM–4:00PM ET  
**Token ID'leri:**
- UP: `88463731335100312181810291660253...`
- DOWN: `68211412602176085058988438825427...`

### Tüm Trade Döngüleri

| # | Zaman (UTC) | UP | DOWN | Spread | Toplam | İmb. | UP Em. | DOWN Em. | UP Poz. | DOWN Poz. |
|---|---|---|---|---|---|---|---|---|---|---|
| 1  | 19:55:01 | $0.37 | $0.61 | $0.02 | $0.98 | 0  | 20 | 20 | 0   | 11  |
| 2  | 19:55:14 | $0.30 | $0.66 | $0.04 | $0.96 | 11 | 24 | 16 | 23  | 26  |
| 3  | 19:55:23 | $0.29 | $0.69 | $0.02 | $0.98 | 3  | 21 | 19 | 36  | 41  |
| 4  | 19:55:39 | $0.29 | $0.69 | $0.02 | $0.98 | 5  | 22 | 18 | 37  | 43  |
| 5  | 19:55:50 | $0.34 | $0.63 | $0.03 | $0.97 | 5  | 22 | 18 | 45  | 45  |
| 6  | 19:55:55 | $0.36 | $0.61 | $0.03 | $0.97 | 0  | 20 | 20 | 46  | 61  |
| 7  | 19:56:04 | $0.33 | $0.65 | $0.02 | $0.98 | 15 | 25 | 15 | 51  | 63  |
| 8  | 19:56:15 | $0.39 | $0.59 | $0.02 | $0.98 | 12 | 24 | 16 | 54  | 78  |
| 9  | 19:56:53 | $0.39 | $0.59 | $0.02 | $0.98 | 24 | 28 | 12 | 62  | 84  |
| 10 | 19:57:00 | $0.44 | $0.54 | $0.02 | $0.98 | 22 | 28 | 12 | 67  | 92  |
| 11 | 19:57:15 | $0.54 | $0.40 | $0.06 | $0.94 | 25 | 29 | 11 | 90  | 96  |
| 12 | 19:57:21 | $0.58 | $0.39 | $0.03 | $0.97 | 6  | 22 | 18 | 100 | 103 |
| 13 | 19:57:32 | $0.72 | $0.25 | $0.03 | $0.97 | 3  | 21 | 19 | 115 | 117 |
| 14 | 19:57:46 | $0.78 | $0.20 | $0.02 | $0.98 | 2  | 21 | 19 | 117 | 126 |
| 15 | 19:58:00 | $0.70 | $0.27 | $0.03 | $0.97 | 9  | 23 | 17 | 120 | 130 |
| 16 | 19:58:11 | $0.68 | $0.26 | $0.06 | $0.94 | 10 | 24 | 17 | 131 | 142 |
| 17 | 19:58:18 | $0.44 | $0.49 | $0.07 | $0.93 | 11 | 24 | 16 | 143 | 157 |
| 18 | 19:58:26 | $0.37 | $0.61 | $0.02 | $0.98 | 14 | 25 | 15 | 157 | 170 |
| 19 | 19:58:31 | $0.49 | $0.49 | $0.02 | $0.98 | 13 | 25 | 15 | 159 | 179 |
| 20 | 19:58:41 | $0.55 | $0.43 | $0.02 | $0.98 | 20 | 27 | 13 | 181 | 182 |
| 21 | 19:58:46 | $0.57 | $0.41 | $0.02 | $0.98 | 1  | 20 | 20 | 186 | 194 |
| 22 | 19:58:51 | $0.56 | $0.41 | $0.03 | $0.97 | 8  | 23 | 17 | 196 | 207 |
| 23 | 19:58:59 | $0.61 | $0.37 | $0.02 | $0.98 | 11 | 24 | 16 | 212 | 211 |

---

## 10. Fiyat Hareketi Anatomisi

Bu pencere son derece ilginç bir fiyat hareketi sergiledi:

### Faz 1 — DOWN Hakimiyeti (22:55:00–22:55:55)

```
UP: $0.29–$0.37    DOWN: $0.61–$0.69
```

Piyasa açılışta BTC'nin **düşeceğine güçlü şekilde inanıyordu**.  
DOWN fiyatı $0.69'a kadar çıktı — bu $0.69 pay için $1.00 kazanma şansı demek.

### Faz 2 — Geçiş (22:56:04–22:57:15)

```
UP: $0.33→$0.54    DOWN: $0.65→$0.40
```

Yavaş bir sentiment değişimi. Belki BTC'nin kısa sürede yön değiştirdiği an.  
Spread $0.06'ya genişledi — bu döngü en yüksek arbitraj marjını sundu ($0.06/share).

### Faz 3 — UP Zirve (22:57:21–22:58:11)

```
UP: $0.58→$0.78    DOWN: $0.39→$0.20
```

**Dramatik ters dönüş.** Piyasa artık BTC'nin çıkacağına %78 ihtimalle inanıyor.  
DOWN $0.20'ye geriledi — bu sadece $0.20 için $1.00 ödeme anlamına geliyor.  
Spread yine $0.06'ya genişledi (döngü #16).

### Faz 4 — Normalizasyon (22:58:18–22:59:00)

```
UP: $0.44→$0.61    DOWN: $0.37→$0.49
```

Denge noktasına dönüş. Döngü #17'de UP $0.44, DOWN $0.49 — toplam sadece **$0.93**, bu pencerenin **en iyi arbitraj fırsatı** ($0.07/share).

---

## 11. Kritik Sorunlar ve Riskler

### Sorun 1: Düşük Fill Oranı

```log
BUY UP 20 @ $0.37 → Pozisyon updated: UP=0   (fill yok)
BUY DOWN 20 @ $0.61 → Pozisyon updated: DOWN=11 (kısmi fill)
```

Döngü #1'de 20 share gönderildi, yalnızca 11'i doldu. Teorik olarak:
- UP fill olsaydı: +20 share UP
- DOWN fill olsaydı: +20 share DOWN
- Her ikisi $0.98 toplama → $0.02/share × 20 = $0.40 kâr

**Neden fill olmuyor?** Bot emirleri ask fiyatına yerleştiriyor. Ancak piyasa çok hızlı hareket ettiğinde ask değişiyor ve mevcut emirler artık en iyi fiyat olmaktan çıkıyor.

### Sorun 2: Tek Taraf Fill Riski

En tehlikeli senaryo:
```
Bot: BUY UP 20 @ $0.37 + BUY DOWN 20 @ $0.61
↓
Sadece DOWN fill oldu (11 share @ $0.61)
BTC yukarı gitti → DOWN = $0.00
Zarar: 11 × $0.61 = $6.71
```

Bu risk DRY RUN'da görünmüyor çünkü gerçek fill gerçekleşmiyor.

### Sorun 3: Market Bekleme Döngüsü Verimsizliği

```log
[19:54:06] Market #108: btc-updown-5m-1777578600 (50s kaldı → DUR)
[19:54:09] Market #109: btc-updown-5m-1777578600 (48s kaldı → DUR)
[19:54:11] Market #110: btc-updown-5m-1777578600 (46s kaldı → DUR)
...
[19:55:00] Market #131: btc-updown-5m-1777578900 (YENİ PENCERE!)
```

Yeni market oluşana kadar **130 gereksiz deneme** yapıldı.  
Her denemede: Gamma API çağrısı + WebSocket bağlantısı kurma + iptal.  
Bu hem gereksiz API yükü hem de sunucu kaynaklarının boşa harcanması demek.

### Sorun 4: Latency Dezavantajı

```
Dublin sunucu → Cloudflare DUB edge: ~1ms
Cloudflare DUB → Polymarket origin (ABD): ~80ms
Toplam round-trip: ~90ms
```

US East tabanlı rakipler ~10ms ile işlem yapabilir. 90ms vs 10ms farkı spread capture gibi hız bağımlı stratejilerde kritiktir.

### Sorun 5: 60s Stop Erken Girebilir

```
stop_before_end_ms = 60000
```

Bu pencerede son trade 22:58:59'da yapıldı ve 22:59:00'da durduruldu.  
Son 60 saniyede çoğu zaman fiyatlar daha netleşir — bu dönemi kaçırmak potansiyel fırsatları kaçırmak demektir.

---

## 12. Performans Metrikleri

### Bu Pencere (22:55–23:00)

| Metrik | Değer |
|---|---|
| Toplam trade döngüsü | 23 |
| Aktif trading süresi | 4 dakika (pencere: 5dk, son 1dk durduruldu) |
| Min spread | $0.02 |
| Max spread | $0.07 (döngü #17) |
| Min arbitraj marjı | $0.02/share |
| Max arbitraj marjı | $0.07/share (döngü #17) |
| Ortalama batch yanıt süresi | ~200ms |
| Min batch yanıt | 87ms (döngü #11) |
| Max batch yanıt | 327ms (döngü #19) |
| WebSocket bağlantı süresi | 142ms |
| Final UP pozisyon | 212 share |
| Final DOWN pozisyon | 211 share |
| Pozisyon dengesi | **99.5%** (neredeyse mükemmel) |

### Günlük Genel İstatistikler (30 Nisan)

| Metrik | Değer |
|---|---|
| Toplam spread tespiti | 2.299 |
| Toplam BUY emri | 6.897 |
| İptal edilen emirler | 4.522 |
| Gerçek fill sayısı | 0 (DRY RUN) |
| Log dosyası boyutu | 6.4 MB |
| İşlem başlangıcı | 10:18 UTC |

---

## 13. Geliştirme Önerileri

### Öncelik 1: Sunucu Migrasyonu (Yüksek Etki)

```
Mevcut:  Dublin (eu-west-1)    → Polymarket: ~90ms RTT
Hedef:   US East (us-east-1)  → Polymarket: ~10ms RTT
Kazanım: ~80ms latency azalması → Fill oranı artışı
```

### Öncelik 2: Market Bekleme Döngüsü Optimizasyonu

Mevcut davranış: Her 2 saniyede API çağrısı yaparak yeni marketi bekliyor.  
Önerilen: Bir sonraki pencere zamanını hesapla, tam o zamana kadar bekle.

```rust
// Mevcut (verimsiz)
loop {
    let market = fetch_market(slug).await;
    if market.window_end > now() { break; }
    sleep(2000).await;
}

// Önerilen
let next_window_start = calc_next_window(interval_minutes);
sleep_until(next_window_start - PRE_FETCH_BUFFER).await;
let market = fetch_market(slug).await;
```

### Öncelik 3: Fill Oranı İzleme

Şu an fill sayısı loglanmıyor. Pozisyon güncellemelerinden çıkarılabilir:

```
Fill oranı = (pozisyon_artışı / gönderilen_emir_boyutu) × 100
```

Bu veriyle hangi fiyat seviyelerinde daha fazla fill olduğu analiz edilebilir.

### Öncelik 4: Dinamik stop_before_end_ms

Sabit 60 saniye yerine, piyasa volatilitesine göre ayarlanabilir:
- Yüksek spread döneminde → daha geç dur (30s)
- Düşük spread döneminde → daha erken dur (90s)

### Öncelik 5: Post-Only Emirler

Taker yerine maker pozisyonu almak için `postOnly` flag kullanımı.  
Bu yaklaşım spread'i taker fee ödemeden yakalar, ancak fill garantisi azalır.

### Öncelik 6: DRY RUN'dan Canlıya Geçiş Checklist

```
□ min_price / max_price değerlerini gerçek risk toleransına göre ayarla
□ max_buy_order_size'ı mevcut cüzdan bakiyesine göre belirle
□ stop_before_end_ms'i optimize et
□ US East sunucusuna taşı
□ Küçük miktarla (1-2 USDC/emir) test et
□ Fill oranını ve P&L'i izle
□ dry_run: false yap
```

---

## Ek — Kimlik Doğrulama Akışı

Bot Polymarket CLOB API'sine Ethereum EIP-712 imzasıyla kimlik doğrular:

```
1. Private Key (cüzdan) → .env dosyasında saklanır
2. POST /auth/derive-api-key → API Key türetilir
3. Her emir Order struct oluşturulur:
   Order(salt, maker, signer, taker, tokenId,
         makerAmount, takerAmount, expiration,
         nonce, feeRateBps, side, signatureType)
4. EIP-712 hash → Private key ile imzalanır
5. İmzalı emir CLOB API'ye gönderilir
```

Desteklenen chain: **Polygon (Chain ID: 137)**

---

*Doküman 30 Nisan 2026 tarihinde `ubuntu@54.170.6.194` sunucusundan canlı log analizi, binary incelemesi ve gerçek zamanlı market izleme yoluyla hazırlanmıştır.*
