# btc-updown-5m-1777125900 — Bot Strateji Analizi

## 1. Market Bilgileri

- **Slug**: `btc-updown-5m-1777125900`
- **Market start (UTC)**: `1777125900729`
- **Market window**: 5 dakika (300 sn)
- **Tick sayısı**: 301
- **Trade sayısı**: 140
- **İlk trade offset**: T+6s
- **Son trade offset**: T+294s

## 2. Bot Davranış Profili

- **Toplam**: 140 trade — {'BUY': 140}
- **UP / DN**: 65 / 75
- **Hacim**: UP $183.64, DN $99.29, **toplam $282.92**
- **Shares**: UP 271.67, DN 278.64
- **Notional**: min $0.01, medyan $2.20, ort $2.02, max $4.90
- **Size**: min 0.04, medyan 5.00, max 5.00
- **Tam sayı size**: 88/140 (62.9%)
- **5'in katı size**: 87/140 (62.1%)
- **Tx**: 118 benzersiz, **18 multi-fill**

## 3. Burst & Eşzamanlılık

- **Aktif saniye**: 61/300
- **Max burst**: 8 trade/sn
- **5+ burst saniye**: 6
- **Eşzamanlı UP+DOWN saniye**: 6
- **Trade/aktif saniye**: 2.30

## 4. Zaman Dilimi (1dk eşit)

| Dilim | Trade | UP | DN | Hacim UP | Hacim DN | Trade UP fiyat | Tick UP mid | Sapma | Score ort |
|-------|-------|----|----|----------|----------|---------------|-------------|-------|-----------|
| 0-60s | 22 | 6 | 16 | $9.60 | $29.90 | 0.467 | 0.531 | -0.064 | 3.77 |
| 60-120s | 19 | 15 | 4 | $26.25 | $9.02 | 0.475 | 0.527 | -0.051 | 3.91 |
| 120-180s | 38 | 15 | 23 | $33.37 | $48.12 | 0.483 | 0.554 | -0.071 | 5.65 |
| 180-240s | 50 | 18 | 32 | $61.53 | $12.25 | 0.851 | 0.858 | -0.006 | 7.24 |
| 240-300s | 11 | 11 | 0 | $52.90 | $0.00 | 0.962 | 0.977 | -0.016 | 7.37 |

## 5. Tick Reaksiyon Analizi

Trade öncesi mid hareketi (UP için up_mid, DOWN için down_mid). Eşik: |Δmid| > 0.005.

| Lookback | UP follow | UP against | UP flat | DN follow | DN against | DN flat |
|----------|-----------|------------|---------|-----------|------------|---------|
| 5s | 25 | 33 | 6 | 53 | 8 | 14 |
| 10s | 25 | 34 | 2 | 58 | 7 | 10 |
| 30s | 23 | 32 | 4 | 54 | 10 | 2 |

_NOT: 'follow' = trend yönünde alım. UP için mid yükselişi + UP alımı = follow. DOWN için mid düşüşü + DOWN alımı = follow (tabloda DN follow kolonu = `dn_against` çünkü o değişken 'mid yükseldi & DOWN' tanımlıydı, ama DOWN trend takibi mid düşüşüdür → `dn_against` rolü swap edildi)._


## 6. Eşik Alımları

- **Lottery DOWN** (≤0.10): 9 trade, $2.70
- **Düşük DOWN** (≤0.20): 30 trade
- **Favori UP** (≥0.85): 19 trade, $82.78
- **Yüksek UP** (≥0.70): 30 trade

## 7. Fiyat Tutarlılığı (vs tick spread)

- **Eşleşen trade** (bid/ask>0): 140/140
- **Spread içi (bid≤p≤ask)**: 71 (50.7%)
- **Maker tahmini (p ≤ bid)**: 75 (53.6%)
- **Taker tahmini (p ≥ ask)**: 58 (41.4%)
- **Inside spread**: 7 (5.0%)
- **Δ(p − ask)**: min -0.070, medyan -0.010, ort +0.000, max +0.200
- **Δ(p − mid)**: min -0.065, medyan -0.005, ort +0.006, max +0.210
- **Anomaliler (|p−ask|>0.02 veya p<bid−0.02)**: 49

**En büyük 10 anomali (|Δ_ask| azalan)**:

