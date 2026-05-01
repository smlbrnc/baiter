# Polymarket Sinyal Motoru

Polymarket BTC piyasasına emir göndermeden önce Binance AggTrade + OKX verisi kullanarak yön belirleyen sinyal sistemi.

---

## Temel Fikir

Analizler gösterdi ki:
- **Binance AggTrade** her düşüş olayının **%100**'ünde Polymarket'ten ortalama **7–10 sn** önce hareketi görüyor
- **OKX** her yükseliş olayının **%45**'inde Polymarket'ten **4.4 sn** (medyan) önde
- Polymarket'in fiyat feed'i CEX verisini **1–10 sn gecikmeli** yansıtıyor

Bu gecikme penceresi → emir göndermeden önce sinyali sorgula, her zaman UP ya da DOWN yönünde işlem aç.

---

## Emir Döngüsü

```
Başlat
  │
  ▼
[Her 2 sn]──► sinyal_sorgula()
                │
                ├── UP   → Polymarket'te YUKARI emir gönder
                └── DOWN → Polymarket'te ASAGI emir gönder
```

Sinyal her zaman **UP** ya da **DOWN** döner — döngü her 2 sn'de mutlaka bir emir gönderir.

---

## Sinyal Motoru Tasarımı

### Katman 1 — Binance CVD (Cumulative Volume Delta)

AggTrade mesajında `m` (is_buyer_maker) alanı gelir:

| `m` değeri | Anlam | Baskı |
|---|---|---|
| `false` | Alıcı agresif (alım emri eşleşti) | Boğa |
| `true` | Satıcı agresif (satış emri eşleşti) | Ayı |

Son **3 saniyelik** pencerede:

```
buy_volume  = Σ qty  (m = false)
sell_volume = Σ qty  (m = true)
CVD         = buy_volume - sell_volume
imbalance   = CVD / (buy_volume + sell_volume)   → [-1.0, +1.0]
```

### Katman 2 — OKX Fiyat Momentumu (EMA)

OKX tick'leriyle sürekli güncellenen çift EMA:

```
ema_fast = 0.40 × fiyat + 0.60 × ema_fast   # α=0.40 → ~2 sn
ema_slow = 0.10 × fiyat + 0.90 × ema_slow   # α=0.10 → ~10 sn

momentum_bps = (ema_fast - ema_slow) / ema_slow × 10000
```

### Katman 3 — Yön Kararı (her zaman UP veya DOWN)

İki kaynaktan gelen ham skor ağırlıklı olarak birleştirilir:

```
skor = (imbalance × 0.6) + (clip(momentum_bps, -5, +5) / 5 × 0.4)
         ↑ Binance ağırlığı        ↑ OKX ağırlığı (normalize)
```

`skor > 0` → **UP**, `skor ≤ 0` → **DOWN**

Eşik yoktur, işaret (pozitif/negatif) tek karar kriteridir.

---

## Uygulama — Python Referans Kodu

### Sinyal Motoru Sınıfı

```python
import asyncio
import json
import time
from collections import deque

class SignalEngine:
    WINDOW_S = 3.0   # CVD penceresi (sn)
    W_CVD    = 0.6   # Binance ağırlığı
    W_MOM    = 0.4   # OKX ağırlığı
    MOM_CAP  = 5.0   # Momentum normalize tavanı (bps)
    EMA_FAST = 0.40
    EMA_SLOW = 0.10

    def __init__(self):
        self.binance_trades = deque()   # (ts, qty, is_maker)
        self.ema_fast = None
        self.ema_slow = None

    # Binance AggTrade mesajından çağır
    def ingest_binance(self, ts: float, price: float, qty: float, is_maker: bool):
        self.binance_trades.append((ts, qty, is_maker))
        cutoff = ts - self.WINDOW_S
        while self.binance_trades and self.binance_trades[0][0] < cutoff:
            self.binance_trades.popleft()

    # OKX tick'inden çağır
    def ingest_okx(self, price: float):
        if self.ema_fast is None:
            self.ema_fast = self.ema_slow = price
            return
        self.ema_fast = self.EMA_FAST * price + (1 - self.EMA_FAST) * self.ema_fast
        self.ema_slow = self.EMA_SLOW * price + (1 - self.EMA_SLOW) * self.ema_slow

    def sinyal(self) -> dict:
        # --- Binance CVD ---
        buy  = sum(q for _, q, m in self.binance_trades if not m)
        sell = sum(q for _, q, m in self.binance_trades if m)
        total = buy + sell
        imbalance = (buy - sell) / total if total > 0 else 0.0

        # --- OKX Momentum ---
        if self.ema_fast is None or self.ema_slow is None or self.ema_slow == 0:
            momentum_bps = 0.0
        else:
            momentum_bps = (self.ema_fast - self.ema_slow) / self.ema_slow * 10000

        # --- Birleşik skor ---
        mom_norm = max(-1.0, min(1.0, momentum_bps / self.MOM_CAP))
        skor = imbalance * self.W_CVD + mom_norm * self.W_MOM

        direction = "UP" if skor > 0 else "DOWN"

        return {
            "direction":    direction,
            "skor":         round(skor, 4),      # -1.0..+1.0, büyük = güçlü sinyal
            "imbalance":    round(imbalance, 4),
            "momentum_bps": round(momentum_bps, 3),
            "buy_vol":      round(buy, 4),
            "sell_vol":     round(sell, 4),
            "trade_count":  len(self.binance_trades),
        }
```

### 2 Saniyelik Emir Döngüsü

