# btc-updown-5m-1777050900 — Bot Strateji Analizi

## 1. Market Bilgileri

- **Slug**: `btc-updown-5m-1777050900`
- **Market start (UTC)**: `1777050900688`
- **Market window**: 5 dakika (300 sn)
- **Tick sayısı**: 301
- **Trade sayısı**: 219
- **İlk trade offset**: T+16s
- **Son trade offset**: T+228s

## 2. Bot Davranış Profili

- **Toplam**: 219 trade — {'BUY': 219}
- **UP / DN**: 137 / 82
- **Hacim**: UP $373.57, DN $1,777.00, **toplam $2,150.57**
- **Shares**: UP 2531.94, DN 2468.71
- **Notional**: min $0.01, medyan $1.51, ort $9.82, max $96.00
- **Size**: min 0.06, medyan 8.53, max 100.00
- **Tam sayı size**: 47/219 (21.5%)
- **5'in katı size**: 37/219 (16.9%)
- **Tx**: 216 benzersiz, **3 multi-fill**

## 3. Burst & Eşzamanlılık

- **Aktif saniye**: 54/300
- **Max burst**: 19 trade/sn
- **5+ burst saniye**: 17
- **Eşzamanlı UP+DOWN saniye**: 5
- **Trade/aktif saniye**: 4.06

## 4. Zaman Dilimi (1dk eşit)

| Dilim | Trade | UP | DN | Hacim UP | Hacim DN | Trade UP fiyat | Tick UP mid | Sapma | Score ort |
|-------|-------|----|----|----------|----------|---------------|-------------|-------|-----------|
| 0-60s | 36 | 1 | 35 | $50.00 | $420.18 | 0.500 | 0.456 | +0.044 | 4.44 |
| 60-120s | 32 | 19 | 13 | $105.90 | $355.91 | 0.368 | 0.353 | +0.015 | 3.56 |
| 120-180s | 112 | 81 | 31 | $192.84 | $716.91 | 0.107 | 0.187 | -0.081 | 2.73 |
| 180-240s | 39 | 36 | 3 | $24.83 | $284.00 | 0.040 | 0.028 | +0.013 | 1.65 |
| 240-300s | 0 | 0 | 0 | $0.00 | $0.00 | 0.000 | 0.000 | +0.000 | 1.69 |

## 5. Tick Reaksiyon Analizi

Trade öncesi mid hareketi (UP için up_mid, DOWN için down_mid). Eşik: |Δmid| > 0.005.

| Lookback | UP follow | UP against | UP flat | DN follow | DN against | DN flat |
|----------|-----------|------------|---------|-----------|------------|---------|
| 5s | 20 | 102 | 14 | 38 | 31 | 13 |
| 10s | 24 | 109 | 3 | 30 | 44 | 8 |
| 30s | 4 | 122 | 10 | 23 | 38 | 0 |

_NOT: 'follow' = trend yönünde alım. UP için mid yükselişi + UP alımı = follow. DOWN için mid düşüşü + DOWN alımı = follow (tabloda DN follow kolonu = `dn_against` çünkü o değişken 'mid yükseldi & DOWN' tanımlıydı, ama DOWN trend takibi mid düşüşüdür → `dn_against` rolü swap edildi)._


## 6. Eşik Alımları

- **Lottery DOWN** (≤0.10): 0 trade, $0.00
- **Düşük DOWN** (≤0.20): 0 trade
- **Favori UP** (≥0.85): 0 trade, $0.00
- **Yüksek UP** (≥0.70): 0 trade

## 7. Fiyat Tutarlılığı (vs tick spread)

- **Eşleşen trade** (bid/ask>0): 218/219
- **Spread içi (bid≤p≤ask)**: 119 (54.6%)
- **Maker tahmini (p ≤ bid)**: 89 (40.8%)
- **Taker tahmini (p ≥ ask)**: 128 (58.7%)
- **Inside spread**: 1 (0.5%)
- **Δ(p − ask)**: min -0.060, medyan +0.000, ort -0.004, max +0.060
- **Δ(p − mid)**: min -0.055, medyan +0.005, ort +0.001, max +0.065
- **Anomaliler (|p−ask|>0.02 veya p<bid−0.02)**: 52

**En büyük 10 anomali (|Δ_ask| azalan)**:

