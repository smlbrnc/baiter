# Harvest v2 — Bulgular, Bug'lar ve PnL Analizi

> **Kaynak:** `docs/harvest-v2.md` (538 satır final doküman) v2 spesifikasyonu, **bot 40** (`btc-updown-5m`) gerçek market verisi (32 session × ~300 tick). Simülator: `scripts/harvest_v2_sim.py`. Çıktılar: `data/harvest_v2_sim_per_session.csv`, `data/harvest_v2_sim_events.csv`.
>
> **Kısa özet:** Strateji v2 spesifikasyonu **çalıştırılır** (32/32 session açıldı, 26/32 pair complete) ancak bot 40 datasında **net zararla** sonuçlanır. Polymarket binary outcome PnL standardına göre hesaplanan **`Σ pnl_final = −4.62 USDC`** (`cost = 618.62 USDC`). Kök nedenleri: (a) doc §3'teki profit formülünün matematiksel olarak yanlış olması (Bug #1), (b) §S4 ters dönüş (reverse pyramid) felaketinin tek session'da −14.74 USDC'ye yol açması ve (c) nötr / düz market'te (`spread = 0`, `signal ≈ 5`) açılış-hedge çiftinin yapısal eksi marja kilitlenmesi.
>
> Bu doküman implementasyon **öncesi** revize edilmesi gereken **14 doc bug'ını + 7 strateji önerisini** sıralar; PnL tablosu §3'te (üç varyant karşılaştırma), event-bazlı detay §4'te, kullanıcı talebi olan **Öneri G — DeepTrade fail-over** §5.G'dedir.

---

## 1. Yöntem — Polymarket PnL standardı

### 1.1 Veri ve parametreler

- **Veri:** `data/baiter.db` SQLite, `market_ticks` tablosu (8 547 satır, bot_id=40). Her tick `yes/no_best_bid/ask`, `signal_score (0–10)`, `ts_ms`. Tick aralığı ≈ 1 sn.
- **Session penceresi:** `market_sessions.start_ts*1000 → end_ts*1000` (300 sn).
- **Strateji parametreleri:** `avg_threshold = 0.98`, `cooldown = 30 000 ms`, `order_usdc = 5`, `signal_weight = 10` (tam etki), `min/max_price = 0.05/0.95`, `tick_size = 0.01`, `api_min_order_size = 5`. Bot 40 config'i `signal_weight` taşımıyor (henüz schema'da yok); simülasyon "doc §5 yorumuna göre full strength" (10) ile koştu.
- **Pasif fill modeli:** Bot her zaman `Buy`. Yes-buy emri için `limit ≥ yes_ask` ⇒ fill@order.price (maker fiyatı), simetrik no-buy için.
- **Resolve heuristic** (`ε = 0.05`): Son tickte `yes_bid > 0.55` ⇒ UP win, `yes_bid < 0.45` ⇒ DOWN win, aksi `pending` (sadece unrealized rapor edilir).

### 1.2 Polymarket binary outcome PnL formülleri

