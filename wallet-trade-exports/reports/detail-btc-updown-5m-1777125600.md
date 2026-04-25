# btc-updown-5m-1777125600 — Bot Strateji Analizi

## 1. Market Bilgileri

- **Slug**: `btc-updown-5m-1777125600`
- **Market start (UTC)**: `1777125600691`
- **Market window**: 5 dakika (300 sn)
- **Tick sayısı**: 301
- **Trade sayısı**: 306
- **İlk trade offset**: T+14s
- **Son trade offset**: T+252s

## 2. Bot Davranış Profili

- **Toplam**: 306 trade — {'BUY': 306}
- **UP / DN**: 149 / 157
- **Hacim**: UP $391.20, DN $195.24, **toplam $586.44**
- **Shares**: UP 599.76, DN 605.97
- **Notional**: min $0.02, medyan $1.85, ort $1.92, max $4.40
- **Size**: min 0.05, medyan 5.00, max 5.00
- **Tam sayı size**: 186/306 (60.8%)
- **5'in katı size**: 183/306 (59.8%)
- **Tx**: 242 benzersiz, **42 multi-fill**

## 3. Burst & Eşzamanlılık

- **Aktif saniye**: 91/300
- **Max burst**: 15 trade/sn
- **5+ burst saniye**: 20
- **Eşzamanlı UP+DOWN saniye**: 30
- **Trade/aktif saniye**: 3.36

## 4. Zaman Dilimi (1dk eşit)

| Dilim | Trade | UP | DN | Hacim UP | Hacim DN | Trade UP fiyat | Tick UP mid | Sapma | Score ort |
|-------|-------|----|----|----------|----------|---------------|-------------|-------|-----------|
| 0-60s | 75 | 33 | 42 | $85.50 | $59.66 | 0.614 | 0.599 | +0.015 | 5.50 |
| 60-120s | 74 | 38 | 36 | $82.28 | $55.20 | 0.614 | 0.620 | -0.006 | 4.37 |
| 120-180s | 72 | 31 | 41 | $67.68 | $52.59 | 0.532 | 0.537 | -0.005 | 3.83 |
| 180-240s | 69 | 35 | 34 | $118.99 | $25.69 | 0.804 | 0.812 | -0.008 | 5.90 |
| 240-300s | 16 | 12 | 4 | $36.75 | $2.10 | 0.727 | 0.451 | +0.276 | 2.14 |

## 5. Tick Reaksiyon Analizi

Trade öncesi mid hareketi (UP için up_mid, DOWN için down_mid). Eşik: |Δmid| > 0.005.

| Lookback | UP follow | UP against | UP flat | DN follow | DN against | DN flat |
|----------|-----------|------------|---------|-----------|------------|---------|
| 5s | 32 | 99 | 18 | 89 | 51 | 17 |
| 10s | 44 | 102 | 3 | 103 | 42 | 12 |
| 30s | 64 | 79 | 0 | 84 | 60 | 0 |

_NOT: 'follow' = trend yönünde alım. UP için mid yükselişi + UP alımı = follow. DOWN için mid düşüşü + DOWN alımı = follow (tabloda DN follow kolonu = `dn_against` çünkü o değişken 'mid yükseldi & DOWN' tanımlıydı, ama DOWN trend takibi mid düşüşüdür → `dn_against` rolü swap edildi)._


## 6. Eşik Alımları

- **Lottery DOWN** (≤0.10): 0 trade, $0.00
- **Düşük DOWN** (≤0.20): 24 trade
- **Favori UP** (≥0.85): 8 trade, $26.10
- **Yüksek UP** (≥0.70): 45 trade

## 7. Fiyat Tutarlılığı (vs tick spread)

- **Eşleşen trade** (bid/ask>0): 306/306
- **Spread içi (bid≤p≤ask)**: 125 (40.8%)
- **Maker tahmini (p ≤ bid)**: 169 (55.2%)
- **Taker tahmini (p ≥ ask)**: 133 (43.5%)
- **Inside spread**: 4 (1.3%)
- **Δ(p − ask)**: min -0.250, medyan -0.010, ort -0.008, max +0.220
- **Δ(p − mid)**: min -0.245, medyan -0.005, ort -0.003, max +0.225
- **Anomaliler (|p−ask|>0.02 veya p<bid−0.02)**: 141

**En büyük 10 anomali (|Δ_ask| azalan)**:

