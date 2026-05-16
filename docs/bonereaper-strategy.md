# Bonereaper stratejisi — Teknik spesifikasyon (kanonik)

**Kaynak kod:** `src/strategy/bonereaper.rs`  
**Varsayılan parametreler:** `src/config.rs` (`StrategyParams` accessor’ları)  
**Amaç:** Polymarket cüzdanı `0xeebde7a0e019a63e6b476eb425505b7b3e6eba30` (“Bonereaper”) davranışının üretim motorunda yeniden üretimi (MIMIC).

Bu belge, eski trade-log türevli `docs/bonereaper.md` içindeki gözlemsel faz modelinden **bağımsız** olarak motorun gerçek karar sırasını ve sayıları tanımlar.

---

## 1. Yüksek seviye mimari

- **Sadece BUY:** Pozisyon satışı yok; kapanışta kazanan outcome `$1/share` ile ödenir.
- **OB reaktif:** Ana akış order book bid/ask ve pozisyon metrikleriyle çalışır; `signal_score` motor içinde okunmaz.
- **BSI (opsiyonel):** İlk emirde `ctx.bsi` varsa ve `|bsi| ≥ 0.30` ise yön Binance kaynaklı spot imbalance ile seçilir; aksi halde bid spread’e göre momentum tarafı.
- **Late Winner (LW):** Kazanan tarafın bid’i eşik üstündeyse ve kapanışa kalan süre LW penceresi içindeyse, taker BUY (ask fiyatından) ile agresif enjeksiyon; cooldown bu yolda bypass edilir.
- **Maker vs taker:** Normal birikim `best_bid` fiyatına GTC limit (maker / stale fill); loser scalp bandında `best_ask` (taker).

---

## 2. Karar zinciri sırası (`decide`)

Aktif oturumda, her tick için özet sıra:

1. `to_end < 0` veya bid eksik → `NoOp`.
2. **Late Winner (+ burst):** kota ve pencere uygunsa, winner `w_bid ≥ late_winner_bid_thr` ise `arb_mult` ile boyutlanmış taker alımı değerlendirilir (aşağıdaki guard’larla).
3. **Cooldown:** son alımdan `buy_cooldown_ms` geçmediyse `NoOp` (LW bu adımı atlar).
4. **Yön (`dir`):**
   - `first_done == false`: `|up_bid - down_bid| < first_spread_min` → bekle; aksi halde BSI veya spread işareti ile ilk yön.
   - Sonrasında: `|up_filled - down_filled| > imbalance_thr` → zayıf taraf rebalance; değilse bid delta (`ob_driven`).
5. **Fiyat / guard:** `loser_side` (bid farkı ≥ 0.20), `min_price` / `max_price`, `avg_loser_max` martingale guard, scalp bantları.
6. **POST_LW_WINNER_MAX_BID (0.70):** LW en az bir kez tetiklendiyse, winner tarafında ve scalp dışındayken `bid > 0.70` ise normal alım yok.
7. **Loser guard:** Loser yönü, scalp değilse ve `bid > loser_scalp_max_price` (default 0.30) ise alım yok.
8. **Emir fiyatı:** Scalp → ask; diğer → bid.
9. **`avg_sum` yumuşak tavan:** `max_avg_sum` (default 1.05); scalp hariç, yeni ortalama + karşı ortalama cap’i aşarsa `NoOp`.

---

## 3. `loser_side` (bid tabanlı)

`LOSER_SPREAD_MIN = 0.20`:

- `|up_bid - down_bid| < 0.20` → belirsiz bölge; `None` (loser guard uygulanmaz).
- `up_bid ≥ down_bid` → loser `Down`; aksi → loser `Up`.

Böylece erken belirsiz piyasada yanlış “loser” etiketiyle tüm alımların kesilmesi engellenir.

---

## 4. Late Winner (LW)

**Tetikleyici:** `max(up_bid, down_bid)` olan taraf “winner”; `winner_bid ≥ bonereaper_late_winner_bid_thr()` ve `winner_ask > 0`.

