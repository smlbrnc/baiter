# btc-updown-5m-1776992100 — Bot Strateji Analizi

## 1. Market Bilgileri

- **Slug**: `btc-updown-5m-1776992100`
- **Market start (UTC)**: `1776992100989`
- **Market window**: 5 dakika (300 sn)
- **Tick sayısı**: 301
- **Trade sayısı**: 787
- **İlk trade offset**: T+14s
- **Son trade offset**: T+310s

## 2. Bot Davranış Profili

- **Toplam**: 787 trade — {'BUY': 787}
- **UP / DN**: 450 / 337
- **Hacim**: UP $3,702.41, DN $8,345.32, **toplam $12,047.72**
- **Shares**: UP 11047.78, DN 11183.08
- **Notional**: min $0.01, medyan $4.65, ort $15.31, max $97.00
- **Size**: min 0.02, medyan 10.96, max 100.00
- **Tam sayı size**: 246/787 (31.3%)
- **5'in katı size**: 185/787 (23.5%)
- **Tx**: 716 benzersiz, **48 multi-fill**

## 3. Burst & Eşzamanlılık

- **Aktif saniye**: 114/300
- **Max burst**: 25 trade/sn
- **5+ burst saniye**: 67
- **Eşzamanlı UP+DOWN saniye**: 55
- **Trade/aktif saniye**: 6.90

## 4. Zaman Dilimi (1dk eşit)

| Dilim | Trade | UP | DN | Hacim UP | Hacim DN | Trade UP fiyat | Tick UP mid | Sapma | Score ort |
|-------|-------|----|----|----------|----------|---------------|-------------|-------|-----------|
| 0-60s | 120 | 65 | 55 | $292.18 | $1,423.62 | 0.322 | 0.349 | -0.028 | 2.85 |
| 60-120s | 252 | 140 | 112 | $657.45 | $2,330.37 | 0.214 | 0.231 | -0.017 | 1.63 |
| 120-180s | 152 | 82 | 70 | $352.80 | $2,180.21 | 0.152 | 0.185 | -0.033 | 1.84 |
| 180-240s | 195 | 111 | 84 | $897.10 | $1,700.19 | 0.309 | 0.299 | +0.011 | 4.06 |
| 240-300s | 38 | 22 | 16 | $160.06 | $710.93 | 0.438 | 0.596 | -0.158 | 4.16 |

## 5. Tick Reaksiyon Analizi

Trade öncesi mid hareketi (UP için up_mid, DOWN için down_mid). Eşik: |Δmid| > 0.005.

| Lookback | UP follow | UP against | UP flat | DN follow | DN against | DN flat |
|----------|-----------|------------|---------|-----------|------------|---------|
| 5s | 163 | 262 | 25 | 185 | 132 | 20 |
| 10s | 205 | 206 | 39 | 152 | 177 | 8 |
| 30s | 188 | 220 | 9 | 138 | 160 | 15 |

_NOT: 'follow' = trend yönünde alım. UP için mid yükselişi + UP alımı = follow. DOWN için mid düşüşü + DOWN alımı = follow (tabloda DN follow kolonu = `dn_against` çünkü o değişken 'mid yükseldi & DOWN' tanımlıydı, ama DOWN trend takibi mid düşüşüdür → `dn_against` rolü swap edildi)._


## 6. Eşik Alımları

- **Lottery DOWN** (≤0.10): 0 trade, $0.00
- **Düşük DOWN** (≤0.20): 0 trade
- **Favori UP** (≥0.85): 34 trade, $1,359.53
- **Yüksek UP** (≥0.70): 34 trade

## 7. Fiyat Tutarlılığı (vs tick spread)

- **Eşleşen trade** (bid/ask>0): 787/787
- **Spread içi (bid≤p≤ask)**: 197 (25.0%)
- **Maker tahmini (p ≤ bid)**: 472 (60.0%)
- **Taker tahmini (p ≥ ask)**: 298 (37.9%)
- **Inside spread**: 17 (2.2%)
- **Δ(p − ask)**: min -0.200, medyan -0.010, ort -0.013, max +0.250
- **Δ(p − mid)**: min -0.195, medyan -0.005, ort -0.007, max +0.255
- **Anomaliler (|p−ask|>0.02 veya p<bid−0.02)**: 454

**En büyük 10 anomali (|Δ_ask| azalan)**:

| ts | outcome | price | size | bid | ask | Δ_ask |
|----|---------|-------|------|-----|-----|-------|
| 1776992342000 | Down | 0.810 | 100.00 | 0.550 | 0.560 | +0.250 |
| 1776992342000 | Down | 0.800 | 100.00 | 0.550 | 0.560 | +0.240 |
| 1776992342000 | Down | 0.790 | 100.00 | 0.550 | 0.560 | +0.230 |
| 1776992342000 | Down | 0.780 | 100.00 | 0.550 | 0.560 | +0.220 |
| 1776992342000 | Down | 0.770 | 100.00 | 0.550 | 0.560 | +0.210 |
| 1776992342000 | Down | 0.760 | 88.67 | 0.550 | 0.560 | +0.200 |
| 1776992342000 | Up | 0.250 | 8.03 | 0.440 | 0.450 | -0.200 |
| 1776992328000 | Down | 0.610 | 14.00 | 0.780 | 0.790 | -0.180 |
| 1776992328000 | Down | 0.610 | 26.60 | 0.780 | 0.790 | -0.180 |
| 1776992342000 | Down | 0.730 | 31.70 | 0.550 | 0.560 | +0.170 |