| ts | outcome | price | size | bid | ask | Δ_ask |
|----|---------|-------|------|-----|-----|-------|
| 1777125768000 | Up | 0.430 | 5.00 | 0.670 | 0.680 | -0.250 |
| 1777125768000 | Down | 0.550 | 5.00 | 0.320 | 0.330 | +0.220 |
| 1777125780000 | Up | 0.640 | 5.00 | 0.750 | 0.850 | -0.210 |
| 1777125772000 | Down | 0.220 | 5.00 | 0.350 | 0.360 | -0.140 |
| 1777125724000 | Down | 0.360 | 5.00 | 0.480 | 0.490 | -0.130 |
| 1777125772000 | Down | 0.230 | 5.00 | 0.350 | 0.360 | -0.130 |
| 1777125772000 | Down | 0.240 | 5.00 | 0.350 | 0.360 | -0.120 |
| 1777125842000 | Down | 0.130 | 5.00 | 0.240 | 0.250 | -0.120 |
| 1777125724000 | Up | 0.630 | 5.00 | 0.510 | 0.520 | +0.110 |
| 1777125772000 | Down | 0.250 | 5.00 | 0.350 | 0.360 | -0.110 |

## 8. Bot Strateji Çıkarımı (bu market)

- **Bütçe**: Çok küçük emirler — medyan $1.85, max $4.40 (sıkı bütçeli bot)
- **Hedge yapısı**: Dengeli iki yön — UP 149 (49%) / DN 157 (aktif hedge stratejisi)
- **Pozisyon yönü**: %100 BUY — pozisyon kapatma yok, scaling-in mantığı
- **Trend takibi**: Trend takipçi — UP alımları %30 mid yükselişi sonrası, DOWN alımları %66 mid düşüşü sonrası (10s lookback)
- **Burst kalıbı**: Algoritmik tetikleme — max 15 trade/sn, 20 saniyede 5+ burst (volatilite anları)
- **Emir bölünmesi**: 42 tx içinde multi-fill — büyük emirler kısmen dolduruluyor
- **Emir tipi**: Çoğunluk maker (%55) — pasif resting order baskın (taker %43)
- **Favori UP**: 8 adet UP @ ≥0.85 ($26.10 hacim) — favori takibi
- **Eşzamanlı hedge**: 30 saniyede aynı anda UP+DOWN alımı — çift yön emir akışı
- **Zamanlama**: En aktif dakika = 0 (75 trade), en sakin = 4 (16 trade)

## 9. Trade Timeline (özet)

**İlk 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777125614000 | T+14s | Up | 0.490 | 5.00 | $2.45 |
| 1777125618000 | T+18s | Down | 0.420 | 5.00 | $2.10 |
| 1777125618000 | T+18s | Down | 0.430 | 5.00 | $2.15 |
| 1777125618000 | T+18s | Down | 0.410 | 5.00 | $2.05 |
| 1777125620000 | T+20s | Down | 0.370 | 5.00 | $1.85 |
| 1777125620000 | T+20s | Down | 0.380 | 5.00 | $1.90 |
| 1777125620000 | T+20s | Down | 0.390 | 5.00 | $1.95 |
| 1777125620000 | T+20s | Down | 0.400 | 5.00 | $2.00 |
| 1777125622000 | T+22s | Up | 0.620 | 5.00 | $3.10 |
| 1777125624000 | T+24s | Down | 0.330 | 2.94 | $0.97 |

**En yoğun saniye trades** (max burst):

_Saniye: 1777125770 (T+170s, 15 trade)_

| outcome | price | size | $ |
|---------|-------|------|---|
| Down | 0.370 | 1.43 | $0.53 |
| Down | 0.380 | 5.00 | $1.90 |
| Down | 0.390 | 5.00 | $1.95 |
| Down | 0.280 | 4.85 | $1.36 |
| Down | 0.290 | 2.19 | $0.64 |
| Down | 0.290 | 2.80 | $0.81 |
| Down | 0.300 | 0.37 | $0.11 |
| Down | 0.300 | 2.63 | $0.79 |
| Down | 0.310 | 2.37 | $0.73 |
| Down | 0.320 | 4.75 | $1.52 |
| Down | 0.340 | 2.00 | $0.68 |
| Down | 0.340 | 0.51 | $0.17 |
| Down | 0.360 | 0.06 | $0.02 |
| Down | 0.360 | 4.93 | $1.77 |
| Down | 0.370 | 3.57 | $1.32 |

**Son 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777125842000 | T+242s | Up | 0.780 | 5.00 | $3.90 |
| 1777125842000 | T+242s | Up | 0.790 | 5.00 | $3.95 |
| 1777125842000 | T+242s | Up | 0.800 | 5.00 | $4.00 |
| 1777125842000 | T+242s | Up | 0.740 | 3.39 | $2.51 |
| 1777125842000 | T+242s | Down | 0.130 | 5.00 | $0.65 |
| 1777125842000 | T+242s | Down | 0.140 | 4.74 | $0.66 |
| 1777125850000 | T+250s | Up | 0.660 | 5.00 | $3.30 |
| 1777125850000 | T+250s | Up | 0.670 | 5.00 | $3.35 |
| 1777125852000 | T+252s | Up | 0.630 | 1.87 | $1.18 |
| 1777125852000 | T+252s | Up | 0.630 | 3.13 | $1.97 |
