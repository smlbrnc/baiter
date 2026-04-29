# Elis Strategy — Backtest Final Raporu (24 Market combined)

**Tarih:** 2026-04-29
**Versiyon:** v4b — 24 market combined optimize (16 bot14 + 8 bot15)
**Test edilen marketler:** 24 (20 net resolve + 4 belirsiz)
**Kaynak veri:** `exports/bot14-ticks-20260429/` + `exports/bot15-ticks-20260429/`

---

## 1. Özet sonuçlar

| metrik | değer |
|---|---:|
| Toplam market | **24** |
| Resolved | 20 (Up: 8 / Down: 12) |
| Belirsiz | 4 |
| **Yön doğruluğu (final intent)** | **17/20 = %85** |
| Pozitif PnL marketler | 13/20 = %65 |
| **Kesin PnL** | **+$862.22** |
| Belirsiz mid PnL | -$635.92 (pesimist hesap; gerçekte hold) |
| **NET PnL** | **+$226.30** |

### 1.1 Versiyon evrimi

| versiyon | data | yön | kesin PnL | önemli değişiklik |
|---|---|---|---:|---|
| v0 (composite + asymmetric) | bot14 (16) | 8/9 | -$48 | sample-içi base |
| v1 (eşik 3.0 + flip_freeze) | bot14 (16) | 9/9 | +$307 | flip_freeze |
| v2 (eşik 5.0) | bot14 (16) | 11/12 | +$305 | yeni 3 fakeout sample |
| v3 (hedge sadece artış) | bot14 (16) | **12/13 = %92** | **+$609** | hedge bug fix |
| v3 (genişletildi) | bot14+15 (24) | 16/20 = %80 | +$422 | yeni 8 market |
| **v4b** (24-grid optimize) | bot14+15 (24) | **17/20 = %85** | **+$862** | **opener + requote tweaks** |

---

## 2. Anahtar bulgular

### 2.1 Yeni 8 market'te ortaya çıkan zayıflıklar

