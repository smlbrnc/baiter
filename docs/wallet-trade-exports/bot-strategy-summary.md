# Bot Stratejisi Ters Mühendislik — 6 BTC 5dk Market

**Cüzdan**: `0xb27bc932bf8110d8f78e55da7d5f0497a18b5b82`

**Market sayısı**: 6 (hepsi BTC 5 dakika up/down)

> **En ince ayrıntıya inen tam rapor**: [`bot-deep-analysis.md`](./bot-deep-analysis.md) — 991 satır,
> her market için: cluster (sweep) yapısı, karar aralıkları, yön değişimleri, hedge ekonomisi
> (UP+DOWN VWAP toplamı = kilitlenmiş kar oranı), spread × konum çapraz tablosu, fiyat seviyesi
> haritası, 60-sn dilim aktivitesi, ±10sn fiyat hareketi (zamanlama isabeti), skor bant tepkisi.


## 0. Halk Diliyle Bot Mantığı

### Bot bir cümleyle ne yapıyor?

> _"5 dakikalık BTC up/down marketinde her iki tarafa da bahis koyuyor; trend belirleninceye kadar dengeli alıyor, sonra kazanan tarafa yükleniyor, kaybeden tarafta ise ucuzlayan fiyatlardan piyango bileti topluyor. Hiçbir şey satmıyor — sadece alıyor, ne olursa olsun marketi sonuna kadar bekliyor."_

### Marketin başından sonuna ne yapıyor (5 dakika)?

**1. Dakika (T+5..T+25 saniye) — Açılış:**
- Market açılır açılmaz hızla iki tarafa da küçük bahisler giriyor.
- Henüz trend belli değil — UP fiyatı 0.50 civarındaysa hem UP hem DOWN alıyor.
- "Riskten ödü kopmuş" değil; başlangıçta cesaretli giriyor.

**2. Dakika — Yön gözleme:**
- Hangi taraf 5-10 sent öne geçtiyse oraya daha çok yatırıyor.
- Tick fiyatı yukarı kıpırdadığında UP, aşağı kıpırdadığında DOWN alıyor — yani trend takipçi.
- Aynı saniyede 10-25 işlem yapabiliyor (insan eli imkansız → algoritma).

**3-4. Dakika — En yoğun zaman:**
- Alımların yarısından çoğu burada gerçekleşiyor.
- Trend kesinleşmişse: kazanan tarafa pahalı bile olsa (UP @ 0.85+) yükleniyor.
- Aynı anda kaybeden tarafa ucuzlayan fiyattan (DOWN @ 0.05-0.10) piyango bileti gibi küçük bahisler atıyor.
- Tek bir büyük emri, fiyat seviyelerinin üstünde "merdiven gibi" doldurabiliyor (örn. 0.50-0.51-0.52-...-0.81 ardışık fiyatlardan tek nefeste).

**5. Dakika — Sonuç:**
- Genelde küçük tutarlı son alımlar (DOWN @ 1-3 sent gibi piyango).
- Pozisyon kapatma yok — ne kazandıysa kazanır, ne kaybettiyse kaybeder.

### Niye böyle yapıyor olabilir?

| Davranış | Sebep (tahmin) |
|----------|---------------|
| Hep BUY, hiç SELL | Polymarket'te 5dk market kısa — likidite az; çıkmak yerine sona kadar bekliyor (resolve etsin). |
| İki tarafa giriş | Hangi yön kazanırsa kazansın bir miktar kazanç. "Ucuz tarafı al + pahalı tarafı az al" karması net pozitif olabilir. |
| Düşük fiyatlı DOWN/UP "piyangoları" (≤0.10) | Maliyet 5-10 sent, kazanırsa $1 → 10x-20x getiri. Volatilitenin küçük şokunda bile kazandırır. |
| Yüksek fiyatlı favori (≥0.85) | Risk neredeyse yok ama kazanç da küçük (15 sent kâr). Kesin sonucu zaten bilen biri için "garanti gibi" alım. |
| Aynı saniyede 10-25 işlem | Order book'ta sırada bekleyen büyük tek bir emir — birden fazla seviyede dolduruluyor. Tek bir karar = onlarca trade. |
| Multi-fill (aynı tx içinde birden fazla fill) | Büyük taker emir order book'u "süpürüyor" (sweep). Hızlı alım, fiyat ne olursa olsun. |
| Trend takibi (çoğu markette) | Tick fiyatı belirli yöne hareket edince o yöne yükleniyor. "Hareket var, devam edebilir" mantığı. |

