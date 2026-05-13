# Bonereaper — Backtest, simülasyon ve kalibrasyon raporu

Bu belge üç katmanı ayırır:

1. **Tarihî tam-piyasa simülasyonu** — `scripts/bonereaper_full_sim_backtest.py` (eski varsayılanlarla üretilmiş metrikler; bkz. bölüm 2).
2. **Sinyal / proxy kazanan backtest’i** — aynı script içi skor vs OB “truth” (bölüm 3–4).
3. **2026 MIMIC kalibrasyonu** — gerçek Bonereaper log regresyonu + `bonereaper.rs` güncellemeleri (bölüm 5–7).

---

## 1. Özet tablo

| Konu | Durum / değer |
|------|----------------|
| Tam sim çıktısı (bir kez üretilmiş) | `data/bonereaper_sim_backtest.json`; **336** oturum, simüle BUY **10 078**, toplam sim harcama **458 961** USDC, proxy PnL toplamı **−7 225** USDC |
| Bu simülasyonun motor parametreleri | Eski dokümantasyonla uyumlu: örn. `late_winner_secs=30`, `late_winner` USDC **1000**, `max_avg_sum` **1.30**, bucket **15/25/30** — **güncel `config.rs` defaultları ile aynı değildir** |
| Log × DB REDEEM eşleşmesi | **0** eşleşme (slug/tarih penceresi uyumsuzluğu) |
| Güncel motor | `src/strategy/bonereaper.rs` + `src/config.rs` — LW **180 sn**, `lw_usdc` **100**, `max_avg_sum` **1.05**, dinamik **`arb_mult`** (bölüm 5) |

**Sonuç:** bölüm 2’deki sayılar “eski sim konfigürasyonu” için geçerlidir; üretim botunun bugünkü PnL’ini temsil etmez. Karşılaştırma yaparken script parametrelerini güncel defaultlarla hizalayın.

---

## 2. Tarihî tam piyasa simülasyonu (script: `bonereaper_full_sim_backtest.py`)

**Üretim komutu:**

```bash
python3 scripts/bonereaper_full_sim_backtest.py
```

**Veri kaynakları (o çalışmada):** Yerel `data/baiter.db` (`market_ticks`, `market_sessions`, `bots.strategy = 'bonereaper'`) ve çeşitli log dosyaları (`genel.log`, `gercekbotlog.log`, vb.).

### 2.1 Özet metrikler (arşiv)

| Metrik | Değer |
| --- | ---: |
| Simüle edilen oturum | **336** |
| Toplam simüle BUY (tick başına motor) | **10 078** |
| Oturum başına trade (medyan) | **33** |
| Toplam simüle harcama (USDC) | **458 961** |
| Proxy kazanan ile sim PnL toplamı | **−7 225** USDC (336 piyasa, **203** pozitif oturum) |
| Log REDEEM ile oturum eşleşmesi | **0** |

### 2.2 Eski motor varsayımları (Python tekrarı)

Motor mantığı o dönemde `bonereaper.rs` ile aynı sayısal varsayılanlar üzerinden Python’da tekrarlanmıştı: ör. cooldown **8 s**, late winner **30 s** / bid≥**0.85** / **1000** USDC, `max_avg_sum` **1.30**, USDC bucket’ları **15 / 25 / 30**, min/max bid **0.05–0.95**.

---

## 3. Sinyal backtest tasarımı

### 3.1 Gerçek kazanan (ideal)

Zincir üstü çözüm + `REDEEM` tablosu yerel DB’de boş veya hizasız olabiliyor; loglardaki `activity[type=REDEEM]` ile yalnızca **log slug’ları** kazanılabiliyor. Yerel oturum slug’ları log ile örtüşmediğinde `truth_redeem_from_logs_*` örneklem sayısı **0** kalır.

### 3.2 Proxy kazanan (OB tabanlı)

**Son 15 saniye** içindeki **son** `market_ticks` satırında:

- `up_mid = (up_bid + up_ask) / 2`
- `down_mid = (down_bid + down_ask) / 2`
- Proxy: `Up` kazanır, eğer `up_mid ≥ down_mid`; aksi halde `Down`.

Bu, Polymarket zincir çözümünün yerine geçmez; defterin kapanışa yakın fiyatlamasını ölçer.

### 3.3 Tahmin kuralları (ikili sınıflandırma)

| Kural | “Up” tahmini |
| --- | --- |
| `score_gt5` | `signal_score > 5` |
| `bsi_gt0` | `bsi > 0` |
| `score_and_bsi` | her iki koşul |
| `ofi_gt0` | `ofi > 0` |
| `cvd_gt0` | `cvd > 0` |

### 3.4 “Same snapshot” uyarısı

Aynı tick satırındaki `signal_score` ile aynı satırdaki mid fiyatlardan üretilen “truth” kıyaslanırsa sonuç **tahmin gücü değil**, skor ile OB’nin eşzamanlı korelasyonu olur. Raporda `same_snapshot_signal_vs_mid_truth_CAUTION` ayrı etiketlenmelidir.

### 3.5 Gerçek öngörü testleri (proxy truth = son OB)

| Test | Açıklama | n | `score>5` | `bsi>0` | `score∧bsi` |
| --- | --- | ---: | ---: | ---: | ---: |
| **predict_T60s** | Kapanıştan ~60 sn önceki en yakın tick | 336 | 0.5208 | 0.5000 | 0.4970 |
| **predict_mid_window** | Pencere ortasına en yakın tick | 336 | **0.5804** | **0.5804** | **0.5804** |

