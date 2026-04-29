# DCA Hedge Strategy Spec — BAITER Stratejisi

**Strateji**: DCA + Kademeli Hedge Arbitrajı (v3)
**Veri seti**: 16 BTC UP/DOWN 5m piyasası (bot14-ticks-20260429), 1 tick/sn book verisi.
**Sonuç**: 16 piyasada **+$88.15 net PnL** (+6.22% ROI, 13 kazanç / 3 kayıp)
**Tarih**: 2026-04-30
**Kaynak**: `scripts/arb_dca_v3.py`

---

## Bölüm 0 — İçindekiler

- [Bölüm 1 — Halk diliyle: ne oluyor burada?](#bölüm-1--halk-diliyle-ne-oluyor-burada)
- [Bölüm 2 — Tanımlar ve sözlük](#bölüm-2--tanımlar-ve-sözlük)
- [Bölüm 3 — Çekirdek fonksiyonlar (pseudocode)](#bölüm-3--çekirdek-fonksiyonlar-pseudocode)
- [Bölüm 4 — State machine](#bölüm-4--state-machine)
- [Bölüm 5 — Mekanizma kuralları](#bölüm-5--mekanizma-kuralları)
- [Bölüm 6 — Güvenlik kuralları (hard guards)](#bölüm-6--güvenlik-kuralları-hard-guards)
- [Bölüm 7 — Parametre tablosu](#bölüm-7--parametre-tablosu)
- [Bölüm 8 — Backtest kanıt tablosu](#bölüm-8--backtest-kanıt-tablosu)
- [Bölüm 9 — Edge case'ler ve hata senaryoları](#bölüm-9--edge-caseler-ve-hata-senaryoları)
- [Bölüm 10 — Replikasyon checklist'i](#bölüm-10--replikasyon-checklisti)

---

## Bölüm 1 — Halk diliyle: ne oluyor burada?

### 1.1. Strateji özeti

Polymarket'te her 5 dakikada bir açılan BTC UP/DOWN piyasalarında iki token satılır: UP ve DOWN. Kim kazanırsa kazansın, elindeki kazanan token sana $1 öder. Eğer hem 1 UP hem 1 DOWN token alırsan, sonuçtan bağımsız olarak **garantili $1** alırsın. Sır şurada: bu çifti $1'den ucuza toplarsan her pay başına kâr edersin.

```
pair_cost = avg_up_price + avg_down_price
pair_cost < 1.00  →  garantili kâr = (1.00 - pair_cost) × paylaşılan_share
```

Bu strateji yön tahmini yapmaz. Bunu okurken aklına "peki UP mu kazanır DOWN mu?" sorusu geliyorsa — bu soruyu sormana gerek yok. Strateji o soruyu hiç sormaz.

### 1.2. Neden yön tahmini yok?

Klasik yaklaşım şudur: önce hangi tarafın kazanacağına karar ver, sonra o tarafa yatır. Bu yaklaşımın problemi: trend tahmin modelleri pahalıdır, yanlış çalıştığında büyük zarar verir.

Bu strateji farklı bir mantık kurar:

> "Kim kazanırsa kazansın kâr edebilirim. Tek ihtiyacım: iki tarafı toplamda $1'den ucuza toplamak."

Bunu yapmanın yolu şu: piyasada bir taraf henüz öne geçmişken diğer taraf ucuzlaşır. Ucuzlaşan tarafa (< $0.50) kademeli hedge emir gönderilir; öne geçen pahalı tarafa (> $0.50) ise DCA ile alım yapılır. İkisi birleşince pair_cost < 1.00 olur ve kâr garantilenir.

### 1.3. İki mekanizma

**Mekanizma 1 — DCA döngüsü (her 2 saniyede çalışır):**

Pahalı taraf (`mid > 0.50`) fiyatı her `DCA_MIN_DROP` (0.01) düştüğünde `bid - 1×TICK` fiyatına yeni bir GTC limit alım emri gönderilir. Fiyat düştükçe ortalama maliyet düşer. Limit: `MAX_USD_PER_SIDE = $500`.

```
Örnek: DOWN mid = 0.78 → bid = 0.77 → emir @ 0.76
       Sonraki 2s: DOWN mid = 0.75 → bid = 0.74 → emir @ 0.73 (avg düştü)
```

**Mekanizma 2 — Kademeli Hedge (DCA fill tetikler):**

DCA emri dolduğunda karşı taraf (ucuz, `mid < 0.50`) için sıralı hedge emri açılır. Hedef: mümkün olan en düşük fiyatla doldurmak.

```
Adım 1: bid - 3×TICK  →  HEDGE_STEP_S (6s) bekle → fill? → ARB kilidi
Adım 2: bid - 2×TICK  →  6s bekle → fill? → ARB kilidi
Adım 3: bid - 1×TICK  →  6s bekle → fill? → ARB kilidi
         Dolmadı → vazgeç (pozisyon unhedged kalır)
```

Hedge fill olduğunda `pair_cost = dca_avg + hedge_avg` hesaplanır. `< 1.00` ise garantili kâr kilitlenir.

### 1.4. Fiyat bandı neden var?

Strateji yalnızca `BAND_LOW (0.10) < fiyat < BAND_HIGH (0.90)` aralığında çalışır.

- **> 0.90**: Pahalı taraf settlement'a yaklaşıyor. $1'e doğru gidiyor zaten; bu noktada alım yapmak DCA avantajı sağlamaz, aksine yüksek maliyet kalıcı hale gelir.
- **< 0.10**: Ucuz taraf settlement'da sıfıra gidecek kaybeden. Bu noktada hedge fill olsa bile pair_cost < 1.00 garantisi artık anlamlı değildir (ucuz tarafın ödeyeceği $0 olacak).

Bant, bot'u settlement öncesi son dakika volatilitesinden ve anlamsız emirlerden korur.

### 1.5. Risk profili

Stratejinin tek riski: **DCA emri doldu, ama hedge fill olmadı.** Bu durumda pahalı taraf share'leri hedge'siz kalır. Piyasa kapanışında:
- Pahalı taraf kazanırsa → hedge'siz share'ler de $1'e gider, ekstra kâr.
- Pahalı taraf kaybederse → hedge'siz share'ler $0 olur, zarar.

16-market backtestte 3 zararlı market bu sebepten zarar etti (hedge fill olmadı, unhedged pahalı taraf kaybetti).

---

## Bölüm 2 — Tanımlar ve sözlük

### 2.1. Piyasa terimleri

| Terim | Tanım | Birim / range |
|---|---|---|
| `epoch` | Pencerenin Unix timestamp olarak başlangıcı (örn. 1777467000) | int seconds |
| `slug` | Piyasanın insan-okur kimliği (örn. `btc-updown-5m-1777467000`) | str |
| `condition_id` | Polymarket'in 0x-prefixed event kimliği | hex32 |
| `asset_id` | UP veya DOWN token'ının on-chain kimliği | uint256 |
| `size` | Token sayısı | float |
| `price` | Token başı USDC fiyatı | float [0, 1] |
| `ts_ms` | Book snapshot'ının zaman damgası | int milliseconds |
| `up_best_bid` | UP token kitabındaki en iyi bid fiyatı | float [0, 1] |
| `up_best_ask` | UP token kitabındaki en iyi ask fiyatı | float [0, 1] |
| `down_best_bid` | DOWN token kitabındaki en iyi bid | float [0, 1] |
| `down_best_ask` | DOWN token kitabındaki en iyi ask | float [0, 1] |
| `t_ep` | `ts_ms / 1000 - epoch`, pencere içi göreli saniye | float [0, 305] |

### 2.2. Emir tipleri

| Tip | Anlam | Kullanım |
|---|---|---|
| `GTC` | Good-Till-Cancelled | DCA ve hedge emirleri — kitapta fill'i bekler |
| **maker** | Kitabın bid tarafında bekleyen emir | Pasif; rebate alır |
| **taker** | Bid/ask'ı süpüren agresif emir | Aktif; fee öder — bu stratejide kullanılmaz |

Bu strateji yalnızca GTC maker emirleri gönderir. Taker emri yoktur.

### 2.3. Stratejik kavramlar

**`mid(side)`**: Tarafın anlık orta fiyatı.
```
mid(UP)   = (up_best_bid + up_best_ask) / 2
mid(DOWN) = (down_best_bid + down_best_ask) / 2
```

**`expensive_side`**: `mid > CHEAP_THRESHOLD (0.50)` olan taraf. DCA uygulanır.

**`cheap_side`**: `mid < CHEAP_THRESHOLD (0.50)` olan taraf. Hedge emri gönderilir.

**`dca_avg(side)`**: O taraftaki birikimli fill'lerin ağırlıklı ortalaması.
```
dca_avg = Σ(fill_price × fill_shares) / Σ(fill_shares)
```

**`pair_cost`**: İki tarafın birikimli ortalama maliyeti toplamı.
```
pair_cost = dca_avg(UP) + dca_avg(DOWN)
```
- `< 1.00` → garantili kâr kilitlenebilir
- `= 1.00` → break-even
- `> 1.00` → kayıp (hedging ile bile zarar)

**`guaranteed_pnl`**: Kilitlenmiş pair'lar üzerinden sonuçtan bağımsız kâr.
```
n_pairs    = min(up_hedged_shares, down_hedged_shares)
guaranteed = n_pairs × (1.00 - pair_cost)
```

**`settle_pnl`**: Hedge edilemeyen (unhedged) share'lerin kapanıştaki değeri.
```
if winner == UP:
    settle = (1 - dca_avg(UP)) × up_unhedged - dca_avg(DOWN) × down_unhedged
if winner == DOWN:
    settle = (1 - dca_avg(DOWN)) × down_unhedged - dca_avg(UP) × up_unhedged
```

**`hedge_job`**: Bir DCA fill'inin ardından açılan kademeli hedge denemesi.
```
HedgeJob {
    main_side:      str,       // DCA fill olan taraf ("UP" | "DOWN")
    hedge_side:     str,       // Hedge gönderilecek taraf
    main_fill_px:   float,     // DCA'nın fill olduğu fiyat
    step:           int,       // 0=bid-3t, 1=bid-2t, 2=bid-1t
    step_ts:        float,     // Mevcut adım başlangıç zamanı
    order_px:       float?,    // Aktif hedge emrinin fiyatı
    done:           bool,      // Tamamlandı mı (fill veya tükendi)
}
```

**`arb_lock`**: Hedge fill olduğunda pair_cost < 1.00 ise oluşan garantili kâr kaydı.
```
ArbLock {
    pair_cost: float,
    shares:    float,   // kilitlenmiş share adedi
    pnl:       float,   // (1.00 - pair_cost) × shares
    t:         int,     // piyasa içi saniye
}
```

**`band_guard`**: Fiyatın çalışma bandı dışında olup olmadığı kontrolü.
```
pass_band_dca(side)   = CHEAP_THRESHOLD < mid(side) < BAND_HIGH
pass_band_hedge(side) = BAND_LOW < mid(side) < CHEAP_THRESHOLD
```

### 2.4. Zaman kavramları

Bu stratejide zaman-bazlı faz geçişi yoktur. Tek zaman kavramı:

| Kavram | Değer | Açıklama |
|---|---|---|
| `POLL_S` | 2s | DCA kontrol döngüsü aralığı |
| `HEDGE_STEP_S` | 6s | Hedge adım geçiş süresi |
| Piyasa süresi | ~300s | Market başlangıcından kapanışa |

Market başlangıcından kapanışına kadar her iki mekanizma da kesintisiz aktiftir.

---

## Bölüm 3 — Çekirdek fonksiyonlar (pseudocode)

### 3.1. Veri yapıları

```rust
struct BookSnapshot {
    ts_ms:          u64,
    up_best_bid:    f64,
    up_best_ask:    f64,
    down_best_bid:  f64,
    down_best_ask:  f64,
}

impl BookSnapshot {
    fn bid(&self, side: &str) -> f64 {
        if side == "UP" { self.up_best_bid } else { self.down_best_bid }
    }
    fn ask(&self, side: &str) -> f64 {
        if side == "UP" { self.up_best_ask } else { self.down_best_ask }
    }
    fn mid(&self, side: &str) -> f64 {
        (self.bid(side) + self.ask(side)) / 2.0
    }
    fn is_expensive(&self, side: &str) -> bool {
        self.mid(side) > CHEAP_THRESHOLD
    }
    fn is_cheap(&self, side: &str) -> bool {
        self.mid(side) < CHEAP_THRESHOLD
    }
}

struct SideBucket {
    side:           String,
    fills:          Vec<(f64, f64)>,   // (price, shares)
    hedged_shares:  f64,
    pending_price:  Option<f64>,
    pending_ts:     Option<f64>,
}

impl SideBucket {
    fn total_shares(&self) -> f64 { self.fills.iter().map(|(_, s)| s).sum() }
    fn total_cost(&self) -> f64   { self.fills.iter().map(|(p, s)| p * s).sum() }
    fn avg(&self) -> f64 {
        if self.total_shares() > 0.0 { self.total_cost() / self.total_shares() } else { 0.0 }
    }
    fn unhedged(&self) -> f64 { self.total_shares() - self.hedged_shares }
    fn has_pending(&self) -> bool { self.pending_price.is_some() }

    fn check_fill(&self, book: &BookSnapshot) -> bool {
        if let Some(px) = self.pending_price {
            book.ask(&self.side) <= px || book.bid(&self.side) >= px
        } else { false }
    }
}

struct HedgeJob {
    main_side:      String,
    hedge_side:     String,
    main_fill_px:   f64,
    ts:             f64,
    step:           usize,         // 0, 1, 2
    step_ts:        f64,
    order_px:       Option<f64>,
    done:           bool,
}

impl HedgeJob {
    fn exhausted(&self) -> bool { self.step >= HEDGE_TICKS.len() }

    fn check_fill(&self, book: &BookSnapshot) -> bool {
        if let Some(px) = self.order_px {
            !self.done &&
            (book.ask(&self.hedge_side) <= px || book.bid(&self.hedge_side) >= px)
        } else { false }
    }
}

struct MarketState {
    epoch:      u64,
    up:         SideBucket,
    down:       SideBucket,
    jobs:       Vec<HedgeJob>,
    arb_locks:  Vec<ArbLock>,
    last_poll:  HashMap<String, f64>,   // side → son poll zamanı
}
```

### 3.2. Band guard

```rust
/// DCA tarafı için band kontrolü
fn pass_band_dca(side: &str, book: &BookSnapshot) -> bool {
    let mid = book.mid(side);
    book.is_expensive(side) &&      // mid > CHEAP_THRESHOLD
    mid < BAND_HIGH &&              // üst bant altında
    mid > CHEAP_THRESHOLD           // alt bant üstünde (zaten is_expensive garantiler)
}

/// Hedge tarafı için band kontrolü
fn pass_band_hedge(side: &str, book: &BookSnapshot) -> bool {
    let mid = book.mid(side);
    book.is_cheap(side) &&          // mid < CHEAP_THRESHOLD
    mid >= BAND_LOW                 // alt bant üstünde
}
```

### 3.3. DCA poll (her 2 saniyede çağrılır)

```rust
fn on_poll_tick(
    side: &str,
    book: &BookSnapshot,
    state: &mut MarketState,
    now: f64,
) -> Option<OrderIntent> {
    let bucket = state.bucket_mut(side);

    // Bant kontrolü
    if !pass_band_dca(side, book) { return None; }

    // Maksimum maliyet limiti
    if bucket.total_cost() >= MAX_USD_PER_SIDE { return None; }

    // Hesaplanan emir fiyatı
    let cur_bid = book.bid(side);
    let entry   = (cur_bid - TICK).max(TICK);

    // Mevcut bekleyen emir varsa: çok daha iyi fiyat oluştuysa güncelle
    if bucket.has_pending() {
        if entry < bucket.pending_price.unwrap() - TICK {
            bucket.cancel_pending();   // eski iptal
        } else {
            return None;               // mevcut emir yeterli
        }
    }

    // DCA tetik: ilk giriş veya avg'dan yeterince düşüş
    let first  = bucket.total_shares() == 0.0;
    let dca_ok = bucket.total_shares() > 0.0 && entry < bucket.avg() - DCA_MIN_DROP;

    if !first && !dca_ok { return None; }

    bucket.pending_price = Some(entry);
    bucket.pending_ts    = Some(now);

    Some(OrderIntent {
        side:       side.to_string(),
        price:      entry,
        size:       SHARES,
        order_type: OrderType::GTC,
    })
}
```

### 3.4. DCA fill işleme

```rust
fn on_dca_fill(
    side: &str,
    fill_px: f64,
    book: &BookSnapshot,
    state: &mut MarketState,
    now: f64,
) -> Option<HedgeJob> {
    // Pozisyona ekle
    state.bucket_mut(side).add_fill(fill_px, SHARES);

    // Hedge tarafını belirle
    let hedge_side = opposite(side);

    // Hedge tarafı bant içinde mi?
    if !pass_band_hedge(hedge_side, book) { return None; }

    // Hedge görevi oluştur
    Some(HedgeJob {
        main_side:    side.to_string(),
        hedge_side:   hedge_side.to_string(),
        main_fill_px: fill_px,
        ts:           now,
        step:         0,
        step_ts:      now,
        order_px:     None,
        done:         false,
    })
}
```

### 3.5. Hedge görevi adım yönetimi

```rust
/// Her tick'te aktif hedge görevleri için çağrılır.
fn advance_hedge_job(
    job: &mut HedgeJob,
    book: &BookSnapshot,
    state: &mut MarketState,
    now: f64,
) -> Option<OrderIntent> {
    if job.done { return None; }

    let hs = &job.hedge_side;

    // Fill kontrolü
    if job.check_fill(book) {
        let px = job.order_px.unwrap();
        let result = on_hedge_fill(job, px, state);
        job.done = true;
        return None;   // Emir zaten doldu, yeni emir gerekmez
    }

    // Adım geçişi: süre doldu veya ilk adım
    let needs_new = job.order_px.is_none() ||
                    (now - job.step_ts) >= HEDGE_STEP_S;

    if !needs_new { return None; }

    // Tüm adımlar tükendi
    if job.exhausted() {
        job.done = true;
        return None;
    }

    // Hedge tarafı bant dışına çıktıysa iptal
    if !pass_band_hedge(hs, book) {
        job.done = true;
        return None;
    }

    let tick_offset = HEDGE_TICKS[job.step];
    let h_bid       = book.bid(hs);
    let h_px        = (h_bid - tick_offset as f64 * TICK).max(TICK).min(0.99);

    job.order_px = Some(h_px);
    job.step_ts  = now;
    job.step    += 1;

    Some(OrderIntent {
        side:       hs.clone(),
        price:      h_px,
        size:       SHARES,
        order_type: OrderType::GTC,
    })
}
```

### 3.6. Hedge fill → ARB kilidi

```rust
fn on_hedge_fill(
    job: &HedgeJob,
    fill_px: f64,
    state: &mut MarketState,
) -> Option<ArbLock> {
    let b_h = state.bucket_mut(&job.hedge_side);
    b_h.add_fill(fill_px, SHARES);

    let b_m = state.bucket(&job.main_side);
    let pair_cost  = b_m.avg() + b_h.avg();
    let hedgeable  = b_m.unhedged().min(b_h.unhedged());

    if pair_cost < 1.00 && hedgeable > 0.0 {
        let pnl = (1.00 - pair_cost) * hedgeable;
        state.bucket_mut(&job.main_side).hedged_shares += hedgeable;
        state.bucket_mut(&job.hedge_side).hedged_shares += hedgeable;
        Some(ArbLock { pair_cost, shares: hedgeable, pnl })
    } else {
        None   // Hedge fill oldu ama pair_cost >= 1.00 → kârsız
    }
}
```

### 3.7. Ana tick döngüsü

```rust
/// Her book güncellemesinde (tick) çağrılır.
fn on_book_tick(book: &BookSnapshot, state: &mut MarketState) -> Vec<Action> {
    let now = book.ts_ms as f64 / 1000.0;
    let mut actions = vec![];

    // Hard guards (Bölüm 6)
    if !pass_hard_guards(book, state) {
        actions.push(Action::CancelAllAndHalt);
        return actions;
    }

    // ── DCA fill kontrolü (her iki taraf) ────────────────────────────
    for side in ["UP", "DOWN"] {
        if state.bucket(side).check_fill(book) {
            let fill_px = state.bucket(side).pending_price.unwrap();
            state.bucket_mut(side).pending_price = None;

            if let Some(job) = on_dca_fill(side, fill_px, book, state, now) {
                state.jobs.push(job);
            }
        }
    }

    // ── Hedge görevleri ──────────────────────────────────────────────
    for job in state.jobs.iter_mut() {
        if let Some(order) = advance_hedge_job(job, book, state, now) {
            actions.push(Action::PlaceOrder(order));
        }
    }
    state.jobs.retain(|j| !j.done);

    // ── DCA poll (her POLL_S saniyede) ───────────────────────────────
    for side in ["UP", "DOWN"] {
        if now - state.last_poll[side] < POLL_S { continue; }
        state.last_poll.insert(side.to_string(), now);

        if let Some(order) = on_poll_tick(side, book, state, now) {
            actions.push(Action::PlaceOrder(order));
        }
    }

    actions
}
```

### 3.8. Settlement

```rust
fn on_market_close(winner: &str, state: &MarketState) -> f64 {
    let b_up = &state.up;
    let b_dn = &state.down;

    let guaranteed = state.arb_locks.iter().map(|l| l.pnl).sum::<f64>();

    let settle = match winner {
        "UP" => (1.0 - b_up.avg()) * b_up.unhedged()
                - b_dn.avg() * b_dn.unhedged(),
        "DOWN" => (1.0 - b_dn.avg()) * b_dn.unhedged()
                  - b_up.avg() * b_up.unhedged(),
        _ => 0.0,
    };

    guaranteed + settle
}
```

---

## Bölüm 4 — State machine

```
┌─────────┐
│  IDLE   │  (market henüz açılmadı)
└────┬────┘
     │ book aktif (up_bid > 0 AND down_bid > 0)
     ▼
┌─────────────────────────────────────────────────────┐
│  ACTIVE   (market boyunca sürekli — t=0 → t≈300s)  │
│                                                      │
│  Her tick:                                           │
│  ├── [DCA_FILL?] bucket.check_fill(book)             │
│  │      └── fill → on_dca_fill → HedgeJob oluştur    │
│  │                                                   │
│  ├── [HEDGE_CASCADE] her aktif HedgeJob için         │
│  │      ├── job.check_fill → on_hedge_fill → ArbLock │
│  │      ├── step_timeout? → sonraki adım             │
│  │      │      bid-3t → (6s) → bid-2t → (6s) → bid-1t│
│  │      └── exhausted → job.done = true              │
│  │                                                   │
│  └── [POLL] her POLL_S (2s) saniyede                 │
│         ├── expensive_side (0.5 < mid < 0.9):        │
│         │      bid düştüyse → GTC @ bid-1t           │
│         └── cheap_side → atla (DCA değil, hedge)     │
│                                                      │
│  Band guard her işlem öncesi:                        │
│  ├── DCA: CHEAP_THRESHOLD < mid < BAND_HIGH          │
│  └── Hedge: BAND_LOW < mid < CHEAP_THRESHOLD         │
└────┬────────────────────────────────────────────────┘
     │ resolution event (t ≥ 305s)
     ▼
┌─────────────────┐
│     CLOSED      │
│  redeem winner  │
│  settle unhedged│
└─────────────────┘
```

### 4.1. Durum geçiş kuralları

| Mevcut → Sonraki | Tetik | Aksiyon |
|---|---|---|
| Idle → Active | `up_bid > 0 AND down_bid > 0` | `last_poll` başlat, bucket'ları sıfırla |
| Active → Active | Her tick | DCA fill, hedge adım, poll döngüsü |
| Active → Closed | `t_ep ≥ 305 OR resolution event` | Redeem winner, arb_locks raporla |
| ANY → Halt | `pass_hard_guards == false` | CancelAll |

---

## Bölüm 5 — Mekanizma kuralları

### 5.1. DCA_POLL — Pahalı tarafa DCA

**Amaç**: Kazanan/pahalı tarafı (`mid > 0.50`) fiyat düştükçe ucuza toplamak. Ortalama maliyet düşer, pair_cost < 1.00 olasılığı artar.

**Tetik koşulları (TÜMÜ sağlanmalı)**:
1. Son poll'dan bu yana `≥ POLL_S (2s)` geçti
2. `mid(side) > CHEAP_THRESHOLD (0.50)` — pahalı taraf
3. `mid(side) < BAND_HIGH (0.90)` — üst bant içinde
4. `bucket.total_cost < MAX_USD_PER_SIDE ($500)`
5. Pending emir yok VEYA mevcut emirden `> 1×TICK` daha iyi fiyat oluştu

**DCA tetik** (koşul 5 sonrası):
- `bucket.total_shares == 0` → ilk giriş, her zaman tetikle
- `entry_price < bucket.avg() - DCA_MIN_DROP (0.01)` → ortalama düşürme, tetikle

**Emir**:
```
price = bid(side) - 1×TICK
type  = GTC
size  = SHARES (40 token)
```

**Yapılmaması gerekenler**:
- `mid > BAND_HIGH (0.90)` iken DCA yapma (settlement yakın, ortalama yüksek kalır)
- `mid < CHEAP_THRESHOLD (0.50)` iken DCA yapma (ucuz tarafa DCA değil, hedge gönder)
- Aynı 2s pencerede aynı tarafa birden fazla emir

### 5.2. HEDGE_CASCADE — Ucuz tarafa kademeli hedge

**Amaç**: DCA fill'in ardından karşı tarafta (ucuz, `mid < 0.50`) mümkün olan en düşük fiyattan hedge almak. pair_cost < 1.00 ise garantili kâr kilitle.

**Tetik**: DCA fill olur ve karşı taraf `pass_band_hedge == true`

**Adım makinesi**:

| Adım | Emir fiyatı | Timeout | Sonuç |
|---|---|---|---|
| 1 | `bid - 3×TICK` | `HEDGE_STEP_S (6s)` | Fill → ArbLock; timeout → Adım 2 |
| 2 | `bid - 2×TICK` | `HEDGE_STEP_S (6s)` | Fill → ArbLock; timeout → Adım 3 |
| 3 | `bid - 1×TICK` | `HEDGE_STEP_S (6s)` | Fill → ArbLock; timeout → Vazgeç |

**Her adımda**:
- Önceki adımın emrini iptal et
- Anlık `bid(hedge_side)` okuyarak yeni emir fiyatını hesapla
- `BAND_LOW ≤ mid(hedge_side) < CHEAP_THRESHOLD` kontrolü yap; bant dışıysa tüm görevi iptal et

**Fill sonrası**:
```
pair_cost = dca_avg(main_side) + dca_avg(hedge_side)
if pair_cost < 1.00:
    pnl = (1.00 - pair_cost) × min(main_unhedged, hedge_unhedged)
    → ArbLock kaydet, share'leri hedged olarak işaretle
```

**Yapılmaması gerekenler**:
- `mid(hedge_side) < BAND_LOW (0.10)` iken hedge emri gönderme — kaybeden taraf settlement'a çok yakın
- 3 adım tükendikten sonra devam etme — pozisyon unhedged kalır, risk kabul edilmiş sayılır
- Hedge fill olmadan yeni bir DCA fill'i hedge etmek için ayrı job başlat — her fill için bağımsız job

---

## Bölüm 6 — Güvenlik kuralları (hard guards)

Bu kurallar **her book tick'inde** kontrol edilir. İhlal halinde `Action::CancelAllAndHalt` döndürülür.

### 6.1. Connectivity guards

| Kural | Eşik | Aksiyon |
|---|---|---|
| WebSocket gecikme | Son tick > 3s önce | Reconnect, REST polling'e geç |
| Heartbeat eksik | Son heartbeat > 4s önce | Acil heartbeat; başarısızsa Halt |
| Clock drift | Local vs server > 500ms | Uyarı; persist ederse Halt |
| API key geçersiz | Response 401/403 | Halt + alert |

### 6.2. Position guards

| Kural | Eşik | Aksiyon |
|---|---|---|
| Taraf başı maliyet | `bucket.total_cost > MAX_USD_PER_SIDE ($500)` | O taraf için yeni DCA emri verme |
| pair_cost | `dca_avg(UP) + dca_avg(DOWN) > 1.00` | Hedge emirlerini durdur (kârsız) |
| Unhedged exposure | `unhedged_shares × avg_price > MAX_UNHEDGED_USD` | Yeni DCA durdur |

### 6.3. Order placement guards

| Kural | Eşik | Aksiyon |
|---|---|---|
| Min order size | `size < 5` (Polymarket minimum) | Locally reject |
| Size multiple | `size % 5 != 0` | 5'in katına yuvarla |
| Price tick size | Tick = 0.01 | 0.01'in katına yuvarla |
| Open order count | `> 50` | En eski / en uzak fiyatlı emri iptal et |

### 6.4. Band guard (her emir öncesi)

```rust
fn pass_hard_guards(book: &BookSnapshot, state: &MarketState) -> bool {
    // Bağlantı
    if !connectivity_ok() { return false; }

    // Her iki taraf: pahalı taraf bandın dışındaysa yalnızca DCA durdur,
    // sistemi halt etme — işlem limitleri bireysel emir seviyesinde uygulanır.
    // Sadece sistematik hata varsa (auth, WS) tam halt yap.
    true
}
```

DCA ve hedge emirleri için band kontrolleri `on_poll_tick` ve `advance_hedge_job` içinde bireysel olarak uygulanır. Sistemi tamamen durdurmaz; yalnızca o tarafa o tick'te emir verilmez.

---

## Bölüm 7 — Parametre tablosu

### 7.1. Temel parametreler

| Parametre | Default | Min | Max | Açıklama |
|---|---:|---:|---:|---|
| `TICK` | 0.01 | 0.001 | 0.01 | Minimum fiyat adımı (Polymarket sabit) |
| `POLL_S` | 2 | 1 | 10 | DCA kontrol aralığı (saniye) |
| `DCA_MIN_DROP` | 0.01 | 0.005 | 0.05 | Avg'dan min düşüş tetik (1 tick = 0.01) |
| `SHARES` | 40.0 | 5.0 | 200.0 | Her emir token miktarı |
| `MAX_USD_PER_SIDE` | 500.0 | 50.0 | 2000.0 | Taraf başına maksimum USDC |
| `HEDGE_STEP_S` | 6 | 2 | 30 | Hedge adım arası bekleme süresi (saniye) |
| `HEDGE_TICKS` | [3, 2, 1] | — | — | Hedge bid offset sırası |

### 7.2. Bant parametreleri

| Parametre | Default | Açıklama |
|---|---:|---|
| `CHEAP_THRESHOLD` | 0.50 | Bu değerin üstü = pahalı (DCA), altı = ucuz (hedge) |
| `BAND_LOW` | 0.10 | Bu altında işlem yapma (settlement yakın, kayıp riski) |
| `BAND_HIGH` | 0.90 | Bu üstünde DCA yapma (settlement yakın, avg yüksek kalır) |

### 7.3. Parametre ayarlama rehberi

| Hedef | Değişiklik | Etki |
|---|---|---|
| Daha az sermaye kullan | `MAX_USD_PER_SIDE` düşür | Daha az fill, daha az ARB kilidi |
| Daha sık DCA | `DCA_MIN_DROP` düşür (0.005) | Daha küçük fiyat düşüşünde tetikle |
| Hedge daha agresif | `HEDGE_STEP_S` düşür (3s) | Her adıma daha az bekle, fill olasılığı düşer |
| Daha geniş bant | `BAND_LOW` düşür, `BAND_HIGH` artır | Daha uzun süre aktif, settlement riski artar |
| Daha az settlement riski | Bant daralt | Daha az DCA fırsatı |

---

## Bölüm 8 — Backtest kanıt tablosu

**Veri**: 16 market, bot14-ticks-20260429, `scripts/arb_dca_v3.py v3`
**Parametreler**: POLL_S=2, DCA_MIN_DROP=0.01, SHARES=40, MAX_USD/taraf=$500, HEDGE_STEP_S=6, BAND=[0.10–0.90]

### 8.1. Market bazında sonuçlar

| Epoch | Winner | UP_sh | UP_avg | DN_sh | DN_avg | Maliyet | ARB | Garanti | Settle | TOPLAM | ROI |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1777467000 | UP | 80✓ | 0.510 | 80 | 0.445 | $76.40 | 2 | +$3.40 | $0.00 | **+$3.40** | +4.5% |
| 1777467300 | DOWN | 80 | 0.500 | 80✓ | 0.450 | $76.00 | 2 | +$4.00 | $0.00 | **+$4.00** | +5.3% |
| 1777467600 | DOWN | 120 | 0.347 | 160✓ | 0.435 | $111.20 | 3 | +$15.60 | +$22.60 | **+$38.20** | +34.4% |
| 1777467900 | DOWN | 160 | 0.450 | 160✓ | 0.500 | $152.00 | 4 | +$8.00 | $0.00 | **+$8.00** | +5.3% |
| 1777468200 | UP | 80✓ | 0.500 | 80 | 0.435 | $74.80 | 2 | +$4.60 | $0.00 | **+$4.60** | +6.1% |
| 1777468500 | DOWN | 40 | 0.490 | 40✓ | 0.460 | $38.00 | 1 | +$2.00 | $0.00 | **+$2.00** | +5.3% |
| 1777471200 | DOWN | 80 | 0.435 | 40✓ | 0.570 | $57.60 | 0 | $0.00 | **-$17.60** | **-$17.60** | -30.6% |
| 1777471800 | DOWN | 160 | 0.500 | 120✓ | 0.447 | $133.60 | 3 | +$5.80 | **-$20.00** | **-$14.20** | -10.6% |
| 1777472100 | UP | 40✓ | 0.490 | 40 | 0.460 | $38.00 | 1 | +$2.00 | $0.00 | **+$2.00** | +5.3% |
| 1777473000 | DOWN | 40 | 0.360 | 40✓ | 0.550 | $36.40 | 1 | +$3.60 | $0.00 | **+$3.60** | +9.9% |
| 1777473900 | DOWN | 160 | 0.508 | 120✓ | 0.417 | $131.20 | 3 | +$9.63 | **-$20.30** | **-$10.67** | -8.1% |
| 1777474500 | DOWN | 40 | 0.500 | 40✓ | 0.450 | $38.00 | 1 | +$2.00 | $0.00 | **+$2.00** | +5.3% |
| 1777474800 | DOWN | 120 | 0.447 | 120✓ | 0.503 | $114.00 | 3 | +$4.80 | $0.00 | **+$4.80** | +4.2% |
| 1777475100 | DOWN | 40 | 0.450 | 80✓ | 0.495 | $57.60 | 1 | +$1.60 | +$20.20 | **+$21.80** | +37.8% |
| 1777476300 | DOWN | 80 | 0.485 | 80✓ | 0.460 | $75.60 | 2 | +$4.20 | $0.00 | **+$4.20** | +5.6% |
| 1777476600 | UP | 240✓ | 0.517 | 200 | 0.410 | $206.00 | 5 | +$12.69 | +$19.33 | **+$32.02** | +15.5% |

**Toplam**: Yatırım $1,416.40 → PnL **+$88.15** → ROI **+6.22%** → **13W / 3L**

### 8.2. Mekanizma kanıtları

| Kural | Gözlem | Örnek market |
|---|---|---|
| DCA: pahalı taraf ilk giriş t=2s | Her markette t=2-3s'de ilk fill | 1777467000: UP@0.52, t=2 |
| DCA: avg drop tetik | 0.01 düşüşte yeni emir | 1777474800: DOWN 0.52→0.49→0.49 |
| Hedge: bid-3t ilk adım | Çoğu hedge adım1'de fill | 1777467000: UP fill@0.52 → DN hedge@0.44 (bid=0.47) |
| Hedge: pair_cost < 1.00 → ARB kilidi | pair_cost=0.94-0.96 arası kilitleme | 1777467000: 0.510+0.440=0.950, +$1.60/pair |
| Band guard: 0.10 altı atla | Kaybeden taraf 0.10 altında hedge yok | 1777467900: DN < 0.10 periyotlarında 0 hedge |
| Band guard: 0.90 üstü DCA yok | Kazanan 0.90 üstüne çıkınca DCA durdu | Tüm marketlerde ARB sayısı piyasa ortası yoğun |
| Unhedged zarar | ARB=0 markette settle negatif | 1777471200: ARB=0, -$17.60 settle |

### 8.3. Zararlı 3 marketin analizi

| Epoch | Sorun | Çözüm önerisi |
|---|---|---|
| 1777471200 | ARB=0: hedge fill olmadı, UP unhedged kaldı, DOWN kazandı | `MAX_USD_PER_SIDE` düşür veya unhedged limit ekle |
| 1777471800 | 3 hedge fill, ama UP unhedged 40sh DOWN kaybetti | Hedge başarı oranı < %100 iken DCA limiti koy |
| 1777473900 | Hedge fill'ler var ama UP unhedged 40sh kaybetti | Settle riski için `MAX_UNHEDGED_USD` guard ekle |

---

## Bölüm 9 — Edge case'ler ve hata senaryoları

### 9.1. Piyasa açıldığında book boş

`up_best_bid = up_best_ask = 0.0` geliyorsa book henüz oluşmamış.

**Davranış**: `mid(side) == 0.0` → `pass_band_dca` false döner → DCA tetiklenmez. Book aktif olana kadar Idle kalır. Tipik olarak t=1-5s içinde book oluşur.

### 9.2. Her iki taraf da 0.50 üstünde / altında

Piyasa başında her iki taraf ~0.50 civarında: `mid(UP)=0.51, mid(DOWN)=0.51` → her iki taraf da "expensive" sayılır ve her iki tarafa da DCA emir gönderilir. Hedge görevi açılmaz çünkü hedge_side `pass_band_hedge` koşulunu geçemez (0.51 > 0.50).

**Davranış**: İki taraflı DCA; hedge olmaz; eğer pair dolduysa pair_cost = 1.02 gibi kârsız olabilir. Çözüm: `DCA_MIN_DROP`'u yüksek tutmak → başta az fill.

### 9.3. Pahalı taraf 0.90 üstüne çıktı

`mid(expensive_side) > BAND_HIGH (0.90)` → `pass_band_dca` false → DCA durur.

**Davranış**: Mevcut açık DCA emri iptal edilmez; fill olursa hedge_job oluşur. Yalnızca yeni DCA emri gönderilmez. Bu doğru davranış: zaten aldığımız share'ler $1'e gidiyor.

### 9.4. Ucuz taraf 0.10 altına düştü

`mid(cheap_side) < BAND_LOW (0.10)` → `pass_band_hedge` false → hedge emri gönderilmez; aktif hedge job'u iptal edilir.

**Davranış**: DCA fill'inin hedge'i yapılamaz; o fill unhedged kalır. Settlement'ta pahalı taraf kazanırsa ekstra kâr; kaybederse zarar.

### 9.5. Hedge fill olmadı (3 adım tükendi)

3 adım (bid-3t, bid-2t, bid-1t) denendikten sonra fill gelmedi.

**Davranış**: `job.done = true`, pozisyon unhedged kalır. Bir sonraki DCA fill için yeni hedge job açılır. Unhedged exposure `settle_pnl` hesabına girer.

### 9.6. Heartbeat kayboldu

Polymarket 5 saniye heartbeat gelmezse TÜM açık GTC emirleri iptal eder.

**Davranış**: 4s aralıklı heartbeat loop. Fail olursa: tüm pending emirleri sıfırla (`bucket.pending_price = None`), tüm hedge job'larının `order_px`'ini sıfırla. Bir sonraki tick'te emirler yeniden oluşturulur.

### 9.7. WS partial subscription failure

Polymarket CLOB WS bazen `match` event eksik gelebilir; fill'ler görünmez.

**Davranış**: 30s'de bir REST `/trades?market=condition_id` polling ile reconcile et. Lokal bucket ile API arasında fark varsa API doğrudur; bucket güncellenir.

### 9.8. Resolution belirsiz

Tick file'da son anlarda UP ve DOWN her ikisi de 0.5 civarında kalabilir.

**Davranış**: Polymarket on-chain oracle'ı kesinleşene kadar redeem çağırma. `condition.payoutNumerators[winner] = 1` görene kadar bekle. Strateji mekanik çalıştığından resolution belirsizliği PnL hesaplamasını etkilemez; yalnızca redeem zamanlamasını.

### 9.9. Polymarket fee etkisi

Her fill'de fee uygulanır: `fee = size × price × feeRate × (price × (1-price))^exponent`

Bu fee ile gerçek pair_cost, hesaplanan pair_cost'tan yüksektir. Garantili kâr marjı daralır.

```rust
fn fill_cost_with_fee(price: f64, size: f64, fee_rate: f64, exponent: f64) -> f64 {
    let fee = size * price * fee_rate * (price * (1.0 - price)).powf(exponent);
    size * price + fee
}
```

Fee dahil hedef: `pair_cost + estimated_fee_per_pair < 1.00` olmalı.

---

## Bölüm 10 — Replikasyon checklist'i

### 10.1. Veri toplama
- [ ] CLOB WS bağlantısı (`wss://ws-subscriptions-clob.polymarket.com/ws/market`) — `markets: [condition_id]` subscribe
- [ ] User WS (`/ws/user`) — MATCHED/CONFIRMED/FAILED event'leri
- [ ] Gamma API — event metadata (start_time, end_time, condition_id, token_ids)
- [ ] Local tick recorder: her saniye `BookSnapshot` kaydet (backtest için)

### 10.2. Emir altyapısı
- [ ] L1 Auth: EIP-712 ClobAuthDomain ile API key
- [ ] L2 Auth: HMAC-SHA256 her transactional request
- [ ] GTC order signing (EIP-712)
- [ ] Order tracking: place → MATCHED → CONFIRMED zinciri
- [ ] Heartbeat loop (4s interval)
- [ ] Open order cancellation API

### 10.3. Strateji çekirdeği
- [ ] `BookSnapshot` + `SideBucket` + `HedgeJob` struct'ları
- [ ] `pass_band_dca` ve `pass_band_hedge` guard fonksiyonları
- [ ] `on_poll_tick` — her 2s DCA emir kararı
- [ ] `on_dca_fill` — fill sonrası hedge job oluşturma
- [ ] `advance_hedge_job` — bid-3t → bid-2t → bid-1t adım makinesi
- [ ] `on_hedge_fill` — pair_cost hesaplama ve ArbLock kaydı
- [ ] `on_book_tick` — ana döngü
- [ ] `on_market_close` — settlement PnL hesaplama

### 10.4. Test / Backtest
- [ ] `arb_dca_v3.py` sonuçlarını Rust implementasyonu ile karşılaştır (16 market)
- [ ] Her market için guaranteed_pnl ve settle_pnl Python ile eşleşmeli
- [ ] Edge case'leri unit test'le (Bölüm 9)
- [ ] Fee dahil net PnL hesaplaması ekle

### 10.5. Production ön-kontroller
- [ ] Polymarket CLOB ping latency < 5ms
- [ ] Heartbeat fail injection testi — recovery doğrula
- [ ] WS drop simulation — reconnect ve bucket reconcile doğrula
- [ ] Daily loss limit + kill switch (örn. Telegram alert)
- [ ] `MAX_USD_PER_SIDE` live ortamda yarıya çek (önce $250)

### 10.6. Live rollout
- [ ] **Read-only mode**: 24 saat sadece "ne emir basardım" log'la, gerçek emir basma
- [ ] **Tek market, küçük size**: `SHARES=10, MAX_USD_PER_SIDE=$100` ile 4-8 saat test
- [ ] **Hedge başarı oranını** izle: `arb_count / dca_fill_count` > %60 olmalı
- [ ] **Unhedged exposure**: her kapanışta unhedged share × avg_price raporla
- [ ] **Scale up**: Bir hafta stable → `SHARES=40, MAX_USD_PER_SIDE=$500`

### 10.7. Sürekli iyileştirme
- [ ] `BAND_LOW` ve `BAND_HIGH` farklı değerlerle backtest (0.05–0.15 arası test)
- [ ] `HEDGE_STEP_S` optimizasyonu (3s, 6s, 10s karşılaştırması)
- [ ] Hedge başarısızlık loglarına bakarak `MAX_UNHEDGED_USD` guard ekle
- [ ] Daha fazla market verisiyle (30+) ROI ve Sharpe istatistiği çıkar

---

## Ek A — Açık sorular

1. **Fee etkisi ne kadar?** Backtestte fee hesaplanmadı. Polymarket fee formülü fiyat seviyesine göre değişiyor (en yüksek 0.50 civarında). Gerçek pair_cost'un hesaplanan değerden ~0.5-1% yüksek olması bekleniyor; ROI düşebilir.

2. **Hedge fill oranı neden < %100?** Bid-3tick emri her zaman fill olmaz — karşı tarafta satıcı yoksa emir kitapta bekler ve timeout olur. Çözüm: `HEDGE_STEP_S` düşürmek fill oranını artırır ama her adım için daha az bekleme süresi kalır.

3. **Her iki tarafın aynı anda pahalı / ucuz olduğu durum nasıl yönetilmeli?** Mevcut kodda her iki taraf da DCA alır, hedge açılmaz. Bu durumda pair_cost kontrolsüz artabilir. Çözüm: pair_cost > 1.00 olduğunda DCA otomatik durdurulmalı.

4. **`DCA_MIN_DROP` = 0.01 yeterli mi?** 0.01 = 1 tick = minimum artım. Bu çok hassas; her 1 tick düşüşte yeni emir açılabilir ve sermaye hızla tüketilir. `0.02` veya `0.03` daha güvenli bir default olabilir.

5. **Negatif pair_cost senaryosu var mı?** Teorik olarak `pair_cost = 0.50 + 0.50 = 1.00` en kötü dengeli senaryo. Ancak her iki tarafın farklı hızda hareket etmesi durumunda garantili kâr marjı değişir. Uzun vadeli istatistik için 100+ market gerekli.

---

## Ek B — Strateji karşılaştırması (v2 vs v3)

| Özellik | v2 (Ucuz taraf DCA) | v3 (Pahalı taraf DCA) |
|---|---|---|
| DCA tarafı | `mid < 0.50` (ucuz/kaybeden) | `mid > 0.50` (pahalı/kazanan) |
| Hedge tarafı | `mid > 0.50` (pahalı) | `mid < 0.50` (ucuz) |
| ROI (16 market) | +5.02% | **+6.22%** |
| Yatırım | $11,227 | **$1,416** |
| Net PnL | $563 | $88 |
| ARB/market ort. | 45 | **2.7** |
| Settlement kâr | nadir | var (unhedged kazananlarda) |
| Zararlı market | 3 | **3** |

v3'ün sermaye verimliliği çok daha yüksek: aynı ROI'yi 8x daha az sermayeyle üretiyor. Bunun nedeni: pahalı taraf fiyatı nadiren düşer → DCA az tetiklenir → düşük toplam maliyet.

---

## Ek C — Hızlı özet (TL;DR)

```
Strateji: DCA + Kademeli Hedge Arbitrajı

Her 2 saniyede:
  expensive_side (0.50 < mid < 0.90):
    bid düştüyse avg'dan 0.01+ → GTC @ bid-1tick
    fill → HEDGE_CASCADE başlat

  HEDGE_CASCADE (cheap_side, 0.10 < mid < 0.50):
    Adım 1: bid-3tick → 6s bekle → fill? → ARB kilidi
    Adım 2: bid-2tick → 6s bekle → fill? → ARB kilidi
    Adım 3: bid-1tick → 6s bekle → fill? → ARB kilidi
    Tükendi → vazgeç (pozisyon unhedged)

  ARB kilidi:
    pair_cost = dca_avg(expensive) + dca_avg(cheap)
    pair_cost < 1.00 → guaranteed_pnl = (1 - pair_cost) × hedged_shares

Settlement:
  Hedged share'ler: guaranteed_pnl (sonuçtan bağımsız)
  Unhedged share'ler: winner → ekstra kâr; loser → zarar

Parametreler (default):
  POLL_S=2s  DCA_MIN_DROP=0.01  SHARES=40
  MAX_USD/taraf=$500  HEDGE_STEP_S=6s  HEDGE_TICKS=[3,2,1]
  BAND=[0.10–0.90]

Backtest (16 market, bot14-ticks-20260429):
  Yatırım: $1,416  PnL: +$88.15  ROI: +6.22%  13W/3L
```

---

*Versiyon 3.0 — 2026-04-30*
*Kaynak kod: `scripts/arb_dca_v3.py`*
*Backtest verisi: `exports/bot14-ticks-20260429/` (16 market)*