### Bizim Alis ile farkı (basit dille)

| Konu | Alis (bizim bot) | Bu cüzdanın botu |
|------|------------------|------------------|
| Hedef | Kâr kilidi (profit-lock) — risk kapat ve çık | Sona kadar bekle, ne çıkarsa kâr |
| Satış | Var — pozisyon kapatabilir | Yok — sadece BUY |
| Faz | DeepTrade/AggTrade gibi katı zamanlama | Faz yok, sürekli ve trend-tepkili akış |
| Hedge | Açılışta dengeli, sonra averaj/pyramid | Açılışta dengeli, sonra trende yüklenme + piyango |
| Risk yönetimi | Avg-down + profit-lock kuralları | "Dağıt ve sona kadar bekle" — risk pozisyona dağıtılmış |

### İki versiyon görüyoruz

- **Agresif sürüm** (M1-M3): işlem başına $10-15, tek bahse $90-97 kadar. Toplam $2k-12k harcıyor.
- **Sıkı bütçeli sürüm** (M4-M6): işlem başına ~$2, max $5. Toplam $300-650. Sabit 5.00 lot kullanıyor.

Aynı cüzdan ama büyük ihtimalle sahibi botu daha sonra daha küçük bütçeyle yeniden ayarlamış (ya da başka bir bot ile çalışıyor).

### Alım fiyatı tick spread'in neresine denk geldi? (2,155 eşleşen trade)

Trade fiyatlarını tick anındaki bid/ask seviyeleriyle 1 sent (1¢) hassasiyetinde karşılaştırdığımızda:

| Konum | Trade | Oran | Anlamı |
|-------|-------|------|--------|
| **ASK üstü** (ask+1, +2, +3...) | 597 | **27.7%** | Agresif taker — order book'u "süpürüyor" (sweep). Tek emir 3-5 fiyat seviyesini birden alıyor. |
| **ASK tam** | 350 | **16.2%** | Klasik taker — best ask'i vurup alıyor. |
| **INSIDE** (bid < p < ask) | 37 | 1.7% | Çok nadir — neredeyse hep bid veya ask'te. |
| **BID tam** | 411 | **19.1%** | Maker — bot'un bid'de bekleyen GTC emri dolduruldu. |
| **BID-1** (1¢ altı) | 286 | **13.3%** | Bid'in 1¢ altında resting emir → "ucuza yakalama". |
| **BID-2** (2¢ altı) | 142 | 6.6% | Bid'in 2¢ altı, daha derin. |
| **BID-3+** (3¢+ altı) | 332 | **15.4%** | Çok derin resting bid'ler — fiyat aşağı çekildiğinde dolan tuzaklar. |

**Toplam:**
- **Taker davranış (≥ ask)**: %43.9 — agresif piyasa alımı
- **Maker davranış (≤ bid)**: %54.4 — pasif resting bid emirleri
- **Inside**: %1.7

### Halk diliyle: bot iki tip emir karışık kullanıyor

**1. Resting maker bid'ler (~%54)** — "bid'den ya da daha aşağıdan ucuza alma"
- Bot, mevcut best bid fiyatından veya 1-3 sent altından order book'a GTC emir bırakıyor.
- Fiyat hafif aşağı çekildiğinde bu emirler doluyor ve bot ucuza pozisyon ediniyor.
- Örnek: best bid 0.45 ise, bot 0.45 / 0.44 / 0.43 / 0.42 seviyelerine küçük emirler dağıtıyor.
- Bu yüzden "bid'in 3+ sent altı" kategorisinde bile %15.4 trade var → bot likidite sağlıyor + indirim avlıyor.

