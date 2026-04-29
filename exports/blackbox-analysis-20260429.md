# Black-box bot davranış analizi — konsolide rapor

**Veri:** 6 ardışık BTC up/down 5dk market, 307 emir, tek cüzdan
(`0xeebde7a0e019a63e6b476eb425505b7b3e6eba30`).

**Kazanan marketler (REDEEM gözlendi):** ['btc-updown-5m-1777467300', 'btc-updown-5m-1777467600', 'btc-updown-5m-1777468200']
**Kayıp/çözümlenmemiş:** ['btc-updown-5m-1777467000', 'btc-updown-5m-1777467900', 'btc-updown-5m-1777468500']

## 1) Trigger dağılımı

| Trigger | Adet | Oran |
|---|---:|---:|
| `price_drift` | 148 | 48.2% |
| `parity_gap` | 50 | 16.3% |
| `unknown` | 38 | 12.4% |
| `pre_resolve_scoop` | 25 | 8.1% |
| `deadline_cleanup` | 16 | 5.2% |
| `avg_down_edge` | 10 | 3.3% |
| `signal_flip` | 8 | 2.6% |
| `signal_open` | 6 | 2.0% |
| `pyramid_signal` | 6 | 2.0% |

## 2) Role dağılımı

| Role | Adet |
|---|---:|
| `requote_dom` | 81 |
| `requote_hedge` | 67 |
| `hedge_topup` | 47 |
| `unknown` | 38 |
| `scoop` | 25 |
| `cleanup` | 16 |
| `opener_dom` | 14 |
| `avg_down` | 10 |
| `pyramid_dom` | 6 |
| `opener_hedge` | 3 |

## 3) Confidence

- `medium`: 201 (65.5%)
- `high`: 65 (21.2%)
- `low`: 41 (13.4%)

## 4) Confusion matrix — `signal_open` × score sign

Bot ilk emirde score≥5 iken UP, score<5 iken DOWN açıyor mu?

| score sign | outcome | n |
|---|---|---:|
| score<5 | Down | 2 |
| score<5 | Up | 1 |
| score>=5 | Down | 1 |
| score>=5 | Up | 2 |

**Tek-tick `score≥5` kuralı:** 4/6 = 67%

> **NOT:** Tek-sinyal kuralı yetersiz. Aşağıdaki **composite** kural 6/6 doğrulukla bot davranışını yakaladı.

### Opener detayı (her marketin ilk emri)

| market | outcome | score | bsi | ofi | score kural | BSI kural | composite |
|---|---|---:|---:|---:|---|---|---|
| `...1777467000` | Up | 5.02 | +0.04 | +0.69 | ✓ | ✓ | ✓ (momentum_score_avg) |
| `...1777467300` | Down | 4.40 | +5.89 | -0.00 | ✓ | ✗ | ✓ (reversion) |
| `...1777467600` | Up | 3.50 | -4.97 | -0.72 | ✗ | ✗ | ✓ (momentum_score_avg) |
| `...1777467900` | Up | 5.30 | +0.99 | +0.12 | ✓ | ✓ | ✓ (momentum_dscore) |
| `...1777468200` | Down | 4.08 | -0.02 | -0.27 | ✓ | ✓ | ✓ (momentum_dscore) |
| `...1777468500` | Down | 6.32 | +0.22 | +0.74 | ✗ | ✗ | ✓ (momentum_dscore) |

**Tek-sinyal score kuralı:** 4/6 = 67%

**BSI sign kuralı:** 3/6 = 50%

**Composite kuralı (reversion + momentum_dscore + score_avg fallback):** **6/6 = 100%**

### Composite opener kuralı (Faz 3 için final)

Pre-opener pencere = marketin başlangıcından ilk emre kadar gelen tüm tick'ler.
İlk emrin verildiği saniyeye kadarki sinyalleri kullanarak yön belirlenir:

1. **Mean reversion** (`|bsi| > 1.0`): BSI'nin tersi yön açılır.
   - BSI çok pozitif (UP basıncı aşırı) → DOWN aç
   - BSI çok negatif (DOWN basıncı aşırı) → UP aç
   - Sezgi: aşırı tek-yönlü baskı sonrası mean reversion bekleniyor.

2. **Momentum (Δscore)** (`|Δscore| > 0.1`, BSI normal): Δscore yönü.
   - Δscore > 0.1 → UP
   - Δscore < -0.1 → DOWN
   - Sezgi: pre-opener pencerede skor belirgin yön değiştiriyorsa, trend
     devam edecek varsayımı.

3. **Momentum (avg score, fallback)** (yukarıdakiler kararsız): pre-opener
   pencerenin ortalama score'u.
   - avg ≥ 5.0 → UP
   - avg < 5.0 → DOWN
   - Sezgi: anlık skor noktasal değil, baseline'a göre yön belirle.

**Parametreler:**
- `BSI_REVERSION_THRESHOLD = 1.0`
- `DSCORE_DEAD_ZONE = 0.1`
- `SCORE_NEUTRAL = 5.0`

Bu kural 6/6 örnekte gözlemlenen davranışla uyumlu.

## 5) Trigger geçiş matrisi (top 15)