| ts | outcome | price | size | bid | ask | Δ_ask |
|----|---------|-------|------|-----|-----|-------|
| 1777126062000 | Down | 0.540 | 5.00 | 0.320 | 0.340 | +0.200 |
| 1777126062000 | Down | 0.530 | 5.00 | 0.320 | 0.340 | +0.190 |
| 1777126062000 | Down | 0.520 | 5.00 | 0.320 | 0.340 | +0.180 |
| 1777126062000 | Down | 0.510 | 5.00 | 0.320 | 0.340 | +0.170 |
| 1777126062000 | Down | 0.500 | 5.00 | 0.320 | 0.340 | +0.160 |
| 1777126062000 | Down | 0.490 | 5.00 | 0.320 | 0.340 | +0.150 |
| 1777126062000 | Down | 0.480 | 5.00 | 0.320 | 0.340 | +0.140 |
| 1777126062000 | Down | 0.470 | 5.00 | 0.320 | 0.340 | +0.130 |
| 1777125910000 | Up | 0.440 | 5.00 | 0.500 | 0.510 | -0.070 |
| 1777125996000 | Up | 0.470 | 5.00 | 0.520 | 0.530 | -0.060 |

## 8. Bot Strateji Çıkarımı (bu market)

- **Bütçe**: Çok küçük emirler — medyan $2.20, max $4.90 (sıkı bütçeli bot)
- **Hedge yapısı**: Dengeli iki yön — UP 65 (46%) / DN 75 (aktif hedge stratejisi)
- **Pozisyon yönü**: %100 BUY — pozisyon kapatma yok, scaling-in mantığı
- **Trend takibi**: Trend takipçi — UP alımları %41 mid yükselişi sonrası, DOWN alımları %77 mid düşüşü sonrası (10s lookback)
- **Burst kalıbı**: Yumuşak akış — max 8 trade/sn (6 burst saniyesi)
- **Emir bölünmesi**: 18 tx içinde multi-fill — büyük emirler kısmen dolduruluyor
- **Emir tipi**: Çoğunluk maker (%54) — pasif resting order baskın (taker %41)
- **Lottery DOWN**: 9 adet DOWN @ ≤0.10 ($2.70 hacim) — düşük fiyat hedge/lottery
- **Favori UP**: 19 adet UP @ ≥0.85 ($82.78 hacim) — favori takibi
- **Eşzamanlı hedge**: 6 saniyede aynı anda UP+DOWN alımı — çift yön emir akışı
- **Zamanlama**: En aktif dakika = 3 (50 trade), en sakin = 4 (11 trade)

## 9. Trade Timeline (özet)

**İlk 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777125906000 | T+6s | Up | 0.440 | 1.79 | $0.79 |
| 1777125908000 | T+8s | Up | 0.440 | 0.81 | $0.36 |
| 1777125908000 | T+8s | Up | 0.440 | 2.40 | $1.05 |
| 1777125910000 | T+10s | Up | 0.440 | 5.00 | $2.20 |
| 1777125912000 | T+12s | Down | 0.500 | 5.00 | $2.50 |
| 1777125912000 | T+12s | Down | 0.510 | 5.00 | $2.55 |
| 1777125918000 | T+18s | Down | 0.470 | 5.00 | $2.35 |
| 1777125920000 | T+20s | Down | 0.470 | 5.00 | $2.35 |
| 1777125920000 | T+20s | Down | 0.460 | 1.66 | $0.76 |
| 1777125920000 | T+20s | Up | 0.520 | 5.00 | $2.60 |

**En yoğun saniye trades** (max burst):

_Saniye: 1777126062 (T+162s, 8 trade)_

| outcome | price | size | $ |
|---------|-------|------|---|
| Down | 0.470 | 5.00 | $2.35 |
| Down | 0.490 | 5.00 | $2.45 |
| Down | 0.510 | 5.00 | $2.55 |
| Down | 0.530 | 5.00 | $2.65 |
| Down | 0.540 | 5.00 | $2.70 |
| Down | 0.520 | 5.00 | $2.60 |
| Down | 0.480 | 5.00 | $2.40 |
| Down | 0.500 | 5.00 | $2.50 |

**Son 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777126178000 | T+278s | Up | 0.970 | 5.00 | $4.85 |
| 1777126180000 | T+280s | Up | 0.970 | 5.00 | $4.85 |
| 1777126182000 | T+282s | Up | 0.940 | 5.00 | $4.70 |
| 1777126182000 | T+282s | Up | 0.950 | 5.00 | $4.75 |
| 1777126182000 | T+282s | Up | 0.950 | 5.00 | $4.75 |
| 1777126182000 | T+282s | Up | 0.960 | 5.00 | $4.80 |
| 1777126182000 | T+282s | Up | 0.970 | 5.00 | $4.85 |
| 1777126192000 | T+292s | Up | 0.960 | 5.00 | $4.80 |
| 1777126194000 | T+294s | Up | 0.970 | 5.00 | $4.85 |
| 1777126194000 | T+294s | Up | 0.980 | 5.00 | $4.90 |