**2. Agresif taker sweep'ler (~%44)** — "ask veya üstünden hemen alma"
- Trend hızlanınca veya yön kesinleşince, bot pasif beklemiyor — best ask'i vurup alıyor.
- "ASK üstü" kategorisi (%27.7) çok dikkat çekici: bot bazen tek emirle order book'taki birkaç fiyat seviyesini birden tüketiyor (örn. 0.50 → 0.51 → ... → 0.55 ardışık doluyor).
- Bu "book sweep" davranışı — büyük marketable emirler (taker FAK).

**3. Inside spread (%1.7) yok denecek kadar az**
- Bot orta fiyat (mid) alımı yapmıyor → ya bid/altı bekliyor ya da ask/üstü vuruyor.
- "Limit order at mid" gibi yumuşak bir strateji kullanmıyor.

### Market başına dağılım

| Market | ASK üstü | ASK | BID | BID altı | Karakter |
|--------|----------|-----|-----|----------|----------|
| M1 (1776969300) | 26.0% | 17.6% | 25.9% | 28.3% | Dengeli karışım |
| M2 (1776992100) | 24.8% | 10.0% | 14.9% | 48.1% | **Maker ağırlıklı** (derin bid'ler) |
| M3 (1777050900) | 17.9% | 40.8% | 19.7% | 21.1% | **Taker ağırlıklı** (klasik ask alımı) |
| M4 (1777125600) | 25.5% | 17.0% | 23.9% | 32.4% | Dengeli karışım |
| M5 (1777125900) | 26.4% | 17.1% | 29.3% | 22.1% | Bid yoğun |
| M6 (1777132200) | 39.0% | 11.9% | 11.6% | 35.8% | **En agresif sweep** + derin bid |

### Pratik çıkarım

> Bot kesinlikle **iki paralel emir akışı** çalıştırıyor:
> 1. **GTC maker bid'ler** order book'a yayılı (best bid ve 1-5¢ altı seviyelere) → ucuz fırsatları yakala
> 2. **Taker FAK sweep'ler** trend hızlandığında → hemen pozisyon edin
>
> "Inside spread alımı" yok denecek kadar az olması, bot'un mid fiyat hesabı yapmadığını, sadece bid ve ask seviyelerinde işlem ürettiğini gösteriyor.

### Bot kararları kaç saniye aralıklarla geliyor? (1,916 karar, tx bazında)

Aynı tx içindeki onlarca fill'i tek "karar" olarak saydığımızda (sweep = 1 karar):

| Karar aralığı | Adet | Oran |
|---------------|------|------|
| **0-1 saniye** (anlık ardışık) | 1,451 | **75.7%** |
| 1-2 saniye | 0 | 0.0% |
| 2-5 saniye | 406 | **21.2%** |
| 5-10 saniye | 39 | 2.0% |
| 10-30 saniye | 17 | 0.9% |
| 30+ saniye | 3 | 0.2% |

- **Medyan: 0 sn** | **Ortalama: 0.8 sn** | **P90: 2 sn**
- Bot ya **aynı saniyede** birden fazla karar veriyor (75.7%, çoklu paralel emir), ya da **2-5 sn** sonra yeni karar (21.2%).
- "1 saniye" gap **hiç yok** → bu çok önemli: timestamp resolution 1 sn olduğu için, "aynı saniyede" hepsi 0 olarak görünüyor; gerçek aralıklar **0 sn ya da ≥2 sn**.
- Bu **2 saniyelik döngü** botun ana karar tick'i olabilir (her 2 sn'de bir tick okuyup karar veriyor).
- M5 hariç (medyan 2 sn) tüm marketler hızlı: **küçük bütçeli M5 botu daha yavaş** (~2-3 sn karar aralığı).

### Bot, spread genişleyince emir fiyatını değiştiriyor mu? (EVET — açıkça spread'e adapte oluyor)