## 8. Bot Strateji Çıkarımı (bu market)

- **Bütçe**: Çoğunlukla küçük (medyan $4.65) ama büyük tek seferlik emirler var (max $97.00)
- **Hedge yapısı**: Dengeli iki yön — UP 450 (57%) / DN 337 (aktif hedge stratejisi)
- **Pozisyon yönü**: %100 BUY — pozisyon kapatma yok, scaling-in mantığı
- **Trend takibi**: Trend takipçi — UP alımları %46 mid yükselişi sonrası, DOWN alımları %45 mid düşüşü sonrası (10s lookback)
- **Burst kalıbı**: Algoritmik tetikleme — max 25 trade/sn, 67 saniyede 5+ burst (volatilite anları)
- **Emir bölünmesi**: 48 tx içinde multi-fill — büyük emirler kısmen dolduruluyor
- **Emir tipi**: Çoğunluk maker (%60) — pasif resting order baskın (taker %38)
- **Favori UP**: 34 adet UP @ ≥0.85 ($1,359.53 hacim) — favori takibi
- **Eşzamanlı hedge**: 55 saniyede aynı anda UP+DOWN alımı — çift yön emir akışı
- **Zamanlama**: En aktif dakika = 1 (252 trade), en sakin = 4 (38 trade)

## 9. Trade Timeline (özet)

**İlk 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1776992114000 | T+14s | Down | 0.550 | 100.00 | $55.00 |
| 1776992114000 | T+14s | Down | 0.560 | 0.62 | $0.35 |
| 1776992114000 | T+14s | Down | 0.560 | 10.00 | $5.60 |
| 1776992114000 | T+14s | Down | 0.560 | 5.00 | $2.80 |
| 1776992114000 | T+14s | Down | 0.560 | 2.27 | $1.27 |
| 1776992114000 | T+14s | Down | 0.560 | 4.36 | $2.44 |
| 1776992114000 | T+14s | Down | 0.560 | 5.00 | $2.80 |
| 1776992114000 | T+14s | Down | 0.560 | 22.73 | $12.73 |
| 1776992114000 | T+14s | Down | 0.560 | 50.00 | $28.00 |
| 1776992116000 | T+16s | Up | 0.430 | 9.00 | $3.87 |

**En yoğun saniye trades** (max burst):

_Saniye: 1776992308 (T+208s, 25 trade)_

| outcome | price | size | $ |
|---------|-------|------|---|
| Down | 0.650 | 44.31 | $28.80 |
| Down | 0.660 | 100.00 | $66.00 |
| Down | 0.670 | 42.60 | $28.54 |
| Up | 0.290 | 72.72 | $21.09 |
| Up | 0.300 | 100.00 | $30.00 |
| Up | 0.310 | 9.51 | $2.95 |
| Down | 0.670 | 1.18 | $0.79 |
| Up | 0.310 | 90.49 | $28.05 |
| Up | 0.320 | 29.88 | $9.56 |
| Up | 0.320 | 5.59 | $1.79 |
| Up | 0.320 | 1.47 | $0.47 |
| Down | 0.670 | 5.00 | $3.35 |
| Up | 0.320 | 7.82 | $2.50 |
| Up | 0.320 | 6.29 | $2.01 |
| Up | 0.320 | 6.71 | $2.15 |
| Up | 0.320 | 7.35 | $2.35 |
| Down | 0.670 | 16.00 | $10.72 |
| Down | 0.670 | 35.21 | $23.59 |
| Up | 0.320 | 5.00 | $1.60 |
| Up | 0.330 | 79.51 | $26.24 |

**Son 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1776992400000 | T+300s | Up | 0.950 | 100.00 | $95.00 |
| 1776992402000 | T+302s | Up | 0.970 | 100.00 | $97.00 |
| 1776992402000 | T+302s | Up | 0.980 | 50.00 | $49.00 |
| 1776992410000 | T+310s | Up | 0.970 | 34.73 | $33.69 |
| 1776992410000 | T+310s | Up | 0.970 | 1.84 | $1.78 |
| 1776992410000 | T+310s | Up | 0.970 | 10.93 | $10.60 |
| 1776992410000 | T+310s | Up | 0.970 | 61.42 | $59.58 |
| 1776992410000 | T+310s | Up | 0.970 | 3.17 | $3.07 |
| 1776992410000 | T+310s | Up | 0.970 | 2.00 | $1.94 |
| 1776992410000 | T+310s | Up | 0.970 | 33.33 | $32.33 |
