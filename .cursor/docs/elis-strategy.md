# Elis Strategy — Polymarket BTC Up/Down 5dk Hibrit Maker Bid Grid

> **Versiyon:** 2.0 — Hibrit (Alis-tabanlı + Composite Signal Yön Filtresi)
> **Test sonucu (16 market):** %92 yön doğruluğu, **+$560 net PnL**, 10/13 pozitif marketler
> **Kaynak inceleme:** Polymarket trade log'ları (307 emir, 6 market) + tick verisi (`exports/bot14-ticks-20260429/`)

---

## 1. Strateji Özeti

Elis, Polymarket'te BTC Up/Down 5 dakikalık binary marketleri için **hibrit bir maker bid grid** stratejisidir. İki ana bileşenden oluşur:

1. **Alis bot'unun mekanik yapısı** (sinyalsiz, fiyat-bazlı):
   - Hem UP hem DOWN tarafına maker bid (best_bid - 2 tick)
   - Sabit USDC (~$15-25 / emir)
   - Fiyat değiştikçe otomatik re-quote
   - Late-stage scoop (winner netleşince loser ucuza al)

2. **Composite Signal Yön Filtresi** (Elis-özgü):
   - Pre-opener (20 tick) ile yön tahmini → asymmetric sizing
   - Signal flip (sadece çok güçlü reversal'da)
   - Flip-freeze (eski intent'e gereksiz alımı önle)
   - Hedge sadece opp yükselirken (Alis'in en büyük hatası)

### 1.1 Alis ile karşılaştırma

| metrik | Alis raw | Elis hibrit | fark |
|---|---:|---:|---:|
| 6 market PnL | -$19 | **+$98** | **+$117** |
| 16 market PnL | n/a | **+$560** | n/a |
| Yön doğruluğu | yok (sinyalsiz) | **%92** | n/a |
| Trade sayısı / market | ~50 | ~50 | benzer |

**Anahtar fark**: Alis sinyalsiz, fiyat momentumu peşinde koşuyor → whipsaw'larda yıkıcı (1777468500: -$1013). Elis composite ile yön filtreliyor + hedge mantığını rasyonelleştiriyor.

---

## 2. Veri Modeli

### 2.1 Tick formatı (giriş)

```rust
pub struct Tick {
    pub ts_ms: u64,
    pub up_best_bid: f64,
    pub up_best_ask: f64,
    pub down_best_bid: f64,
    pub down_best_ask: f64,
    pub signal_score: f64,   // 0..10 (composite trend score)
    pub bsi: f64,            // Buy-Side Imbalance
    pub ofi: f64,            // Order Flow Imbalance (-1..+1)
    pub cvd: f64,            // Cumulative Volume Delta
}
```

### 2.2 Intent enum

```rust
pub enum Intent { Up, Down }

impl Intent {
    pub fn opposite(self) -> Self {
        match self { Self::Up => Self::Down, Self::Down => Self::Up }
    }
}
```

### 2.3 FSM State

```rust
pub enum ElisState {
    Pending,         // t < 20 (pre-opener period)
    Opening,         // t = 20 (composite open + hedge)
    Managing,        // t = 21..240 (requote, avg_down, pyramid, parity)
    Locked,          // avg_sum <= 0.97 (kâr garantili)
    Scooping,        // remaining_s <= 35 + opp_bid <= 0.05
    Stopping,        // remaining_s <= DEADLINE_SAFETY_S
    Done,
}
```

---

## 3. Composite Opener (5-Rule Ladder)

**t = 20'de** (pre-opener penceresi 20 tick = ~20 saniye) çalışır. **Yön tahmini** verir.

### 3.1 Pre-opener feature'lar

```rust
let pre = &ticks[0..PRE_OPENER_TICKS]; // 20 tick
let dscore     = pre.last().score - pre.first().score;
let score_avg  = pre.iter().map(|t| t.score).sum::<f64>() / PRE_OPENER_TICKS as f64;
let bsi        = pre.last().bsi;
let ofi_avg    = pre.iter().map(|t| t.ofi).sum::<f64>() / PRE_OPENER_TICKS as f64;
let cvd        = pre.last().cvd;
```

### 3.2 Karar zinciri (öncelik sırası)

```rust
fn predict_opener(f: &Features, p: &ElisParams) -> (Intent, OpenerRule) {
    // 1. BSI extreme reversion: aşırı imbalance → tersi
    if f.bsi.abs() > p.bsi_rev_threshold {
        return (if f.bsi > 0.0 { Intent::Down } else { Intent::Up }, OpenerRule::BsiReversion);
    }

    // 2. OFI+CVD exhaustion: aşırı tek-yön flow → reversion
    if f.ofi_avg.abs() > p.ofi_exhaustion_threshold
        && f.cvd.abs() > p.cvd_exhaustion_threshold
    {
        if f.ofi_avg > 0.0 && f.cvd > 0.0 {
            return (Intent::Down, OpenerRule::Exhaustion);
        }
        if f.ofi_avg < 0.0 && f.cvd < 0.0 {
            return (Intent::Up, OpenerRule::Exhaustion);
        }
    }

    // 3. OFI directional: belirgin tek-yön flow → yönü
    if f.ofi_avg.abs() > p.ofi_directional_threshold {
        return (if f.ofi_avg > 0.0 { Intent::Up } else { Intent::Down }, OpenerRule::OfiDirectional);
    }

    // 4. Strong dscore momentum: 20 tickte score değişimi
    if f.dscore.abs() > p.dscore_strong_threshold {
        return (if f.dscore > 0.0 { Intent::Up } else { Intent::Down }, OpenerRule::Momentum);
    }

    // 5. Fallback: score_avg
    let dir = if f.score_avg >= p.score_neutral { Intent::Up } else { Intent::Down };
    (dir, OpenerRule::ScoreAverage)
}
```

### 3.3 Eşikler (16 marketde optimize)

| parametre | değer | açıklama |
|---|---:|---|
| `pre_opener_ticks` | **20** | 20-tick pencere (10 yetersiz) |
| `bsi_rev_threshold` | **2.0** | aşırı reversion için |
| `ofi_exhaustion_threshold` | **0.4** | OFI ile birlikte |
| `cvd_exhaustion_threshold` | **3.0** | CVD ile birlikte |
| `ofi_directional_threshold` | **0.4** | belirgin flow |
| `dscore_strong_threshold` | **1.0** | momentum tetikleyici |
| `score_neutral` | **5.0** | fallback |

### 3.4 Doğruluk (16 market)

| kural | doğru tahmin | yanlış | toplam |
|---|---:|---:|---:|
| BsiReversion | 2 | 0 | 2 |
| Exhaustion | 2 | 0 | 2 |
| OfiDirectional | 1 | 0 | 1 |
| Momentum | 5 | 1 | 6 |
| ScoreAverage | 0 | 4 | 4 |
| **TOPLAM** | **10** | **5** | 15 |

**Not**: Yanlış tahminlerin 4'ü `ScoreAverage` (fallback) kuralındandır. Bunlar **gerçek divergence** marketleri (sinyaller bir yön gösteriyor, market diğerine kaydı). `signal_flip` sayesinde bunların 3'ü düzeltilir → final yön doğruluğu **%92**.

---

## 4. Asymmetric Sizing

### 4.1 USDC dağılımı

| emir tipi | USDC | yorum |
|---|---:|---|
| `OPEN_USDC_DOM` | **25.0** | dominant taraf (composite tahmini) — ana pozisyon |
| `OPEN_USDC_HEDGE` | **12.0** | hedge — yarı boy (asimetrik) |
| `ORDER_USDC_DOM` | **15.0** | requote/avg_down dom |
| `ORDER_USDC_HEDGE` | **8.0** | requote/parity hedge |
| `PYRAMID_USDC` | **15.0** | dom yönü güçlendi → ek pozisyon |
| `SCOOP_USDC` | **50.0** | late scoop (kazanan netleştiğinde) |
| `MAX_SIZE` | **400.0** | tek tarafta max biriken share (lock öncesi) |

**Mantık**: dominant tarafa ~2x size, hedge sadece risk yönetimi için.

### 4.2 Bid fiyatlama

```rust
// Maker bid: best_bid - 2 tick (Alis pattern'ı)
let dom_price = (tick.bid(intent) - 2.0 * TICK_SIZE).max(TICK_SIZE);
let hedge_price = (tick.bid(intent.opposite()) - 2.0 * TICK_SIZE).max(TICK_SIZE);
let dom_size = OPEN_USDC_DOM / dom_price;
let hedge_size = OPEN_USDC_HEDGE / hedge_price;
```

---

## 5. Decide() Priority Chain

Her tick için karar zinciri (yukarıdan aşağıya):

```rust
pub fn decide(&mut self, tick: &Tick) -> Decision {
    // 0. Pending: t < 20
    if t_s < OPEN_TICK_S { return Decision::None; }

    // 1. Opening (one-shot at t=20)
    if !self.opened {
        return self.open_with_composite(tick);
    }

    // 2. Deadline safety: son 8s tüm aktivite dur
    if self.remaining_s(tick) <= DEADLINE_SAFETY_S {
        return Decision::Stop;
    }

    // 3. Pre-resolve scoop (lock'a aldırmaz)
    if opp_bid <= SCOOP_OPP_BID_MAX
        && remaining_s <= SCOOP_MIN_REMAINING_S
        && t_s - self.last_scoop_t_s >= SCOOP_COOLDOWN_S
    {
        return Decision::Scoop { /* dom @ ask-1 tick, $50 */ };
    }

    // 4. Signal flip (lock'a aldırmaz, max 1 kez)
    if dscore_from_open.abs() > SIGNAL_FLIP_THRESHOLD
        && self.flip_count < SIGNAL_FLIP_MAX_COUNT
    {
        return Decision::Flip { /* dom 2x boost, hedge 0.3x, freeze 60s */ };
    }

    if self.is_locked() { return Decision::None; }  // kâr garantili

    // 5. Avg-down (one-shot)
    if !self.avg_down_used
        && dom_bid + AVG_DOWN_MIN_EDGE <= avg_dom
    {
        return Decision::AvgDown { /* dom @ bid, $15 */ };
    }

    // 6. Pyramid (dom yönü güçleniyor)
    if tick.ofi >= PYRAMID_OFI_MIN
        && (t_s - score_persist_since_s) >= PYRAMID_SCORE_PERSIST_S
        && (t_s - last_pyr_t_s) >= PYRAMID_COOLDOWN_S
        && score_dir == self.intent
    {
        return Decision::Pyramid { /* dom @ bid, $15 */ };
    }

    // 7. Dom requote (fiyat 2 tick değişti)
    if (dom_bid - last_dom_price).abs() >= REQUOTE_PRICE_EPS
        && t_s - last_requote_dom_t_s >= REQUOTE_COOLDOWN_S
    {
        actions.push(Decision::RequoteDom { /* dom @ bid, $15 */ });
    }

    // 8. Hedge requote — SADECE opp YÜKSELİYORSA (kritik!)
    let hedge_drift = opp_bid - last_hedge_price;
    if hedge_drift >= REQUOTE_PRICE_EPS
        && t_s - last_requote_hedge_t_s >= REQUOTE_COOLDOWN_S
        && opp_bid >= PARITY_OPP_BID_MIN
        && t_s >= flip_freeze_until_s
    {
        actions.push(Decision::RequoteHedge { /* opp @ bid, $8 */ });
    }

    // 9. Parity gap (asimetri kontrolü)
    let gap = (up_filled - down_filled).abs();
    if gap > PARITY_MIN_GAP_QTY
        && t_s - last_parity_t_s >= PARITY_COOLDOWN_S
        && opp_bid >= PARITY_OPP_BID_MIN
        && t_s >= flip_freeze_until_s
    {
        actions.push(Decision::Parity { /* opp @ bid, gap qty */ });
    }

    actions
}
```

---

## 6. Signal Flip (Yön Düzeltici)

### 6.1 Tetikleyici

```
|score_now - opener_score| > SIGNAL_FLIP_THRESHOLD (= 5.0)
AND flip_count < SIGNAL_FLIP_MAX_COUNT (= 1)
AND new_intent != current_intent
```

**Neden 5.0?** Düşük eşik (2.0-3.0) **fakeout marketlerine** kapılıyor (7474800, 7476300 bu yüzden -$130 kayıp veriyordu eşik 3.0'da). 5.0 sadece **gerçek reversal** marketlerinde tetikleniyor (7473900, 7474500, 7468200).

### 6.2 Flip aksiyonu

```rust
// 1. State güncelle
self.flip_count += 1;
self.last_flip_t_s = t_s;
self.flip_freeze_until_s = t_s + FLIP_FREEZE_OPP_S; // 60s
self.intent = new_intent;
self.opener_score = score; // YENİ REFERANS
self.avg_down_used = false;
self.score_persist_since_s = t_s;

// 2. Yeni dom'a 2x boost (kayıpları telafi etmek için)
buy(new_intent, dom_bid, ORDER_USDC_DOM * 2.0 / dom_bid, "signal_flip");

// 3. Yeni hedge çok küçük (eski intent'e zaten çok pozisyon var)
buy(opposite(new_intent), hedge_bid, ORDER_USDC_HEDGE * 0.3 / hedge_bid, "flip_hedge");
```

### 6.3 Flip-Freeze (kritik!)

Flip sonrası **60 saniye** boyunca **opp tarafına alım yok** (`requote_hedge`, `parity_topup`). Sebep: flip = "eski intent yanlıştı". Eski intent'in tarafı şimdi opp ve **çok pahalı pozisyon** birikmiş durumda. O tarafa daha fazla alım kaybı büyütür.

Bu kural **7474500'da -$194 → +$0** ve **7473900'da -$50 → +$30** dönüşünü sağladı.

---

## 7. Hedge Requote — SADECE Opp Yükselirken

### 7.1 Eski (Alis-tarzı) sorun

Alis bot her hedge_drift'te (yukarı veya aşağı) requote yapıyor:

```python
if abs(opp_bid - last_hedge_price) >= eps: requote_hedge()  # YANLIŞ
```

Sonuç: opp_bid 0.30 → 0.20 → 0.15 → 0.10 düşerken bot her seferinde DOWN'a alım yapıyor. Bu **gereksiz** çünkü:
- DOWN düşüyor = UP kazanıyor (hedge gereksiz)
- Daha çok DOWN biriktirmek **beklenen kayıp** ekler

### 7.2 Yeni (Elis) kuralı

```rust
let hedge_drift = opp_bid - last_hedge_price;
if hedge_drift >= REQUOTE_PRICE_EPS  // SADECE artış
    && t_s - last_requote_hedge_t_s >= REQUOTE_COOLDOWN_S
    && opp_bid >= PARITY_OPP_BID_MIN
    && t_s >= flip_freeze_until_s
{
    // requote_hedge
}
```

**Sebep**: opp YÜKSELDİĞİNDE = "winner zayıflıyor, hedge tarafı güçleniyor" sinyali → hedge yenile. Opp DÜŞÜYORSA = "winner netleşiyor" → hedge gereksiz.

**Etki**: 1777476600 marketinde -$28 → **+$59** (hedge alımları 23 → 16 trade'e düştü).

---

## 8. Avg-Down (One-Shot)

### 8.1 Tetikleyici

```
!avg_down_used
AND avg_dom > 0  (yani önce dom alımı oldu)
AND dom_bid + AVG_DOWN_MIN_EDGE <= avg_dom
```

`AVG_DOWN_MIN_EDGE = 2.3 * TICK_SIZE = 0.023`

### 8.2 Aksiyon

```rust
self.avg_down_used = true;
buy(self.intent, dom_bid, ORDER_USDC_DOM / dom_bid, "avg_down");
```

**Mantık**: dom fiyatı **2.3 tick** düştü = pozisyon kaybediyor. Tek seferlik average-down ile maliyet düşür. **One-shot** çünkü tekrarlanırsa çukura düşülür.

---

## 9. Pyramid (Dom Yön Güçleniyor)

### 9.1 Tetikleyici

```
ofi >= PYRAMID_OFI_MIN (= 0.83)
AND score_persist_since_s + PYRAMID_SCORE_PERSIST_S elapsed (= 5s)
AND last_pyr_t_s + PYRAMID_COOLDOWN_S elapsed (= 3s)
AND score_dir matches intent
AND |dscore_from_open| < 1.0  (henüz flip alanına girmedi)
```

### 9.2 Aksiyon

```rust
buy(self.intent, dom_bid, PYRAMID_USDC / dom_bid, "pyramid");
self.last_pyr_t_s = t_s;
```

**Mantık**: dom yönü "iyi gidiyor" → ek alım ile kâr potansiyelini büyüt. Cooldown ile spam'i önle.

---

## 10. Parity Gap (Asimetri Düzeltme)

### 10.1 Tetikleyici

```
|up_filled - down_filled| > PARITY_MIN_GAP_QTY (= 250)
AND t_s - last_parity_t_s >= PARITY_COOLDOWN_S (= 5s)
AND opp_bid >= PARITY_OPP_BID_MIN (= 0.15)
AND t_s >= flip_freeze_until_s
```

### 10.2 Aksiyon

```rust
let size = (gap).min(80.0);  // tek seferde max 80 share
buy(opposite(self.intent), opp_bid, size, "parity_topup");
self.last_parity_t_s = t_s;
```

**Mantık**: kâr garantisi için `qty_dom ≈ qty_hedge` olmalı. Aşırı asimetri olduğunda ucuz hedge alımı ekle. **Floor**: opp_bid 0.15'in altındaysa zaten kazanan netleşmiş, hedge gereksiz.

---

## 11. Lock (Kâr Garantili Durum)

### 11.1 Tetikleyici

```
both_filled  (hem up_filled > 0 hem down_filled > 0)
AND avg_up + avg_down <= LOCK_AVG_THRESHOLD (= 0.97)
```

### 11.2 Etki

`Locked` state'inde **hiçbir yeni alım yok**. Sadece `signal_flip` ve `scoop` lock'ı bypass edebilir.

**Mantık**: Toplam maliyet 1'den az = kazanan tarafın 1.0 payout'u kâr garantili. Buradan sonra alım = ekstra risk.

### 11.3 Örnek

```
avg_up = 0.42, avg_down = 0.43
avg_sum = 0.85 ≤ 0.97 → LOCK
Hangi taraf kazanırsa: 1.0 - max(avg_up, avg_down) = 0.57 / share kâr (lock öncesi)
```

---

## 12. Scoop (Pre-Resolve Kâr Maksimizasyonu)

### 12.1 Tetikleyici

```
opp_bid <= SCOOP_OPP_BID_MAX (= 0.05)
AND remaining_s <= SCOOP_MIN_REMAINING_S (= 35s)
AND t_s - last_scoop_t_s >= SCOOP_COOLDOWN_S (= 2s)
```

### 12.2 Aksiyon

```rust
let dom_ask = tick.ask(self.intent);
let price = (dom_ask - TICK_SIZE).max(0.01);
let size = SCOOP_USDC / price;
buy(self.intent, price, size, "scoop");
self.last_scoop_t_s = t_s;
```

**Mantık**: opp_bid ≤ 0.05 = "loser fiyatı 5 cent'in altında" = winner neredeyse kesin. Bu noktada **dom @ 0.95+** alım = "neredeyse risksiz" pozisyon büyütme. Polymarket trade log'larında 1777467300 t=296'da 4358 share @ $0.01 alındı → ~$4314 payout (massive scoop pattern'ı).

---

## 13. Deadline Safety

```
remaining_s <= DEADLINE_SAFETY_S (= 8)
```

State `Stopping` → tüm aktivite durur. Son 8 saniyede yeni emir yok (resolve riski).

---

## 14. Konfigürasyon

### 14.1 `src/config.rs` — `ElisParams`

```rust
#[derive(Debug, Clone)]
pub struct ElisParams {
    // === TICK ===
    pub tick_size: f64,                    // 0.01

    // === COMPOSITE OPENER ===
    pub pre_opener_ticks: usize,           // 20
    pub bsi_rev_threshold: f64,            // 2.0
    pub ofi_exhaustion_threshold: f64,     // 0.4
    pub cvd_exhaustion_threshold: f64,     // 3.0
    pub ofi_directional_threshold: f64,    // 0.4
    pub dscore_strong_threshold: f64,      // 1.0
    pub score_neutral: f64,                // 5.0

    // === SIGNAL FLIP ===
    pub signal_flip_threshold: f64,        // 5.0
    pub signal_flip_max_count: u32,        // 1
    pub flip_freeze_opp_s: f64,            // 60.0

    // === ASYMMETRIC SIZING ===
    pub open_usdc_dom: f64,                // 25.0
    pub open_usdc_hedge: f64,              // 12.0
    pub order_usdc_dom: f64,               // 15.0
    pub order_usdc_hedge: f64,             // 8.0
    pub pyramid_usdc: f64,                 // 15.0
    pub scoop_usdc: f64,                   // 50.0
    pub max_size_share: f64,               // 400.0

    // === REQUOTE ===
    pub requote_price_eps: f64,            // 0.02 (2 tick)
    pub requote_cooldown_s: f64,           // 3.0

    // === AVG-DOWN ===
    pub avg_down_min_edge: f64,            // 0.023 (2.3 tick)

    // === PYRAMID ===
    pub pyramid_ofi_min: f64,              // 0.83
    pub pyramid_score_persist_s: f64,      // 5.0
    pub pyramid_cooldown_s: f64,           // 3.0

    // === PARITY ===
    pub parity_min_gap_qty: f64,           // 250.0
    pub parity_cooldown_s: f64,            // 5.0
    pub parity_opp_bid_min: f64,           // 0.15

    // === LOCK ===
    pub lock_avg_threshold: f64,           // 0.97

    // === SCOOP ===
    pub scoop_opp_bid_max: f64,            // 0.05
    pub scoop_min_remaining_s: f64,        // 35.0
    pub scoop_cooldown_s: f64,             // 2.0

    // === DEADLINE ===
    pub deadline_safety_s: f64,            // 8.0
    pub market_duration_s: f64,            // 300.0 (5 dakika)
}

impl Default for ElisParams {
    fn default() -> Self {
        Self {
            tick_size: 0.01,
            pre_opener_ticks: 20,
            bsi_rev_threshold: 2.0,
            ofi_exhaustion_threshold: 0.4,
            cvd_exhaustion_threshold: 3.0,
            ofi_directional_threshold: 0.4,
            dscore_strong_threshold: 1.0,
            score_neutral: 5.0,
            signal_flip_threshold: 5.0,
            signal_flip_max_count: 1,
            flip_freeze_opp_s: 60.0,
            open_usdc_dom: 25.0,
            open_usdc_hedge: 12.0,
            order_usdc_dom: 15.0,
            order_usdc_hedge: 8.0,
            pyramid_usdc: 15.0,
            scoop_usdc: 50.0,
            max_size_share: 400.0,
            requote_price_eps: 0.02,
            requote_cooldown_s: 3.0,
            avg_down_min_edge: 0.023,
            pyramid_ofi_min: 0.83,
            pyramid_score_persist_s: 5.0,
            pyramid_cooldown_s: 3.0,
            parity_min_gap_qty: 250.0,
            parity_cooldown_s: 5.0,
            parity_opp_bid_min: 0.15,
            lock_avg_threshold: 0.97,
            scoop_opp_bid_max: 0.05,
            scoop_min_remaining_s: 35.0,
            scoop_cooldown_s: 2.0,
            deadline_safety_s: 8.0,
            market_duration_s: 300.0,
        }
    }
}
```

### 14.2 `.env` örneği

```bash
ELIS_PRE_OPENER_TICKS=20
ELIS_BSI_REV_THRESHOLD=2.0
ELIS_OFI_EXHAUSTION_THRESHOLD=0.4
ELIS_CVD_EXHAUSTION_THRESHOLD=3.0
ELIS_OFI_DIRECTIONAL_THRESHOLD=0.4
ELIS_DSCORE_STRONG_THRESHOLD=1.0

ELIS_SIGNAL_FLIP_THRESHOLD=5.0
ELIS_FLIP_FREEZE_OPP_S=60

ELIS_OPEN_USDC_DOM=25
ELIS_OPEN_USDC_HEDGE=12
ELIS_ORDER_USDC_DOM=15
ELIS_ORDER_USDC_HEDGE=8
ELIS_SCOOP_USDC=50

ELIS_REQUOTE_PRICE_EPS=0.02
ELIS_REQUOTE_COOLDOWN_S=3
ELIS_PARITY_MIN_GAP_QTY=250
ELIS_PARITY_COOLDOWN_S=5
ELIS_PARITY_OPP_BID_MIN=0.15

ELIS_LOCK_AVG_THRESHOLD=0.97
ELIS_SCOOP_OPP_BID_MAX=0.05
ELIS_SCOOP_MIN_REMAINING_S=35
ELIS_DEADLINE_SAFETY_S=8
```

---

## 15. Test Sonuçları (16 market — 2026-04-29)

### 15.1 Genel istatistikler

| metrik | değer |
|---|---:|
| Toplam market | 16 |
| Kesin sonuçlu (final 0.99/0.01) | 13 |
| Belirsiz (mid-resolve) | 3 |
| **Yön doğruluğu (final intent)** | **12/13 = %92** |
| **Pozitif PnL marketler** | **10/13 = %77** |
| **Toplam kesin PnL** | **+$609** |
| Belirsiz mid PnL | -$49 |
| **NET PnL** | **+$560** |

### 15.2 Market başına PnL

| market | true | opener | flip | trade | PnL | yön |
|---|---|---|---|---:|---:|:---:|
| 1777467000 | Up | Up (ofi_dir) | - | 82 | **+$100** | ✓ |
| 1777467300 | Down | Down (bsi_rev) | - | 78 | **+$112** | ✓ |
| 1777467600 | Up? | Up (exhaustion) | - | 77 | UP+$31/DN-$174 | ? |
| 1777467900 | Down | Up (momentum) | - | 3 | -$24 | ✗ |
| 1777468200 | Up | Down (momentum) | →Up | 60 | -$76 | ✓ |
| 1777468500 | Down? | Down (exhaustion) | - | 22 | UP-$185/DN+$140 | ? |
| 1777471200 | Down | Down (momentum) | - | 62 | **+$97** | ✓ |
| 1777471800 | Down? | Up (score_avg) | - | 123 | UP+$293/DN-$205 | ? |
| 1777472100 | Up | Up (bsi_rev) | - | 18 | **+$68** | ✓ |
| 1777473000 | Down | Down (momentum) | - | 40 | **+$98** | ✓ |
| 1777473900 | Down | Up (score_avg) | →Down | 72 | **+$47** | ✓ |
| 1777474500 | Down | Up (score_avg) | →Down | 112 | -$127 | ✓ |
| 1777474800 | Down | Down (exhaustion) | - | 20 | **+$29** | ✓ |
| 1777475100 | Down | Down (score_avg) | - | 42 | **+$80** | ✓ |
| 1777476300 | Down | Down (momentum) | - | 103 | **+$148** | ✓ |
| 1777476600 | Up | Up (momentum) | - | 16 | **+$59** | ✓ |

### 15.3 Önceki versiyonlarla karşılaştırma

| versiyon | yön | PnL (kesin) | yorum |
|---|---|---:|---|
| v0 (composite + asymmetric, eşik 2.0) | 8/9 | -$48 | sample-içi, 12 market |
| v1 (eşik 3.0 + flip_freeze) | 9/9 | +$307 | 12 market |
| v2 (eşik 5.0) | 11/12 | +$305 | 15 market (yeni 3 fakeout sample) |
| **v3 (hedge sadece artış)** | **12/13** | **+$609** | **16 market — final** |

---

## 16. Faz 3 Implementasyon Planı

### 16.1 Görev listesi

- [ ] `src/config.rs`: `ElisParams` struct + `Default` + env loader
- [ ] `src/strategy/elis.rs`: `ElisState` enum (signal-driven)
- [ ] `src/strategy/elis.rs`: `compute_pre_opener_features()` helper
- [ ] `src/strategy/elis.rs`: `predict_opener()` 5-rule ladder
- [ ] `src/strategy/elis.rs`: `decide()` priority chain (10 katman)
- [ ] `src/strategy/elis.rs`: `place_open_pair()` / `requote_open_pair()` helpers
- [ ] `src/strategy/elis.rs`: `try_avg_down()` / `try_pyramid()` / `reconcile_parity()`
- [ ] `src/strategy/elis.rs`: `try_scoop()` / `signal_flip_action()` / `stop_trade()`
- [ ] `cargo check` + `cargo test`
- [ ] Backtest port: `scripts/backtest_market.py` → Rust integration test

### 16.2 Mevcut kod refactor

**Eski `src/strategy/elis.rs` (411 satır)** zone-tabanlı. **Tamamen rewrite gerek**.

`src/strategy/alis.rs`'i referans olarak okuma — ortak API'lar (`Strategy` trait, `Decision` enum) korunacak.

### 16.3 Test stratejisi

1. **Unit testler**:
   - `predict_opener()` — 16 market'in pre-opener feature'ları → beklenen intent
   - `signal_flip` tetikleyici — gerçek score evolution → flip aksiyonu
   - `is_locked()` — avg_sum hesabı

2. **Integration test** (`tests/elis_backtest.rs`):
   - 16 market tick verisini yükle, `Elis::decide()` ile simüle et
   - PnL ve yön doğruluğu Python sim ile karşılaştır
   - Tolerans: ±5% PnL fark (fill modeli farkları için)

3. **Live shadow run**:
   - Production'da 24 saat shadow mode (emir gönderimi yok, log ile)
   - Gerçek market'lerde tahmin doğruluğu izle

---

## 17. Bilinmeyen / Risk Alanları

### 17.1 Out-of-sample doğruluk
16 market sample-içi %92 → out-of-sample muhtemelen **%75-85** (overfitting riski). Production'da düşüş bekleniyor.

### 17.2 Fill modeli farkı
Sim **%100 fill** varsayıyor (her bid hedef seviyesine değince fill). Polymarket'te gerçek fill rate **%30-50** (taker bağımlılığı). Sonuç: **gerçek trade sayısı sim'in %30-50'si**, gerçek PnL **sim'in %30-70'i** olabilir.

### 17.3 Belirsiz market'ler
3 marketde (7467600, 7468500, 7471800) sonuç 5dk içinde resolve olmadı. Bot bu durumda mid-market satışla çıkış yapmalı (Polymarket'te SELL desteği gerek).

### 17.4 7467900 yanlış yön
`Up (momentum)` opener, gerçek DOWN. dscore=+1.0 yetersiz tahmin (gerçek DOWN trendine kayma t=180+). Eşik düşürmek (dscore_strong_threshold=0.5) bu marketi kurtarır ama başka yanlış pozitifler doğurur.

---

## 18. Geliştirme Yol Haritası

### v3.1 — Pyramid optimization
Şu an `pyramid_ofi_min=0.83` çok yüksek (16 marketde 1-2 kez tetikleniyor). 0.6'ya düşürmek + score_persist 8s'ye yükseltmek test edilebilir.

### v3.2 — Adaptive flip threshold
Eşik 5.0 sabit. Marketin score volatilite'sine göre adaptive yapılabilir:
```rust
let flip_th = (score_volatility_pre_opener * 3.0).max(3.0).min(7.0);
```

### v3.3 — Multi-market portfolio
Aynı anda 5-10 market'i izleyen ana scheduler. Her market'e $300 cap, toplam $3000 portfolio.

### v3.4 — Mid-market sell desteği
Polymarket Maker SDK'da SELL emri ekle. Belirsiz market'lerde t > 250s'de pozisyonu sat.