Önce spread dağılımına bakalım — markette zaman içinde spread nasıl?

| Spread genişliği | Trade sayısı | Oran |
|------------------|-------------|------|
| **Dar (1¢)** | 2,008 | **93.2%** |
| Orta (2¢) | 95 | 4.4% |
| Geniş (3-4¢) | 29 | 1.3% |
| Çok geniş (5¢+) | 23 | 1.1% |

Spread genişliği değiştikçe bot'un trade fiyatı konumu nasıl değişiyor? (her satır içinde %)

| Spread | ASK üstü (sweep) | ASK | INSIDE | BID | BID-1 | BID-2 | BID-3+ |
|--------|------------------|-----|--------|-----|-------|-------|--------|
| **Dar (1¢)** | 27.4% | 16.7% | 0.0% | 19.3% | 14.0% | 6.8% | **15.7%** |
| **Orta (2¢)** | 31.6% | 10.5% | **15.8%** | 18.9% | 3.2% | 6.3% | 13.7% |
| **Geniş (3-4¢)** | 24.1% | 10.3% | **48.3%** | 13.8% | 3.4% | 0.0% | 0.0% |
| **Çok geniş (5¢+)** | **39.1%** | 4.3% | **34.8%** | 8.7% | 0.0% | 0.0% | 13.0% |

Bid-altı maker emirlerin **derinliği** (bid-N için ortalama N, sent cinsinden):

| Spread bucket | Maker derinlik (medyan) | Taker sweep derinliği (medyan, ask+N) |
|---------------|-------------------------|---------------------------------------|
| Dar (1¢) | 2.0¢ altı | 2.0¢ üstü |
| Orta (2¢) | 3.5¢ altı | **5.5¢ üstü** (daha agresif) |
| Geniş (3-4¢) | — (sadece 1 trade) | 2.0¢ üstü |
| Çok geniş (5¢+) | **11.0¢ altı** (çok temkinli) | 3.0¢ üstü |

### Halk diliyle: bot spread'e nasıl adapte oluyor

**1. Dar spread (1¢) — normal koşullar:**
- Bot **bid'e yakın resting maker** koyuyor (bid, bid-1, bid-2 toplam %40)
- INSIDE alımı **YOK** (%0) — çünkü zaten bid+1 = ask, "arada" bir yer yok
- Aynı zamanda taker sweep de yapıyor (%27.4 ASK üstü)

**2. Spread genişlemeye başladığında (2¢ → 3-4¢):**
- BID-1 alımları **ciddi düşüyor** (%14 → %3) → bot artık bid'in 1¢ altında resting bırakmıyor
- INSIDE alımları **patlıyor** (%0 → %15 → **%48**) → bot artık spread'in **ortasına limit order** koyuyor
- Yani bot mantığı: "spread daraldı bid'in 1 altına koy, spread genişledi ortasına koy"

**3. Çok geniş spread (5¢+) — panik veya fırsat:**
- Maker derinlik **11¢'e kadar açılıyor** (çok temkinli, çok ucuza al)
- Aynı zamanda agresif TAKER sweep oranı **%39.1'e fırlıyor** (en yüksek) → "spread çok açıldı, hemen al"
- Yani bot **iki uca da** kayıyor: ya çok ucuza bekle ya da hemen vur

### Pratik çıkarım — bot adaptif

> **EVET, bot spread'e tepki veriyor ve emir fiyatını yeniden hesaplıyor.**
>
> - Dar spread'de: bid'e yapışık veya 1-2¢ altı resting GTC
> - Spread genişlerse: bid-altı emirleri iptal edip **mid'e yakın** (inside spread) yeni emir
> - Spread çok genişlerse: **bid'in 10+¢ altı** çok temkinli emir + agresif TAKER sweep paralel
>
> Bu, klasik bir **adaptive market-making + opportunistic taker** kombinasyonu.

---


## 1. Genel Tablo

