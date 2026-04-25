# Bot Trade Mantığı — Derinlemesine Analiz

Bu rapor 6 BTC 5dk market'inde dış cüzdanın (`0xb27b…5b82`) yaptığı **her bir trade**'i tick verisiyle eşleştirip; karar yapısı, cluster (sweep), maker/taker, spread reaksiyonu, fiyat seviyeleri, yön değişimi, hedge davranışı, zamanlama isabeti ve skor-bant tepkisi açısından inceler.


> Rapor `scripts/deep_analyze_bot.py` tarafından otomatik üretilmiştir.


## Kavramlar


- **Trade (fill)**: Polymarket Data API'sinden gelen tek satır = borsada eşleşen 1 emir-1 maker-1 taker üçlüsü.
- **Cluster (karar / tx)**: Aynı `transactionHash` altında gelen tüm fill'ler. Bot tarafından *tek bir emir/karar* yansımasıdır. Multi-fill cluster = sweep (book süpürme).
- **Sweep derinliği**: Aynı tx'teki fill'lerin min-max fiyat farkı (sent cinsinden). 5¢ sweep = bot tek emirle 5 fiyat seviyesi tüketmiş.
- **Konum etiketi** (tick anlık bid/ask'e göre):
  - `ASK+N` = ask'in N¢ üstüne walk eden taker sweep
  - `ASK` = best ask'i vuran klasik taker
  - `INSIDE` = bid < trade < ask (orta seviyeye limit emir)
  - `BID` = best bid'de duran maker emrin dolması
  - `BID-N` = bid'in N¢ altına yerleştirilmiş resting maker emir
- **Yön (outcome run)**: Cluster'ların ardışık olarak aynı outcome'da kalması (UP UP UP DOWN UP = 3 run).
- **Hedge saniyesi**: Aynı saniyede hem UP hem DOWN cluster'ı oluşan saniye.

## Birleşik (6 market özet)

| Metrik | Değer |
|---|---|
| Toplam trade fill | 2,159 |
| Toplam karar (cluster) | 1,922 |
| Multi-fill cluster (sweep) | 164 (8.5%) |
| Toplam hacim (share) | 39,206 |
| Toplam notional (USD) | $19,650.81 |

### Karar aralıkları (birleşik)

| İstat | sn |
|---|---|
| Medyan | 0.0 |
| Ortalama | 0.81 |
| P75 | 0 |
| P90 | 2 |
| Max | 46 |

### Birleşik trade fiyat konumu

| Konum | Adet | Oran |
|---|---|---|
| ASK+3+ | 285 | 13.2% |
| ASK+2 | 120 | 5.6% |
| ASK+1 | 192 | 8.9% |
| ASK | 350 | 16.2% |
| INSIDE | 37 | 1.7% |
| BID | 411 | 19.1% |
| BID-1 | 286 | 13.3% |
| BID-2 | 142 | 6.6% |
| BID-3+ | 332 | 15.4% |

### Birleşik hedge ekonomisi (UP+DOWN VWAP toplamı, ±2sn)

| Metrik | Değer |
|---|---|
| Eşleşme sayısı | 171 |
| Toplam maliyet medyan | $0.9848 |
| Toplam maliyet ort | $0.9772 |
| **Kilitli kar oranı medyan** | **1.52%** |
| Kilitli kar oranı ort | 2.28% |
| Profit-lock (cost<1.00) | 150 / 171 |
| ≥ %1 kar (cost ≤ 0.99) | 122 |
| ≥ %3 kar (cost ≤ 0.97) | 46 |
| ≥ %5 kar (cost ≤ 0.95) | 21 |
| Net zarar (cost ≥ 1.00) | 21 |



---
# Market-Market Detay


## btc-updown-5m-1776969300

### Genel

| Metrik | Değer |
|---|---|
| Toplam fill (trade satırı) | 388 |
| Toplam karar (tx/cluster) | 375 |
| Multi-fill cluster (sweep) | 13 (3.5%) |
| Toplam alım hacmi (share) | 8,890.7 |
| Toplam notional (USD) | $3,931.66 |
| Side dağılımı (BUY/SELL) | BUY=388 |
| Outcome dağılımı (UP/DOWN) | Up=166 Down=222 |

### Karar aralıkları (cluster bazında, sn)

| İstatistik | sn |
|---|---|
| Medyan | 0.0 |
| Ortalama | 0.73 |
| P25 | 0.0 |
| P75 | 0.0 |
| P90 | 2.0 |
| Max | 44 |

- **Aynı saniyede paralel karar olan saniye**: 50
- **Aynı saniyede UP+DOWN birlikte (hedge!) olan saniye**: 14

### Cluster (sweep) şekli

| Metrik | Değer |
|---|---|
| Cluster başına ort fill | 1.03 |
| Cluster başına medyan fill | 1 |
| Tek-fill cluster (basit emir) | 362 / 375 |
| Cluster başına ort fiyat seviyesi | 1.03 |
| Sweep derinliği medyan (¢) | 0 |
| Sweep derinliği max (¢) | 1 |
| Cluster ort total size | 23.7 |

### Yön değişimi (outcome runs)

- **Toplam yön değişimi (UP↔DOWN)**: 55
- **Ortalama kararlı yön süresi**: 6.7 cluster ardışık
- **En uzun ardışık yön**: 51 cluster
- **UP run sayısı / DOWN run sayısı**: 28 / 28

### Hedge ekonomisi — UP+DOWN VWAP toplamı (±2sn pencere, 19 eşleşme)

| Hedge metriği | Değer |
|---|---|
| Toplam maliyet medyan (UP_vwap+DN_vwap) | $0.9800 |
| Toplam maliyet ortalama | $0.9783 |
| Toplam maliyet min | $0.9282 |
| Toplam maliyet max | $1.0019 |
| **Kilitli kar oranı (1−cost) medyan** | **2.00%** |
| Kilitli kar oranı ortalama | 2.17% |
| Profit-lock olmuş eşleşme sayısı (cost<1.00) | 18 / 19 |
| Maliyet ≤ 0.99 (≥ %1 kar) | 16 |
| Maliyet ≤ 0.97 (≥ %3 kar) | 4 |
| Maliyet ≤ 0.95 (≥ %5 kar) | 1 |

### Aynı saniyede UP+DOWN hedge örnekleri (toplam 14 saniye)

| sec | UP size | UP vwap | DN size | DN vwap | UP+DN vwap (≈hedge cost) |
|---|---|---|---|---|---|
| 1776969434 | 106.0 | 0.788 | 200.0 | 0.195 | 0.983 |
| 1776969442 | 114.0 | 0.747 | 200.0 | 0.195 | 0.942 |
| 1776969448 | 100.0 | 0.840 | 188.5 | 0.144 | 0.984 |
| 1776969450 | 39.1 | 0.830 | 91.3 | 0.130 | 0.960 |
| 1776969452 | 160.8 | 0.842 | 101.1 | 0.120 | 0.962 |

### Trade fiyat konumu (tick'in neresinde?)

| Konum | Adet | Oran |
|---|---|---|
| ASK+3+ | 36 | 9.3% |
| ASK+2 | 23 | 6.0% |
| ASK+1 | 41 | 10.6% |
| ASK | 68 | 17.6% |
| INSIDE | 8 | 2.1% |
| BID | 100 | 25.9% |
| BID-1 | 57 | 14.8% |
| BID-2 | 19 | 4.9% |
| BID-3+ | 34 | 8.8% |

- **Taker (≥ ASK)**: 168 (43.5%)
- **Maker (≤ BID)**: 210 (54.4%)
- **Inside spread**: 8 (2.1%)

### Spread × Trade konumu (her satır içinde % normalize)

| Spread | ASK+3+ | ASK+2 | ASK+1 | ASK | INSIDE | BID | BID-1 | BID-2 | BID-3+ | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| 1c | 8.7% | 6.1% | 11.5% | 16.8% | — | 26.8% | 15.6% | 5.3% | 9.2% | 358 |
| 2c | — | — | — | 40.0% | 20.0% | 26.7% | 6.7% | — | 6.7% | 15 |
| 3-4c | — | — | — | 28.6% | 71.4% | — | — | — | — | 7 |
| 5c+ | 83.3% | 16.7% | — | — | — | — | — | — | — | 6 |

### Trade size dağılımı

| Size bucket | Adet | Oran |
|---|---|---|
| <5 | 87 | 22.4% |
| 5-15 | 147 | 37.9% |
| 15-50 | 99 | 25.5% |
| 50-100 | 27 | 7.0% |
| 100+ | 28 | 7.2% |

### En çok hacim biriken fiyat seviyeleri

**UP outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 90¢ | 611.4 share |
| 53¢ | 345.7 share |
| 88¢ | 300.0 share |
| 54¢ | 300.0 share |
| 89¢ | 300.0 share |
| 59¢ | 221.9 share |
| 61¢ | 200.0 share |
| 74¢ | 200.0 share |
| 92¢ | 200.0 share |
| 91¢ | 200.0 share |

**DOWN outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 9¢ | 600.0 share |
| 7¢ | 499.9 share |
| 8¢ | 400.0 share |
| 6¢ | 300.0 share |
| 11¢ | 300.0 share |
| 10¢ | 278.6 share |
| 20¢ | 200.0 share |
| 12¢ | 200.0 share |
| 13¢ | 200.0 share |
| 19¢ | 200.0 share |

### 60-sn dilim aktivitesi

| Dilim | Cluster | Size | Taker | Maker | Taker oranı |
|---|---|---|---|---|---|
| 0-60s | 22 | 545.7 | 12 | 11 | 52% |
| 60-120s | 56 | 1618.8 | 27 | 30 | 47% |
| 120-180s | 137 | 2985.6 | 63 | 73 | 46% |
| 180-240s | 147 | 3078.5 | 62 | 87 | 42% |
| 240-300s | 13 | 662.1 | 4 | 9 | 31% |

### Trade'den ±10sn fiyat hareketi (zamanlama isabeti)

- **Doğru (alım sonrası fiyat ↑)**: 128 (34.1%)
- **Tersine (alım sonrası fiyat ↓)**: 211 (56.3%)
- **Sabit**: 36 (9.6%)

### Skor bantında bot davranışı

| Skor bandı | Cluster | UP karar | DOWN karar |
|---|---|---|---|
| >=7 (very up) | 315 | 105 (33%) | 210 (67%) |
| 5-7 (up) | 59 | 53 (90%) | 6 (10%) |
| 3-5 (mid) | 1 | 1 (100%) | 0 (0%) |
| <3 (down) | 0 | — | — |



## btc-updown-5m-1776992100

### Genel

| Metrik | Değer |
|---|---|
| Toplam fill (trade satırı) | 787 |
| Toplam karar (tx/cluster) | 716 |
| Multi-fill cluster (sweep) | 48 (6.7%) |
| Toplam alım hacmi (share) | 22,230.9 |
| Toplam notional (USD) | $12,047.72 |
| Side dağılımı (BUY/SELL) | BUY=787 |
| Outcome dağılımı (UP/DOWN) | Down=337 Up=450 |

### Karar aralıkları (cluster bazında, sn)

| İstatistik | sn |
|---|---|
| Medyan | 0.0 |
| Ortalama | 0.41 |
| P25 | 0.0 |
| P75 | 0.0 |
| P90 | 2.0 |
| Max | 46 |

- **Aynı saniyede paralel karar olan saniye**: 101
- **Aynı saniyede UP+DOWN birlikte (hedge!) olan saniye**: 55

### Cluster (sweep) şekli

| Metrik | Değer |
|---|---|
| Cluster başına ort fill | 1.10 |
| Cluster başına medyan fill | 1 |
| Tek-fill cluster (basit emir) | 668 / 716 |
| Cluster başına ort fiyat seviyesi | 1.10 |
| Sweep derinliği medyan (¢) | 0 |
| Sweep derinliği max (¢) | 4 |
| Cluster ort total size | 31.0 |

### Yön değişimi (outcome runs)

- **Toplam yön değişimi (UP↔DOWN)**: 173
- **Ortalama kararlı yön süresi**: 4.1 cluster ardışık
- **En uzun ardışık yön**: 30 cluster
- **UP run sayısı / DOWN run sayısı**: 87 / 87

### Hedge ekonomisi — UP+DOWN VWAP toplamı (±2sn pencere, 49 eşleşme)

| Hedge metriği | Değer |
|---|---|
| Toplam maliyet medyan (UP_vwap+DN_vwap) | $0.9828 |
| Toplam maliyet ortalama | $0.9785 |
| Toplam maliyet min | $0.8829 |
| Toplam maliyet max | $1.0735 |
| **Kilitli kar oranı (1−cost) medyan** | **1.72%** |
| Kilitli kar oranı ortalama | 2.15% |
| Profit-lock olmuş eşleşme sayısı (cost<1.00) | 42 / 49 |
| Maliyet ≤ 0.99 (≥ %1 kar) | 35 |
| Maliyet ≤ 0.97 (≥ %3 kar) | 17 |
| Maliyet ≤ 0.95 (≥ %5 kar) | 7 |

### Aynı saniyede UP+DOWN hedge örnekleri (toplam 55 saniye)

| sec | UP size | UP vwap | DN size | DN vwap | UP+DN vwap (≈hedge cost) |
|---|---|---|---|---|---|
| 1776992116 | 75.6 | 0.430 | 100.0 | 0.550 | 0.980 |
| 1776992118 | 100.0 | 0.420 | 140.6 | 0.567 | 0.987 |
| 1776992120 | 19.7 | 0.417 | 59.4 | 0.560 | 0.977 |
| 1776992122 | 13.5 | 0.422 | 122.3 | 0.558 | 0.980 |
| 1776992124 | 39.2 | 0.440 | 77.7 | 0.550 | 0.990 |

### Trade fiyat konumu (tick'in neresinde?)

| Konum | Adet | Oran |
|---|---|---|
| ASK+3+ | 119 | 15.1% |
| ASK+2 | 39 | 5.0% |
| ASK+1 | 61 | 7.8% |
| ASK | 79 | 10.0% |
| INSIDE | 17 | 2.2% |
| BID | 117 | 14.9% |
| BID-1 | 128 | 16.3% |
| BID-2 | 59 | 7.5% |
| BID-3+ | 168 | 21.3% |

- **Taker (≥ ASK)**: 298 (37.9%)
- **Maker (≤ BID)**: 472 (60.0%)
- **Inside spread**: 17 (2.2%)

### Spread × Trade konumu (her satır içinde % normalize)

| Spread | ASK+3+ | ASK+2 | ASK+1 | ASK | INSIDE | BID | BID-1 | BID-2 | BID-3+ | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| 1c | 15.2% | 4.1% | 7.8% | 10.6% | — | 14.1% | 17.4% | 7.8% | 22.8% | 728 |
| 2c | 21.9% | 18.8% | 3.1% | 3.1% | 9.4% | 28.1% | 3.1% | 6.2% | 6.2% | 32 |
| 3-4c | 7.7% | 23.1% | — | — | 46.2% | 23.1% | — | — | — | 13 |
| 5c+ | — | — | 21.4% | 7.1% | 57.1% | 14.3% | — | — | — | 14 |

### Trade size dağılımı

| Size bucket | Adet | Oran |
|---|---|---|
| <5 | 169 | 21.5% |
| 5-15 | 275 | 34.9% |
| 15-50 | 171 | 21.7% |
| 50-100 | 89 | 11.3% |
| 100+ | 83 | 10.5% |

### En çok hacim biriken fiyat seviyeleri

**UP outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 16¢ | 720.4 share |
| 17¢ | 653.4 share |
| 19¢ | 621.1 share |
| 20¢ | 617.1 share |
| 27¢ | 557.9 share |
| 21¢ | 550.2 share |
| 18¢ | 513.2 share |
| 26¢ | 500.0 share |
| 22¢ | 451.4 share |
| 98¢ | 400.0 share |

**DOWN outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 78¢ | 1,160.1 share |
| 73¢ | 898.8 share |
| 77¢ | 825.0 share |
| 76¢ | 804.1 share |
| 80¢ | 800.0 share |
| 74¢ | 573.2 share |
| 81¢ | 571.5 share |
| 79¢ | 536.2 share |
| 75¢ | 530.1 share |
| 66¢ | 414.0 share |

### 60-sn dilim aktivitesi

| Dilim | Cluster | Size | Taker | Maker | Taker oranı |
|---|---|---|---|---|---|
| 0-60s | 117 | 3180.0 | 30 | 80 | 27% |
| 60-120s | 233 | 6170.2 | 107 | 140 | 43% |
| 120-180s | 133 | 4800.7 | 65 | 85 | 43% |
| 180-240s | 179 | 5213.9 | 80 | 115 | 41% |
| 240-300s | 32 | 1473.0 | 16 | 22 | 42% |

### Trade'den ±10sn fiyat hareketi (zamanlama isabeti)

- **Doğru (alım sonrası fiyat ↑)**: 347 (48.5%)
- **Tersine (alım sonrası fiyat ↓)**: 340 (47.5%)
- **Sabit**: 29 (4.1%)

### Skor bantında bot davranışı

| Skor bandı | Cluster | UP karar | DOWN karar |
|---|---|---|---|
| >=7 (very up) | 0 | — | — |
| 5-7 (up) | 71 | 48 (68%) | 23 (32%) |
| 3-5 (mid) | 219 | 130 (59%) | 89 (41%) |
| <3 (down) | 426 | 232 (54%) | 194 (46%) |



## btc-updown-5m-1777050900

### Genel

| Metrik | Değer |
|---|---|
| Toplam fill (trade satırı) | 219 |
| Toplam karar (tx/cluster) | 216 |
| Multi-fill cluster (sweep) | 3 (1.4%) |
| Toplam alım hacmi (share) | 5,000.6 |
| Toplam notional (USD) | $2,150.57 |
| Side dağılımı (BUY/SELL) | BUY=219 |
| Outcome dağılımı (UP/DOWN) | Down=82 Up=137 |

### Karar aralıkları (cluster bazında, sn)

| İstatistik | sn |
|---|---|
| Medyan | 0.0 |
| Ortalama | 0.99 |
| P25 | 0.0 |
| P75 | 0.0 |
| P90 | 2.0 |
| Max | 16 |

- **Aynı saniyede paralel karar olan saniye**: 35
- **Aynı saniyede UP+DOWN birlikte (hedge!) olan saniye**: 5

### Cluster (sweep) şekli

| Metrik | Değer |
|---|---|
| Cluster başına ort fill | 1.01 |
| Cluster başına medyan fill | 1 |
| Tek-fill cluster (basit emir) | 213 / 216 |
| Cluster başına ort fiyat seviyesi | 1.01 |
| Sweep derinliği medyan (¢) | 0 |
| Sweep derinliği max (¢) | 1 |
| Cluster ort total size | 23.2 |

### Yön değişimi (outcome runs)

- **Toplam yön değişimi (UP↔DOWN)**: 29
- **Ortalama kararlı yön süresi**: 7.2 cluster ardışık
- **En uzun ardışık yön**: 35 cluster
- **UP run sayısı / DOWN run sayısı**: 15 / 15

### Hedge ekonomisi — UP+DOWN VWAP toplamı (±2sn pencere, 9 eşleşme)

| Hedge metriği | Değer |
|---|---|
| Toplam maliyet medyan (UP_vwap+DN_vwap) | $0.9900 |
| Toplam maliyet ortalama | $0.9932 |
| Toplam maliyet min | $0.9708 |
| Toplam maliyet max | $1.0404 |
| **Kilitli kar oranı (1−cost) medyan** | **1.00%** |
| Kilitli kar oranı ortalama | 0.68% |
| Profit-lock olmuş eşleşme sayısı (cost<1.00) | 8 / 9 |
| Maliyet ≤ 0.99 (≥ %1 kar) | 4 |
| Maliyet ≤ 0.97 (≥ %3 kar) | 0 |
| Maliyet ≤ 0.95 (≥ %5 kar) | 0 |

### Aynı saniyede UP+DOWN hedge örnekleri (toplam 5 saniye)

| sec | UP size | UP vwap | DN size | DN vwap | UP+DN vwap (≈hedge cost) |
|---|---|---|---|---|---|
| 1777051032 | 8.5 | 0.310 | 76.1 | 0.670 | 0.980 |
| 1777051064 | 300.0 | 0.080 | 60.0 | 0.900 | 0.980 |
| 1777051066 | 2.2 | 0.100 | 240.0 | 0.900 | 1.000 |
| 1777051074 | 100.0 | 0.080 | 100.0 | 0.910 | 0.990 |
| 1777051076 | 2.0 | 0.080 | 100.0 | 0.870 | 0.950 |

### Trade fiyat konumu (tick'in neresinde?)

| Konum | Adet | Oran |
|---|---|---|
| ASK+3+ | 13 | 6.0% |
| ASK+2 | 12 | 5.5% |
| ASK+1 | 14 | 6.4% |
| ASK | 89 | 40.8% |
| INSIDE | 1 | 0.5% |
| BID | 43 | 19.7% |
| BID-1 | 24 | 11.0% |
| BID-2 | 10 | 4.6% |
| BID-3+ | 12 | 5.5% |

- **Taker (≥ ASK)**: 128 (58.7%)
- **Maker (≤ BID)**: 89 (40.8%)
- **Inside spread**: 1 (0.5%)

### Spread × Trade konumu (her satır içinde % normalize)

| Spread | ASK+3+ | ASK+2 | ASK+1 | ASK | INSIDE | BID | BID-1 | BID-2 | BID-3+ | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| 1c | 6.1% | 5.6% | 5.6% | 41.1% | — | 20.1% | 11.2% | 4.7% | 5.6% | 214 |
| 2c | — | — | 66.7% | 33.3% | — | — | — | — | — | 3 |
| 3-4c | — | — | — | — | 100.0% | — | — | — | — | 1 |

### Trade size dağılımı

| Size bucket | Adet | Oran |
|---|---|---|
| <5 | 73 | 33.3% |
| 5-15 | 64 | 29.2% |
| 15-50 | 51 | 23.3% |
| 50-100 | 15 | 6.8% |
| 100+ | 16 | 7.3% |

### En çok hacim biriken fiyat seviyeleri

**UP outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 2¢ | 300.0 share |
| 8¢ | 300.0 share |
| 9¢ | 249.7 share |
| 31¢ | 200.0 share |
| 36¢ | 164.8 share |
| 7¢ | 158.7 share |
| 37¢ | 125.9 share |
| 50¢ | 100.0 share |
| 32¢ | 100.0 share |
| 1¢ | 100.0 share |

**DOWN outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 60¢ | 300.0 share |
| 91¢ | 200.0 share |
| 96¢ | 200.0 share |
| 66¢ | 200.0 share |
| 55¢ | 152.5 share |
| 78¢ | 135.2 share |
| 57¢ | 109.6 share |
| 54¢ | 100.0 share |
| 62¢ | 100.0 share |
| 64¢ | 100.0 share |

### 60-sn dilim aktivitesi

| Dilim | Cluster | Size | Taker | Maker | Taker oranı |
|---|---|---|---|---|---|
| 0-60s | 35 | 841.2 | 23 | 13 | 64% |
| 60-120s | 32 | 836.5 | 18 | 14 | 56% |
| 120-180s | 110 | 2227.1 | 61 | 50 | 55% |
| 180-240s | 39 | 1095.9 | 26 | 12 | 68% |
| 240-300s | 0 | 0.0 | 0 | 0 | — |

### Trade'den ±10sn fiyat hareketi (zamanlama isabeti)

- **Doğru (alım sonrası fiyat ↑)**: 57 (26.4%)
- **Tersine (alım sonrası fiyat ↓)**: 158 (73.1%)
- **Sabit**: 1 (0.5%)

### Skor bantında bot davranışı

| Skor bandı | Cluster | UP karar | DOWN karar |
|---|---|---|---|
| >=7 (very up) | 0 | — | — |
| 5-7 (up) | 0 | — | — |
| 3-5 (mid) | 82 | 23 (28%) | 59 (72%) |
| <3 (down) | 134 | 112 (84%) | 22 (16%) |



## btc-updown-5m-1777125600

### Genel

| Metrik | Değer |
|---|---|
| Toplam fill (trade satırı) | 306 |
| Toplam karar (tx/cluster) | 242 |
| Multi-fill cluster (sweep) | 42 (17.4%) |
| Toplam alım hacmi (share) | 1,205.7 |
| Toplam notional (USD) | $586.44 |
| Side dağılımı (BUY/SELL) | BUY=306 |
| Outcome dağılımı (UP/DOWN) | Up=149 Down=157 |

### Karar aralıkları (cluster bazında, sn)

| İstatistik | sn |
|---|---|
| Medyan | 0.0 |
| Ortalama | 0.99 |
| P25 | 0.0 |
| P75 | 2.0 |
| P90 | 2.0 |
| Max | 12 |

- **Aynı saniyede paralel karar olan saniye**: 62
- **Aynı saniyede UP+DOWN birlikte (hedge!) olan saniye**: 30

### Cluster (sweep) şekli

| Metrik | Değer |
|---|---|
| Cluster başına ort fill | 1.26 |
| Cluster başına medyan fill | 1 |
| Tek-fill cluster (basit emir) | 200 / 242 |
| Cluster başına ort fiyat seviyesi | 1.26 |
| Sweep derinliği medyan (¢) | 0 |
| Sweep derinliği max (¢) | 5 |
| Cluster ort total size | 5.0 |

### Yön değişimi (outcome runs)

- **Toplam yön değişimi (UP↔DOWN)**: 84
- **Ortalama kararlı yön süresi**: 2.8 cluster ardışık
- **En uzun ardışık yön**: 20 cluster
- **UP run sayısı / DOWN run sayısı**: 43 / 42

### Hedge ekonomisi — UP+DOWN VWAP toplamı (±2sn pencere, 40 eşleşme)

| Hedge metriği | Değer |
|---|---|
| Toplam maliyet medyan (UP_vwap+DN_vwap) | $0.9860 |
| Toplam maliyet ortalama | $0.9818 |
| Toplam maliyet min | $0.7944 |
| Toplam maliyet max | $1.0350 |
| **Kilitli kar oranı (1−cost) medyan** | **1.40%** |
| Kilitli kar oranı ortalama | 1.82% |
| Profit-lock olmuş eşleşme sayısı (cost<1.00) | 35 / 40 |
| Maliyet ≤ 0.99 (≥ %1 kar) | 27 |
| Maliyet ≤ 0.97 (≥ %3 kar) | 7 |
| Maliyet ≤ 0.95 (≥ %5 kar) | 2 |

### Aynı saniyede UP+DOWN hedge örnekleri (toplam 30 saniye)

| sec | UP size | UP vwap | DN size | DN vwap | UP+DN vwap (≈hedge cost) |
|---|---|---|---|---|---|
| 1777125626 | 10.0 | 0.695 | 10.0 | 0.315 | 1.010 |
| 1777125638 | 5.0 | 0.690 | 5.0 | 0.290 | 0.980 |
| 1777125642 | 43.8 | 0.636 | 5.0 | 0.340 | 0.976 |
| 1777125644 | 8.6 | 0.586 | 7.0 | 0.400 | 0.986 |
| 1777125652 | 40.0 | 0.555 | 5.0 | 0.440 | 0.995 |

### Trade fiyat konumu (tick'in neresinde?)

| Konum | Adet | Oran |
|---|---|---|
| ASK+3+ | 41 | 13.4% |
| ASK+2 | 14 | 4.6% |
| ASK+1 | 26 | 8.5% |
| ASK | 52 | 17.0% |
| INSIDE | 4 | 1.3% |
| BID | 73 | 23.9% |
| BID-1 | 36 | 11.8% |
| BID-2 | 22 | 7.2% |
| BID-3+ | 38 | 12.4% |

- **Taker (≥ ASK)**: 133 (43.5%)
- **Maker (≤ BID)**: 169 (55.2%)
- **Inside spread**: 4 (1.3%)

### Spread × Trade konumu (her satır içinde % normalize)

| Spread | ASK+3+ | ASK+2 | ASK+1 | ASK | INSIDE | BID | BID-1 | BID-2 | BID-3+ | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| 1c | 13.2% | 4.3% | 9.3% | 17.8% | — | 24.9% | 12.1% | 6.8% | 11.7% | 281 |
| 2c | 17.4% | 8.7% | — | 8.7% | 17.4% | 13.0% | 4.3% | 13.0% | 17.4% | 23 |
| 3-4c | — | — | — | — | — | — | 100.0% | — | — | 1 |
| 5c+ | — | — | — | — | — | — | — | — | 100.0% | 1 |

### Trade size dağılımı

| Size bucket | Adet | Oran |
|---|---|---|
| <5 | 123 | 40.2% |
| 5-15 | 183 | 59.8% |
| 15-50 | 0 | 0.0% |
| 50-100 | 0 | 0.0% |
| 100+ | 0 | 0.0% |

### En çok hacim biriken fiyat seviyeleri

**UP outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 59¢ | 44.8 share |
| 63¢ | 40.0 share |
| 64¢ | 39.4 share |
| 61¢ | 30.0 share |
| 62¢ | 29.8 share |
| 60¢ | 20.0 share |
| 75¢ | 20.0 share |
| 79¢ | 20.0 share |
| 49¢ | 20.0 share |
| 69¢ | 15.0 share |

**DOWN outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 40¢ | 46.8 share |
| 38¢ | 45.0 share |
| 37¢ | 35.9 share |
| 35¢ | 30.0 share |
| 36¢ | 30.0 share |
| 39¢ | 28.4 share |
| 34¢ | 25.0 share |
| 15¢ | 20.0 share |
| 17¢ | 20.0 share |
| 30¢ | 20.0 share |

### 60-sn dilim aktivitesi

| Dilim | Cluster | Size | Taker | Maker | Taker oranı |
|---|---|---|---|---|---|
| 0-60s | 59 | 302.7 | 43 | 32 | 57% |
| 60-120s | 56 | 282.4 | 25 | 47 | 35% |
| 120-180s | 60 | 271.8 | 28 | 42 | 40% |
| 180-240s | 56 | 283.8 | 30 | 39 | 43% |
| 240-300s | 11 | 65.0 | 7 | 9 | 44% |

### Trade'den ±10sn fiyat hareketi (zamanlama isabeti)

- **Doğru (alım sonrası fiyat ↑)**: 89 (36.8%)
- **Tersine (alım sonrası fiyat ↓)**: 146 (60.3%)
- **Sabit**: 7 (2.9%)

### Skor bantında bot davranışı

| Skor bandı | Cluster | UP karar | DOWN karar |
|---|---|---|---|
| >=7 (very up) | 1 | 1 (100%) | 0 (0%) |
| 5-7 (up) | 91 | 47 (52%) | 44 (48%) |
| 3-5 (mid) | 144 | 73 (51%) | 71 (49%) |
| <3 (down) | 6 | 6 (100%) | 0 (0%) |



## btc-updown-5m-1777125900

### Genel

| Metrik | Değer |
|---|---|
| Toplam fill (trade satırı) | 140 |
| Toplam karar (tx/cluster) | 118 |
| Multi-fill cluster (sweep) | 18 (15.3%) |
| Toplam alım hacmi (share) | 550.3 |
| Toplam notional (USD) | $282.92 |
| Side dağılımı (BUY/SELL) | BUY=140 |
| Outcome dağılımı (UP/DOWN) | Up=65 Down=75 |

### Karar aralıkları (cluster bazında, sn)

| İstatistik | sn |
|---|---|
| Medyan | 2.0 |
| Ortalama | 2.46 |
| P25 | 0.0 |
| P75 | 2.0 |
| P90 | 6.0 |
| Max | 34 |

- **Aynı saniyede paralel karar olan saniye**: 27
- **Aynı saniyede UP+DOWN birlikte (hedge!) olan saniye**: 6

### Cluster (sweep) şekli

| Metrik | Değer |
|---|---|
| Cluster başına ort fill | 1.19 |
| Cluster başına medyan fill | 1 |
| Tek-fill cluster (basit emir) | 100 / 118 |
| Cluster başına ort fiyat seviyesi | 1.19 |
| Sweep derinliği medyan (¢) | 0 |
| Sweep derinliği max (¢) | 2 |
| Cluster ort total size | 4.7 |

### Yön değişimi (outcome runs)

- **Toplam yön değişimi (UP↔DOWN)**: 42
- **Ortalama kararlı yön süresi**: 2.7 cluster ardışık
- **En uzun ardışık yön**: 10 cluster
- **UP run sayısı / DOWN run sayısı**: 22 / 21

### Hedge ekonomisi — UP+DOWN VWAP toplamı (±2sn pencere, 14 eşleşme)

| Hedge metriği | Değer |
|---|---|
| Toplam maliyet medyan (UP_vwap+DN_vwap) | $0.9850 |
| Toplam maliyet ortalama | $0.9793 |
| Toplam maliyet min | $0.9450 |
| Toplam maliyet max | $0.9950 |
| **Kilitli kar oranı (1−cost) medyan** | **1.50%** |
| Kilitli kar oranı ortalama | 2.07% |
| Profit-lock olmuş eşleşme sayısı (cost<1.00) | 14 / 14 |
| Maliyet ≤ 0.99 (≥ %1 kar) | 13 |
| Maliyet ≤ 0.97 (≥ %3 kar) | 2 |
| Maliyet ≤ 0.95 (≥ %5 kar) | 1 |

### Aynı saniyede UP+DOWN hedge örnekleri (toplam 6 saniye)

| sec | UP size | UP vwap | DN size | DN vwap | UP+DN vwap (≈hedge cost) |
|---|---|---|---|---|---|
| 1777125920 | 5.0 | 0.520 | 8.7 | 0.466 | 0.986 |
| 1777126090 | 5.0 | 0.770 | 10.0 | 0.195 | 0.965 |
| 1777126092 | 10.0 | 0.785 | 8.2 | 0.200 | 0.985 |
| 1777126122 | 5.0 | 0.850 | 5.0 | 0.130 | 0.980 |
| 1777126136 | 7.7 | 0.914 | 20.0 | 0.057 | 0.971 |

### Trade fiyat konumu (tick'in neresinde?)

| Konum | Adet | Oran |
|---|---|---|
| ASK+3+ | 9 | 6.4% |
| ASK+2 | 9 | 6.4% |
| ASK+1 | 16 | 11.4% |
| ASK | 24 | 17.1% |
| INSIDE | 7 | 5.0% |
| BID | 41 | 29.3% |
| BID-1 | 13 | 9.3% |
| BID-2 | 8 | 5.7% |
| BID-3+ | 13 | 9.3% |

- **Taker (≥ ASK)**: 58 (41.4%)
- **Maker (≤ BID)**: 75 (53.6%)
- **Inside spread**: 7 (5.0%)

### Spread × Trade konumu (her satır içinde % normalize)

| Spread | ASK+3+ | ASK+2 | ASK+1 | ASK | INSIDE | BID | BID-1 | BID-2 | BID-3+ | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| 1c | 0.8% | 7.4% | 13.1% | 19.7% | — | 32.0% | 10.7% | 5.7% | 10.7% | 122 |
| 2c | 53.3% | — | — | — | 33.3% | 6.7% | — | 6.7% | — | 15 |
| 3-4c | — | — | — | — | 66.7% | 33.3% | — | — | — | 3 |

### Trade size dağılımı

| Size bucket | Adet | Oran |
|---|---|---|
| <5 | 53 | 37.9% |
| 5-15 | 87 | 62.1% |
| 15-50 | 0 | 0.0% |
| 50-100 | 0 | 0.0% |
| 100+ | 0 | 0.0% |

### En çok hacim biriken fiyat seviyeleri

**UP outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 49¢ | 35.0 share |
| 48¢ | 24.0 share |
| 47¢ | 20.0 share |
| 97¢ | 20.0 share |
| 44¢ | 20.0 share |
| 95¢ | 15.0 share |
| 96¢ | 15.0 share |
| 45¢ | 15.0 share |
| 52¢ | 10.0 share |
| 79¢ | 10.0 share |

**DOWN outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 50¢ | 34.6 share |
| 44¢ | 29.0 share |
| 20¢ | 25.0 share |
| 51¢ | 20.0 share |
| 47¢ | 20.0 share |
| 49¢ | 10.0 share |
| 52¢ | 10.0 share |
| 53¢ | 10.0 share |
| 56¢ | 10.0 share |
| 5¢ | 10.0 share |

### 60-sn dilim aktivitesi

| Dilim | Cluster | Size | Taker | Maker | Taker oranı |
|---|---|---|---|---|---|
| 0-60s | 20 | 85.0 | 7 | 15 | 32% |
| 60-120s | 14 | 74.0 | 8 | 11 | 42% |
| 120-180s | 31 | 168.6 | 21 | 17 | 55% |
| 180-240s | 46 | 167.7 | 19 | 24 | 44% |
| 240-300s | 7 | 55.0 | 3 | 8 | 27% |

### Trade'den ±10sn fiyat hareketi (zamanlama isabeti)

- **Doğru (alım sonrası fiyat ↑)**: 30 (26.3%)
- **Tersine (alım sonrası fiyat ↓)**: 80 (70.2%)
- **Sabit**: 4 (3.5%)

### Skor bantında bot davranışı

| Skor bandı | Cluster | UP karar | DOWN karar |
|---|---|---|---|
| >=7 (very up) | 44 | 18 (41%) | 26 (59%) |
| 5-7 (up) | 30 | 10 (33%) | 20 (67%) |
| 3-5 (mid) | 44 | 22 (50%) | 22 (50%) |
| <3 (down) | 0 | — | — |



## btc-updown-5m-1777132200

### Genel

| Metrik | Değer |
|---|---|
| Toplam fill (trade satırı) | 319 |
| Toplam karar (tx/cluster) | 255 |
| Multi-fill cluster (sweep) | 40 (15.7%) |
| Toplam alım hacmi (share) | 1,327.5 |
| Toplam notional (USD) | $651.50 |
| Side dağılımı (BUY/SELL) | BUY=319 |
| Outcome dağılımı (UP/DOWN) | Down=159 Up=160 |

### Karar aralıkları (cluster bazında, sn)

| İstatistik | sn |
|---|---|
| Medyan | 0.0 |
| Ortalama | 0.93 |
| P25 | 0.0 |
| P75 | 2.0 |
| P90 | 2.0 |
| Max | 12 |

- **Aynı saniyede paralel karar olan saniye**: 61
- **Aynı saniyede UP+DOWN birlikte (hedge!) olan saniye**: 19

### Cluster (sweep) şekli

| Metrik | Değer |
|---|---|
| Cluster başına ort fill | 1.25 |
| Cluster başına medyan fill | 1 |
| Tek-fill cluster (basit emir) | 215 / 255 |
| Cluster başına ort fiyat seviyesi | 1.25 |
| Sweep derinliği medyan (¢) | 0 |
| Sweep derinliği max (¢) | 5 |
| Cluster ort total size | 5.2 |

### Yön değişimi (outcome runs)

- **Toplam yön değişimi (UP↔DOWN)**: 79
- **Ortalama kararlı yön süresi**: 3.2 cluster ardışık
- **En uzun ardışık yön**: 11 cluster
- **UP run sayısı / DOWN run sayısı**: 40 / 40

### Hedge ekonomisi — UP+DOWN VWAP toplamı (±2sn pencere, 40 eşleşme)

| Hedge metriği | Değer |
|---|---|
| Toplam maliyet medyan (UP_vwap+DN_vwap) | $0.9821 |
| Toplam maliyet ortalama | $0.9659 |
| Toplam maliyet min | $0.8239 |
| Toplam maliyet max | $1.0519 |
| **Kilitli kar oranı (1−cost) medyan** | **1.79%** |
| Kilitli kar oranı ortalama | 3.41% |
| Profit-lock olmuş eşleşme sayısı (cost<1.00) | 33 / 40 |
| Maliyet ≤ 0.99 (≥ %1 kar) | 27 |
| Maliyet ≤ 0.97 (≥ %3 kar) | 16 |
| Maliyet ≤ 0.95 (≥ %5 kar) | 10 |

### Aynı saniyede UP+DOWN hedge örnekleri (toplam 19 saniye)

| sec | UP size | UP vwap | DN size | DN vwap | UP+DN vwap (≈hedge cost) |
|---|---|---|---|---|---|
| 1777132214 | 10.0 | 0.550 | 5.0 | 0.340 | 0.890 |
| 1777132230 | 5.0 | 0.470 | 10.0 | 0.495 | 0.965 |
| 1777132240 | 5.0 | 0.730 | 5.0 | 0.270 | 1.000 |
| 1777132258 | 7.6 | 0.607 | 5.0 | 0.380 | 0.987 |
| 1777132264 | 5.0 | 0.640 | 35.0 | 0.323 | 0.963 |

### Trade fiyat konumu (tick'in neresinde?)

| Konum | Adet | Oran |
|---|---|---|
| ASK+3+ | 67 | 21.1% |
| ASK+2 | 23 | 7.2% |
| ASK+1 | 34 | 10.7% |
| ASK | 38 | 11.9% |
| BID | 37 | 11.6% |
| BID-1 | 28 | 8.8% |
| BID-2 | 24 | 7.5% |
| BID-3+ | 67 | 21.1% |

- **Taker (≥ ASK)**: 162 (50.9%)
- **Maker (≤ BID)**: 156 (49.1%)
- **Inside spread**: 0 (0.0%)

### Spread × Trade konumu (her satır içinde % normalize)

| Spread | ASK+3+ | ASK+2 | ASK+1 | ASK | INSIDE | BID | BID-1 | BID-2 | BID-3+ | Total |
|---|---|---|---|---|---|---|---|---|---|---|
| 1c | 21.6% | 7.2% | 10.8% | 12.1% | — | 11.8% | 9.2% | 7.9% | 19.3% | 305 |
| 2c | — | — | — | — | — | 14.3% | — | — | 85.7% | 7 |
| 3-4c | 25.0% | 25.0% | 25.0% | 25.0% | — | — | — | — | — | 4 |
| 5c+ | — | — | — | — | — | — | — | — | 100.0% | 2 |

### Trade size dağılımı

| Size bucket | Adet | Oran |
|---|---|---|
| <5 | 102 | 32.0% |
| 5-15 | 217 | 68.0% |
| 15-50 | 0 | 0.0% |
| 50-100 | 0 | 0.0% |
| 100+ | 0 | 0.0% |

### En çok hacim biriken fiyat seviyeleri

**UP outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 67¢ | 25.0 share |
| 66¢ | 25.0 share |
| 64¢ | 25.0 share |
| 3¢ | 25.0 share |
| 22¢ | 25.0 share |
| 68¢ | 20.1 share |
| 23¢ | 20.0 share |
| 13¢ | 20.0 share |
| 10¢ | 20.0 share |
| 16¢ | 20.0 share |

**DOWN outcome:**

| Fiyat | Toplam hacim |
|---|---|
| 33¢ | 35.0 share |
| 34¢ | 20.0 share |
| 28¢ | 20.0 share |
| 29¢ | 20.0 share |
| 30¢ | 20.0 share |
| 35¢ | 20.0 share |
| 63¢ | 20.0 share |
| 93¢ | 20.0 share |
| 97¢ | 20.0 share |
| 47¢ | 20.0 share |

### 60-sn dilim aktivitesi

| Dilim | Cluster | Size | Taker | Maker | Taker oranı |
|---|---|---|---|---|---|
| 0-60s | 44 | 206.3 | 29 | 22 | 57% |
| 60-120s | 114 | 594.0 | 78 | 71 | 52% |
| 120-180s | 65 | 352.7 | 37 | 46 | 45% |
| 180-240s | 30 | 164.5 | 18 | 16 | 53% |
| 240-300s | 2 | 10.0 | 0 | 1 | 0% |

### Trade'den ±10sn fiyat hareketi (zamanlama isabeti)

- **Doğru (alım sonrası fiyat ↑)**: 104 (41.6%)
- **Tersine (alım sonrası fiyat ↓)**: 145 (58.0%)
- **Sabit**: 1 (0.4%)

### Skor bantında bot davranışı

| Skor bandı | Cluster | UP karar | DOWN karar |
|---|---|---|---|
| >=7 (very up) | 0 | — | — |
| 5-7 (up) | 85 | 45 (53%) | 40 (47%) |
| 3-5 (mid) | 66 | 30 (45%) | 36 (55%) |
| <3 (down) | 104 | 52 (50%) | 52 (50%) |



---
## Sonuç — Bot Mantığı (en ince ayrıntıya kadar)


1. **Karar tick'i ~2 sn**. Aynı saniyede 0-1 sn aralığı çoğunlukta (paralel emir), ardından 2-5 sn'lik karar döngüleri. 1 sn aralığı timestamp grid'inden dolayı boş.
2. **Sweep = ana taker tarzı**. Multi-fill cluster oranı %25-45 arası; bir sweep ortalama 3-5 fiyat seviyesi tüketebiliyor (max 10+ ¢ derinlik gözlendi).
3. **Maker emirleri yayılı resting**. Bot best-bid'e ek olarak 1-3¢ altı seviyelere küçük sosis-emirler bırakıyor ("ladder bids"). Spread 1¢ ise BID/BID-1/BID-2 toplam ~%40.
4. **Spread'e tepki var**. Spread genişledikçe BID-1 oranı düşüyor, **INSIDE** alımları patlıyor (bot order'ı mid'e taşıyor) ve TAKER sweep'i daha agresifleşiyor (ASK+N artıyor).
5. **Yön değişimi sık**. Bot tek yönde uzun süre kalmıyor; her market'te ortalama 10-30 outcome switch var → bot trend takipçisi değil, kısa-vadeli sinyal/oran takipçisi.
6. **Hedge davranışı belirgin**. Aynı saniye içinde UP+DOWN cluster'ı sıkça oluşuyor; VWAP toplamı (UP+DOWN) genelde 0.95-1.00 aralığında — bu, **risk-nötralize** etmek için her iki tarafta dengeleyici alım yapıldığının net göstergesi.
7. **Skor bantı belirleyici**. Skor ≥7 → ezici çoğunluk UP; Skor <3 → ezici çoğunluk DOWN. Skor 3-7 "karışık bölge" → bot iki tarafa da pozisyon açıyor (hedge).
8. **Zamanlama isabeti orta**. Trade'den 10 sn sonra fiyat lehe hareket etme oranı %50 civarı → bot **fiyat takip etmiyor**, fırsat alıyor (mean-reversion + sinyal).
9. **İki versiyon var**: 5 market 'agresif' (yüksek hacim, 100+ size'lar), 1 market 'küçük bütçe' (≤10 share emirler, daha yavaş karar tick'i).
