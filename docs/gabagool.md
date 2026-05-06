# Bilateral Accumulation Strategy — Sağlam Patternlar

> Gabagool stratejisinden çıkarılmış, herhangi bir async/trading/state-management projesinde kullanılabilir 10 pattern. Her pattern: **prensip → neden mantıklı → implementation → tuzaklar**.

---

## İçindekiler

1. [Fee-Aware Edge Threshold](#1-fee-aware-edge-threshold)
2. [Hedged Lock Condition (Min-of-Legs Rule)](#2-hedged-lock-condition)
3. [Pending/Real Quantity Ayrımı](#3-pendingreal-quantity-ayrımı)
4. [Improvement-Based Decision Logic](#4-improvement-based-decision-logic)
5. [Microstructure Filter Pipeline](#5-microstructure-filter-pipeline)
6. [TTL Cache for Expensive Computes](#6-ttl-cache-for-expensive-computes)
7. [Two-Loop Architecture (Hot + Maintenance)](#7-two-loop-architecture)
8. [Atomic Persistence](#8-atomic-persistence)
9. [Callback-Based Fill Management](#9-callback-based-fill-management)
10. [Engine Lifecycle & Status](#10-engine-lifecycle--status)
11. [Cross-Cutting Prensipler](#11-cross-cutting-prensipler)
12. [Implementation Sırası (Pratik Roadmap)](#12-implementation-sırası)

---

## 1. Fee-Aware Edge Threshold

### Prensip

Bir işlemin "garantili kâr" sayılabilmesi için **tüm işlem maliyetleri** teorik break-even'dan düşülmüş olmalı. Naif `cost < payout` kontrolü yetersizdir.

```
effective_payout  = nominal_payout × (1 − fee_rate) − fixed_costs
edge_threshold    = effective_payout − safety_margin
trade_condition   = total_cost < edge_threshold
```

### Neden mantıklı

Fee'siz inequality maliyetin altındaki ince marjı (1–3%) tamamen yer. Polymarket örneğinde: nominal $1.00, fee %2 → effective $0.98. Eğer threshold'u $0.99'a koyarsan, gerçek profit %0–1, fakat slippage + spread bunu kolayca eksiye çevirir. Safety margin (%0.5–1) bu volatiliteyi absorbe eder.

### Implementation

```rust
pub struct EdgeConfig {
    pub nominal_payout: f64,      // 1.0 (Polymarket binary)
    pub fee_rate: f64,            // 0.02
    pub fixed_cost_per_trade: f64,// gas, bridge, vs. (0.0 if irrelevant)
    pub safety_margin: f64,       // 0.005
}

impl EdgeConfig {
    pub fn threshold(&self) -> f64 {
        self.nominal_payout * (1.0 - self.fee_rate)
            - self.fixed_cost_per_trade
            - self.safety_margin
    }
}

// Polymarket: 1.0 * 0.98 - 0 - 0.005 = 0.975
```

### Genelleme

Aynı formül uygulanabilir alanlar:

- **Cash-and-carry futures arb**: `spot + carry_cost + funding < futures × (1 − exec_fee)`
- **DEX triangular arb**: her hop fee'sini çıkar
- **Cross-exchange spread**: fee_a + fee_b + withdrawal_fee + slippage_buffer

### Tuzaklar

- **Sabit fee varsayımı yanlış olabilir.** Polymarket'in fee formülü `C × p × feeRate × (p × (1−p))^exponent` — yani p=0.5'te fee yüksek, edge'lerde düşük. Threshold'u dinamik tut, sabit `0.02` kullanma.
- **Slippage safety_margin'e dahil değil.** Market depth düşükse `safety_margin` taker impact'ini de absorbe etmeli (örn. 0.5% margin, 0.3% slippage tahmini → toplam 0.8%).
- **Birikim sırasında threshold değişebilir.** İlk emirde geçerli olan eşik, ikinci emirde fee yapısı değiştiyse geçersiz.

---

## 2. Hedged Lock Condition

### Prensip

İki bacaklı (binary, long/short, A/B) bir pozisyonun "kârı locked" sayılabilmesi için **iki koşul birden** sağlanmalı:

```
Koşul 1: avg_price_a + avg_price_b < threshold
Koşul 2: min(qty_a, qty_b) > total_cost   ← genellikle atlanır, kritik
```

### Neden mantıklı

Sadece avg fiyat üzerinden lock kararı vermek yanıltıcıdır. Asimetrik miktar varsa, hedge edilmeyen bacak kâr garantisini bozar.

**Somut örnek:**

| Bacak | Qty | Avg Price | Cost |
|-------|-----|-----------|------|
| YES   | 100 | $0.40     | $40  |
| NO    | 50  | $0.50     | $25  |

- `avg_yes + avg_no = 0.90 < 0.975` → Koşul 1 geçti, **görünüşte kârlı**.
- Hedged qty = `min(100, 50) = 50`. Total cost = `$65`.
- `50 > 65` ? **HAYIR** → Lock ETME.

Senaryo analizi:
- YES kazanırsa: 100 × $1 = $100 gelir, $65 maliyet → +$35 kâr.
- NO kazanırsa: 50 × $1 = $50 gelir, $65 maliyet → **−$15 zarar**.

Yani "garanti kâr" değil, sadece bir tarafı kazanmaya bahis. Min-of-legs koşulu bu durumu yakalar: hedge edilmeyen 50 YES, asimetrik bahis demek.

### Implementation

```rust
#[derive(Debug)]
pub struct Position {
    pub qty_a: f64, pub cost_a: f64,
    pub qty_b: f64, pub cost_b: f64,
}

impl Position {
    pub fn avg_a(&self) -> f64 {
        if self.qty_a > 0.0 { self.cost_a / self.qty_a } else { 0.0 }
    }
    pub fn avg_b(&self) -> f64 {
        if self.qty_b > 0.0 { self.cost_b / self.qty_b } else { 0.0 }
    }
    pub fn hedged_qty(&self) -> f64 {
        self.qty_a.min(self.qty_b)
    }
    pub fn total_cost(&self) -> f64 {
        self.cost_a + self.cost_b
    }
    pub fn pair_cost(&self) -> f64 {
        if self.qty_a > 0.0 && self.qty_b > 0.0 {
            self.avg_a() + self.avg_b()
        } else {
            f64::INFINITY  // tek bacak → lock imkansız
        }
    }
    pub fn is_locked(&self, threshold: f64) -> bool {
        self.pair_cost() < threshold
            && self.hedged_qty() > self.total_cost()
    }
}
```

### Genelleme

- **Pairs trading**: long A + short B; hedge ratio'ya bağlı kâr garantisi
- **Cross-margin futures**: long perp + short perp, funding rate convergence
- **Liquidity provisioning**: IL hesaplarında, asimetrik withdrawal sonrası kalan exposure

### Tuzaklar

- **`pair_cost` formülü tek bacak yokken `INFINITY` döndürmeli**, `0` veya `2.0` değil — yanlış lock'a yol açar.
- **Hedged qty = `min` ama hedged_value = `min × resolution_payout`**. Eğer payoutlar farklıysa (örn. binary yerine spread bet), `min` yerine value bazlı min hesapla.
- **Locked ≠ realized.** Settlement öncesi market kapanırsa veya delisting olursa, locked profit kâğıt üzerindedir. "Locked" durumunu da kill switch'e dahil et.

---

## 3. Pending/Real Quantity Ayrımı

### Prensip

Async order management'ta **üç** ayrı state tut:

```
real_qty     : exchange'de confirmed, on-chain settled
pending_qty  : queue'da veya exchange'de open ama henüz fill olmamış
visible_qty  : real + pending (sizing/risk hesabı için)
```

Critical decision'lar (lock check, risk limit, sizing) **sadece `real_qty`** üstünden yapılır.

### Neden mantıklı

İki yaygın race condition'u önler:

1. **Double counting**: Emir gönderdin, fill bekleniyor. Aynı tick'te yine "yetersiz pozisyon" görüp tekrar emir gönderirsen iki kat alırsın. Pending'i visible'a dahil ederek kendini sayarsın.
2. **Premature lock**: Pending'i real saysan, "lock" kondisyonuna ulaştığını sanıp yeni emir göndermeyi keser, ama emirler cancel olursa lock asla gerçekleşmez.

### State transitions

```
                  submit_order
                       │
                       ▼
              pending += qty, cost
                       │
             ┌─────────┴─────────┐
             ▼                   ▼
         on_fill              on_order_end
        (filled_qty)         (remaining_qty)
             │                   │
             ▼                   ▼
   real    += filled         pending -= remaining
   pending -= filled              (cancel/expire path)
```

### Implementation

```rust
#[derive(Default, Debug)]
pub struct LegState {
    pub real_qty:     f64,
    pub real_cost:    f64,
    pub pending_qty:  f64,
    pub pending_cost: f64,
}

impl LegState {
    pub fn submit(&mut self, qty: f64, price: f64) {
        self.pending_qty  += qty;
        self.pending_cost += qty * price;
    }

    pub fn on_fill(&mut self, filled_qty: f64, fill_price: f64) {
        // Real'e gerçek fill price üzerinden ekle (pending'deki tahmin değil)
        self.real_qty  += filled_qty;
        self.real_cost += filled_qty * fill_price;

        // Pending'i orantılı azalt — taşma ihtimaline karşı clamp
        let pending_reduction_ratio =
            (filled_qty / self.pending_qty.max(filled_qty)).min(1.0);
        self.pending_qty  = (self.pending_qty - filled_qty).max(0.0);
        self.pending_cost = self.pending_cost
            * (1.0 - pending_reduction_ratio).max(0.0);
    }

    pub fn on_order_end(&mut self, remaining_qty: f64, remaining_cost: f64) {
        self.pending_qty  = (self.pending_qty - remaining_qty).max(0.0);
        self.pending_cost = (self.pending_cost - remaining_cost).max(0.0);
    }

    /// Sizing kararları için.
    pub fn visible_qty(&self) -> f64 { self.real_qty + self.pending_qty }
    /// Lock kararları için — sadece confirmed.
    pub fn confirmed_qty(&self) -> f64 { self.real_qty }
}
```

### Genelleme

- **Banking/payments**: posted vs available balance — kart hold edildi ama settle olmadı
- **Inventory systems**: reserved (cart) vs available (warehouse)
- **Distributed lock**: acquired vs in-flight

### Tuzaklar

- **`max(0.0)` clamp şart.** Floating-point error veya geç gelen callback `pending`'i negatife düşürebilir.
- **Pending cost hesabı**: emir limit fiyatından gönderilir, fill gerçek fiyattan olur (özellikle FAK/Market'te). `on_fill`'de gerçek fiyatı kullan.
- **Idempotency**: aynı `on_fill` callback'i iki kez gelirse double count. `order_id`'yi state'te tut ve duplicate'i filtrele.
- **Ghost pending**: order submit oldu ama exchange'den ne fill ne cancel geldi. Maintenance loop'unda timeout-based cleanup gerekli (Pattern 7).

---

## 4. Improvement-Based Decision Logic

### Prensip

"Bu ölçüt iyi mi?" yerine **"Hangi seçenek bu ölçütü en çok iyileştiriyor?"** sor. Greedy descent.

```
current_metric = compute(current_state)
for each option in options:
    simulated_state = apply(option, current_state)
    new_metric      = compute(simulated_state)
    improvement     = current_metric − new_metric
    if improvement > min_improvement:
        candidates.push((option, improvement))
choose option with max improvement
```

### Neden mantıklı

Statik threshold ("pair_cost < 0.95 ise al") opsiyonlar arasında ayrım yapamaz. Improvement-based yaklaşım:

1. Her tick'te **en yüksek katkıyı** sağlayan aksiyonu seçer.
2. **Local minimum'a** doğru iter — her adım ölçütü daha iyiye götürür.
3. **Gürültü trade'lerini** engeller (`min_improvement` eşiği).

### `min_improvement` nasıl seçilir?

```
min_improvement ≥ tick_size + slippage_estimate + (fee_per_trade / position_size)
```

**Polymarket örneği:**
- tick = $0.001
- slippage ~ $0.002 (1–2 tick taker impact)
- fee/trade @ $25 = $0.50 → $0.50 / 25 shares = $0.020 per share
- Toplam: $0.001 + $0.002 + $0.020 = **$0.023**

Repo'daki `0.001` değeri sadece tick'i karşılıyor, fee ve slippage'ı ignore ediyor → her trade negative EV. **Senin için doğru değer ~0.025–0.035 aralığında**.

### Implementation

```rust
pub enum Action { BuyA, BuyB, Skip }

pub struct Decision {
    pub action:      Action,
    pub size_usd:    f64,
    pub improvement: f64,
}

pub fn decide(
    pos:      &Position,
    price_a:  f64, price_b: f64,
    size_usd: f64,
    cfg:      &EdgeConfig,
    min_improvement: f64,
) -> Decision {
    let current = pos.pair_cost();

    // Senaryo A: Buy A
    let qty_a_new  = pos.qty_a + size_usd / price_a;
    let cost_a_new = pos.cost_a + size_usd;
    let avg_a_new  = cost_a_new / qty_a_new;
    let pair_if_a  = if pos.qty_b > 0.0 { avg_a_new + pos.avg_b() }
                     else { f64::INFINITY };

    // Senaryo B: Buy B
    let qty_b_new  = pos.qty_b + size_usd / price_b;
    let cost_b_new = pos.cost_b + size_usd;
    let avg_b_new  = cost_b_new / qty_b_new;
    let pair_if_b  = if pos.qty_a > 0.0 { pos.avg_a() + avg_b_new }
                     else { f64::INFINITY };

    let imp_a = current - pair_if_a;
    let imp_b = current - pair_if_b;

    let a_ok = imp_a > min_improvement && pair_if_a < cfg.threshold();
    let b_ok = imp_b > min_improvement && pair_if_b < cfg.threshold();

    match (a_ok, b_ok) {
        (true,  true)  => if imp_a > imp_b { Decision::buy_a(size_usd, imp_a) }
                          else             { Decision::buy_b(size_usd, imp_b) },
        (true,  false) => Decision::buy_a(size_usd, imp_a),
        (false, true)  => Decision::buy_b(size_usd, imp_b),
        _              => Decision::skip(),
    }
}
```

### Genelleme

- **Portfolio rebalancing**: hangi asset rebalance, en çok target weight'e yaklaştırır?
- **Cache eviction**: hangi key'i evict, hit rate'i en az düşürür?
- **Route selection** (DEX, network): hangi rota effective price'ı en çok iyileştirir?

### Tuzaklar

- **Optimistic simulation**: bu kod anlık spot price üzerinden simüle eder. Gerçek fill slippage'lı olur. Simülasyona realistic fill price kullan: `expected_fill = ask_price + slippage_estimate(size, depth)`.
- **Tek-step lookahead yetersiz**: bu greedy local optimum, global optimum değil. Bazen "şimdi A al, sonra B al" iki-adım plan, "şimdi B al, sonra A" planından daha iyidir. Multi-step lookahead için MCTS / dynamic programming gerekir.
- **Improvement = 0 ama threshold üstünde** → trade etme. Lock'a yaklaşmıyor olsan da threshold'a uzaksan beklemenin sense'i yok.

---

## 5. Microstructure Filter Pipeline

### Prensip

Filtreler **sinyal üretmez, sinyali kapatır**. Pipeline:

```
candidates = generate_candidates(state, market)
for filter in filters:
    candidates = filter.apply(candidates, market)
if candidates.empty(): no_action()
else: pick_best(candidates)
```

Her filter monotonicity şartı: **never adds, only removes**.

### Neden mantıklı

1. **Decoupling**: signal generation logic'i filter logic'inden bağımsız. Yeni filter eklemek = pipeline'a satır ekle, generation'a dokunma.
2. **Composability**: filter'ları farklı kombinasyonlarda dene (backtest'te kapatılabilir).
3. **Audit trail**: hangi filter neyi blokladı log'lanabilir → sonradan tuning kolay.

### Tipik filtreler

#### A) Order Book Imbalance (OBI)

```
OBI = (bid_volume − ask_volume) / (bid_volume + ask_volume)
   ∈ [−1, +1]

bid pressure  (OBI > +0.3)  → satış tarafına geçme yasak
ask pressure  (OBI < −0.3)  → alış tarafına geçme yasak
```

**Mantığı**: dominant taraf yüksek olasılıkla fiyatı kendi yönüne çekecek. Karşı yönde pozisyon = anti-edge.

#### B) RSI Trend Filter

```
RSI > 70  → overbought, contra-trend long yasak
RSI < 30  → oversold,  contra-trend short yasak
```

**Mantığı**: extreme momentum'da mean-reversion pahalı; "düşen bıçak" / "patlayan tepe" yakalama.

#### C) Volatility Filter

```
realized_vol(window) > vol_threshold → tüm yeni emirler bloklu
```

Kaos rejiminde edge dağılır, sadece var olan pozisyonu yönet.

#### D) Time-of-Day / Lifecycle Filter

```
window_age < min_age           → çok erken, beklenmedik price action riski
window_age > max_age           → çok geç, settlement riski
```

### Implementation

```rust
pub trait Filter {
    fn apply(&self, candidates: &mut CandidateSet, ctx: &MarketContext);
    fn name(&self) -> &'static str;
}

pub struct ObiFilter { pub threshold: f64 }
impl Filter for ObiFilter {
    fn apply(&self, c: &mut CandidateSet, ctx: &MarketContext) {
        if ctx.obi_a > self.threshold { c.block(Action::BuyB, "OBI_A_pressure"); }
        if ctx.obi_b > self.threshold { c.block(Action::BuyA, "OBI_B_pressure"); }
    }
    fn name(&self) -> &'static str { "OBI" }
}

pub struct RsiFilter {
    pub overbought: f64,  // 70.0
    pub oversold:   f64,  // 30.0
}
impl Filter for RsiFilter {
    fn apply(&self, c: &mut CandidateSet, ctx: &MarketContext) {
        if let Some(rsi) = ctx.rsi {
            if rsi > self.overbought { c.block(Action::BuyB, "RSI_overbought"); }
            if rsi < self.oversold   { c.block(Action::BuyA, "RSI_oversold");   }
        }
    }
    fn name(&self) -> &'static str { "RSI" }
}

// Pipeline:
let mut candidates = CandidateSet::all();
for f in &self.filters {
    f.apply(&mut candidates, &ctx);
    if candidates.is_empty() { return Action::Skip; }
}
candidates.pick_best()
```

### Genelleme

- **Spam/abuse detection**: rule chains
- **A/B test eligibility**: cohort filters
- **Risk limits**: position cap, daily loss cap, exposure cap → hepsi filter olarak modellenir

### Tuzaklar

- **Filter sırası matter**. Pahalı filter'ları sona koy (önce ucuz blokla).
- **`None` veriyle filter ne yapacak?**: yetersiz history'de RSI hesaplanamaz. Default davranış = filter pas geçer (no-op), block etmez. Aksi halde bot soğuk başlangıçta hiçbir şey yapmaz.
- **Filter'lar correlated olabilir**: OBI ve RSI aynı dump'ı ölçüyor olabilir. Backtest'te filter contribution'ı izole et.
- **Override mekanizması olmamalı**. Kodda gördük: `Oracle BUY → filter bypass`. Bu pattern'i kırar — bir filter'ı override eden başka bir filter, audit trail'i bozar. Override yerine "high-priority candidate generator" olarak modelle.

---

## 6. TTL Cache for Expensive Computes

### Prensip

Sık çağrılan + pahalı computation'lar için time-to-live cache:

```
get(key):
    if key in cache and (now - cache[key].timestamp) < ttl:
        return cache[key].value
    value = compute(key)
    cache[key] = (now, value)
    return value
```

### Neden mantıklı

- **Hot loop'ta her tick RSI hesaplama** = 100 fiyat üstünde 14-period rolling = ~100μs × N market = ms'lere çıkar
- **TTL'i sinyalin yarı-ömrüne göre seç**: orderbook indicator → 1–5s, trend indicator → 30–60s

### Implementation

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct TtlCache<K, V> {
    map: HashMap<K, (Instant, V)>,
    ttl: Duration,
}

impl<K: std::hash::Hash + Eq, V: Clone> TtlCache<K, V> {
    pub fn new(ttl_secs: f64) -> Self {
        Self { map: HashMap::new(), ttl: Duration::from_secs_f64(ttl_secs) }
    }

    pub fn get_or_compute<F: FnOnce() -> Option<V>>(&mut self, key: K, f: F) -> Option<V> {
        if let Some((ts, v)) = self.map.get(&key) {
            if ts.elapsed() < self.ttl { return Some(v.clone()); }
        }
        let v = f()?;
        self.map.insert(key, (Instant::now(), v.clone()));
        Some(v)
    }

    pub fn invalidate(&mut self, key: &K) { self.map.remove(key); }

    /// Periodically called from maintenance loop.
    pub fn evict_expired(&mut self) {
        self.map.retain(|_, (ts, _)| ts.elapsed() < self.ttl * 2);
    }
}
```

### Tuzaklar

- **Stale data riski**: TTL çok uzunsa sinyal güncelliğini kaybeder. RSI 14-period @ 1s tick için 5s TTL = ~%35 stale bias.
- **Memory leak**: `evict_expired`'i maintenance loop'tan çağır, yoksa sonsuz market_id birikir.
- **Cache invalidation on event**: yeni fill geldiyse RSI cache'ini invalidate et — fiyat history değişmiştir.

---

## 7. Two-Loop Architecture

### Prensip

İki ayrı loop:

```
Hot Loop  (every tick, ~10–100ms)
    └─ market data ingestion
    └─ decision logic
    └─ order submission
    └─ NO disk I/O, NO blocking calls

Maintenance Loop  (every 30–60s)
    └─ persistence flush
    └─ stale order cleanup
    └─ position reconciliation
    └─ settlement / redeem
    └─ cache eviction
    └─ metric reporting
```

### Neden mantıklı

Hot path'te ms önemli. Disk I/O (1–10ms), HTTP polling (50–500ms), database query (1–100ms) hot loop'u öldürür. Bunları async maintenance loop'a aktar.

### Implementation

```rust
pub struct Engine {
    // ... state ...
    maintenance_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Engine {
    pub async fn start(&mut self) {
        // Hot loop: WebSocket subscription, event-driven
        self.subscribe_market_ws().await;

        // Maintenance loop: fixed interval
        let state = self.state.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                if let Err(e) = run_maintenance(&state).await {
                    tracing::warn!("maintenance error: {}", e);
                }
            }
        });
        self.maintenance_handle = Some(handle);
    }
}

async fn run_maintenance(state: &SharedState) -> Result<()> {
    persistence_flush(state).await?;
    cleanup_stale_orders(state).await?;
    reconcile_positions(state).await?;
    redeem_settled(state).await?;
    evict_caches(state).await?;
    Ok(())
}
```

### Maintenance loop'un yapması gereken kontroller

- [ ] **Stale pending orders**: 5 dk önce submit edilmiş ama hala pending olan → exchange'den status çek, gerekirse pending'i temizle
- [ ] **Reconciliation**: real_qty'leri exchange API'den doğrula (drift detection)
- [ ] **Settlement check**: resolved marketleri redeem et
- [ ] **Persistence**: critical state diske flush
- [ ] **Heartbeat**: bazı API'ler 5–30s heartbeat ister, yoksa orderları cancel'lar
- [ ] **Cache eviction**: expired entries
- [ ] **Metric snapshot**: P&L, fill rate, latency p99

### Tuzaklar

- **Hot loop'ta async lock alıp uzun tutma.** Maintenance loop aynı lock'u beklerse heartbeat kaçar.
- **Reconciliation aggressiveness**: drift varsa ne yap? Repo'daki kod hemen MARKET SELL ile dengeliyor — bu strateji'yle çelişiyor (Pattern 4 improvement-based decision'ı sabote ediyor). Doğrusu: drift'i logla, kullanıcıya alert at, otomatik action almaktansa manual review iste.
- **Maintenance interval'ın blocking olmamasına dikkat**: `tokio::time::interval` ile sabit interval kullan, `sleep` chain'i değil — drift birikir.

---

## 8. Atomic Persistence

### Prensip

Critical state diske yazılırken:

1. **Geçici dosyaya yaz** (`state.json.tmp`)
2. **`fsync`** (POSIX'te dosya descriptor'ı disk'e zorla)
3. **Atomik rename** (`state.json.tmp` → `state.json`)
4. Crash'lar arası en kötü ihtimal: önceki valid state, hiçbir zaman yarı-yazılmış state değil

### Neden mantıklı

Direct write sırasında power loss → JSON yarısı yazılı → boot'ta corrupted → state kaybı.

POSIX'te rename atomik garanti. Windows'ta `MoveFileEx` ile `MOVEFILE_REPLACE_EXISTING` benzer.

### Repo'daki naif yaklaşım

```python
with open(self._persistence_path, "w") as f:
    json.dump(data, f, indent=2)   # ← crash here = corrupted file
```

### Doğru implementation

```rust
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub fn atomic_write_json<P: AsRef<Path>, T: serde::Serialize>(
    path: P, data: &T,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let tmp_path = path.with_extension("json.tmp");

    let mut tmp = File::create(&tmp_path)?;
    let json = serde_json::to_vec_pretty(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    tmp.write_all(&json)?;
    tmp.sync_all()?;          // fsync — disk'e zorla
    drop(tmp);                // close

    fs::rename(&tmp_path, path)?;  // atomic on POSIX
    Ok(())
}

pub fn load_json_or_default<P: AsRef<Path>, T: serde::de::DeserializeOwned + Default>(
    path: P,
) -> T {
    match fs::read_to_string(&path) {
        Ok(s)  => serde_json::from_str(&s).unwrap_or_else(|e| {
            tracing::warn!("corrupted state file: {}, using default", e);
            T::default()
        }),
        Err(_) => T::default(),
    }
}
```

### Save trigger stratejisi

İki yaklaşım:

| Strateji | Pro | Con | Ne zaman |
|----------|-----|-----|----------|
| **Every state change** | Veri kaybı yok | I/O overhead | Az frekanslı state changes (<10/s) |
| **Periodic (1–10s)** | Düşük I/O | 1–10s veri kaybı riski | Yüksek frekanslı state |

Trading'te genelde "**every fill, every lock change**" yeterli — 1–10/s.

### Tuzaklar

- **fsync'i unutma**. Linux page cache rename'den önce yazılı sayar ama crash'ta veri yine kayıp.
- **Tmp dosya cleanup**: önceki crash'tan kalan `.tmp` dosyaları boot'ta sil.
- **Versioning**: schema değişince eski JSON'u parse edemezsin. `version: u32` field ekle, migration logic yaz.
- **Concurrent write**: aynı dosyaya iki process yazıyorsa race condition. Process-level lock (`flock`) ekle.

---

## 9. Callback-Based Fill Management

### Prensip

Strategy engine **emir durumlarını polling yapmaz**. Order executor fill/cancel olaylarında callback'leri tetikler.

```
Strategy:                    Executor:
  submit_order()  ─────────►  send to exchange
                              listen WebSocket / poll
                              on order event:
  ◄─── on_fill() ────────────  detect MATCHED
  ◄─── on_order_end() ───────  detect CANCELED/EXPIRED
```

### Neden mantıklı

1. **Decoupling**: strategy logic, exchange transport detaylarını bilmek zorunda değil.
2. **Latency**: WebSocket push polling'den 100–1000x hızlı.
3. **Backpressure**: birden fazla strategy aynı executor'ı paylaşabilir; her biri ilgilendiği event'lere subscribe olur.

### Implementation

```rust
pub trait FillHandler: Send + Sync {
    fn on_fill(&self, market_id: &str, order_id: &str,
               side: Side, filled_qty: f64, fill_price: f64);
    fn on_order_end(&self, market_id: &str, order_id: &str,
                    side: Side, remaining_qty: f64);
}

pub struct Executor {
    handlers: HashMap<String, Arc<dyn FillHandler>>, // market_id -> handler
    // ...
}

impl Executor {
    pub fn register(&mut self, market_id: &str, handler: Arc<dyn FillHandler>) {
        self.handlers.insert(market_id.to_string(), handler);
    }

    async fn process_ws_event(&self, event: WsEvent) {
        match event {
            WsEvent::Matched { market_id, order_id, qty, price, side } => {
                if let Some(h) = self.handlers.get(&market_id) {
                    h.on_fill(&market_id, &order_id, side, qty, price);
                }
            }
            WsEvent::Canceled { market_id, order_id, remaining, side } => {
                if let Some(h) = self.handlers.get(&market_id) {
                    h.on_order_end(&market_id, &order_id, side, remaining);
                }
            }
        }
    }
}
```

### Idempotency

Aynı `order_id` için iki kez `on_fill` gelebilir (network retry). Handler:

```rust
struct StrategyHandler {
    processed_events: Mutex<HashSet<String>>, // order_id+seq
    // ...
}

impl FillHandler for StrategyHandler {
    fn on_fill(&self, _: &str, order_id: &str, ..., filled_qty: f64, ...) {
        let event_key = format!("{}:fill", order_id);
        let mut seen = self.processed_events.lock().unwrap();
        if !seen.insert(event_key) { return; }  // duplicate
        drop(seen);
        // ... actual processing
    }
}
```

### Tuzaklar

- **Callback re-entrancy**: handler içinde başka callback tetikleyen action yapma → deadlock.
- **Lost events**: WebSocket disconnect olduysa kaçan event'leri REST API'den fetch et. Reconnect'te `since=last_seq` ile gap doldur.
- **Order vs market mapping**: bazı API'ler order_id'yi market_id olmadan yollar. Local order book tut: `order_id → market_id`.
- **Repo'daki gibi gerçek fiyatı ignore etme**: `on_fill` callback'inde gerçek fill price'ı kullan, submit price'ı değil.

---

## 10. Engine Lifecycle & Status

### Prensip

Bir long-running stateful engine için açık state machine:

```
       start()
   ┌───────────►  Stopped
   │              │  start()
   │              ▼
   │           Running ◄────┐
   │           │            │ resume()
   │   stop()  │  pause()   │
   │           ▼            │
   │           Paused ──────┘
   │           │  stop()
   │           ▼
   └────────  Stopped
```

### Neden mantıklı

- **Graceful shutdown**: in-flight emir varken kapatma → orphan order. `stop()` → cancel all pending → save state → exit.
- **Pause vs stop**: `pause()` state'i korur, callback'leri unregister etmez. `stop()` her şeyi temizler.
- **Resilience**: crash recovery'de durum bilinir (Stopped'ta başla, persistence yükle, sonra Running'e geç).

### Implementation

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineStatus { Stopped, Running, Paused }

pub struct Engine {
    status: Arc<Mutex<EngineStatus>>,
    state:  Arc<RwLock<State>>,
    handles: Vec<JoinHandle<()>>,
}

impl Engine {
    pub async fn start(&mut self) -> Result<()> {
        let mut status = self.status.lock().await;
        if *status != EngineStatus::Stopped { return Err(EngineError::AlreadyRunning); }

        // 1. Load persistence
        load_state(&self.state).await?;

        // 2. Register callbacks
        self.executor.register(&self.handler);

        // 3. Spawn loops
        self.handles.push(tokio::spawn(maintenance_loop(self.state.clone())));

        *status = EngineStatus::Running;
        Ok(())
    }

    pub async fn pause(&self) -> Result<()> {
        let mut s = self.status.lock().await;
        if *s == EngineStatus::Running { *s = EngineStatus::Paused; }
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        // 1. Status flip — yeni emir alma
        { *self.status.lock().await = EngineStatus::Stopped; }

        // 2. Cancel maintenance/spawned tasks
        for h in self.handles.drain(..) { h.abort(); }

        // 3. Cancel in-flight orders
        self.executor.cancel_all().await?;

        // 4. Final persistence flush
        save_state(&self.state).await?;

        // 5. Unregister callbacks
        self.executor.unregister(&self.handler);

        Ok(())
    }
}

// Hot loop'ta status check:
async fn on_market_tick(&self, ...) {
    if *self.status.lock().await != EngineStatus::Running { return; }
    // ... decision logic
}
```

### Tuzaklar

- **Mutex/RwLock kullanımı**: status için Mutex (`AtomicU8` ile lock-free yapılabilir). State için RwLock — okuma çok daha sık.
- **Shutdown deadline**: `stop()` infinite block edebilir. Timeout koy, expire olursa force shutdown + warning.
- **Restart semantik**: stop sonrası start = fresh start mı, recovery mi? Net olsun, dokümante et.

---

## 11. Cross-Cutting Prensipler

### A. State'e tek source of truth

Birden fazla yerde "qty" tutma. Tek bir struct (`Position`), her okuma oradan, her yazma lock arkasında.

### B. Number safety

```rust
// Floating point comparison her zaman epsilon'la
fn approx_eq(a: f64, b: f64) -> bool { (a - b).abs() < 1e-9 }

// Money/qty için decimal kullan, mümkünse:
use rust_decimal::Decimal;

// Division by zero guard ZORUNLU
let avg = if qty > 0.0 { cost / qty } else { 0.0 };
```

### C. Logging structure

```rust
tracing::info!(
    market_id = %market_id,
    action = ?decision.action,
    pair_cost = pos.pair_cost(),
    improvement = decision.improvement,
    "trade decision"
);
```

Structured logging post-mortem analizi 100x kolaylaştırır.

### D. Configuration hot-reload

Trading params'ı runtime'da değiştirilebilir tut (Pattern 7'deki maintenance loop tarayabilir). Restart-required config = kullanıcı için friction = yanlış config'le çalışan bot.

### E. Dry-run mode

Engine'in `simulate: bool` flag'i olsun. `simulate=true` → emir gönderme, sadece logla. Production deploy'dan önce 24h dry-run zorunlu.

### F. Kill switch (panik düğmesi)

Tek bir flag (`global_halt`) tüm engine'leri durdurur. UI'dan veya environment variable'dan. **Liquidation'ı stub olarak bırakma** — repo'daki kritik bug.

### G. Idempotency her yerde

Network retry, callback duplicate, restart sonrası replay. Her event handler ve state mutation idempotent olmalı.

### H. Test seviyesi

| Tier | Coverage | Örnek |
|------|----------|-------|
| Unit | %80+ | `pair_cost`, `is_locked`, decision matrix |
| Integration | Critical paths | Fake exchange + 100 random fill sequence |
| Property-based | Invariants | proptest: hiçbir state'te `pending_qty < 0` |
| Backtest | Historical replay | Geçmiş 30 gün, deterministic |
| Dry-run | Live data | 24h paper trading |

---

## 12. Implementation Sırası

Sıfırdan benzer bir engine kurarken, **bu sırayla** build et:

1. **Pattern 2** (Position struct + lock condition) — core data model. Her şey bunun üstüne kurulu.
2. **Pattern 1** (Edge threshold) — config + math. Test edilebilir, dependency yok.
3. **Pattern 3** (Pending/real state) — order lifecycle'in temeli.
4. **Pattern 8** (Atomic persistence) — debug için kritik, baştan al.
5. **Pattern 10** (Lifecycle & status) — start/stop iskeleti.
6. **Pattern 9** (Callback-based fills) — executor abstraction.
7. **Pattern 4** (Improvement decision) — strategy core.
8. **Pattern 7** (Two-loop) — production şartı.
9. **Pattern 5** (Filter pipeline) — extension point. Önce 1 filter (volatility veya OBI), sonra ekle.
10. **Pattern 6** (TTL cache) — performance optimization, son.

Her pattern bağımsız test edilebilir. CI'da each pattern için ayrı test suite.

---

## Hızlı Checklist

Yeni engine deployment öncesi:

- [ ] Edge threshold dynamic fee model'i kullanıyor (Pattern 1)
- [ ] Lock kondisyonu hem pair_cost hem hedged_qty kontrolü içeriyor (Pattern 2)
- [ ] Pending/real ayrı, lock sadece real üstünden (Pattern 3)
- [ ] `min_improvement > tick + slippage + fee/size` (Pattern 4)
- [ ] Filter pipeline override yapısı içermiyor (Pattern 5)
- [ ] TTL cache'leri maintenance loop'ta evict ediliyor (Pattern 6)
- [ ] Hot loop'ta disk I/O yok (Pattern 7)
- [ ] Persistence atomic (tmp + rename + fsync) (Pattern 8)
- [ ] Fill callback'leri idempotent (Pattern 9)
- [ ] Liquidation kodu **stub değil**, gerçekten emir gönderiyor (Pattern 10)
- [ ] Dry-run mode mevcut, 24h paper trading yapılmış
- [ ] Property-based test invariants tanımlı (`pending_qty ≥ 0`, `pair_cost ≥ 0`)
- [ ] Structured logging her decision için
- [ ] Kill switch (`global_halt`) UI ve env var ile erişilebilir

---

*Bu dokümanın amacı: Gabagool stratejisinin **konsept olarak doğru** parçalarını projeden bağımsız pattern'lar olarak yeniden kullanılabilir yapmak. Stratejinin spesifik magic number'ları (0.975, 15dk, 70/30 RSI vb.) sadece Polymarket UP/DOWN context'i için referans — kendi domain'inizde fee/spread/volatility yapınıza göre yeniden tune edin.*