# Polymarket Sinyal Hesaplama: 2026 Güncel Referans Dokümanı

> **Hazırlayan**: BAITER (Binance-Aware Instant Taker & Entry Responder) Phase 1 + Phase 2 araştırması  
> **Tarih**: Mayıs 2026  
> **Kapsam**: Tüm internetteki güncel açık kaynak Polymarket bot implementasyonları, akademik paper'lar, Polymarket resmi dokümanları, post-Feb 2026 kural değişiklikleri, mikroyapı sinyalleri ve CLOB V2 migration  
> **Hedef**: BTC, ETH, SOL, XRP × 5m / 15m / 1h Up-Down piyasaları için sinyal üretim kataloğu

---

## 0. Yönetici Özeti

Polymarket'te kârlı bir botun "tek bir sinyal" değil, **birden fazla bağımsız sinyal kaynağının fusion'ı** olduğu açıkça görülüyor. Tarama yaptığım 10+ canlı/açık kaynak bot ve 7+ derinlikli makaleden çıkan ortak bulgular:

1. **Tek tip sinyal yok** — En iyi botlar 5–7 indikatörü ağırlıklı puanlama (composite weighted score) ile birleştiriyor.
2. **Edge 30–90 saniyeden 2–3 saniyeye düştü** — 2024'te 12+ saniyeden 2026'da ortalama 2.7 saniyeye sıkıştı; tekil takerlar artık ölü.
3. **18 Şubat 2026 sonrası paradigma değişti** — 500ms taker gecikmesi kaldırıldı, dinamik taker fee'leri (~%1.56 tepe) geldi, maker rebate ekonomisi açıldı.
4. **28 Nisan 2026 CLOB V2 migration** — Order struct değişti (`feeRateBps`/`nonce`/`taker` çıktı; `timestamp`/`metadata`/`builder` girdi), pUSD geldi.
5. **Sinyal kalitesi her zaman frekanstan önemli** — Threshold kalibrasyonu (ör. BTC 5m'de 0.3%/60s) günde 3-8 sinyal üretmeli, yoksa gürültü trading.
6. **Window delta hâlâ kral** — 5-15 dk'lık binary piyasalarda `(current - window_open) / window_open` tek başına diğer tüm TA indikatörlerinden 5-7 kat daha güçlü.

---

## 1. Polymarket Mikroyapı: Ne Trade Ediyoruz?

### 1.1 Binary Up/Down Piyasası

Polymarket'in 5m / 15m / 1h crypto piyasaları (BTC, ETH, SOL, XRP) deterministik slug pattern'lerle açılıyor:

```
5m  : btc-updown-5m-{window_ts}    where window_ts % 300 == 0
15m : btc-updown-15m-{window_ts}   where window_ts % 900 == 0
1h  : btc-updown-1h-{window_ts}    where window_ts % 3600 == 0
```

Her pencere 2 token üretir: `outcomes[0]` = UP, `outcomes[1]` = DOWN. Pencere sonunda Binance/Coinbase oracle (Chainlink/UMA) referansıyla resolve olur. Kazanan token $1.00, kaybeden $0.00 öder.

### 1.2 CLOB Yapısı

Polymarket = hibrit (off-chain order matching + on-chain settlement) **CLOB**. Önemli mekanik: bir tarafta her bid'in karşılığı diğer tarafta inverse ask olarak yansıtılır. `1 YES + 1 NO = $1.00` core invariant.

Bu invariant doğrudan iki kritik sinyal kaynağı yaratır:
- **Complete-set arbitrage**: `yes_ask + no_ask < 1.00` → guaranteed profit
- **Avg-sum bilateral arbitrage**: `avg_yes + avg_no < 1.00` (senin Special/Harvest stratejilerinde olduğu gibi)

### 1.3 Latency Asimetrisi (Edge'in Kaynağı)

Tüm latency arbitrage stratejileri tek bir gözlemden besleniyor:

| Veri kaynağı | Tick frekansı | Latency |
|---|---|---|
| Binance `@aggTrade` (spot) | ~200ms | Mikro saniye seviyesi |
| Binance `@bookTicker` | Tick-level | Sub-50ms global, 5-12ms AMS |
| Polymarket CLOB orderbook | Discrete on-chain TX | 30-90s lag (büyük hareketlerde) |
| Polymarket WS `best_bid_ask` | Event-driven | Aggregated, slow propagation |

**Kritik istatistik**: 2024'te ortalama arbitraj penceresi 12+ saniyeydi; 2026'da 2.7 saniye. 500ms gecikme = senaryonun yarısı kaybedildi demek.

---

## 2. Kritik 2026 Kural Değişiklikleri

### 2.1 Şubat 18, 2026: 500ms Taker Gecikmesinin Kaldırılması

Eski yapıda her taker order 500ms beklerdi — bu market maker'lara stale quote'ları cancel edebilmeleri için ücretsiz "sigorta" sağlıyordu. Şubat 2026'da kaldırıldı. Sonuç:

- Saf taker arbitraj (gabagool22 dahil) **öldü** çünkü fee tek başına spread'den büyük
- Maker stratejileri öne çıktı: zero fee + USDC rebate
- Cancel/replace döngüsü zorunlu olarak <100ms'e indi

### 2.2 Dinamik Taker Fee Yapısı

Polymarket `fee_equivalent` formülünü kullanıyor (USDC cinsinden):

```
fee = C × feeRate × p × (1 - p)
```

Burada `C` = trade edilen share sayısı, `p` = share fiyatı (probability). Fee curve **simetrik** — 30¢ ve 70¢ aynı $ fee'sini öder.

Kategori bazlı `feeRate` (Mart 30, 2026 itibariyle güncel):

| Kategori | Taker feeRate | Maker feeRate | Tepe efektif fee (p=0.50) |
|---|---|---|---|
| Crypto | 0.07 | 0 | **1.56%** |
| Sports | 0.03 | 0 | 0.44% |
| Finance / Politics / Mentions / Tech | 0.04 | 0 | 0.94% |
| Economics / Culture / Weather / Other | 0.05 | 0 | 1.18% |
| Geopolitics | 0 | 0 | Fee-free |

**BAITER için kritik**: Crypto kategorisinde p=0.50'de %1.56 tepe fee var. Yani break-even için p_true – p_market > 0.0156 olmalı (1.56 yüzde puanı edge). Saf taker latency arb için bu artık marjinal.

### 2.3 Maker Rebate Programı

```
maker_rebate = (your_fee_equivalent / total_fee_equivalent_in_market) × rebate_pool
```

Rebate pool = market'te o gün toplanan taker fee'lerinin %20'si (crypto) veya %25'i (diğer). Günlük dağıtım, $1 minimum payout.

Önemli: Rebate per-market hesaplanır — yani aynı market'teki maker'larla rekabet ediyorsun, başka market'tekilerle değil.

### 2.4 Liquidity Rewards (dYdX-türevi quadratic scoring)

Maker rebate'in yanı sıra, **eski liquidity incentive program** hâlâ aktif. Quadratic scoring:

```
S(v, s) = ((v - s) / v)² × b

Q_one  = Σ S(v, Spread_m_i) × BidSize_m_i + Σ S(v, Spread_m'_i) × AskSize_m'_i
Q_two  = Σ S(v, Spread_m_i) × AskSize_m_i + Σ S(v, Spread_m'_i) × BidSize_m'_i

# midpoint ∈ [0.10, 0.90]: tek-taraflı düşük puan kazanır
Q_min  = max(min(Q_one, Q_two), max(Q_one/c, Q_two/c))    # c = 3.0
# midpoint ∉ [0.10, 0.90]: çift taraflı zorunlu
Q_min  = min(Q_one, Q_two)
```

`v` = max spread (cent), `s` = order'ın midpoint'ten cent cinsinden uzaklığı, `b` = in-game çarpan. Score her dakika örneklenir, 1 epoch = 10,080 sample (1 hafta).

Crypto Up/Down piyasalarında genelde aktif değil ama spor (NBA, EPL, MLB, UFC) ve esports piyasalarında ciddi pool'lar var (Nisan 2026 toplam $5M+).

### 2.5 CLOB V2 Migration (28 Nisan 2026)

**Bu en kritik son değişiklik — eski kodun çalışmaz.**

Order struct değişiklikleri:

```diff
SignedOrder {
  salt, maker, signer, side, signatureType, signature,
- nonce          ← KALDIRILDI
- feeRateBps     ← KALDIRILDI  (artık otomatik)
- taker          ← KALDIRILDI
+ timestamp      ← EKLENDİ (millisecond)
+ metadata       ← EKLENDİ
+ builder        ← EKLENDİ (Builder Program code)
}
```

Diğer değişiklikler:
- `verifyingContract` adresi değişti (yeni Exchange V2 sözleşmeleri)
- Collateral token: USDC.e → **pUSD** (USDC backed ERC-20 on Polygon)
- Cutover'da tüm açık order'lar wipe edildi
- Cutover sırasında ~1 saat trading paused

**BAITER aksiyon kalemi**: `rs-clob-client`'ın V2'ye uyumlu olduğunu doğrula. SDK auto-handle etse bile manuel order signing varsa eski struct kalmamalı.

---

## 3. Sinyal Taksonomisi: 5 Kategori

Tüm internetteki Polymarket bot literatüründen çıkan sinyal kaynakları:

| # | Sinyal Kategorisi | Dayanak | Edge Süresi | Risk |
|---|---|---|---|---|
| 1 | **Latency Arbitrage** | Binance spot vs Polymarket lag | 30-90s (azalıyor) | Spot reversal, fee drag |
| 2 | **Composite Technical Analysis** | Window delta + EMA + RSI + volume | Pencere boyu | Noise overruling |
| 3 | **Theoretical Probability (GBM)** | Black-Scholes-türevi normal CDF | Pencere boyu | Vol misestimation |
| 4 | **Cross-Market Arbitrage** | Polymarket vs Kalshi vs Polyscript | 2-7s | Partial fill, liquidity |
| 5 | **Microstructure Signals** | Order book imbalance, VPIN, aggressor ratio | Saniye-dakika | Adverse selection |

Aşağıda her birini formülleri ve referans implementasyonlarıyla detaylandırıyorum.

---

## 4. Sinyal 1: Latency Arbitrage (Binance → Polymarket)

### 4.1 Temel Mantık

Polymarket'in CLOB market maker'ları Binance'i izliyor ama:
- Binance update frekansı: 1ms-200ms (WebSocket aggTrade)
- Polymarket repricing: 30-90s (bot ekosistemi yavaşlıyor)
- Bu pencerede UP/DOWN token mispriced

**Tetikleme mantığı (Chudi Nnorukam, 69.6% win rate, 23 canlı trade)**:

```python
THRESHOLD_PCT = 0.003   # %0.3
WINDOW_SECS = 60

if abs(pct_move) >= THRESHOLD_PCT:
    direction = "UP" if pct_move > 0 else "DOWN"
    # → Polymarket'te ilgili token'a maker bid yerleştir
```

### 4.2 Threshold Kalibrasyonu (BTC, deneysel)

| Threshold / Window | Sinyal/Gün | İn-range Win Rate | Kullanım |
|---|---|---|---|
| 0.15% / 30s | 15-20 | %~50 | Çok gürültü, kullanılmaz |
| **0.30% / 60s** | **3-8** | **%62-69** | **Sweet spot (BTC)** |
| 0.50% / 60s | 0-2 | %~75 | Çok seyrek, validation zor |

ETH, SOL, XRP için noise floor farklı — kalibre etmek gerekiyor. ETH genellikle BTC'ye yakın (0.30-0.35%), SOL ve XRP daha gürültülü (0.45-0.6% gerekiyor).

### 4.3 Deque-Based Rolling Window (Production-Grade)

```python
from collections import deque

class MomentumDetector:
    THRESHOLD_PCT = 0.003
    WINDOW_SECS = 60
    
    def __init__(self):
        self._window: deque[Tick] = deque()
    
    def update(self, tick: Tick) -> Optional[Direction]:
        self._window.append(tick)
        self._prune(tick.timestamp)
        
        if len(self._window) < 2:
            return None
        
        oldest = self._window[0].price
        newest = self._window[-1].price
        pct_move = (newest - oldest) / oldest
        
        if pct_move >= self.THRESHOLD_PCT:  return Direction.UP
        if pct_move <= -self.THRESHOLD_PCT: return Direction.DOWN
        return None
    
    def _prune(self, now: float):
        cutoff = now - self.WINDOW_SECS
        while self._window and self._window[0].timestamp < cutoff:
            self._window.popleft()
```

**Neden circular buffer değil deque**: Volatil günlerde 1000+ tick/s gelir; deque doğal büyür. Reconnect sonrası REST kline ile reseed kolay. Flash crash gibi durumlarda intermediate price'lar atlanmaz.

### 4.4 Signal Guard (Cooldown)

Tek bir sustained 3-dk rally 3 ayrı sinyal yaratır → 3 üst üste long pozisyon (effectively 1 trade with 3x risk). Çözüm:

```python
class SignalGuard:
    COOLDOWN_SECS = 120   # 5-min market için kalibre
    
    def should_trade(self, direction):
        if (direction == self._last_direction
            and time.time() - self._last_signal_ts < self.COOLDOWN_SECS):
            return False
        self._last_direction = direction
        self._last_signal_ts = time.time()
        return True
```

**Yön değişimi cooldown'u resetler** — UP'tan hemen sonra DOWN sinyali bağımsız trade'dir.

Cooldown kalibrasyonu:
- 5m markets: 120s
- 15m markets: 300s (hâlâ aynı pencerede stack'lemekten kaçın)
- 1h markets: 600-900s

Pratikte sinyal guard ham sinyallerin %30-40'ını filtreliyor — bunlar çoğunlukla aynı momentum'un kopyaları.

### 4.5 BAITER Phase 2 Bağlantısı

PRISM v4 reverse engineering bulgularıyla örtüşen yerler:
- Phase 3 (180-270s) %75 kâr üretiyordu — bu, latency sinyalinin OPTIMAL window'u
- 2-second event loop yeterli (sub-second gerekmiyor crypto windows için)
- 75.3% same-second multi-fill = aynı sinyal birden fazla market'i triggerliyor (BTC + ETH örneğin)

---

## 5. Sinyal 2: Composite Technical Analysis (Multi-Indicator Scoring)

### 5.1 Archetapp 7-Indicator Composite (En Detaylı Açık Kaynak)

5-min BTC bot (gist.github.com/Archetapp/7680adabc48f812a561ca79d73cbac69) — **window delta'nın diğer her şeyden 5-7 kat ağır** olduğunu test ederek bulmuş:

| # | Indicator | Weight | Hesaplama |
|---|---|---|---|
| 1 | **Window Delta** | **5-7** | `(current - window_open) / window_open × 100` |
| 2 | Micro Momentum | 2 | Son 2 1m candle yönü |
| 3 | Acceleration | 1.5 | Latest candle move vs 2 candles ago |
| 4 | EMA Crossover 9/21 | 1 | EMA9 > EMA21 = bullish |
| 5 | RSI 14 | 1-2 | >75 (weight 2 short), <25 (weight 2 long) |
| 6 | Volume Surge | 1 | 3-bar avg volume / önceki 3-bar avg ≥ 1.5x |
| 7 | Real-Time Tick Trend | 2 | 60%+ directional consistency, >0.005% move |

Window delta tier sistemi (en kritik kısım):

```python
window_pct = (current_price - window_open_price) / window_open_price * 100

if window_pct > 0.10:    weight = 7   # Decisive — neredeyse kesin
elif window_pct > 0.02:  weight = 5   # Strong
elif window_pct > 0.005: weight = 3   # Moderate
elif window_pct > 0.001: weight = 1   # Slight
else:                    weight = 0
```

### 5.2 Confidence Hesaplama

```python
total_score = sum(weight_i × direction_i for each indicator)  # +UP, -DOWN
confidence = min(abs(total_score) / 7.0, 1.0)
```

Neden /7 (ve /10 değil): 5-min binary'de uzun vadeli indikatörler (EMA, RSI) az alakalı; window delta dominant olduğu için meaningful confidence'a 7'lik divisor'la daha kolay erişiliyor.

### 5.3 Trading Mode Calibration

| Mode | Bet Size | Min Confidence | Felsefe |
|---|---|---|---|
| Safe | 25% bankroll | 30% | 4 üst üste loss = bankroll'un %68'i, slow compound |
| Aggressive | All proceeds (orijinal korunur) | 20% | Profit hızla compound, principal protected |
| Degen | All-in | 0% | Kazandığında 2x, kaybettiğinde wipe |

### 5.4 T-10s Snipe Loop (Critical Timing)

Bot tek atış yapmaz — T-10s'te polling loop'a girer:

```python
# T-10s'ten T-5s'e kadar:
1. Her 2 saniyede analyze() çalıştır
2. Highest |score|'u track et
3. Score 1.5+ jump = "tipping moment", hemen ateş
4. Confidence threshold karşılanırsa ateş
5. T-5s hard deadline: Best signal'la fire et (asla skip etme)
```

**Token pricing modeli (backtest realism için)**:

```
delta < 0.005% → $0.50   (coin flip)
delta ~ 0.02%  → $0.55
delta ~ 0.05%  → $0.65
delta ~ 0.10%  → $0.80
delta ~ 0.15%+ → $0.92-0.97
```

Bu olmadan backtest fake %80+ win rate gösterir. Gerçek piyasa = market maker'lar da aynı delta'yı görür ve fiyatlar.

### 5.5 NautilusTrader Fusion Engine (15-min Bot)

aulekator/Polymarket-BTC-15-Minute-Trading-Bot 7-phase mimari:

```
[Coinbase, Binance, News, Solana]
     ↓
Ingestion (Unify & Validate)
     ↓
Nautilus Core (Trading Framework)
     ↓
Signal Processors (Spike, Sentiment, Divergence)
     ↓
Fusion Engine (Weighted Voting)
     ↓
Risk Management ($1 Max, Stop Loss)
     ↓
Execution → Monitoring (Grafana) → Learning (Weight Optimization)
```

`SPIKE_THRESHOLD = 0.15`, `DIVERGENCE_THRESHOLD = 0.05` default. >70% directional confidence → pozisyon. Self-learning weight optimization sürekli geri-besleme ile ağırlıkları ayarlıyor.

---

## 6. Sinyal 3: Theoretical Probability via GBM (Black-Scholes Türevi)

### 6.1 Formal Model

Kalshibot'un GBM yaklaşımı — BTC'nin contract süresi boyunca random walk yapacağını varsayar:

```
P(UP) = Φ(Z)

where:
    Z = move / (σ × √T_remaining)
    move = (S_current - S_open) / S_open
    σ = realized_volatility (window = contract_duration)
    T_remaining = time_remaining / total_duration ∈ [0, 1]
    Φ(x) = standart normal CDF
```

**Yorum**: "Şu anki spot move, kalan vol ile re-traverse edilebilir mi?" Z büyük ve pozitif → P(UP) yüksek; Z = 0 → P = 0.50.

### 6.2 Normal CDF Yaklaşımı (Abramowitz-Stegun, Hızlı + Hassas)

```javascript
// |x| < 8 standard sapma için 1e-7 hassasiyet
function Phi(x) {
    const sign = Math.sign(x);
    const abs_x = Math.abs(x);
    const t = 1 / (1 + 0.3275911 * abs_x);
    const a1 = 0.254829592, a2 = -0.284496736, a3 = 1.421413741;
    const a4 = -1.453152027, a5 = 1.061405429;
    const P = (((((a5*t + a4)*t) + a3)*t + a2)*t + a1)*t;
    const Y = 1 - P * Math.exp(-abs_x*abs_x / 2) / Math.sqrt(2*Math.PI);
    return 0.5 * (1 + sign * Y);
}
```

`Φ(x)` clamp [-8, +8] sigma; output clamp [0.01, 0.99] (extreme Kelly sizing'i önler).

### 6.3 Realized Volatility Estimation (Rolling)

1-saniyelik Binance fiyat sample'larından log-return method:

```
r_i = ln(P_i / P_{i-1})  for each consecutive sample

mean = (1/N) × Σ r_i
variance = (1/N) × Σ (r_i - mean)²
std_per_sample = √variance

avg_interval_ms = (t_last - t_first) / (N - 1) × 1000
samples_in_window = window_seconds × 1000 / avg_interval_ms

σ = std_per_sample × √samples_in_window
```

Default fallback: 0.15% per 15-min if N < 10. Buffer 600 sample (~10 dakika).

### 6.4 Edge Cases

| Durum | Davranış |
|---|---|
| No Binance price | Neutral (0.50) |
| σ × √T_remaining < 1e-5 | Pure momentum (0.99 / 0.01) |
| < 10 sample | Default vol fallback |

### 6.5 Edge Hesaplama ve Sinyal Tetikleme

```
modelEdge_YES = (P(UP) - YES_ask) × 100
modelEdge_NO  = (P(DOWN) - NO_ask) × 100

if modelEdge > MIN_DIVERGENCE (default 8%):
    → BUY mispriced side
```

**Önemli not (5-min markets için)**: GBM crypto'da gerçek dağılımı yakalamaz (fat tails, jumps). Gerçek sigma'yı GBM σ'sı ile karşılaştır — log-return distribution'ında jump component varsa **logit jump-diffusion** modeli daha doğru (arXiv:2510.15205, "Toward Black-Scholes for Prediction Markets", Daedalus Research, Eki 2025).

---

## 7. Sinyal 4: Cross-Market & Cross-Exchange Arbitrage

### 7.1 Complete-Set Arbitrage (Risk-Free)

```
combined = YES_ask + NO_ask
if combined < 1.00 - safety_margin:
    BUY both YES and NO simultaneously
    profit = (1.00 - combined) per pair
```

Default safety margin %2 (combined < 0.98). Senin AVG SUM mantığınla aynı.

**Yakalama hızı kritik**: 2026'da bu opportunity'ler ortalama <3 saniye süriyor. Sub-100ms execution + FOK both legs.

### 7.2 Bilateral Averaging (Senin Special / Harvest Stratejilerin)

```
avg_yes + avg_no < 1.00 - threshold
```

Pyramid score / position imbalance dinamikleri:

```
pyramid_score = Σ (cost_i × time_decay_i)
imbalance = |yes_position - no_position| / total_position
```

Threshold tablosu (mevcut PRISM v4 spec'inden):

| Mode | avg_sum threshold |
|---|---|
| Aggressive | < 0.980 |
| Moderate | < 0.990 |
| Conservative | < 0.995 |

Bu rakamlar **fee-aware** olmalı — `feeRate × p × (1-p)` çıkarılınca gerçek edge görülür.

### 7.3 Cross-Exchange (Polymarket vs Kalshi)

```
polyFair_UP = (Polymarket_UP_bid + Polymarket_UP_ask) / 2
polyEdge_YES = (polyFair_UP - Kalshi_YES_ask) × 100

if polyEdge > MIN_EDGE (default 5%):
    BUY underpriced side on Kalshi
```

**Mantık**: Polymarket daha derin liquidity → daha efficient price discovery. Polymarket mid > Kalshi ask = Kalshi undervalued.

Aynı simetri tersi de geçerli (Kalshi → Polymarket). 2026'da hem `polymarket-kalshi-arbitrage-bot` hem `polymarket-kalshi-arbitrage-trading-bot` repoları aktif.

### 7.4 Pre-Order / Two-Leg Strategies

PolyScripts/polymarket-5min-15min-1hr-btc-arbitrage-trading-bot-rust gibi botlar:
- Both UP and DOWN için concurrent maker limit orders
- Market-neutral entry
- 20ms order placement (50 checks/sec)
- Both fills → arbitrage; one fill → directional + averaging

State machine yaklaşımı (senin mevcut Arbigab v2 spec'inle uyumlu):

```
WaitSpread → InitialPending → SecondLegPhase / Averaging → ProfitLocked / Unwinding
```

### 7.5 Synth AI / Bittensor SN50 Edge (Yeni 2026 Stratejisi)

Bittensor'ın SN50 (Synth) probabilistic forecast subnet'inden BTC/ETH 5-15dk forecast'leri çekerek Polymarket implied odds vs Synth probability divergence'ı arbitrage et:

```
edge = synth_prob - polymarket_implied_prob
if abs(edge) > 0.10:   # %10+ mispricing
    BUY mispriced side
```

dev-protocol/Polymarket-Trading-Bot-with-Synth-AI bu stratejinin top performer'ı olduğunu iddia ediyor (2026 üst stratejilerinden).

---

## 8. Sinyal 5: Microstructure Signals (Yüksek Frekans)

### 8.1 Order Book Imbalance (OBI)

```
OBI = (bid_depth - ask_depth) / (bid_depth + ask_depth) ∈ [-1, +1]
```

Pozitif OBI = buy pressure baskın. GameTyrant analizi: OBI > +0.6 (veya < -0.6) sonraki 5-15 dk hareketinin yönünü %60+ olasılıkla doğru tahmin ediyor.

Multi-level OBI (daha sağlam):

```python
def obi_weighted(book, levels=5):
    bids = book.bids[:levels]
    asks = book.asks[:levels]
    weighted_bid = sum(size / (i+1) for i, (price, size) in enumerate(bids))
    weighted_ask = sum(size / (i+1) for i, (price, size) in enumerate(asks))
    return (weighted_bid - weighted_ask) / (weighted_bid + weighted_ask)
```

Polymarket gibi thin-book piyasada level-5 yeterli; Binance derinliğinde level-20'ye kadar gidilir.

### 8.2 VPIN (Volume-Synchronized Probability of Informed Trading)

Easley, López de Prado, O'Hara (2012) — informed flow detection:

```
1. Volume'u N eşit "bucket"'a böl (ör. her 1000 contract)
2. Her bucket'ta buy volume (V_B) ve sell volume (V_S) ayır
3. VPIN = avg(|V_B - V_S| / (V_B + V_S)) over last K buckets
```

VPIN spike = informed trader activity. 2010 Flash Crash'ten saatler önce VPIN anormal yükseldi.

**Polymarket on-chain version (insiders.bot benzeri)**: Smart money wallet'larının trade'leri = en doğrudan microstructure signal. Bilinen "alpha wallet"ları izle, large positions açıldığında follow trade.

### 8.3 Aggressor Ratio

```
aggressor_ratio = market_buys / (market_buys + market_sells)
```

Tick rule (her trade'in aggressor side'ını sınıflandır):
- Trade >= prev_mid → buy aggressor
- Trade <= prev_mid → sell aggressor

Aggressor ratio > 0.65 sustained → directional momentum confirmed.

### 8.4 Effective Spread Compression

```
effective_spread = 2 × |trade_price - mid_price|
```

Spread compression → liquidity geliyor → directional move yakın. Wrestling AC analizi: Polymarket'te effective spread compression + volume surge sonraki 5-15dk hareketini %60+ predicts.

### 8.5 Composite Microstructure Signal (BAITER için önerilen)

```python
ms_score = (
    0.35 * obi_weighted +
    0.25 * (vpin - 0.5) * 2 +              # normalize to [-1, +1]
    0.25 * (aggressor_ratio - 0.5) * 2 +
    0.15 * spread_compression_normalized
)
# direction = sign(ms_score), confidence = abs(ms_score)
```

Polymarket'in thin orderbook'unda VPIN ve aggressor ratio için **Binance** tarafında hesaplanmalı (tick frequency yetersiz Polymarket'te). OBI ve spread Polymarket native.

---

## 9. Sinyal Fusion Engine (Multi-Signal Combination)

### 9.1 Weighted Voting (Standard)

```python
class FusionEngine:
    def __init__(self):
        self.weights = {
            'window_delta':      5.0,
            'latency_arb':       3.0,
            'gbm_probability':   2.5,
            'composite_ta':      2.0,
            'microstructure':    1.5,
            'cross_market':      4.0,   # very high confidence when fires
        }
    
    def fuse(self, signals: dict) -> tuple[Direction, float]:
        score = 0
        for name, signal in signals.items():
            if signal is None: continue
            score += self.weights[name] * signal.direction * signal.confidence
        
        confidence = min(abs(score) / sum(self.weights.values()), 1.0)
        direction = Direction.UP if score > 0 else Direction.DOWN
        return direction, confidence
```

### 9.2 Self-Learning Weights

aulekator bot'unun feedback loop'u:

```
For each closed position:
    pnl = position.pnl
    contributing_signals = position.entry_signals
    for signal_name in contributing_signals:
        if pnl > 0:
            weights[signal_name] *= (1 + learning_rate)
        else:
            weights[signal_name] *= (1 - learning_rate)
    normalize_weights()
```

`learning_rate = 0.01-0.05`, period rolling 100 trade. Overfitting'e dikkat — weight değişiklikleri %20'yi aşmamalı clip.

### 9.3 Orthogonalization (PANews/insiders.bot Yaklaşımı)

11-step combinatorial engine'den 9. adım: signal'lar arasındaki bilgi örtüşmesini kaldır. Eğer GBM signal ve composite TA aynı window delta'dan besleniyorsa double-counting var.

```python
# Gram-Schmidt benzeri orthogonalization
def orthogonalize(signals_history):
    correlations = compute_correlation_matrix(signals_history)
    # Yüksek korelasyonlu sinyalleri downweight et
    for i in signals:
        redundancy = max(correlations[i, j] for j in signals if j != i)
        signals[i].weight *= (1 - redundancy)
```

### 9.4 Threshold Hierarchy

```
high_confidence_threshold  = 0.70   # full Kelly
medium_threshold          = 0.50   # half Kelly
low_threshold             = 0.30   # quarter Kelly
skip_threshold            = 0.15   # don't trade
```

---

## 10. Position Sizing (Kelly + Variants)

### 10.1 Full Kelly

```
b = (1 - p_market) / p_market    # net odds (binary $1 payout)
q = 1 - p_true
f* = (p_true × b - q) / b

position_dollars = f* × bankroll
```

Worked example (chudi.dev, BTC 5min): p_true = 0.69, p_market = 0.58:
```
b = 0.42 / 0.58 = 0.724
q = 0.31
f* = (0.69 × 0.724 - 0.31) / 0.724 = 0.187 / 0.724 ≈ 26%
```

### 10.2 Fractional Kelly (Production Standard)

```
KELLY_FRACTION = 0.25    # quarter-Kelly
position_dollars = min(
    f* × KELLY_FRACTION × bankroll,
    MAX_POSITION_SIZE,
    0.25 × bankroll        # absolute cap
)
```

**Neden quarter-Kelly**:
- Variance %75 azalır, growth %50 korunur
- p_true estimation error toleransı yüksek
- Ruin probability < %5
- Drawdown protection

### 10.3 Self-Tuning Position Sizing

chudi.dev/blog/self-tuner-adaptive-position-sizing-python prensibi:

```python
# Rolling 50-trade window
recent_win_rate = wins_50 / 50

if recent_win_rate < 0.50:
    bet_size_multiplier *= 0.95   # azalt
elif recent_win_rate > 0.65:
    bet_size_multiplier *= 1.02   # artır
    
bet_size_multiplier = clip(bet_size_multiplier, 0.5, 2.0)
```

Drawdown circuit breaker:
- Session loss > %10 → position size %50'ye düş
- Session loss > %20 → 15 dk pause

---

## 11. Fee Curve & EV Math (Production-Critical)

### 11.1 EV Formula

Binary $1 payout markets için basitleşmiş:

```
EV per share = p_true - p_market
```

Taker fee dahil:

```
EV_effective = (p_true - p_market) - feeRate × p_market × (1 - p_market)
```

Crypto (`feeRate = 0.07`) için break-even edge:

```
At p_market = 0.50: edge needed = 0.07 × 0.5 × 0.5 = 0.0175 = 1.75 yüzde puan
At p_market = 0.30: edge needed = 0.07 × 0.3 × 0.7 = 0.0147 = 1.47 yüzde puan
At p_market = 0.10: edge needed = 0.07 × 0.1 × 0.9 = 0.0063 = 0.63 yüzde puan
```

### 11.2 Maker vs Taker Switch

Maker (zero fee + rebate):

```
EV_maker = (p_true - p_market) + maker_rebate_share
```

Taker fee'nin tükettiği edge maker tarafında negatif feeRate gibi davranır → marginal trade'ler maker yapılırsa profit, taker yapılırsa loss.

**BAITER tasarım kuralı**:
- Latency arb fırsat penceresi > 2× cancel/replace cycle → taker dene
- Sinyal güçlü değilse veya pencere küçükse → maker

### 11.3 Maker Rebate Optimization

Maker rebate optimum yapmak için:

```
your_fee_equivalent = your_filled_volume × feeRate × p × (1-p)
```

Fee curve p × (1-p) maksimumu p=0.50'de. Yani aynı volume için 0.50 civarı quotelar daha fazla rebate getirir. Stratejik mod:

- Düşük vol piyasa → 0.30-0.70 aralığında quote (fee curve sweet spot)
- Yüksek vol → spread'i daraltıp fill yakala (rebate volume × p × (1-p))

---

## 12. Execution Timing

### 12.1 T-10s Sweet Spot (5-min Markets)

| Entry Time | Token Price | Win Rate | Profit/Trade |
|---|---|---|---|
| T-150s (early) | $0.50-0.55 | %55 | High variance |
| **T-10s** | **$0.65-0.85** | **%70-90** | **Sweet spot** |
| T-5s (late) | $0.92-0.97 | %95+ | Loss-making |

T-5s'te %95 win rate ama $0.95'ten alıp $1.00'a satıyorsun = $0.05 kazanç vs $0.95 kayıp. Break-even için 95%+ win gerekiyor → real-world ortalama %85-90 → kayıp.

### 12.2 Adaptive Scan Rate (Vol-Based)

Kalshibot yaklaşımı:

```python
vol = recent_volatility(window=300s)

if vol < 0.001:      scan_interval = 3000ms   # calm
elif vol <= 0.003:   scan_interval = 2000ms   # normal
else:                scan_interval = 1000ms   # volatile (max speed)
```

Sakinde API'yi yormaz, volatilde max signal capture.

### 12.3 Order Type Patterns (Polymarket-Spesifik)

| Order Type | Kullanım | Avantaj | Dezavantaj |
|---|---|---|---|
| **FOK** | Latency arb (both legs) | Partial fill yok | Reject olabilir |
| **FAK** | Aggressive single leg | Likidite ne varsa al | Partial fill |
| **GTC** | Maker limit | Zero fee + rebate | Doldurulmayabilir |
| **GTC + tick alignment** | Maker bilateral | Optimal | Tick size enforcement gerekli |

**Tick size traps (BAITER mevcut bilinen sorun)**:
- FOK/FAK: maker amount max 2 ondalık (`RoundingStrategy::ToZero`)
- GTC: effective price = `makerAmt/takerAmt` ratio, tick'e align olmalı

### 12.4 Cancel/Replace Loop Hedef Süreleri

| Süre | Durum |
|---|---|
| > 200ms | Adverse selection riski (post-Feb 2026) |
| 100-200ms | Acceptable maker quote |
| < 100ms | Top-tier (rebate-only profitable) |
| < 50ms | Ideal (London proximity gerekli) |

GCP London `c4-standard-2` (1-3ms CLOB latency) BAITER için optimal — bunu zaten kuruyorsun.

---

## 13. Reference Tables: Threshold Calibration

### 13.1 Asset × Timeframe × Threshold Matrix (Empirical)

Latency arb threshold (% move in 60s window for triggering):

| Asset | 5m | 15m | 1h |
|---|---|---|---|
| BTC | 0.30% | 0.45% | 0.80% |
| ETH | 0.35% | 0.50% | 0.90% |
| SOL | 0.50% | 0.75% | 1.30% |
| XRP | 0.55% | 0.85% | 1.50% |

### 13.2 Cooldown by Timeframe

| Timeframe | Same-direction cooldown |
|---|---|
| 5m | 120s |
| 15m | 300s |
| 1h | 600-900s |

### 13.3 Maker Spread Targets (Crypto Bucket Markets)

| Volatility Regime | YES bid offset from mid | NO bid offset from mid |
|---|---|---|
| Calm (< 0.1% recent vol) | -2 cents | -2 cents |
| Normal | -1.5 cents | -1.5 cents |
| Volatile | -0.5 to -1 cent (faster cancel) | -0.5 to -1 cent |

---

## 14. Açık Kaynak Bot Reference

| Repo | Dil | Strateji | Notlar |
|---|---|---|---|
| `aulekator/Polymarket-BTC-15-Minute-Trading-Bot` | Python | NautilusTrader, 7-phase fusion | Self-learning weights, Grafana, 70 stars |
| `Archetapp/PolymarketBot.md` (gist) | Python | 7-indicator composite, T-10s snipe | En detaylı public signal calc |
| `Polymarket/agents` | Python | Official AI agents framework | Newsapi + Chroma vectorization |
| `warproxxx/poly-maker` | Python | Market making, Google Sheets config | Author: "no longer profitable" |
| `Trum3it/polymarket-arbitrage-bot` | Rust | Cross-market 15m + 1h ETH/BTC | Market-neutral |
| `singhparshant/Polymarket` | Rust | MM + arb experiments | |
| `gamma-trade-lab/polymarket-copy-trading-bot` | Rust | Copy trading post-Feb 2026 | "Latency bot dead" disclaimer |
| `lorine93s/polymarket-copy-trading-bot` | TypeScript | Copy trading | Production scaffold |
| `dev-protocol/Polymarket-Trading-Bot-with-Synth-AI` | Multi | Bittensor SN50 + copy + arb | 322+ stars |
| `PolyScripts/polymarket-5min-15min-1hr-btc-arbitrage-trading-bot-rust` | Rust | 20ms order placement, market-neutral both legs | |
| `brandononchain/kalshibot` | JS | GBM + cross-exchange + dual-side | En matematiksel açık örnek |
| `ent0n29/polybot` | Java/Spring | Multi-service: ingestor/strategy/executor | "Reverse-engineer every Polymarket strategy" |

---

## 15. Critical Pitfalls & Bilinen Tuzaklar

### 15.1 Backtest Optimism

- **Fixed $0.50 token pricing** → fake %80+ win rate, %200 returns. **Delta-based pricing kullan.**
- **Survivor bias**: sadece pencereyi resolve eden trade'leri sayma; cancel'lanan/timeout olanlar dahil.
- **Look-ahead bias**: T-10s analyze'de bir sonraki saniyenin price'ı sızabilir.

### 15.2 Polymarket-Spesifik Bug'lar

| Bug | Etki | Çözüm |
|---|---|---|
| `update_balance_allowance()` post-fill | Sell order'ları silently fail eder | Sadece `get_balance_allowance()` kullan |
| Signature type mismatch | Order rejected (silent) | Type 1 = MagicLink, Type 2 = Safe/Gnosis (BAITER) |
| Tick alignment GTC | Reject "tick size violation" | Effective price'ı `makerAmt/takerAmt`'tan compute et |
| Decimal precision FOK | Reject "decimal precision" | `RoundingStrategy::ToZero`, max 2 ondalık |
| WebSocket silent disconnect | 6 saat sinyal kaçırma | Health monitor + REST kline reseed |
| Zombie position post-fill | Position var ama bot bilmez | Her sinyalde `get_open_positions()` poll |

### 15.3 Adverse Selection (Post-Feb 2026)

Maker stratejisi yapıyorsun ama cancel/replace 200ms'i geçiyor:

1. Quote stale, fair price kayıyor
2. Aggressive taker senin stale quote'unu doldurur
3. Sen kaybeden tarafa girersin

Çözüm: `expiration` field'ı her order'da set et (kısa, ör. 5s). 5s içinde cancel edemediysen otomatik wipe.

### 15.4 Fee Drag Compounding

%62 win rate'in p_market=0.50'de kâr getirebilir. Ama:
- 100 trade × 1.56% taker fee × $0.50 = $78 fee
- (62 × $0.50) - (38 × $0.50) = $12 gross win
- Net = -$66 → 5.5% loss

**Kural**: Fee curve dahil EV simülasyonu yapmadan canlıya çıkma.

---

## 16. BAITER-Spesifik Öneriler

### 16.1 Phase 1 (Hedged Arbitrage Only) — Mevcut Durum

Şu an yaptığın doğru sinyal yapısı:
- `avg_yes + avg_no < 0.980` (aggressive) → her iki bacağa entry
- Pyramid score ile averaging
- Fee-aware threshold

Bu dokümandan eklenecek:
- **GBM probability cross-check**: `(P(UP) - YES_ask)`'a basit threshold (>5%) ekle, "arbitraj fırsatı doğal mı yoksa flash crash trap mı" ayırt etmek için
- **OBI confirmation**: Imbalance > 0.6 entry'ye ek confidence verir

### 16.2 Phase 2 (Binance @aggTrade Directional Overlay)

Önerilen sinyal hiyerarşisi:

```
1. Latency arb (Binance > 0.30%/60s) → DOMINANT
2. Window delta (current - window_open) → CONFIRMING
3. GBM probability → SIZING (Kelly fraction kalibrasyonu)
4. OBI / aggressor ratio → ENTRY TIMING (within edge window)
5. VPIN → REGIME FILTER (yüksek VPIN'de küçült)
```

PRISM v4'teki 4-rejim sınıflandırıcı (Trending/MeanReverting/Breakout/Chaos) ile bu sinyal hiyerarşisini gating et:
- Trending: latency arb dominant, OBI confirming
- MeanReverting: GBM dominant (mid'e dönüş bekle)
- Breakout: window delta + cascade detector dominant
- Chaos: trade etme

### 16.3 V2 Migration Acil İşi

`rs-clob-client` V2 uyumu doğrula:

1. `Order` struct'ında `nonce`, `feeRateBps`, `taker` çıkarıldı mı?
2. `timestamp` (ms), `metadata`, `builder` eklendi mi?
3. `verifyingContract` yeni Exchange V2 sözleşmesini point ediyor mu?
4. Collateral USDC.e → pUSD migration tamamlandı mı?

**Test**: Paper trading mode'unda 24 saat order placement validation.

### 16.4 Threshold Initial Values (Rust Config)

```toml
[signal.latency_arb]
btc_5m_threshold_pct = 0.0030
eth_5m_threshold_pct = 0.0035
sol_5m_threshold_pct = 0.0050
xrp_5m_threshold_pct = 0.0055
window_secs = 60

[signal.composite_ta]
window_delta_decisive_pct = 0.10
window_delta_strong_pct   = 0.02
window_delta_moderate_pct = 0.005
confidence_divisor = 7.0
min_confidence = 0.30

[signal.gbm]
volatility_window_secs = 900   # 15-min markets
realized_vol_default = 0.0015
phi_clamp_sigma = 8.0
prob_clamp = [0.01, 0.99]

[signal.fusion]
weight_latency_arb = 5.0
weight_window_delta = 5.0
weight_gbm = 2.5
weight_obi = 1.5
weight_cross_market = 4.0

[signal.guard]
cooldown_5m_secs = 120
cooldown_15m_secs = 300
cooldown_1h_secs = 600

[execution]
fok_decimal_rounding = "ToZero"
gtc_tick_align = true
order_expiration_secs = 5
cancel_replace_max_ms = 100
heartbeat_interval_secs = 5

[fee]
crypto_taker_rate = 0.07
fetch_dynamic = true   # /fee-rate endpoint'ini her market için poll
```

### 16.5 Self-Learning Loop Sonradan Eklenecek

Phase 1 stable olduktan sonra, fusion engine ağırlıklarını kapalı pozisyonların PnL'inden geri-besleme ile ayarla. **Önemli**: Polymarket fee curve'üne göre normalize et — sinyal $0.50 fiyatlı bir trade'de mi, $0.85'te mi geldi büyük fark.

---

## 17. Aksiyon Listesi (BAITER Roadmap'i için)

| Öncelik | İş | Dayanak |
|---|---|---|
| **P0** | CLOB V2 (28 Nis 2026) order struct uyumu doğrula | `docs.polymarket.com/changelog` |
| **P0** | `feeRateBps` artık order body'de yok — auto-handle olduğunu test et | V2 migration notu |
| **P1** | GBM probability hesaplayıcı modülü (Rust) | Kalshibot reference impl |
| **P1** | OBI weighted (level-5) Polymarket native sinyal | GameTyrant + hftbacktest |
| **P1** | Fee-aware EV simulator (backtest validation) | chudi.dev/blog/directional-betting |
| **P2** | Adaptive scan rate (vol-driven 1-3s) | Kalshibot |
| **P2** | Multi-asset threshold calibration (her asset için ayrı) | Bu doküman §13.1 |
| **P3** | VPIN microstructure filter (regime gating) | PANews/insiders.bot |
| **P3** | Self-learning weight optimization | aulekator + chudi.dev/blog/self-tuner |
| **P3** | Synth AI / Bittensor SN50 cross-reference | dev-protocol/Synth-AI bot |

---

## 18. Kaynaklar

### Açık Kaynak Repolar
- aulekator/Polymarket-BTC-15-Minute-Trading-Bot (NautilusTrader, 7-phase)
- Archetapp/7680adabc48f812a561ca79d73cbac69 (5-min BTC, 7-indicator composite)
- Polymarket/agents (resmi AI agents framework)
- warproxxx/poly-maker (market making reference)
- brandononchain/kalshibot (GBM + cross-exchange, en matematiksel)
- gamma-trade-lab/polymarket-copy-trading-bot (Rust, post-Feb 2026)
- ent0n29/polybot (Java multi-service, "reverse-engineer every strategy")
- PolyScripts/polymarket-5min-15min-1hr-btc-arbitrage-trading-bot-rust
- dev-protocol/Polymarket-Trading-Bot-with-Synth-AI
- Trum3it/polymarket-arbitrage-bot (Rust)
- lorine93s/polymarket-copy-trading-bot (TypeScript)

### Makaleler & Akademik
- chudi.dev/blog/how-i-built-polymarket-trading-bot (69.6% win rate, Mar 2026)
- chudi.dev/blog/binance-polymarket-momentum-signal-pipeline (deque rolling window detayı)
- chudi.dev/blog/directional-betting-binary-markets-math (Kelly + EV)
- chudi.dev/blog/self-tuner-adaptive-position-sizing-python
- arxiv.org/pdf/2510.15205 (logit jump-diffusion, Daedalus Research)
- navnoorbawa.substack.com/p/the-mathematical-execution-behind (gamma + Black-Scholes)
- panewslab.com/en/articles/019d9926... (VPIN, microstructure, 11-step combinatorial)
- gametyrant.com/news/order-book-mechanics-on-prediction-markets (OBI > 60%)
- wrestlingac.com/unlocking-the-signal-polymarket-stats... (microstructure stats)
- medium.com/@aulegabriel381/the-ultimate-guide-building-a-polymarket-btc-15... 
- medium.com/coinmonks/polymarket-just-changed-its-fees... (Apr 2026 fee expansion)
- odaily.news/en/post/5209447 ("Polymarket New Rules": maker > taker, post-Feb 2026)
- weex.com/news/detail/polymarket-new-rule-release... (cancel/replace <100ms)

### Polymarket Resmi Dokümanlar
- docs.polymarket.com/market-makers/maker-rebates (rebate formülü)
- docs.polymarket.com/market-makers/liquidity-rewards (quadratic scoring, Apr 2026 pools)
- docs.polymarket.com/changelog (CLOB V2 migration, 28 Nis 2026)
- docs.polymarket.com/developers/CLOB/clients/methods-l1 (SignedOrder struct)
- docs.polymarket.com/trading/fees (kategori bazlı feeRate)

---

*Doküman BAITER Phase 1 + Phase 2 araştırması için hazırlanmıştır. Threshold değerleri ve weight'ler kalibrasyon gerektirir; canlıya çıkmadan önce minimum 72 saat paper trading + fee-curve aware backtest yapılmalıdır.*