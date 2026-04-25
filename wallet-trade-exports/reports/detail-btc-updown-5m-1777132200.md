# btc-updown-5m-1777132200 — Bot Strateji Analizi

## 1. Market Bilgileri

- **Slug**: `btc-updown-5m-1777132200`
- **Market start (UTC)**: `1777132200471`
- **Market window**: 5 dakika (300 sn)
- **Tick sayısı**: 301
- **Trade sayısı**: 319
- **İlk trade offset**: T+8s
- **Son trade offset**: T+244s

## 2. Bot Davranış Profili

- **Toplam**: 319 trade — {'BUY': 319}
- **UP / DN**: 160 / 159
- **Hacim**: UP $258.44, DN $393.05, **toplam $651.50**
- **Shares**: UP 662.16, DN 665.36
- **Notional**: min $0.01, medyan $1.75, ort $2.04, max $4.90
- **Size**: min 0.02, medyan 5.00, max 5.00
- **Tam sayı size**: 219/319 (68.7%)
- **5'in katı size**: 217/319 (68.0%)
- **Tx**: 255 benzersiz, **40 multi-fill**

## 3. Burst & Eşzamanlılık

- **Aktif saniye**: 87/300
- **Max burst**: 14 trade/sn
- **5+ burst saniye**: 21
- **Eşzamanlı UP+DOWN saniye**: 19
- **Trade/aktif saniye**: 3.67

## 4. Zaman Dilimi (1dk eşit)

| Dilim | Trade | UP | DN | Hacim UP | Hacim DN | Trade UP fiyat | Tick UP mid | Sapma | Score ort |
|-------|-------|----|----|----------|----------|---------------|-------------|-------|-----------|
| 0-60s | 51 | 22 | 29 | $52.48 | $44.20 | 0.606 | 0.598 | +0.008 | 4.74 |
| 60-120s | 149 | 80 | 69 | $167.16 | $118.36 | 0.512 | 0.503 | +0.009 | 4.39 |
| 120-180s | 83 | 42 | 41 | $33.96 | $137.30 | 0.196 | 0.222 | -0.025 | 2.09 |
| 180-240s | 34 | 15 | 19 | $4.80 | $88.30 | 0.067 | 0.036 | +0.031 | 0.89 |
| 240-300s | 2 | 1 | 1 | $0.05 | $4.90 | 0.010 | 0.015 | -0.005 | 0.96 |

## 5. Tick Reaksiyon Analizi

Trade öncesi mid hareketi (UP için up_mid, DOWN için down_mid). Eşik: |Δmid| > 0.005.

| Lookback | UP follow | UP against | UP flat | DN follow | DN against | DN flat |
|----------|-----------|------------|---------|-----------|------------|---------|
| 5s | 34 | 114 | 11 | 71 | 85 | 3 |
| 10s | 46 | 109 | 4 | 77 | 72 | 5 |
| 30s | 35 | 118 | 0 | 41 | 107 | 0 |

_NOT: 'follow' = trend yönünde alım. UP için mid yükselişi + UP alımı = follow. DOWN için mid düşüşü + DOWN alımı = follow (tabloda DN follow kolonu = `dn_against` çünkü o değişken 'mid yükseldi & DOWN' tanımlıydı, ama DOWN trend takibi mid düşüşüdür → `dn_against` rolü swap edildi)._


## 6. Eşik Alımları

- **Lottery DOWN** (≤0.10): 0 trade, $0.00
- **Düşük DOWN** (≤0.20): 0 trade
- **Favori UP** (≥0.85): 0 trade, $0.00
- **Yüksek UP** (≥0.70): 7 trade

## 7. Fiyat Tutarlılığı (vs tick spread)

- **Eşleşen trade** (bid/ask>0): 318/319
- **Spread içi (bid≤p≤ask)**: 73 (23.0%)
- **Maker tahmini (p ≤ bid)**: 156 (49.1%)
- **Taker tahmini (p ≥ ask)**: 162 (50.9%)
- **Inside spread**: 0 (0.0%)
- **Δ(p − ask)**: min -0.220, medyan +0.000, ort -0.008, max +0.170
- **Δ(p − mid)**: min -0.215, medyan +0.005, ort -0.003, max +0.175
- **Anomaliler (|p−ask|>0.02 veya p<bid−0.02)**: 188

**En büyük 10 anomali (|Δ_ask| azalan)**:

| ts | outcome | price | size | bid | ask | Δ_ask |
|----|---------|-------|------|-----|-----|-------|
| 1777132310000 | Down | 0.580 | 5.00 | 0.790 | 0.800 | -0.220 |
| 1777132328000 | Up | 0.190 | 5.00 | 0.310 | 0.370 | -0.180 |
| 1777132358000 | Up | 0.240 | 5.00 | 0.060 | 0.070 | +0.170 |
| 1777132356000 | Down | 0.680 | 5.00 | 0.820 | 0.840 | -0.160 |
| 1777132216000 | Down | 0.320 | 2.26 | 0.470 | 0.480 | -0.160 |
| 1777132356000 | Down | 0.690 | 2.11 | 0.820 | 0.840 | -0.150 |
| 1777132356000 | Down | 0.690 | 2.89 | 0.820 | 0.840 | -0.150 |
| 1777132328000 | Up | 0.220 | 5.00 | 0.310 | 0.370 | -0.150 |
| 1777132216000 | Down | 0.330 | 5.00 | 0.470 | 0.480 | -0.150 |
| 1777132216000 | Down | 0.330 | 2.35 | 0.470 | 0.480 | -0.150 |

## 8. Bot Strateji Çıkarımı (bu market)

- **Bütçe**: Çok küçük emirler — medyan $1.75, max $4.90 (sıkı bütçeli bot)
- **Hedge yapısı**: Dengeli iki yön — UP 160 (50%) / DN 159 (aktif hedge stratejisi)
- **Pozisyon yönü**: %100 BUY — pozisyon kapatma yok, scaling-in mantığı
- **Yön nötr**: Tick reaksiyonu zayıf — UP follow %29, DOWN follow %50
- **Burst kalıbı**: Algoritmik tetikleme — max 14 trade/sn, 21 saniyede 5+ burst (volatilite anları)
- **Emir bölünmesi**: 40 tx içinde multi-fill — büyük emirler kısmen dolduruluyor
- **Emir tipi**: Karışık — maker %49 / taker %51 / inside %0
- **Eşzamanlı hedge**: 19 saniyede aynı anda UP+DOWN alımı — çift yön emir akışı
- **Zamanlama**: En aktif dakika = 1 (149 trade), en sakin = 4 (2 trade)

## 9. Trade Timeline (özet)

**İlk 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777132208000 | T+8s | Down | 0.440 | 5.00 | $2.20 |
| 1777132208000 | T+8s | Down | 0.450 | 5.00 | $2.25 |
| 1777132210000 | T+10s | Down | 0.460 | 5.00 | $2.30 |
| 1777132210000 | T+10s | Down | 0.460 | 5.00 | $2.30 |
| 1777132210000 | T+10s | Down | 0.440 | 5.00 | $2.20 |
| 1777132214000 | T+14s | Up | 0.550 | 5.00 | $2.75 |
| 1777132214000 | T+14s | Down | 0.340 | 5.00 | $1.70 |
| 1777132214000 | T+14s | Up | 0.550 | 5.00 | $2.75 |
| 1777132216000 | T+16s | Down | 0.330 | 2.35 | $0.77 |
| 1777132216000 | T+16s | Down | 0.320 | 2.26 | $0.72 |

**En yoğun saniye trades** (max burst):

_Saniye: 1777132282 (T+82s, 14 trade)_

| outcome | price | size | $ |
|---------|-------|------|---|
| Down | 0.280 | 5.00 | $1.40 |
| Up | 0.670 | 5.00 | $3.35 |
| Up | 0.660 | 0.04 | $0.03 |
| Up | 0.680 | 5.00 | $3.40 |
| Up | 0.690 | 4.20 | $2.90 |
| Up | 0.700 | 4.72 | $3.30 |
| Up | 0.720 | 5.00 | $3.60 |
| Down | 0.280 | 5.00 | $1.40 |
| Up | 0.710 | 3.45 | $2.45 |
| Up | 0.710 | 1.22 | $0.87 |
| Down | 0.290 | 5.00 | $1.45 |
| Down | 0.300 | 5.00 | $1.50 |
| Down | 0.310 | 5.00 | $1.55 |
| Down | 0.270 | 5.00 | $1.35 |

**Son 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777132406000 | T+206s | Down | 0.970 | 5.00 | $4.85 |
| 1777132412000 | T+212s | Up | 0.030 | 5.00 | $0.15 |
| 1777132414000 | T+214s | Down | 0.960 | 5.00 | $4.80 |
| 1777132416000 | T+216s | Up | 0.030 | 4.66 | $0.14 |
| 1777132416000 | T+216s | Up | 0.030 | 0.34 | $0.01 |
| 1777132428000 | T+228s | Down | 0.980 | 5.00 | $4.90 |
| 1777132436000 | T+236s | Down | 0.970 | 5.00 | $4.85 |
| 1777132438000 | T+238s | Up | 0.020 | 5.00 | $0.10 |
| 1777132442000 | T+242s | Down | 0.980 | 5.00 | $4.90 |
| 1777132444000 | T+244s | Up | 0.010 | 5.00 | $0.05 |