| ts | outcome | price | size | bid | ask | Δ_ask |
|----|---------|-------|------|-----|-----|-------|
| 1777050922000 | Down | 0.550 | 40.00 | 0.480 | 0.490 | +0.060 |
| 1777050922000 | Down | 0.550 | 50.20 | 0.480 | 0.490 | +0.060 |
| 1777050922000 | Down | 0.550 | 9.80 | 0.480 | 0.490 | +0.060 |
| 1777051010000 | Down | 0.570 | 9.57 | 0.620 | 0.630 | -0.060 |
| 1777051076000 | Down | 0.870 | 4.89 | 0.920 | 0.930 | -0.060 |
| 1777051076000 | Down | 0.870 | 40.00 | 0.920 | 0.930 | -0.060 |
| 1777051076000 | Down | 0.870 | 25.00 | 0.920 | 0.930 | -0.060 |
| 1777051076000 | Down | 0.870 | 11.22 | 0.920 | 0.930 | -0.060 |
| 1777051076000 | Down | 0.870 | 3.50 | 0.920 | 0.930 | -0.060 |
| 1777051076000 | Down | 0.870 | 15.38 | 0.920 | 0.930 | -0.060 |

## 8. Bot Strateji Çıkarımı (bu market)

- **Bütçe**: Çoğunlukla küçük (medyan $1.51) ama büyük tek seferlik emirler var (max $96.00)
- **Yön ağırlığı**: UP ağırlık — UP 137 (63%) / DN 82 (UP trend takibi olası)
- **Pozisyon yönü**: %100 BUY — pozisyon kapatma yok, scaling-in mantığı
- **Kontrarian davranış**: Trend tersine alım baskın — UP follow %18, DOWN follow %37
- **Burst kalıbı**: Algoritmik tetikleme — max 19 trade/sn, 17 saniyede 5+ burst (volatilite anları)
- **Emir tipi**: Karışık — maker %41 / taker %59 / inside %0
- **Eşzamanlı hedge**: 5 saniyede aynı anda UP+DOWN alımı — çift yön emir akışı
- **Zamanlama**: En aktif dakika = 2 (112 trade), en sakin = 4 (0 trade)

## 9. Trade Timeline (özet)

**İlk 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777050916000 | T+16s | Down | 0.600 | 60.64 | $36.38 |
| 1777050916000 | T+16s | Down | 0.600 | 2.52 | $1.51 |
| 1777050916000 | T+16s | Down | 0.600 | 5.00 | $3.00 |
| 1777050916000 | T+16s | Down | 0.600 | 8.65 | $5.19 |
| 1777050916000 | T+16s | Down | 0.600 | 3.08 | $1.84 |
| 1777050916000 | T+16s | Down | 0.600 | 2.60 | $1.56 |
| 1777050916000 | T+16s | Down | 0.600 | 5.00 | $3.00 |
| 1777050916000 | T+16s | Down | 0.600 | 12.50 | $7.50 |
| 1777050918000 | T+18s | Down | 0.590 | 12.20 | $7.20 |
| 1777050918000 | T+18s | Down | 0.590 | 12.20 | $7.20 |

**En yoğun saniye trades** (max burst):

_Saniye: 1777051070 (T+170s, 19 trade)_

| outcome | price | size | $ |
|---------|-------|------|---|
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.100 | 1.11 | $0.11 |
| Up | 0.100 | 5.00 | $0.50 |
| Up | 0.100 | 1.11 | $0.11 |
| Up | 0.100 | 10.67 | $1.07 |
| Up | 0.090 | 2.00 | $0.18 |
| Up | 0.090 | 2.00 | $0.18 |
| Up | 0.090 | 2.00 | $0.18 |
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.090 | 2.09 | $0.19 |
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.090 | 28.57 | $2.57 |
| Up | 0.090 | 1.10 | $0.10 |
| Up | 0.090 | 2.09 | $0.19 |
| Up | 0.090 | 1.10 | $0.10 |

**Son 10 trade:**

| ts | T+ | outcome | price | size | $ |
|----|----|---------|-------|------|---|
| 1777051104000 | T+204s | Up | 0.020 | 20.00 | $0.40 |
| 1777051104000 | T+204s | Up | 0.020 | 9.18 | $0.18 |
| 1777051104000 | T+204s | Up | 0.020 | 16.33 | $0.33 |
| 1777051104000 | T+204s | Up | 0.020 | 6.12 | $0.12 |
| 1777051106000 | T+206s | Up | 0.020 | 41.35 | $0.83 |
| 1777051116000 | T+216s | Down | 0.960 | 100.00 | $96.00 |
| 1777051120000 | T+220s | Up | 0.020 | 86.34 | $1.73 |
| 1777051120000 | T+220s | Up | 0.020 | 13.66 | $0.27 |
| 1777051122000 | T+222s | Up | 0.020 | 100.00 | $2.00 |
| 1777051128000 | T+228s | Up | 0.010 | 100.00 | $1.00 |