**Zaman penceresi:** `to_end ≤ bonereaper_late_winner_secs()` (default 180 sn, üst sınır 300). `lw_secs = 0` → LW kapalı.

**Kota:** `lw_max_per_session` (default 20, max 50); `0` → kotasız (riskli, önerilmez).

**Burst:** `lw_burst_secs` default **0** (kapalı). `lw_burst_usdc > 0` ve `lw_burst_secs > 0` ise son X saniyede ayrı dalga.

**Notional:** `late_winner_usdc` (default **100** USDC) × **`arb_mult`** ÷ `w_ask`, yukarı yuvarlanmış share.

### 4.1 `arb_mult` — 2D tablo (fiyat × kalan süre)

Taban: `lw_usdc = 100`. `to_end` = kapanışa kalan saniye.

| `w_ask` bantı | T > 120 | T ∈ (60,120] | T ∈ (30,60] | T ∈ (10,30] | T ≤ 10 |
|---------------|---------|--------------|-------------|-------------|--------|
| ≥ 0.99 | **20.0** | 11.5 | 5.5 | 5.7 | 1.7 |
| ≥ 0.97 | 9.0 | 4.4 | 6.1 | 3.7 | 1.0 |
| ≥ 0.95 | 2.0* | 2.0* | 4.0 | 4.0 | 4.0 |
| < 0.95 | 1.0 | 1.0 | 1.0 | 1.0 | 1.0 |

\* `w_ask ∈ [0.95, 0.97)` için `to_end > 60` → 2.0; `to_end ≤ 60` → 4.0.

**Kalibrasyon notu:** 104 market / 1619 LW shot istatistiği ile hacim uyumu ~%95.7 hedeflendi. BTC delta / harici hız katsayıları ile korelasyon **zayıf** bulundu (log analizi); shot büyüklüğü birincil olarak **Polymarket winner ask** ve **kalan süre** ile modellendi. Canlı örnekte (slug `1778615100` civarı) ~17× efektif çarpan görülünce üst uç **13 → 20** yapıldı.

### 4.2 LW öncesi guard’lar

- `LW_OPP_AVG_MAX = 0.50`: Karşı taraf doluysa ve `opp_avg > 0.50` → LW yok.
- Kombinasyon: `opp_avg > 0.40` **ve** `w_ask > 0.90` → LW yok (pahalı karşı + pahalı winner).

---

## 5. İlk emir ve BSI

- `first_spread_min` (default **0.02**): `|up_bid - down_bid|` bu değerin altındayken **hiç** ilk BUY yok.
- **Kritik A/B bulgusu:** `first_spread_min = 0.10` gibi yüksek eşikler erken momentum kaçırıp gerçek bot ile **ters yön** seçimine yol açtı (Bot 133 / “TIGHT FIRST” deneyi). Üretim taklidi için **0.02** önerilir.
- BSI eşiği: **0.30** (sabit `BSI_THRESHOLD`).

---

## 6. Boyutlandırma (normal alım)

USDC notional, **piecewise lineer interpolasyon** ile hesaplanır (3 anchor noktası):

| Anchor | Bid | Parametre | Default |
|--------|-----|-----------|--------:|
| longshot | `0.30` | `size_longshot_usdc` | 10 |
| mid | `0.65` | `size_mid_usdc` | 25 |
| high | `lw_thr` | `size_high_usdc` | 80 |

**Formül (`bonereaper_interp_usdc`):**

- `bid ≤ 0.30` → `longshot` (sabit)
- `0.30 < bid ≤ 0.65` → `longshot + (mid − longshot) × (bid − 0.30) / 0.35`
- `0.65 < bid < lw_thr` → `mid + (high − mid) × (bid − 0.65) / (lw_thr − 0.65)`
- `bid ≥ lw_thr` → `high` (LW akışı kontrol eder; bu fallback)

Bant sınırlarında sıçrama yok; gerçek bot 5m/15m markette superlineer artış gösteriyor (3995 trade analizi: 5m hata %28 azalır, 15m korelasyon 30× artar).

Winner tarafında ve `to_end ≤ late_pyramid_secs` (default 150) iken: `base × winner_size_factor` (default **2.0**, clamp 1–10).

