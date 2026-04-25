# btc-updown-5m-1776969300 — Bot Strateji Analizi

## 1. Market Bilgileri

- **Slug**: `btc-updown-5m-1776969300`
- **Market start (UTC)**: `1776969300567`
- **Market window**: 5 dakika (300 sn)
- **Tick sayısı**: 301
- **Trade sayısı**: 388
- **İlk trade offset**: T+22s
- **Son trade offset**: T+296s

## 2. Bot Davranış Profili

- **Toplam**: 388 trade — {'BUY': 388}
- **UP / DN**: 166 / 222
- **Hacim**: UP $3,413.39, DN $518.27, **toplam $3,931.66**
- **Shares**: UP 4580.92, DN 4309.74
- **Notional**: min $0.02, medyan $3.04, ort $10.13, max $90.00
- **Size**: min 0.17, medyan 10.49, max 100.00
- **Tam sayı size**: 140/388 (36.1%)
- **5'in katı size**: 102/388 (26.3%)
- **Tx**: 375 benzersiz, **13 multi-fill**

## 3. Burst & Eşzamanlılık

- **Aktif saniye**: 64/300
- **Max burst**: 22 trade/sn
- **5+ burst saniye**: 34
- **Eşzamanlı UP+DOWN saniye**: 14
- **Trade/aktif saniye**: 6.06

## 4. Zaman Dilimi (1dk eşit)

| Dilim | Trade | UP | DN | Hacim UP | Hacim DN | Trade UP fiyat | Tick UP mid | Sapma | Score ort |
|-------|-------|----|----|----------|----------|---------------|-------------|-------|-----------|
| 0-60s | 24 | 18 | 6 | $236.21 | $45.00 | 0.531 | 0.527 | +0.004 | 5.40 |
| 60-120s | 59 | 52 | 7 | $904.86 | $40.00 | 0.613 | 0.623 | -0.011 | 6.87 |
| 120-180s | 139 | 39 | 100 | $909.12 | $270.88 | 0.819 | 0.845 | -0.026 | 8.32 |
| 180-240s | 151 | 56 | 95 | $1,352.99 | $126.85 | 0.904 | 0.912 | -0.008 | 8.38 |
| 240-300s | 15 | 1 | 14 | $10.22 | $35.54 | 0.900 | 0.867 | +0.033 | 7.77 |

## 5. Tick Reaksiyon Analizi

Trade öncesi mid hareketi (UP için up_mid, DOWN için down_mid). Eşik: |Δmid| > 0.005.

| Lookback | UP follow | UP against | UP flat | DN follow | DN against | DN flat |
|----------|-----------|------------|---------|-----------|------------|---------|
| 5s | 51 | 93 | 22 | 163 | 45 | 12 |
| 10s | 62 | 86 | 18 | 166 | 41 | 13 |
| 30s | 117 | 35 | 11 | 170 | 26 | 19 |

_NOT: 'follow' = trend yönünde alım. UP için mid yükselişi + UP alımı = follow. DOWN için mid düşüşü + DOWN alımı = follow (tabloda DN follow kolonu = `dn_against` çünkü o değişken 'mid yükseldi & DOWN' tanımlıydı, ama DOWN trend takibi mid düşüşüdür → `dn_against` rolü swap edildi)._


## 6. Eşik Alımları

- **Lottery DOWN** (≤0.10): 111 trade, $181.39
- **Düşük DOWN** (≤0.20): 209 trade
- **Favori UP** (≥0.85): 69 trade, $1,710.20
- **Yüksek UP** (≥0.70): 107 trade

## 7. Fiyat Tutarlılığı (vs tick spread)

- **Eşleşen trade** (bid/ask>0): 386/388
- **Spread içi (bid≤p≤ask)**: 160 (41.5%)
- **Maker tahmini (p ≤ bid)**: 210 (54.4%)
- **Taker tahmini (p ≥ ask)**: 168 (43.5%)
- **Inside spread**: 8 (2.1%)
- **Δ(p − ask)**: min -0.100, medyan -0.010, ort -0.006, max +0.060
- **Δ(p − mid)**: min -0.095, medyan -0.005, ort -0.001, max +0.065
- **Anomaliler (|p−ask|>0.02 veya p<bid−0.02)**: 136

**En büyük 10 anomali (|Δ_ask| azalan)**:

| ts | outcome | price | size | bid | ask | Δ_ask |
|----|---------|-------|------|-----|-----|-------|
| 1776969442000 | Up | 0.740 | 100.00 | 0.830 | 0.840 | -0.100 |
| 1776969378000 | Up | 0.520 | 9.17 | 0.580 | 0.590 | -0.070 |
| 1776969378000 | Up | 0.520 | 2.55 | 0.580 | 0.590 | -0.070 |
| 1776969378000 | Up | 0.520 | 31.25 | 0.580 | 0.590 | -0.070 |
| 1776969378000 | Up | 0.520 | 31.25 | 0.580 | 0.590 | -0.070 |
| 1776969378000 | Up | 0.520 | 14.94 | 0.580 | 0.590 | -0.070 |
| 1776969378000 | Up | 0.520 | 10.83 | 0.580 | 0.590 | -0.070 |
| 1776969406000 | Down | 0.400 | 48.33 | 0.330 | 0.340 | +0.060 |
| 1776969370000 | Up | 0.540 | 100.00 | 0.590 | 0.600 | -0.060 |
| 1776969378000 | Up | 0.530 | 12.91 | 0.580 | 0.590 | -0.060 |