| # | Slug suffix | Trade | UP/DN | Toplam $ | Ort/işlem | Max $ | Max burst | Sync hedge sn |
|---|-------------|-------|-------|----------|-----------|-------|-----------|---------------|
| M1 | ...1776969300 | 388 | 166/222 | $3,931.66 | $10.13 | $90.00 | 22/sn | 14 |
| M2 | ...1776992100 | 787 | 450/337 | $12,047.72 | $15.31 | $97.00 | 25/sn | 55 |
| M3 | ...1777050900 | 219 | 137/82 | $2,150.57 | $9.82 | $96.00 | 19/sn | 5 |
| M4 | ...1777125600 | 306 | 149/157 | $586.44 | $1.92 | $4.40 | 15/sn | 30 |
| M5 | ...1777125900 | 140 | 65/75 | $282.92 | $2.02 | $4.90 | 8/sn | 6 |
| M6 | ...1777132200 | 319 | 160/159 | $651.50 | $2.04 | $4.90 | 14/sn | 19 |

## 2. Bot Versiyonu Tespiti

- **Büyük emir profili** (3 market): max > $20

  - `btc-updown-5m-1776969300`: ort $10.13, max $90.00, toplam $3,931.66
  - `btc-updown-5m-1776992100`: ort $15.31, max $97.00, toplam $12,047.72
  - `btc-updown-5m-1777050900`: ort $9.82, max $96.00, toplam $2,150.57
- **Küçük emir profili** (3 market): max ≤ $20

  - `btc-updown-5m-1777125600`: ort $1.92, max $4.40, toplam $586.44
  - `btc-updown-5m-1777125900`: ort $2.02, max $4.90, toplam $282.92
  - `btc-updown-5m-1777132200`: ort $2.04, max $4.90, toplam $651.50

_Hipotez: bot konfigürasyonu zamanla değişmiş veya iki farklı bot çalışmış._


## 3. Zaman Dilimi (1dk eşit) — Trade Sayısı

| Dilim | M1 | M2 | M3 | M4 | M5 | M6 | Toplam |
|-------|---|---|---|---|---|---|---|
| 0-60s | 24 | 120 | 36 | 75 | 22 | 51 | 328 |
| 60-120s | 59 | 252 | 32 | 74 | 19 | 149 | 585 |
| 120-180s | 139 | 152 | 112 | 72 | 38 | 83 | 596 |
| 180-240s | 151 | 195 | 39 | 69 | 50 | 34 | 538 |
| 240-300s | 15 | 38 | 0 | 16 | 11 | 2 | 82 |

## 4. Zaman Dilimi — Hacim ($)

| Dilim | M1 | M2 | M3 | M4 | M5 | M6 | Toplam |
|-------|---|---|---|---|---|---|---|
| 0-60s | 281 | 1,716 | 470 | 145 | 39 | 97 | 2,749 |
| 60-120s | 945 | 2,988 | 462 | 137 | 35 | 286 | 4,853 |
| 120-180s | 1,180 | 2,533 | 910 | 120 | 81 | 171 | 4,996 |
| 180-240s | 1,480 | 2,597 | 309 | 145 | 74 | 93 | 4,698 |
| 240-300s | 46 | 871 | 0 | 39 | 53 | 5 | 1,013 |

## 5. Tick Reaksiyon (10s lookback) — Trend Takip Oranı

| Slug | UP follow % | DN follow % | Karakter |
|------|------------|-------------|----------|
| ...1776969300 | 37% | 75% | Trend takipçi |
| ...1776992100 | 46% | 45% | Trend takipçi |
| ...1777050900 | 18% | 37% | Kontrarian |
| ...1777125600 | 30% | 66% | Trend takipçi |
| ...1777125900 | 41% | 77% | Trend takipçi |
| ...1777132200 | 29% | 50% | Nötr |

## 6. Maker / Taker Oranı (her market)