```python
async def emir_dongusu(engine: SignalEngine, polymarket_client):
    """Her 2 saniyede bir sinyal sorgular, UP veya DOWN yönünde emir gönderir."""
    while True:
        cycle_start = asyncio.get_event_loop().time()

        sig = engine.sinyal()
        print(
            f"[{time.strftime('%H:%M:%S')}] "
            f"dir={sig['direction']:4s}  "
            f"skor={sig['skor']:+.3f}  "
            f"imb={sig['imbalance']:+.3f}  "
            f"mom={sig['momentum_bps']:+.2f}bps"
        )

        if sig["direction"] == "UP":
            await polymarket_client.buy(amount_usdc=10)
        else:  # DOWN
            await polymarket_client.sell(amount_usdc=10)

        # Döngü tam 2 sn olsun (işlem süresi dahil)
        elapsed = asyncio.get_event_loop().time() - cycle_start
        await asyncio.sleep(max(0, 2.0 - elapsed))
```

### WebSocket Besleyici (mevcut benchmark WS'den)

Mevcut `benchmark` binary'si `ws://localhost:8766` üzerinden tick yayıyor:

```python
async def ws_besleyici(engine: SignalEngine):
    """benchmark binary WebSocket'inden tick al ve engine'e besle."""
    import websockets
    async with websockets.connect("ws://localhost:8766") as ws:
        async for raw in ws:
            msg = json.loads(raw)
            if msg.get("t") != "tick":
                continue
            src   = msg["source"]
            price = msg["price"]
            ts    = msg["recv_ts_ns"] / 1e9

            if src == "binance_aggtrade":
                is_maker = msg.get("is_maker", True)   # ← AggTrade'e bu alan eklenecek
                qty      = msg.get("qty", 1.0)
                engine.ingest_binance(ts, price, qty, is_maker)

            elif src == "okx_trades":
                engine.ingest_okx(price)

async def main():
    engine = SignalEngine()
    polymarket_client = PolymarketClient(...)  # API entegrasyonu

    await asyncio.gather(
        ws_besleyici(engine),
        emir_dongusu(engine, polymarket_client),
    )
```

---

## Rust Tarafına Gereken Değişiklikler

Şu an `binance_aggtrade.rs` sadece fiyat gönderiyor. Sinyalin doğru çalışması için iki alan eklenmeli:

### 1. `is_maker` ve `qty` alanlarını WebSocket mesajına ekle

`src/collectors/binance_aggtrade.rs` içinde `aggTrade` stream yanıtından:

```rust
// aggTrade mesajı: {"e":"aggTrade","m":true,"q":"0.001","p":"78150.00",...}
let is_maker = msg["m"].as_bool().unwrap_or(true);
let qty_str  = msg["q"].as_str().unwrap_or("0");
let qty: f64 = qty_str.parse().unwrap_or(0.0);

// broadcast mesajına ekle:
json!({
    "t":        "tick",
    "source":   "binance_aggtrade",
    "price":    price,
    "qty":      qty,
    "is_maker": is_maker,
    // ... mevcut alanlar
})
```

### 2. OKX side alanını ekle (opsiyonel, şimdilik fiyat yeterli)

```rust
// OKX trades: side = "buy" | "sell"
let side = msg["side"].as_str().unwrap_or("buy");
json!({
    "t":      "tick",
    "source": "okx_trades",
    "price":  price,
    "side":   side,
    // ...
})
```

---

## Skor Yorumlama

| `skor` aralığı | Anlam |
|---|---|
| `+0.60` .. `+1.00` | Güçlü UP — her iki kaynak da yükseliş gösteriyor |
| `+0.00` .. `+0.60` | Zayıf UP — baskın ama karışık sinyal |
| `-0.60` .. `+0.00` | Zayıf DOWN — baskın ama karışık sinyal |
| `-1.00` .. `-0.60` | Güçlü DOWN — her iki kaynak da düşüş gösteriyor |

Yön her zaman kesindir; skor büyüklüğü güven düzeyini gösterir.

---

## Parametre Özeti

| Parametre | Varsayılan | Açıklama |
|---|---|---|
| `WINDOW_S` | 3.0 sn | CVD hesaplama penceresi |
| `W_CVD` | 0.60 | Binance ağırlığı |
| `W_MOM` | 0.40 | OKX ağırlığı |
| `MOM_CAP` | 5.0 bps | Momentum normalize tavanı |
| `EMA_FAST α` | 0.40 | ~2 sn yarı ömür |
| `EMA_SLOW α` | 0.10 | ~10 sn yarı ömür |
| Döngü aralığı | 2.0 sn | Polymarket emir sıklığı |

---

## Güvenlik Kuralları

```
1. Ardışık 5 yön değişimi (UP→DOWN→UP...) → engine'i sıfırla, logu incele
2. Polymarket API hatası → döngüyü durdur, alarm ver
3. benchmark WS bağlantısı koptu → emir döngüsünü durdur
4. 60 sn içinde hiç Binance/OKX tick gelmedi → döngüyü durdur
```

---

## Uygulama Sırası

- [ ] `binance_aggtrade.rs`'ye `is_maker` + `qty` alanları ekle
- [ ] `SignalEngine` sınıfını `signal_engine.py` olarak yaz
- [ ] `ws_besleyici` ile benchmark WS'e bağlan ve engine'i test et
- [ ] Sinyal + skor loglarını dosyaya yaz, 10 dk kayıtla geriye dönük doğrula
- [ ] `emir_dongusu`'nu önce simülasyon modunda çalıştır (emir göndermeden logla)
- [ ] Polymarket API entegrasyonu ekle
- [ ] Canlıya al