Web doğrulama: [polymarket101.com – PnL tracker](https://www.polymarket101.com/en/docs/trading/checking-history/), [docs.polymarket.us – Valuation API](https://docs.polymarket.us/institutional/valuation/overview), [polysage.dev – Position Profit](https://polysage.dev/calculators/position-profit). Standart:

```text
cost_basis      = Σ (buy_price × size)                       // tüm BUY fill'leri toplam
realized_pnl    = Σ (winning_shares × $1) − cost_basis       // yalnız market resolved
unrealized_pnl  = Σ (shares × current_mid) − cost_basis      // market açıkken
```

Resolution: YES shares kazandıysa $1, kaybettiyse $0; NO shares aynı şekilde simetrik. Tek yönlü pozisyonlarda risk binary: ya tam getiri ya sıfır.

### 1.3 Status sınıflaması

Her session için `status` etiketi:

| Status | Koşul | PnL kolonu |
|---|---|---|
| `no_pos` | `cost == 0` (hiç fill yok) | `pnl_final = 0` (sunk cost yok) |
| `open_unr` | Pair complete değil + market `pending` | `pnl_final = pnl_unrealized` (mark) |
| `open_res` | Pair complete değil + market `resolved` | `pnl_final = pnl_realized` |
| `pair_unr` | Pair complete + market `pending` | `pnl_final = pnl_unrealized` |
| `pair_res` | Pair complete + market `resolved` | `pnl_final = pnl_realized` |

> **Önemli:** Bot 40'ın 32 sessionının **hiçbirinde** `status = no_pos` çıkmadı — `signal_score ≠ 5` olan tüm sessionlarda OpenPair'in açılan tarafı taker eşiğine basıp **en az tek-leg fill aldı**. Bu, kullanıcı talebi olan "DeepTrade'de hiç fill yoksa fail-over" kuralının (Öneri G) bot 40 datasında **hiç tetiklenmemesinin** nedenidir; ileride `signal_weight ≈ 0` ile çalışan botlarda kural devreye girecektir.

### 1.4 Doc §3 formülünün **kullanılmaması** (Bug #1)

Doc §3 ve §10'daki `profit = (1 − pair_avg_cost) × pair_count` formülü **Polymarket standardına uymuyor**. Doğrusu:

```text
profit_pair = pair_count × (1 − avg_yes − avg_no)
```

Bu formül `pair_count` shares hem yes hem no satın alındığı için iki ayrı cost (toplam `avg_yes + avg_no` per share) olduğunu, kazananın `$1` getirdiğini doğru modeller. Doc formülü `(avg_yes + avg_no)/2` ortalama kullanıp pair maliyetini yarıya indirir — sahte +%50 marj raporu.

**Doğrulama (session 355):** `avg_yes = 0.137`, `avg_no = 0.84`, `pair_count = 149`.
- Doc formül: `149 × (1 − 0.4885) = +76.21 USDC` (yanlış, sahte profit).
- Doğru formül: `149 × (1 − 0.137 − 0.84) = +3.43 USDC` ≈ simülasyon `pnl_final = +3.41` ✓.

Findings'te tüm tablolar **`pnl_unrealized` / `pnl_realized` / `pnl_final`** kolonlarını kullanır; doc §3 formülü tamamen elenmiştir.

---

## 2. Doküman bug'ları (implementasyondan önce düzeltilmeli)

### A. Kritik (algoritma/PnL)

#### 1. **Profit formülü yanlış** ⚠️ kritik

§3 + §10 + §S1–S6 senaryoları: `(1 − pair_avg_cost) × pair_count` ⇒ Polymarket standardıyla `pair_count × (1 − avg_yes − avg_no)`. Tüm senaryo aritmetiği yeniden hesaplanmalı (S1, S2, S3, S5, S6'da PnL rakamları ~%50 fazla raporlanmış).

#### 2. **§S4 ters-dönüş senaryosu — alternatif önerinin yönü yanlış** ⚠️ kritik

§S4 sonu "opposite pyramid'i opsiyonel kapat" diyor. **Veri reddediyor:**

| Varyant | Σ cost | Σ pnl_final | pair_complete |
|---|---|---|---|
| (a) Default v2 (mevcut) | 618.62 | **−4.62** | 26 / 32 |
| (b) `opposite_pyramid_enabled = false` | 587.09 | **−15.99** | 26 / 32 |
| Δ (b − a) | −31.53 | **−11.37** | 0 |

Session 353 detayı:
- (a) cost=47.74, pair=N, win=Down → shares_no=33 × $1 = 33 → PnL = **−14.74**.
- (b) cost=26.41, pair=N, win=Down → shares_no=0 (opposite pyramid kapatıldı, hiç DOWN fill yok) → PnL = **−26.41**.

Doc §16'daki "opsiyonel kapatma" önerisi kaldırılmalı. **Doğru fix Bug #3**'tür (hedge_side dinamik).

#### 3. **§9 hedge re-place'te `hedge_side` referansı sabit `filled_side`** ⚠️ kritik

§9 adım 2: `new_hedge_price = avg_threshold − avg_filled_side`. Pyramiding karşı tarafa olduğunda `avg_filled_side` (avg_yes) sabit kalır → fiyat değişmez → §16'daki "skip optimization" devreye girer ⇒ **hedge tarafı da güncellenmez**.

353 etkisi: 5× DOWN pyramid sonra hedge hâlâ DOWN @0.52, market 0.30'a kadar düştü, hedge fill yok. Doğru kural:

```text
hedge_side = if shares_yes > shares_no { Down }       // UP fazla → hedge DOWN
             elif shares_no > shares_yes { Up }
             else { skip — pair complete }
new_hedge_price = avg_threshold − avg_(majority_side)
```

#### 4. **§10 partial fill tablosundaki avg-down hesabı tutarsız**

T+30s satırı: `UP @ 0.48 (size 11)` ama `avg_up = (9×0.54 + 11×0.49)/20 = 0.5125`. Fiyat `0.48` mi `0.49` mu? Tüm senaryo aritmetiği bir kez gözden geçirilmeli.

#### 5. **§4 state machine: imbalance < min_order_size durumunda eksik geçiş**

§9 adım 7 `size = imbalance.abs()`; eğer `< api_min_order_size` ise hedge planlanamaz → hangi state'e geçilir? §10 "Done'a geçilir" diyor ama §4 mermaid'inde `HedgeUpdating → Done` yolu yok.

### B. Orta öncelikli

#### 6. **§5 nötr (`signal_weight=0` veya `score≈5`) açılışta hedge fill garantisi yok**

`open_price = yes_bid` (pasif maker) + `hedge_price = avg_threshold − yes_bid`. Spread > 0 ise her iki taraf da pasif maker → fill garantisi yok. Bot 40'ta spread çoğunlukla 0.01 olduğu için open emir taker eşiğine basıp fill alıyor (374, 377 dahil); ama **`signal_weight = 0`** ile çalışan botlarda bu durum sıkça oluşur. Çözüm: nötr durumda OpenPair'i ertele (`δ_min` parametresi) — bkz. §5.G/Öneri G.

#### 7. **§3 + §8 `rising_side` tanımı 0.5 sınırında belirsiz**

`if best_bid_yes > 0.5 { Up } else { Down }` — `yes_bid == 0.5` ⇒ Down işaretlenir. Düz market'te (374–377) sürekli `rising = Down` etiketi → AggTrade pyramid → yapısal eksi marj.

**Çözüm:**

```text
rising_side = match yes_bid {
    p if p > 0.5 + ε => Up,
    p if p < 0.5 − ε => Down,
    _                => None,    // pyramid skip
}
ε = tick_size = 0.01
```

#### 8. **Terminoloji: `rising_side` aslında `favored_side`**

`yes_bid > 0.5` "yes şu an favori"; "yükseliyor" değil (statik snapshot). §8 zaten "trend devam ediyor" alt-koşuluyla momentum kontrolü yapıyor. İsim `favored_side` olmalı.

#### 9. **§13 emir matrisi vs §6 — DeepTrade hedge update tutarsızlığı**

DeepTrade'de averaging yok ⇒ hedge update fiilen tetiklenmez. §13 satırı `—` olmalı.

#### 10. **§7 tetik koşulu `<` strict**

`best_ask < avg_filled_side` ⇒ eşitlik tetiklemez. Tick snap nedeniyle gerçek hayatta sıkça eşit gelir.

#### 11. **§9 cancel response gecikmesi (adım 5) tick model'iyle uyumsuz**

DryRun simulator cancel response sentetik üretmez. Live latency ~100-500ms. Doc'a "DryRun: anında OK" notu eklenmeli.

#### 12. **`composite_score` vs `signal_score` isim tutarsızlığı**

Doc `composite_score`, DB kolon `signal_score`. Tek isim seçilmeli (öneri: `effective_signal_score`).

### C. Küçük

#### 13. **§13 "Buy GTC × 2" + §5 "taker eşiği" — fill_price modeli**

Polymarket CLOB GTC limit emirler taker bastığında fiyatı `book ask`'tan dolar (order.price'tan değil). Doc §S1: open fill 0.54 (order price), gerçekte 0.53 (yes_ask). `fill_price = max(order.price, opposite_book_top)` modeli netleştirilmeli.

#### 14. **`hedge_id` persist consistency**

§16 PositionOpen `hedge_id` taşır; bot restart sonrası CLOB ID eşleşemiyorsa migration belirsiz. Öneri: ilk tickte live order taraması.

---

## 3. PnL tablosu — bot 40 / 32 session, 3 varyant karşılaştırma

### 3.1 Default (a) — `docs/harvest-v2.md` spec aynen

| sid | zone | res | win | status | fo | pair | cost | pnl_unr | pnl_real | **pnl_final** |
|---:|:--|:-:|:--|:--|:-:|:-:|---:|---:|---:|---:|
| 346 | Stop | Y | Down | pair_res | – | ✓ | 39.14 | +0.86 | +0.86 | **+0.86** |
| 347 | Stop | Y | Up | pair_res | – | ✓ | 10.08 | +0.86 | +1.92 | **+1.92** |
| 348 | Stop | Y | Down | pair_res | – | ✓ | 10.27 | −0.27 | −0.27 | **−0.27** |
| 349 | Stop | Y | Down | pair_res | – | ✓ | 10.26 | +0.74 | +0.74 | **+0.74** |
| 350 | Stop | Y | Up | pair_res | – | ✓ | 19.60 | +0.40 | +0.40 | **+0.40** |
| 351 | Stop | Y | Down | pair_res | – | ✓ | 34.21 | +0.79 | +0.79 | **+0.79** |
| 352 | Stop | Y | Down | pair_res | – | ✓ | 27.35 | +0.65 | +0.65 | **+0.65** |
| **353** | Stop | Y | Down | **open_res** | – | ✗ | **47.74** | −14.62 | **−14.74** | **−14.74** |
| 354 | Stop | Y | Up | pair_res | – | ✓ | 27.57 | +0.43 | +0.43 | **+0.43** |
| **355** | Stop | Y | Up | pair_res | – | ✓ | **145.59** | +3.41 | +3.41 | **+3.41** |
| 356 | Stop | Y | Down | pair_res | – | ✓ | 21.48 | +0.52 | +0.52 | **+0.52** |
| 357 | Stop | Y | Down | pair_res | – | ✓ | 10.66 | −0.37 | −0.66 | **−0.66** |
| 358 | Stop | Y | Down | pair_res | – | ✓ | 10.26 | −0.26 | −0.26 | **−0.26** |
| 359 | Stop | Y | Down | pair_res | – | ✓ | 10.68 | −0.67 | −0.68 | **−0.68** |
| 360 | Stop | Y | Up | pair_res | – | ✓ | 10.26 | −0.26 | −0.26 | **−0.26** |
| 361 | Stop | Y | Down | pair_res | – | ✓ | 10.26 | −0.26 | −0.26 | **−0.26** |
| 362 | Stop | Y | Up | pair_res | – | ✓ | 10.26 | −0.23 | −0.26 | **−0.26** |
| 363 | Stop | Y | Down | pair_res | – | ✓ | 10.70 | +1.29 | +1.30 | **+1.30** |
| 364 | Stop | Y | Up | pair_res | – | ✓ | 10.28 | −0.28 | −0.28 | **−0.28** |
| 365 | Stop | Y | Up | pair_res | – | ✓ | 10.42 | −1.42 | −1.42 | **−1.42** |
| 366 | Stop | Y | Up | pair_res | – | ✓ | 23.57 | +0.43 | +0.43 | **+0.43** |
| 367 | Stop | Y | Down | pair_res | – | ✓ | 10.27 | −0.27 | −0.27 | **−0.27** |
| 368 | Stop | Y | Down | pair_res | – | ✓ | 10.26 | −0.26 | −0.26 | **−0.26** |
| 369 | Stop | Y | Down | pair_res | – | ✓ | 10.28 | −0.28 | −0.28 | **−0.28** |
| 370 | Stop | Y | Up | pair_res | – | ✓ | 20.65 | +0.35 | +0.35 | **+0.35** |
| 371 | Stop | Y | Down | pair_res | – | ✓ | 10.70 | +1.29 | +1.30 | **+1.30** |
| 372 | Stop | Y | Down | pair_res | – | ✓ | 10.28 | +0.01 | −0.28 | **−0.28** |
| 373 | Stop | Y | Down | open_res | – | ✗ | 5.04 | −0.04 | +2.96 | **+2.96** |
| 374 | Stop | N | – | open_unr | – | ✗ | 5.00 | −0.05 | 0.00 | **−0.05** |
| 375 | Stop | N | – | pair_unr | – | ✓ | 10.20 | −0.20 | 0.00 | **−0.20** |
| 376 | Stop | N | – | pair_unr | – | ✓ | 10.20 | −0.20 | 0.00 | **−0.20** |
| 377 | Normal | N | – | open_unr | – | ✗ | 5.10 | −0.05 | 0.00 | **−0.05** |
| **TOPLAM** |  | 28 res / 4 pend |  | – | 0 fo | **26 / 32** | **618.62** | **−7.94** | **−4.12** | **−4.62** |

> **Status dağılımı:** `pair_res = 26`, `pair_unr = 2` (375, 376), `open_res = 2` (353, 373), `open_unr = 2` (374, 377), `no_pos = 0`.
>
> **Resolved sessionlar (28):** Toplam realized PnL `Σ pnl_realized = −4.12 USDC`. Pending sessionlar (4): Toplam unrealized `Σ pnl_unrealized = −0.50 USDC`.
>
> **`pnl_final`** kolonu raporlama standardı: resolved varsa `realized`, yoksa `unrealized`.
>
> **374, 377 (`open_unr`) yorumu:** Open emir tek-leg fill aldı (cost = 5 USDC), hedge fill almadı, market henüz resolve değil (pencere son ticki `yes_bid ≈ 0.50`). PnL_unrealized = −0.05 (mark çok yakın); resolve olunca `±5` USDC'lik binary risk taşır. Polymarket'te bu pozisyon "açık tek-leg, mark-to-market −0.05" olarak gösterilir; "sıfır" gösterimi (status `no_pos`) sadece `cost == 0` durumudur (374/377 bu durumda **değil** çünkü cost > 0).

### 3.2 Üç varyant karşılaştırma

| metric | (a) default | (b) `opposite_pyramid = false` | (c) `single_leg_failover = true` |
|---|---:|---:|---:|
| sessions | 32 | 32 | 32 |
| status: `no_pos` | 0 | 0 | 0 |
| status: `open_unr` | 2 | 4 | 2 |
| status: `open_res` | 2 | 2 | 2 |
| status: `pair_unr` | 2 | 0 | 2 |
| status: `pair_res` | 26 | 26 | 26 |
| failover tetiklendi | 0 | 0 | **0** |
| cost basis (USDC) | 618.62 | 587.09 | 618.62 |
| Σ pnl_unrealized | −7.94 | −19.14 | −7.94 |
| Σ pnl_realized | −4.12 | −15.79 | −4.12 |
| **Σ pnl_final** | **−4.62** | **−15.99** | **−4.62** |

### 3.3 Kümülatif metrikler (default)

| Metrik | Değer |
|---|---|
| Toplam notional cost | 618.62 USDC |
| Σ avg-down emri | 18 |
| Σ pyramid emri | 8 |
| Σ hedge re-place | 20 |
| Σ pnl_final | **−4.62 USDC** |
| En kötü session | **353** (−14.74 USDC) |
| En iyi session | **355** (+3.41 USDC) |
| 353 hariç toplam | **+10.12 USDC** |
| Hit rate (`pnl_final > 0`) | 11/32 (34.4%) |
| Mean PnL/session | −0.144 USDC |

### 3.4 Doc §3 yanlış formülü ile karşılaştırma (referans)

Eğer doc §3'teki `(1 − pair_avg)/2 × pair_count` kullanılsaydı: `Σ "realized_profit" = +297.72 USDC` (sahte, +%48 cost return raporu). Polymarket standardıyla gerçek `Σ pnl_final = −4.62 USDC` (−%0.75 cost return). **Bug #1 düzeltilmeden bu strateji kâr ediyor sanılır.**

---

## 4. Olay-bazlı detay (öne çıkan session'lar)

### Session 353 — `pnl_final = −14.74 USDC` (Bug #2 + #3 birleşik)

```
T=0       OpenPair UP@0.54(10), hedge DOWN@0.44(12); UP fill (avg_up=0.54)
T+30/60/90/120s  4× AvgDown UP → fill, hedge DOWN re-place 4 kez (0.45 → 0.47 → 0.49 → 0.52)
          [zone NormalTrade → AggTrade; market dönüyor — yes_bid < 0.5]
T+121s    Pyramid DOWN@0.66(8) → fill (filled_side hâlâ Up; hedge re-place DOWN@0.52(50) — Bug #3)
T+150/180/210s   3× DOWN pyramid → fill, hedge sabit DOWN@0.52
T+240s    Pyramid DOWN@0.95(6) — kitapta bekledi
T+285s    StopTrade → cancel_all
Final     shares_up=58, shares_no=33; cost=47.74; market DOWN resolved
          pnl_realized = 33×$1 + 58×$0 − 47.74 = −14.74 USDC
```

**Bug #3 etkisi:** 5 DOWN pyramid'in her birinden sonra hedge update tetiklendi ama `new_hedge_price = 0.98 − avg_up = 0.466 → 0.52` (sabit, çünkü avg_up değişmedi). "Skip optimization" yüzünden hedge **yön de güncellenmedi** — hâlâ DOWN tarafına basıldı, market UP'a hiç dönmedi. Doğru kural (Öneri B): `hedge_side = imbalance.sign()` ⇒ shares_up=58 > shares_no=33 ⇒ hedge UP olmalıydı (UP @0.30-0.40 limit, market 0.30 → fill çoktan, pair tamamlanırdı).

### Session 355 — `pnl_final = +3.41 USDC` (Bug #1 326% sahte raporlar)

```
T=0       OpenPair UP@0.54(9), hedge DOWN@0.44(11); UP fill
3× AvgDown + 3× hedge re-place (NormalTrade)
Final     shares_up=149, shares_no=149; avg_up=0.137, avg_no=0.84
          cost = 149×0.137 + 149×0.84 = 145.59
          pnl_realized = 149×$1 + 149×$0 − 145.59 = +3.41 USDC
          (doc §3 formülü: 149×(1−0.4886) = +76.21 USDC — YANLIŞ)
```

### Session 374 / 377 — nötr açılış, hedge fill yok (Bug #6)

```
374  T=0   score=4.55, no_ask=0.50 → DOWN@0.50(10) fill; hedge UP@0.48(11) maker
     T+5m no_ask hep 0.50+; UP fill yok; StopTrade cancel
     Final shares_no=10 only; market pending (yes_bid≈0.50)
     pnl_unrealized = 10×0.495 − 5 = −0.05 USDC; resolve olunca ±5 USDC
377  Aynı pattern; cost=5.10 sunk
```

`signal_weight=0` ile çalışan bir botta bu pattern **tüm sessionlarda** olur — cost sunk, hedge hiç fill almaz. Öneri G.2'nin (nötr açılışta erteleme) çözmek istediği problem.

### Session 375 / 376 — pyramid Down (Bug #7)

```
375  T=0   score=5.02, yes_bid=0.51 → UP@0.51(10) fill (ask=bid spread=0)
     AggTrade: yes_bid=0.50 → rising=Down (Bug #7 — eşitlikte Down)
     Pyramid DOWN@0.51(10) → fill
     pair_complete: shares_up=10, shares_no=10; avg_up=avg_no=0.51 → pair_avg=0.51 > 0.50
     pnl_final = (10×mid + 10×mid) − 10.20 = −0.20 (yapısal eksi marj)
```

---

## 5. Strateji önerileri

### Öneri A: **Bug #1 düzeltmesi + tüm doc'taki sayılar tekrar hesaplansın**

`(1 − pair_avg_cost) × pair_count` ⇒ `pair_count × (1 − avg_yes − avg_no)`. §3, §10, §15 + 6 senaryo (S1-S6) PnL aritmetiği yeniden yazılmalı.

### Öneri B: **Bug #3 düzeltmesi — `hedge_side` dinamik**

§9'u:

```text
hedge_side = if shares_yes > shares_no { Down }
             elif shares_no > shares_yes { Up }
             else { skip — pair complete }
new_hedge_price = avg_threshold − avg_(majority_side)
hedge_size      = imbalance.abs()
```

Pyramiding karşı tarafa olduğunda hedge otomatik yön değiştirir → 353 senaryosu fix.

### Öneri C: **Bug #6 — nötr açılış davranışı seçimi**

İki alternatif:

1. Hedge'i de taker eşiğine kadar bas (her iki taraf da taker, anında pair_complete, küçük negatif marj).
2. Nötr durumda OpenPair'i **ertele** — `BotConfig.signal_open_threshold = 0.5` (default), `|score − 5| < 0.5` ise OpenPair hiç tetiklenmez.

Öneri: **2** (sunk cost yok, fırsatı atla).

### Öneri D: **Bug #7 — `rising_side` 0.5 sınırında pyramiding skip**

```text
rising_side = if yes_bid > 0.5 + ε { Up } elif yes_bid < 0.5 − ε { Down } else { None }
ε = tick_size = 0.01
```

Düz market'te yapısal eksi pyramid (375, 376) elimine edilir.

### Öneri E: **avg-down/pyramid maliyet kontrolü** (yeni güvenlik kapısı)

Her yeni avg-down/pyramid emri öncesi:

```text
projected_avg_filled = (cost_filled + price × size) / (qty + size)
kabul: projected_avg_filled + avg_(opposite_side) < avg_threshold
```

Aşacak emir reddedilir. 353 gibi pyramid kaskadında son emirler bloklanır.

### Öneri F: **`order_usdc` lot büyüklüğü revisited**

`order_usdc = 5` + `api_min = 5` ⇒ pair_avg ≈ 0.5 olduğunda 1 emir = 10 share. Slippage marjı dar; üretim için `order_usdc ≥ 10` öneri (hem fee impact azalır hem tick-noise toleransı artar).

### Öneri G: **DeepTrade fail-over → NormalTrade single-leg restart** (kullanıcı talebi)

**Tetik:** Zone DeepTrade → NormalTrade geçişinde `state == OpenPair` ve `filled_side == None` ve `cost == 0` (yani DeepTrade boyunca **iki taraf da fill almadı**).

**Aksiyon:**

```text
1. cancel(open_order, hedge_order)        # OpenPair'in iki emri de iptal
2. rising = if yes_bid > 0.5 + ε { Up } elif yes_bid < 0.5 − ε { Down } else { skip }
3. if rising:
       price = best_bid(rising)            # pasif maker
       size  = max(⌈order_usdc/price⌉, api_min_order_size)
       place GTC Buy(rising, price, size, role="single_leg_open")
       state = OpenPair_Retry
       last_averaging_ms = now             # cooldown sayaç
   else:
       state = Pending                     # eşit market, yeniden Pending'den OpenPair denemesi
```

**Fill sonrası** (Soru cevabı = B):

```text
on fill(role == "single_leg_open"):
    filled_side = rising
    hedge_price = clamp(snap(avg_threshold − avg_filled_side), min, max)
    place GTC Buy(opposite(filled_side), hedge_price, hedge_size, role="hedge")
    state = PositionOpen
```

Ardından **standard NormalTrade avg-down + hedge update** akışı (§7, §9) devam eder, ProfitLock korunur.

**Edge cases:**
- Single-leg emir de NormalTrade boyunca fill almazsa: `cooldown_threshold` dolduğunda cancel + yeni `best_bid(rising)` ile re-place (mevcut `handle_open_averaging` kalıbı).
- AggTrade'e geçerken hâlâ fill yoksa: `OpenPair_Retry` state'inde re-place çalışmaya devam eder; basit yaklaşımda taker eşiğine yükseltme (`best_ask + |delta|`) opsiyonel.
- StopTrade'e kadar hiç fill olmazsa: cancel + `Done`, `status = no_pos`, `pnl = 0` (sunk cost yok).

**Bot 40 datasında etki:** **Sıfır.** 32/32 session'da `signal_score ≠ 5` olduğu için OpenPair'in açılan tarafı her zaman `score`'a göre delta ile taker eşiğine basıp **en az tek-leg fill aldı** (cost > 0). Bu nedenle fail-over koşulu (`cost == 0`) hiç sağlanmadı. Kural **`signal_weight = 0`** ile çalışan veya **`signal_score ≈ 5`** sıkça gözlenen botlarda devreye girer; 374/377 gibi sunk-cost senaryoları `cost = 0` olduğunda fail-over otomatik tetiklenip rising tarafa pasif emir koyacaktır.

> **Alternatif gevşetme** (kullanıcı isterse 4. varyant olarak test edilebilir): `cost == 0` koşulunu `not pair_complete` ile değiştir → 374/377 dahil tüm tek-leg açıkları kapsar. Bu, hedge perspektifinden zaten Öneri B (hedge_side dinamik) ile çakışır; ikisini birden uygulamak yerine **Öneri B + Öneri G dar tetiği** kombinasyonu tercih edilebilir.

---

## 6. Eylem planı (özet)

| # | İş | Tip | Önc | Süre tahmini |
|---|---|---|---|---|
| 1 | Doc §3 + senaryo PnL formülünü Polymarket standardına çevir (Bug #1) | doc | **kritik** | 30 dk |
| 2 | Doc §9 hedge_side dinamikleştir (Bug #3 + Öneri B) | doc | **kritik** | 45 dk |
| 3 | Doc §16 "opposite pyramid disable" önerisini geri çek + agresif hedge re-pricing notu ekle (Bug #2) | doc | yüksek | 15 dk |
| 4 | Doc §5 nötr açılış davranışı netleştir + Öneri C2 ekle (Bug #6) | doc | yüksek | 30 dk |
| 5 | Doc §3 + §8 `rising_side` eşitlik durumu (Bug #7 + Öneri D) | doc | orta | 15 dk |
| 6 | Doc §10 partial fill tablo aritmetiği (Bug #4) | doc | orta | 15 dk |
| 7 | Doc §4 mermaid `HedgeUpdating → Done` yolu (Bug #5) | doc | orta | 15 dk |
| 8 | **Doc §5 / §6'ya Öneri G fail-over alt-akışını ekle** | doc | yüksek | 30 dk |
| 9 | Doc §13 DeepTrade hedge sütunu = `—` (Bug #9) | doc | düşük | 5 dk |
| 10 | Doc §7 `<` vs `<=` (Bug #10) | doc | düşük | 5 dk |
| 11 | Doc §3 `composite_score` ↔ `signal_score` isim (Bug #12) | doc | düşük | 5 dk |
| 12 | Doc §15 metrik kataloğuna `status` alanı ekle | doc | düşük | 10 dk |
| 13 | Implementasyon turunda fill_price taker model (Bug #13) | code | orta | 1 sa |
| 14 | Öneri E (avg/pyramid maliyet kontrolü) prototip + test | code | yüksek | 2 sa |

**Doc revision toplam:** ~3.5 saat. Kritik path: Bug #1 + #3 fix → simülator yeniden koş (`scripts/harvest_v2_sim.py` parametre swap'la kontrolü kolay). Sonra Öneri G code-side prototype.

---

## 7. Reproduce

```bash
cd /Users/dorukbirinci/Desktop/baiter-pro
python3 scripts/harvest_v2_sim.py
# stdout: Default (a) varyantı session tablosu + 3 varyant kümülatif karşılaştırma
# data/harvest_v2_sim_per_session.csv: 32 session × 3 varyant kolonları yan yana
# data/harvest_v2_sim_events.csv     : (variant, session_id, ts_ms, event) tüm aksiyon
```

**Varyant flag'leri** (script `simulate(...)` çağrıları):

| Flag | Default | Açıklama |
|---|---|---|
| `opposite_pyramid` | `True` (a) / `False` (b) | §8: `rising_side != filled_side` durumunda pyramid yapılır mı? |
| `single_leg_failover` | `False` (a, b) / `True` (c) | §5/§6 Öneri G: DeepTrade'de fill yoksa NormalTrade'de cancel + tek-taraflı bid |

**Strateji parametreleri** (script başında sabitler): `AVG_THRESHOLD = 0.98`, `ORDER_USDC = 5`, `COOLDOWN_MS = 30 000`, `SIGNAL_WEIGHT = 10`, `RESOLVE_EPS = 0.05`, `TICK_SIZE = 0.01`, `MIN_PRICE = 0.05`, `MAX_PRICE = 0.95`.

**Veri kaynakları:** `data/baiter.db` → `bots(id=40)`, `market_sessions(bot_id=40)` (32 satır), `market_ticks(bot_id=40)` (8 547 satır). DB hâlâ snapshot fazında (bot STOPPED, last_active_ms ≈ 2026-04-20 18:21).

**Web referansları (Polymarket PnL standardı):**
- [polymarket101.com – PnL tracker](https://www.polymarket101.com/en/docs/trading/checking-history/)
- [docs.polymarket.us – Valuation API Overview](https://docs.polymarket.us/institutional/valuation/overview)
- [docs.polymarket.com – Positions & Tokens](https://docs.polymarket.com/concepts/positions-tokens)
- [polysage.dev – Position Profit Calculator](https://polysage.dev/calculators/position-profit)
