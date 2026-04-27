# Polymarket Dual-Side Inventory Arbitrage Strategy

## 1. Amaç

Bu strateji, Polymarket binary marketlerinde:

* YES ve NO fiyatlarının toplamı < 1 olduğunda
* Her iki tarafta dengeli pozisyon alarak
* Guaranteed payout = 1 üzerinden arbitraj kârı üretmeyi hedefler

Binary payout:

```
YES wins  → YES = 1 , NO = 0
NO wins   → YES = 0 , NO = 1
```

Kar formülü:

```
profit = 1 - (avg_yes + avg_no)
```

Risk-free olması için:

```
qty_yes ≈ qty_no
```

---

# 2. Core Strategy

Bot aynı anda iki tarafı trade eder:

* YES tarafında passive bid
* NO tarafında passive bid

Amaç:

```
avg_yes + avg_no < 1
```

Ve:

```
qty_yes ≈ qty_no
```

Bu sağlandığında garantili payout oluşur.

---

# 3. Entry Koşulu

Bot sadece şu durumda trade eder:

```
yes_best_bid + no_best_bid < 0.985
```

Sebep:

* slippage buffer
* fill asimetrisi
* fee buffer

Örnek:

```
YES bid = 0.47
NO  bid = 0.50

0.47 + 0.50 = 0.97  → TRADE
```

---

# 4. Layered Order Modeli

Her tick'te 3 katmanlı emir:

## YES Side

```
size_small @ bid
size_mid   @ bid - 0.01
size_small @ bid - 0.02
```

## NO Side

```
size_small @ bid
size_mid   @ bid - 0.01
size_small @ bid - 0.02
```

Bu sayede:

* passive fill
* avg iyileşmesi
* spread capture

---

# 5. Inventory Management

Bot sürekli inventory izler:

```
imbalance = qty_yes - qty_no
```

Kural:

```
abs(imbalance) < 5
```

Eğer aşılırsa:

* fazla taraf durdurulur
* eksik taraf agresifleşir

---

# 6. Hedge Mekanizması

Eğer:

```
qty_yes > qty_no
```

Bot:

* NO tarafında daha yukarı bid
* YES tarafında pasif kal

Tersi:

```
qty_no > qty_yes
```

Bot:

* YES tarafı agresif
* NO tarafı pasif

Market order kullanılmaz.

Sadece maker hedge.

---

# 7. Ortalama Hesaplama

Her fill sonrası:

```
avg_yes = total_cost_yes / qty_yes
avg_no  = total_cost_no  / qty_no
```

Toplam:

```
total_avg = avg_yes + avg_no
```

---

# 8. Kar Kilitleme

Eğer:

```
total_avg < 0.975
```

Bot:

* yeni trade durdur
* hedge tamamla
* pozisyonu kilitle

---

# 9. Trade Mode

Bot 3 modda çalışır:

## MODE 1 — BALANCED

```
abs(imbalance) < 3
```

iki taraf aktif

---

## MODE 2 — HEDGE

```
imbalance > 3
```

tek taraf trade

---

## MODE 3 — LOCK

```
total_avg < target
```

trade durdur
sadece dengele

---

# 10. Order Cancel Kuralları

Order iptal edilir:

* bid değişti
* imbalance büyüdü
* spread kapandı
* momentum spike
* avg > 1

---

# 11. Risk Yönetimi

## Hard Stop

```
total_avg > 1.01
```

* tüm order cancel
* hedge ASAP

---

## Inventory Stop

```
abs(imbalance) > 10
```

force hedge

---

## Trend Stop

```
price move > 0.08
```

trade freeze

---

# 12. Position Size

Önerilen:

```
layer1 = 1 contract
layer2 = 2 contract
layer3 = 1 contract
```

Toplam:

```
4 contract / side
```

---

# 13. Expected Profit

Örnek:

```
YES avg = 0.44
NO  avg = 0.52

total = 0.96
profit = 0.04
```

1000 kontrat:

```
profit = 40$
```

---

# 14. Execution Loop

```
while market_open:

    read_orderbook()

    spread = yes_bid + no_bid

    if spread < threshold:
        place_layered_orders()

    on_fill():
        update_avg()
        check_imbalance()
        hedge()

    if total_avg < lock_target:
        lock_mode()

    if momentum_detected:
        hedge_only()
```

---

# 15. Momentum Filtresi

Eğer:

```
signal_score > 5
```

veya

```
delta(signal_score) > threshold
```

Bot:

* yeni trade durdurur
* sadece hedge

---

# 16. Strateji Avantajları

* direction tahmin yok
* market neutral
* inventory based
* sürekli arbitraj
* passive maker edge

---

# 17. Riskler

* tek taraf fill
* runaway trend
* liquidity boşluğu
* last minute spike

Hepsi:

* inventory control
* threshold
* hedge logic

ile çözülür.

---

# 18. Parametreler

```
ENTRY_THRESHOLD = 0.985
LOCK_TARGET = 0.975
MAX_IMBALANCE = 5
HARD_STOP = 1.01
LAYER_STEP = 0.01
BASE_SIZE = 1
```

---

# 19. Final Mantık

Bot:

1. spread kontrol eder
2. iki taraf bid koyar
3. fill olunca avg hesaplar
4. imbalance hedge eder
5. avg < 1 kilitler
6. payout bekler

Bu yapı:

direction almadan
pure arbitrage üretir.