**Loser scalp:** `bid ≤ loser_scalp_max_price` (default 0.30) ve `loser_scalp_usdc > 0` (default **10**) → scalp notional (interp devre dışı). `avg_loser_max` (default 0.50) aşıldıysa loser’da sadece minimal scalp yolu.

---

## 7. Tam varsayılan parametre tablosu (`config.rs`)

| Alan | Default | Not |
|------|--------:|-----|
| `bonereaper_buy_cooldown_ms` | 3000 | 1000–60000 |
| `bonereaper_late_winner_secs` | 180 | ≤300; 25-market logda LW’nin ~%98.6’sı T−180 içinde |
| `bonereaper_late_winner_bid_thr` | 0.90 | 0.50–0.99 |
| `bonereaper_late_winner_usdc` | 100 | `arb_mult` bu taban üzerinden |
| `bonereaper_lw_max_per_session` | 20 | ≤50 |
| `bonereaper_imbalance_thr` | 1000 | Aşırı salınımı bastırmak için yüksek |
| `bonereaper_max_avg_sum` | 1.05 | Dengeli pozisyonda zararı sınırlar |
| `bonereaper_first_spread_min` | 0.02 | 0–0.20 |
| `bonereaper_size_longshot_usdc` | 10 | Lineer interp anchor @ 0.30 |
| `bonereaper_size_mid_usdc` | 25 | Lineer interp anchor @ 0.65 |
| `bonereaper_size_high_usdc` | 80 | Lineer interp anchor @ lw_thr |
| `bonereaper_loser_min_price` | 0.01 | |
| `bonereaper_loser_scalp_usdc` | 10 | |
| `bonereaper_loser_scalp_max_price` | 0.30 | |
| `bonereaper_late_pyramid_secs` | 150 | |
| `bonereaper_winner_size_factor` | 1.0 | |
| `bonereaper_lw_burst_secs` | 0 | Kapalı |
| `bonereaper_lw_burst_usdc` | 0 | |
| `bonereaper_avg_loser_max` | 0.50 | |

Bot başına `strategy_params` JSON ile tümü override edilebilir; `order_usdc` ayrı alan olarak da etkiler (UI / API).

---

## 8. Telemetri / `reason` stringleri

- `bonereaper:buy:{up,down}` — normal maker birikim
- `bonereaper:scalp:{up,down}` — loser scalp (taker)
- `bonereaper:lw:{up,down}` — late winner ana
- `bonereaper:lwb:{up,down}` — late winner burst (açıksa)

---

## 9. İzleme ve doğrulama

- **`scripts/monitor/full_compare.py`:** Gerçek bot JSON logları ile VPS `baiter.db` üzerinden bot trade’lerinin karşılaştırılması; faz bantları, `arb_mult` shot doğrulaması (TAKER odaklı), TAKER/MAKER distinct shot gruplama, çoklu bug kriteri. Yol ve bot ID’leri script içinde yapılandırılır.
- **`scripts/monitor/deep_compare.py` / `live_compare.py`:** Önceki iterasyonlar; tam özellik seti için `full_compare` tercih edilir.

Maker fill’lerin aynı an / aynı fiyatta birden fazla satıra düşmesi tek “konsept emir” olarak gruplanmalıdır; aksi halde cooldown ihlali gibi **sahte pozitif** üretilir.

---

## 10. Bilinen sınırlar

- Gerçek cüzdanın iç kodu bilinmiyor; tüm kurallar **dış gözlem + log regresyonu** ile yaklaştırılmıştır.
- `bonereaper_full_sim_backtest.py` çıktıları eski varsayılanlarla üretilmiş olabilir; güncel defaultlarla yeniden koşturulmadıkça PnL satırları motorla **otomatik uyumlu değildir** (`docs/bonereaper-backtest-report.md`).
- Zincir üstü `REDEEM` ile yerel DB’nin tarih/slug hizası uyumsuzsa log-tabanlı “ground truth” boş kalır.

---

*Son güncelleme: 13 Mayıs 2026*