## 8. Bot Strateji Çıkarımı (bu market)

- **Bütçe**: Çoğunlukla küçük (medyan $3.04) ama büyük tek seferlik emirler var (max $90.00)
- **Hedge yapısı**: Dengeli iki yön — UP 166 (43%) / DN 222 (aktif hedge stratejisi)
- **Pozisyon yönü**: %100 BUY — pozisyon kapatma yok, scaling-in mantığı
- **Trend takibi**: Trend takipçi — UP alımları %37 mid yükselişi sonrası, DOWN alımları %75 mid düşüşü sonrası (10s lookback)
- **Burst kalıbı**: Algoritmik tetikleme — max 22 trade/sn, 34 saniyede 5+ burst (volatilite anları)
- **Emir bölünmesi**: 13 tx içinde multi-fill — büyük emirler kısmen dolduruluyor
- **Emir tipi**: Çoğunluk maker (%54) — pasif resting order baskın (taker %44)
- **Lottery DOWN**: 111 adet DOWN @ ≤0.10 ($181.39 hacim) — düşük fiyat hedge/lottery
- **Favori UP**: 69 adet UP @ ≥0.85 ($1,710.20 hacim) — favori takibi
- **Eşzamanlı hedge**: 14 saniyede aynı anda UP+DOWN alımı — çift yön emir akışı
- **Zamanlama**: En aktif dakika = 3 (151 trade), en sakin = 4 (15 trade)

## 9. Trade Timeline (özet)

**İlk 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1776969322000 | T+22s | Up | 0.530 | 4.26 | $2.26 |
| 1776969322000 | T+22s | Up | 0.530 | 42.55 | $22.55 |
| 1776969324000 | T+24s | Up | 0.530 | 5.00 | $2.65 |
| 1776969330000 | T+30s | Down | 0.450 | 31.89 | $14.35 |
| 1776969330000 | T+30s | Down | 0.450 | 21.70 | $9.77 |
| 1776969330000 | T+30s | Down | 0.450 | 18.18 | $8.18 |
| 1776969330000 | T+30s | Down | 0.450 | 3.67 | $1.65 |
| 1776969330000 | T+30s | Down | 0.450 | 5.00 | $2.25 |
| 1776969332000 | T+32s | Down | 0.450 | 19.55 | $8.80 |
| 1776969340000 | T+40s | Up | 0.520 | 72.51 | $37.71 |

**En yoğun saniye trades** (max burst):

_Saniye: 1776969444 (T+144s, 22 trade)_

| outcome | price | size | $ |
|---------|-------|------|---|
| Down | 0.180 | 24.39 | $4.39 |
| Down | 0.180 | 40.00 | $7.20 |
| Down | 0.180 | 4.92 | $0.89 |
| Down | 0.160 | 2.10 | $0.34 |
| Down | 0.160 | 11.90 | $1.90 |
| Down | 0.160 | 5.00 | $0.80 |
| Down | 0.160 | 2.12 | $0.34 |
| Down | 0.160 | 6.64 | $1.06 |
| Down | 0.170 | 20.00 | $3.40 |
| Down | 0.160 | 5.00 | $0.80 |
| Down | 0.160 | 18.87 | $3.02 |
| Down | 0.160 | 5.06 | $0.81 |
| Down | 0.160 | 8.00 | $1.28 |
| Down | 0.160 | 5.78 | $0.92 |
| Down | 0.160 | 10.22 | $1.64 |
| Down | 0.170 | 4.78 | $0.81 |
| Down | 0.170 | 4.00 | $0.68 |
| Down | 0.170 | 5.96 | $1.01 |
| Down | 0.170 | 5.00 | $0.85 |
| Down | 0.170 | 60.24 | $10.24 |

**Son 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1776969546000 | T+246s | Down | 0.070 | 1.08 | $0.08 |
| 1776969546000 | T+246s | Down | 0.070 | 21.81 | $1.53 |
| 1776969546000 | T+246s | Down | 0.070 | 10.10 | $0.71 |
| 1776969550000 | T+250s | Down | 0.050 | 100.00 | $5.00 |
| 1776969550000 | T+250s | Down | 0.060 | 100.00 | $6.00 |
| 1776969594000 | T+294s | Down | 0.050 | 40.79 | $2.04 |
| 1776969594000 | T+294s | Down | 0.050 | 5.00 | $0.25 |
| 1776969594000 | T+294s | Down | 0.050 | 5.00 | $0.25 |
| 1776969596000 | T+296s | Down | 0.030 | 100.00 | $3.00 |
| 1776969596000 | T+296s | Down | 0.040 | 100.00 | $4.00 |