| Slug | Eşleşen | Maker % | Taker % | Inside % | Spread içi % |
|------|---------|---------|---------|----------|--------------|
| ...1776969300 | 386 | 54.4% | 43.5% | 2.1% | 41.5% |
| ...1776992100 | 787 | 60.0% | 37.9% | 2.2% | 25.0% |
| ...1777050900 | 218 | 40.8% | 58.7% | 0.5% | 54.6% |
| ...1777125600 | 306 | 55.2% | 43.5% | 1.3% | 40.8% |
| ...1777125900 | 140 | 53.6% | 41.4% | 5.0% | 50.7% |
| ...1777132200 | 318 | 49.1% | 50.9% | 0.0% | 23.0% |

## 7. Eşik Alımları (her market)

| Slug | DN ≤0.10 | DN ≤0.20 | UP ≥0.85 | UP ≥0.70 |
|------|---------|---------|---------|---------|
| ...1776969300 | 111 | 209 | 69 | 107 |
| ...1776992100 | 0 | 0 | 34 | 34 |
| ...1777050900 | 0 | 0 | 0 | 0 |
| ...1777125600 | 0 | 24 | 8 | 45 |
| ...1777125900 | 9 | 30 | 19 | 30 |
| ...1777132200 | 0 | 0 | 0 | 7 |

## 8. Çıkarılan Bot Kuralları (genel)

1. **%100 BUY**: 2159 trade'in tamamı BUY (BUY=2159, SELL=0). Pozisyon kapatma yok — scaling-in mantığı.
2. **Hedge dengesi**: ortalama %51 UP / %49 DN. Dengeli iki yön.
3. **Algoritmik tetikleme**: ortalama max burst 17 trade/sn, ortalama 28 burst saniyesi. Volatiliteye duyarlı bot.
4. **Emir bölünmesi**: ortalama 27 multi-fill tx/market. Büyük emirler kısmen dolduruluyor.
5. **Eşzamanlı hedge**: ortalama 22 saniyede aynı anda UP+DOWN alımı. Çift yön emir akışı.
6. **Eşik alımları**: toplam 120 lottery DOWN (≤0.10), 130 favori UP (≥0.85). Düşük fiyat hedge + yüksek fiyat takip karması.
7. **Emir tipi**: ortalama %46 taker. Karışık maker/taker.

## 9. Genel Davranış Karakteri

Bu cüzdanın botu, BTC 5dk marketlerde **scaling-in + iki yön hedge** stratejisi uyguluyor:

- Market açılışından sonra (T+5..T+25s) hızla iki tarafa pozisyon açıyor.
- 2-3. dakikada en yoğun aktivite — burada hem hedge hem scaling-in görülüyor.
- Burst saniyeleri (10-25 trade/sn) volatilite anlarına denk geliyor — kesinlikle algoritmik tetikleme.
- DOWN tarafında düşük fiyat (≤0.10) hedge alımları belirgin (lottery ticket mantığı).
- UP tarafında yüksek fiyat (≥0.85) favori takibi görülüyor (tek yön kazanan kesinleştiğinde).
- Multi-fill oranı yüksek → emirler büyük ve kısmen dolduruluyor (taker FAK veya GTC parça parça).
- Bot iki versiyonda görünüyor: eski/agresif (büyük emir, M1-M3) ve yeni/sıkı bütçe (küçük emir, M4-M6).

**Bizim Alis stratejisi ile karşılaştırma**: Alis profit-lock ve faz tabanlı (DeepTrade/AggTrade) çalışırken, bu bot daha sürekli ve trend-tepkili bir scaling-in yapıyor. Profit-lock benzeri bir kapatma davranışı görülmüyor (hep BUY).


## 10. Detay Raporlar

- [btc-updown-5m-1776969300](detail-btc-updown-5m-1776969300.md)
- [btc-updown-5m-1776992100](detail-btc-updown-5m-1776992100.md)
- [btc-updown-5m-1777050900](detail-btc-updown-5m-1777050900.md)
- [btc-updown-5m-1777125600](detail-btc-updown-5m-1777125600.md)
- [btc-updown-5m-1777125900](detail-btc-updown-5m-1777125900.md)
- [btc-updown-5m-1777132200](detail-btc-updown-5m-1777132200.md)