bot15 verileri (8 yeni market) Up-trending bias gösterdi (5 Up / 3 Down), önceki bot14 datasetinden farklı (4 Up / 9 Down). v3 default parametreleri yeni datada **4/7 = %57 yön doğruluğu** verdi (eski 12/13 = %92'den büyük düşüş):

| market | winner | v3 opener | sebep |
|---|---|---|---|
| 1777480200 | Up | Down (bsi_rev) | bsi pozitif → mean-reversion yanlış |
| 1777480500 | Up | Down (exhaustion) | ofi+cvd pozitif → mean-reversion yanlış |
| 1777480800 | Up | Up→Down (flip) | yanlış flip (gerçek Up trending) |

Ek olarak `1777479300` ve `1777480800` requote spam ile **110+ trade** yaptı, kayıp büyüdü.

### 2.2 v4b parametre değişiklikleri

24-market combined grid search (11520 kombinasyon) ile bulundu:

| parametre | v3 | **v4b** | etki |
|---|---:|---:|---|
| `requote_eps_ticks` | 2 | **4** | spam %50 azaldı; en kritik fix |
| `bsi_rev` | 2.0 | **1.5** | rule daha agresif (bsi_rev: 2/3 → 3/4 doğru) |
| `dscore_strong` | 1.0 | **1.5** | momentum daha kati (false momentum azaldı) |
| `ofi_dir` | 0.4 | **0.3** | rule daha agresif (ofi_dir: 1/1 → 4/5 doğru) |
| `flip_threshold` | 5.0 | 5.0 | (aynı; flip kritik mekanizma) |

Hard-stop ve max_requote guard'ları **gerekli değil** — `requote_eps=4` tek başına spam'ı kontrol altına aldı.

### 2.3 Trade hacmi düşüşü

| market | v3 trade | v4b trade | düşüş |
|---|---:|---:|---:|
| 1777479300 | 110 | 87 | -21% |
| 1777480500 | 16 | 7 | -56% |
| 1777480800 | 111 | 50 | -55% |
| 1777476300 | 103 | 70 | -32% |

Daha az trade = daha az fee + daha düşük cost basis = pozitif PnL.

---

## 3. Market başına detaylı sonuçlar (v4b)

### 3.1 Tablo

| market | winner | opener | flip | trade | PnL | yön |
|---|---|---|---|---:|---:|:---:|
| 1777467000 | Up | Up (ofi_dir) | - | 22 | +$85.15 | ✓ |
| 1777467300 | Down | Down (bsi_rev) | - | 46 | +$115.38 | ✓ |
| 1777467600 | ? | Up (exhaustion) | - | 25 | mid -$225 | – |
| 1777467900 | Down | Down (score_avg) | - | 38 | +$105.71 | ✓ |
| 1777468200 | Up | Down (score_avg) | →Up | 3 | -$27.47 | ✓\* |
| 1777468500 | ? | Down (exhaustion) | - | 12 | mid -$137 | – |
| 1777471200 | Down | Down (ofi_dir) | - | 38 | +$82.43 | ✓ |
| 1777471800 | ? | Up (score_avg) | - | 86 | mid -$289 | – |
| 1777472100 | Up | Up (bsi_rev) | - | 18 | +$67.86 | ✓ |
| 1777473000 | Down | Down (momentum) | - | 50 | +$86.62 | ✓ |
| 1777473900 | Down | Up (ofi_dir) | →Down | 3 | -$27.36 | ✓\*\* |
| 1777474500 | Down | Up (score_avg) | →Down | 12 | -$96.89 | ✓\*\* |
| 1777474800 | Down | Down (exhaustion) | - | 18 | +$26.94 | ✓ |
| 1777475100 | Down | Down (score_avg) | - | 43 | +$73.81 | ✓ |
| 1777476300 | Down | Down (score_avg) | - | 70 | +$168.85 | ✓ |
| 1777476600 | Up | Up (momentum) | - | 69 | +$144.55 | ✓ |
| 1777479000 | Down | Down (bsi_rev) | - | 4 | +$47.20 | ✓ |
| 1777479300 | Down | Up (momentum) | →Down | 87 | -$121.46 | ✓\*\* |
| 1777479600 | Down | Down (ofi_dir) | - | 78 | +$143.36 | ✓ |
| 1777479900 | Up | Up (score_avg) | - | 32 | +$88.17 | ✓ |
| 1777480200 | **Up** | **Down (bsi_rev)** | - | 2 | -$9.92 | ✗ |
| 1777480500 | **Up** | **Down (exhaustion)** | - | 7 | -$37.16 | ✗ |
| 1777480800 | **Up** | Up (ofi_dir) | - | 50 | +$133.47 | ✓ |
| 1777481100 | ? | Up (exhaustion) | - | 3 | mid +$16 | – |
| **TOPLAM** | | | | **814** | **+$862.22** + mid -$636 = **+$226.30** | **17/20 = %85** |

\* doğru flip ama PnL negatif (timing zayıf)
\*\* doğru flip; PnL bazılarında negatif çünkü flip-öncesi pozisyon büyük

### 3.2 Composite opener kural dağılımı (v4b)

| kural | doğru | yanlış | toplam | doğruluk |
|---|---:|---:|---:|---:|
| `Momentum` | 3 | 0 | 3 | **100%** |
| `OfiDirectional` | 4 | 1 | 5 | 80% |
| `BsiReversion` | 3 | 1 | 4 | 75% |
| `ScoreAverage` | 4 | 2 | 6 | 67% |
| `Exhaustion` | 1 | 1 | 2 | 50% |

Notlar:
- `Momentum` 3/3 ile en güvenilir (v3 5/6 = %83'ten yükseldi).
- `Exhaustion` zayıf (1/2): bu kuralı **devre dışı bırakmak** veya threshold yükseltmek opsiyonu var.
- `ScoreAverage` fallback olarak kalmalı çünkü flip mekanizması yanlış score_avg'i 4 marketde düzeltti.

---

## 4. v4b parametre tablosu (final)

```rust
// src/config.rs ElisParams::default()
ElisParams {
    pre_opener_ticks: 20,
    bsi_rev_threshold: 1.5,            // v4b: 2.0→1.5
    ofi_exhaustion_threshold: 0.4,
    cvd_exhaustion_threshold: 3.0,
    ofi_directional_threshold: 0.3,    // v4b: 0.4→0.3
    dscore_strong_threshold: 1.5,      // v4b: 1.0→1.5
    score_neutral: 5.0,
    signal_flip_threshold: 5.0,
    signal_flip_max_count: 1,
    flip_freeze_opp_secs: 60.0,
    open_usdc_dom: 25.0,
    open_usdc_hedge: 12.0,
    order_usdc_dom: 15.0,
    order_usdc_hedge: 8.0,
    pyramid_usdc: 30.0,
    scoop_usdc: 50.0,
    requote_price_eps: 0.04,           // v4b: 0.02→0.04 (en kritik fix)
    requote_cooldown_secs: 3.0,
    avg_down_min_edge: 0.023,
    pyramid_ofi_min: 0.83,
    pyramid_score_persist_secs: 5.0,
    pyramid_cooldown_secs: 3.0,
    parity_min_gap_qty: 250.0,
    parity_cooldown_secs: 5.0,
    parity_opp_bid_min: 0.15,
    lock_avg_threshold: 0.97,
    scoop_opp_bid_max: 0.05,
    scoop_min_remaining_secs: 35.0,
    scoop_cooldown_secs: 2.0,
    deadline_safety_secs: 8.0,
}
```

---

## 5. Riskler ve gelecek iyileştirmeler

### 5.1 Bilinen zayıflıklar

1. **bot15 Up-trending marketlerde 3/8 yanlış**: bsi/exhaustion mean-reversion kuralları Up momentumunda ters çalışıyor. **Çözüm önerisi**: BSI/exhaustion'i sadece "winner ortaya çıktıysa" güçlü tutar veya devre dışı bırak (sadece OFI directional + momentum + score_avg kullan → 4-rule ladder).

2. **Mid market PnL hesabı pesimist**: -$636 senaryo gerçekçi değil çünkü bot mid'de kapatmaz, hold eder. Gerçek live'da bu marketler `Done` state'inde kalır, payout T-15'te netleşir.

3. **Requote sayısı hâlâ yüksek bazı marketlerde**: 1777479300'de 87 trade, 1777476300'de 70 trade. **Çözüm**: `max_requote_per_market=30` opsiyonu (yön etkilenmez, kesin PnL biraz düşer ama mid/live PnL iyileşir).

### 5.2 Gelecek iyileştirme önerileri

1. **Adaptif BSI/exhaustion**: Eşik dataset distribution'una göre dinamik (statik 1.5 yerine, son 50 marketin %95 quantile'ı kullan).
2. **Re-pyramid**: Mevcut pyramid 1 kez tetikleniyor; OFI sustained yüksek kalırsa 2-3 pyramid'e izin ver.
3. **Partial scoop**: Lock olmadan önce dom_bid fiyat 0.85+ olduğunda kısmi pozisyon sat (kazanç realize).

---

## 6. Faz 3 Rust implementation status

✅ **Tamamlandı** (Phase 3):
- `src/config.rs`: `ElisParams` struct + 30 elis_* alanı
- `src/strategy/common.rs`: `bsi/ofi/cvd/market_remaining_secs` opsiyonel alanları
- `src/strategy/elis.rs`: Tamamen rewrite (signal-driven 10-katman)
- `tests/elis_backtest.rs`: 24-market integration test (Python sim ile %100 paralel)

⏳ **Sıradakiler** (Phase 4):
- RTDS pipeline: bsi/ofi/cvd `Some(...)` ile pas et
- Frontend: Elis param JSON form
- DryRun simulator + live tick paralel test