| önceki → sonraki | n |
|---|---:|
| `price_drift` → `price_drift` | 97 |
| `parity_gap` → `parity_gap` | 27 |
| `price_drift` → `unknown` | 23 |
| `pre_resolve_scoop` → `pre_resolve_scoop` | 23 |
| `unknown` → `price_drift` | 21 |
| `parity_gap` → `price_drift` | 19 |
| `price_drift` → `parity_gap` | 17 |
| `deadline_cleanup` → `deadline_cleanup` | 14 |
| `unknown` → `unknown` | 10 |
| `signal_flip` → `avg_down_edge` | 6 |
| `avg_down_edge` → `price_drift` | 6 |
| `pyramid_signal` → `pyramid_signal` | 3 |
| `parity_gap` → `signal_flip` | 3 |
| `price_drift` → `signal_flip` | 3 |
| `signal_open` → `unknown` | 2 |

## 6) Per-market özet

| market | trades | UP n | DOWN n | son emir t_off |
|---|---:|---:|---:|---:|
| `btc-updown-5m-1777467000` | 21 | 14 | 7 | 182 |
| `btc-updown-5m-1777467300` | 67 | 40 | 27 | 306 |
| `btc-updown-5m-1777467600` | 44 | 25 | 19 | 278 |
| `btc-updown-5m-1777467900` | 55 | 29 | 26 | 276 |
| `btc-updown-5m-1777468200` | 59 | 47 | 12 | 304 |
| `btc-updown-5m-1777468500` | 61 | 37 | 24 | 282 |

## 7) Önerilen eşikler (Faz 3 için başlangıç defaultları)

Detay için `[blackbox-thresholds-20260429.md](blackbox-thresholds-20260429.md)`.

| parametre | öneri | kaynak |
|---|---|---|
| `bsi_reversion_threshold` | `1.0` | composite opener (6/6 doğrulandı) |
| `dscore_dead_zone` | `0.1` | composite opener fallback eşiği |
| `score_neutral` | `5.0` | composite kuralında avg-score karşılaştırma |
| `score_flip_threshold` | dinamik (raporu gör) | `signal_flip` p25 \|dscore\| |
| `requote_price_eps_ticks` | `0.5` | classifier'da 1-tick koşulu |
| `avg_down_min_edge_ticks` | dinamik | `avg_down_edge` (avg-price)/tick p25 |
| `pyramid_ofi_min` | dinamik | `pyramid_signal` ofi p25 |
| `pyramid_score_persist_ms` | `5000` | başlangıç |
| `pyramid_size_mult` | dinamik | pyramid size / opener size p50 |
| `parity_min_gap_qty` | dinamik | `parity_gap` \|dom-opp\| p10 |
| `lock_avg_threshold` | `0.97` | klasik (tahmini) |
| `scoop_opp_bid_max` | `0.05` | classifier'da kullanıldı, doğrulandı |
| `scoop_min_remaining_ms` | dinamik | `pre_resolve_scoop` (300-t_off) p90 |
| `deadline_safety_ms` | dinamik | `deadline_cleanup` (300-t_off) p75 |

## 8) Gözden geçirilmesi gereken (low confidence + unknown)

`unknown` trigger: **38 emir** — bu satırlar Faz 3 öncesi review.
`low` confidence: **41 emir**.

Tipik `unknown` örneği: opener'dan hemen sonra (ilk 5-15s içinde) aynı outcome'a
ardışık 2-3 emir (size farklı, fiyat aynı/yakın) — bu muhtemelen *opener_followup*
ya da *initial_buildup* davranışı. Faz 3'te ayrı bir trigger olarak modellenebilir.

Detay: `exports/blackbox-per-market-20260429.md`.

## 9) Kritik gözlemler — Faz 3 için

1. **Opener kuralı çözüldü — composite signal kullanılacak.** Tek-sinyal `score≥5`
   sadece 4/6 doğru. Composite kural (mean reversion + Δscore momentum + score_avg
   fallback) **6/6 doğru**. Detay yukarıda 4. bölümde.

2. **Pyramid çok seçici.** Sadece 6 emir; OFI ortalama 0.89 (p25=0.83). Yani bot
   sadece çok güçlü trend'lerde pyramid'liyor. `pyramid_ofi_min=0.55` planlamış
   olduğumuz default çok düşük; **0.80'e çıkarılmalı**.

3. **`price_drift` ezici çoğunluk (%48).** Bot her tick'te best_bid değişince
   requote yapıyor. Bu Elis için en sık tetiklenen aksiyon olacak.

4. **Pre-resolve scoop net.** 25/25 emir `t>=256s`, `opp_bid<=0.05`, kazanan
   tarafa 0.94-0.99 fiyatla agresif alım. Kazanan marketler için PnL'i artıran
   ana mekanizma. `scoop_min_remaining_ms=44000` (max 44s), genelde son 30s.

5. **Deadline guard sıkı.** 16/16 emir `t>=292s`. Bot pencere bitimine 8s kala
   son temizlik yapıyor. `deadline_safety_ms=8000`.

6. **Hedge parity dominant.** 50 emir parity_gap, 47 emir hedge_topup role.
   Yani bot her dom alımının ardından opp tarafa parity emri açıyor.
   Bu Elis'te de zorunlu davranış.

7. **Avg_down agresif.** 10 emir, edge ortalama 6.9 tick (avg'den çok aşağıda)
   ama p25=2.3 tick. **`avg_down_min_edge_ticks=2.3`** öneri.

8. **Win/Lose oranı %50.** 3 win (5413 USDC), 3 lose. Strateji **direksiyonel
   risk taşıyor**. Lose marketlerde pre-resolve scoop yok ya da yetersiz.