**Yorum:** Pencere ortası skoru son OB proxy’sine göre ~%58 isabet; T−60s profili ~%52 (rastgeleye yakın).

---

## 4. Literatür (OFI / mikroyapı)

Kısa vadede order book ve OFI ile fiyat hareketleri arasında çok kısa ufukta ilişkiler bulunur; latency ve slipaj dış örneklem performansını düşürür. Özet referanslar: Lee & Ready (1991), Cont–Kukanov–Stoikov (2014) çizgisi. Proje içi sinyal üretimi: `docs/signal.md`.

---

## 5. 2026 MIMIC kalibrasyonu (gerçek bot logları → Rust)

### 5.1 Veri hacmi

- Onlarca ila **104+** `btc-updown-5m` market logu; toplamda **1619+** LW shot, yüzlerce `$0.99+` bölümü.
- Hedef: gerçek botun **USDC hacim ve shot büyüklüğü dağılımını** yakalamak; tek boyutlu (sadece fiyat) `arb_mult` yetersiz kaldı.

### 5.2 `arb_mult` — 2D model

Shot büyüklüğü **winner ask (`w_ask`)** ve **kapanışa kalan süre (`to_end`)** ile modellendi. BTC delta vb. ile korelasyon analizde **zayıf** kaldı.

Tablo ve üst uç **20×** (`w_ask ≥ 0.99` ve `to_end > 120`) canlı örnekte ~17× efektif çarpanın görülmesiyle **13’ten yükseltildi**.

Tam sayısal tablo: **`docs/bonereaper-strategy.md`** içinde “4.1 `arb_mult`” alt başlığı.

### 5.3 Diğer kritik parametre güncellemeleri (`config.rs`)

- **`late_winner_secs`:** 300 → **180** (loglarda LW’nin büyük çoğunluğu son 3 dakikada).
- **`late_winner_usdc`:** üretim taklidi için **100** USDC taban; asıl volatilite `arb_mult` ile.
- **`lw_max_per_session`:** **20** (gerçek botta yüksek shot sayıları gözlendi).
- **`loser_scalp_usdc`:** **10** (scoop shot medyanına yakın).
- **Bucket USDC:** longshot **15**, mid **23**, high **37** (25-market bant medyanlarıyla hizalı).
- **`imbalance_thr`:** **1000** share — düşük eşikle rebalance salınımı ve `avg_sum ≈ 1.0` üzerinde çift taraflı zarar riski azaltıldı.
- **`max_avg_sum`:** **1.05** — dengeli pozisyonda üst üste pahalı ikinci bacak birikimini frenler (LW ve loser scalp bu cap’tan muaf tutulur).

### 5.4 Kod içi davranış ekleri

- **`loser_side`:** Bid farkı **≥ 0.20** değilse loser etiketi yok (erken belirsizlikte yanlış guard önlemi).
- **LW `opp_avg` guard:** `opp_avg > 0.50` veya (`opp_avg > 0.40` ve `w_ask > 0.90`) → LW atlanır.
- **`POST_LW_WINNER_MAX_BID = 0.70`:** LW sonrası winner’da aşırı pahalı normal birikimi keser (örnek market `1778588700` analizi).

### 5.5 Bilinen regresyon / A/B uyarıları

- **`first_spread_min = 0.10`:** Erken giriş gecikmesi, gerçek botla **yön uyumsuzluğu** (Bot 133). Taklit için **0.02** önerilir.
- **Monitor:** `arb_mult` doğrulaması **`lw_usdc` ile tutarlı** olmalı; maker partial fill’ler tek shot’ta birleştirilmeli.

---

## 6. `data/genel.json` ve canlı loglar

Kısa pencereli agregeler (`genel.json`, son ~2 saat) **tek başına** strateji parametrelerini değiştirmek için yeterli istatistiksel güç taşımayabilir. Emin olunmayan öneriler uygulanmadı; uzun log seti (bölüm 5) önceliklidir.

---

## 7. Implementasyon notu (`signal_score`)

Rust motorunda `signal_score` **okunmaz**; OB + BSI (ilk emir) + pozisyon metrikleri kullanılır. Tick verisinde skor ile yön korelasyonu gözlenebilir; bu ya harici bir katman ya da aynı mikroyapıdan türeyen hizalanma ile açıklanabilir.

---

## 8. Log × DB uyumsuzluğu (neden karşılaştırma 0?)

Log slug’ları ile yerel `baiter.db` session slug’ları farklı unix pencerelerine denk gelebilir → `n_compare = 0`. Çözüm: aynı dönem export’u + VPS DB kopyası veya aynı kaynakta üretilmiş oturumlar.

---

## 9. İlgili dosyalar

| Dosya | Rol |
|-------|-----|
| `scripts/bonereaper_full_sim_backtest.py` | Tam sim + sinyal backtest |
| `data/bonereaper_sim_backtest.json` | Sim ham çıktı |
| `scripts/monitor/full_compare.py` | Canlı gerçek bot vs bizim botlar |
| `docs/bonereaper-strategy.md` | Güncel motor spesifikasyonu |
| `docs/bonereaper.md` | Tarihî log-türetilmiş gözlemler + arşiv notları |

---

*Son güncelleme: 13 Mayıs 2026*
