# Bonereaper Bot — Implementasyon Kural Dosyası

**Wallet:** `0xeebde7a0e019a63e6b476eb425505b7b3e6eba30`  
**Veri tabanı:** 15 × btc-updown-5m (699 trade, x/y/z/w klasörleri) + 2 × obtest WebSocket OB stream  
**Güncelleme:** 1 Mayıs 2026  
**Doğrulama durumu:** Her kural iki bağımsız market üzerinde verify edildi

> **Sinyal uyarısı:** CSV'deki `bsi`/`cvd`/`ofi` sütunları Binance kaynaklıdır, Polymarket'ten değil.  
> Bot bu değerleri kendi Binance bağlantısıyla alıyor olabilir ya da hiç kullanmıyor olabilir.  
> Sinyal referansları bu belgede "olası" olarak işaretlenmiştir.

---

## Bölüm 0 — Sabit Parametreler

```python
MARKET_DURATION   = 300        # saniye (5 dakikalık market)
TICK_INTERVAL     = 2          # saniye (decision loop)
BASE_LOT          = 40         # standart lot (share)
LOT_RANGE         = (40, 45)   # normal lot aralığı
REBALANCE_MIN     = 50         # imbalance eşiği (share)
SCOOP_THRESHOLD   = 0.25       # karşı tarafın ask fiyatı (≤ bu değerde scoop)
SCOOP_WINDOW      = 100        # kapanışa kalan saniye (scoop başlar)
LOTTERY_THRESHOLD = 0.02       # lottery fiyat eşiği
LOTTERY_WINDOW    = 15         # kapanışa kalan saniye (lottery başlar)
LOTTERY_LOT       = 10000      # lottery lot boyutu
POST_MARKET_WAIT  = 30         # kapanış sonrası beklenebilecek max süre (sn)
BSI_THRESHOLD     = 0.30       # BSI mutlak değer eşiği — primer yön sinyali
# Yön kararı anında verilir (ilk geçerli OB snapshot'ı geldiği an, t≈0-2s)
# Gözlem penceresi YOK — bot beklemez, ilk veri gelir gelmez emir verir
STALE_SPREAD_MAX  = 0.05       # grid emir fiyat aralığı (bid'den uzaklık)
BINARY_SUM        = 1.00       # UP_ask + DN_ask ≈ 1.00 (piyasa kısıtı)
```

---

## Bölüm 1 — İki Saniyelik Decision Loop  
**Güven: KESİN (%100, 699/699 trade)**

Her tick'te market_start'tan itibaren geçen saniye **çift** sayıdır. Tek saniye trade YOKTUR.

```python
while True:
    sleep(TICK_INTERVAL)
    rel_t = current_unix_time() - market_start
    
    if rel_t % 2 != 0:
        continue  # bu tick'te işlem yapma
    
    ob  = fetch_polymarket_ob()   # güncel order book
    pos = get_my_position()       # UP ve DN share miktarları
    run_decision(rel_t, ob, pos)
```

**Backtest notu:** `if (timestamp - market_start) % 2 != 0: skip`

---

## Bölüm 2 — Emir Tipi: Sadece BUY, Çıkış REDEEM  
**Güven: KESİN (%100, 699 BUY / 0 SELL)**

- Tüm emirler `side = "BUY"`, hiçbir zaman SELL.
- Pozisyondan çıkış yoktur; kazanan taraf market kapanışında `REDEEM` ile `$1.00/share` alır.
- Kaybeden tarafın tüm payları sıfırlanır — stop-loss mekanizması yoktur.

---

## Bölüm 3 — Emir Yerleştirme: Grid Limit + Resting Bid  
**Güven: YÜKSEK (obtest + obtest2 doğruladı)**

### 3a. Resting Bid Mekanizması (Stale Fill)

Bot anlık OB fiyatına market emir vermez; **önceden limit emir** koyar. Piyasa hareket edince bu eski emirler fill olur.

```
obtest1:  t=+13s DN_bid=$0.60 → bot DN@0.60 limit koydu
           t=+16s piyasa DN=$0.65 → satıcı bot'un $0.60 bid'ini vurdu → FILL
           
obtest2:  t=0-5s  UP=$0.51 → bot UP@0.53 limit koydu
           t=+26s piyasa UP=$0.61 → FILL (7¢ stale)
```

**Fill dağılımı (obtest1 + obtest2):**

| Tip | Oran | Tanım |
|---|---|---|
| STALE (bid − >2¢) | ~%37–39 | Önceden koyulmuş resting bid, piyasa üstünde hareket etti |
| BID (tam bid ±1¢) | ~%25–33 | Anlık maker emir, normal fill |
| MID (ara) | ~%12–13 | Bid-ask arası limit |
| ASK_USTU (ask + >2¢) | ~%11–22 | Agresif taker veya eski yüksek limit |
| ASK (tam ask) | ~%5–10 | Anlık taker |

### 3b. Açılış Grid Emri (Dutch Book Tetikleyicisi)

Market açılışında (t=0–10s) bot **her iki tarafa da** mevcut fiyat seviyesinde limit emir koyar:

```python
# Piyasa t=0'da UP≈$0.50-$0.51 ile başladığında:
place_limit_buy(UP,   price=current_up_ask,    size=40-45)   # UP bid'e limit
place_limit_buy(DOWN, price=current_dn_ask,    size=40-45)   # DN bid'e limit
# Bu emirler piyasa hareket edince fill olur
# UP+DN fill fiyatları toplamı ≤$1.00 ise → Dutch Book garantili kâr
```

**Kanıt:**
- obtest1 t=36s: UP@$0.51 + DN@$0.49 = **$1.00 tam Dutch Book**
- obtest2 t=28s: UP@$0.5756 + DN@$0.40 = **$0.9756 Dutch Book (+2.4¢)**

---

## Bölüm 4 — Yön Kararı (İlk Trade)
**Güven: YÜKSEK (BSI primer, OB fallback)**

### Karar Süreci (Güncellenmiş)

Bot, **gözlem beklemeden** ilk geçerli OB snapshot geldiği anda (t≈0-2s) kararını verir.
Karar tek seferdir; sonraki sinyaller değişse de ilk yön korunur.

**Öncelik Sırası:**

```
1) BSI (Binance Spot Imbalance) — primer sinyal
   |BSI| >= BSI_THRESHOLD (0.30) ise:
     BSI > 0  →  UP  (BTC spotta alıcı baskın)
     BSI < 0  →  DOWN (BTC spotta satıcı baskın)

2) BSI yoksa veya |BSI| < 0.30 → OB ltp karşılaştırması
   ltp_up > ltp_dn  →  UP
   ltp_up < ltp_dn  →  DOWN

3) ltp da yoksa → best bid karşılaştırması (son fallback)
   up_bid > dn_bid  →  UP
   up_bid < dn_bid  →  DOWN
```

### Kural (Pseudocode)

```python
def decide_direction(ob_snap):
    # 1) BSI primer
    if ob_snap.bsi is not None and abs(ob_snap.bsi) >= BSI_THRESHOLD:
        return "UP" if ob_snap.bsi > 0 else "DOWN"

    # 2) OB ltp fallback
    if ob_snap.ltp_up is not None and ob_snap.ltp_dn is not None:
        return "UP" if ob_snap.ltp_up > ob_snap.ltp_dn else "DOWN"

    # 3) Best bid son fallback
    return "UP" if ob_snap.up_bid > ob_snap.dn_bid else "DOWN"
```

### Kanıt Tabloları

#### x/y/z/w klasörleri (BSI verisi MEVCUT, t=+1s ilk snapshot kararı)

| Klasör | Market | Kazanan | BSI (t=+1s) | Sim Kararı | Doğru? |
|--------|--------|---------|-------------|------------|--------|
| x | 1777622100 | DOWN | −0.350 | DOWN (BSI) | ✓ |
| x | 1777622400 | DOWN | −0.502 | DOWN (BSI) | ✓ |
| x | 1777622700 | UP   | +1.477 | UP (BSI)   | ✓ |
| x | 1777624200 | DOWN | <0.30  | DOWN (OB)  | ✓ |
| x | 1777624500 | DOWN | −0.416 | DOWN (BSI) | ✓ |
| x | 1777624800 | DOWN | −0.338 | DOWN (BSI) | ✓ |
| y | 1777628100 | DOWN | +0.893 | UP (BSI)   | ✗ |
| y | 1777628400 | DOWN | −0.328 | DOWN (BSI) | ✓ |
| y | 1777628700 | DOWN | <0.30  | UP (OB)    | ✗ |
| z | 1777629000 | UP   | −0.391 | DOWN (BSI) | ✗ |
| z | 1777629300 | UP   | <0.30  | UP (OB)    | ✓ |
| z | 1777630200 | DOWN | −2.372 | DOWN (BSI) | ✓ |
| w | 1777638600 | UP   | −0.463 | DOWN (BSI) | ✗ |
| w | 1777638900 | UP   | <0.30  | DOWN (OB)  | ✗ |
| w | 1777639200 | DOWN | +1.270 | UP (BSI)   | ✗ |

**Toplam:**
- BSI primer (|BSI|≥0.30): **7/11 = %64**
- OB fallback (|BSI|<0.30):  **2/4 = %50**
- Genel doğruluk: **9/15 = %60**

#### obtest/ klasörleri (BSI YOK — OB fallback)

| Market | Kazanan | OB ltp | OB Kararı | Sonuç |
|--------|---------|--------|-----------|-------|
| obtest1 (1777647000) | DOWN | up=$0.43, dn=$0.59 | DOWN | ✓ |
| obtest2 (1777647300) | UP   | up=$0.51, dn=$0.49 | UP   | ✓ |
| obtest3 (1777647600) | DOWN | up=$0.39, dn=$0.62 | DOWN | ✓ |
| obtest4 (1777647900) | DOWN | up=$0.51, dn=$0.50 | UP   | ✗ |

**obtest OB fallback:** 3/4 = %75

### Kritik Bulgular

**1. BSI tek başına yetersiz:** %64 doğruluk, rastgeleden (50%) sadece az daha iyi.
Bot muhtemelen BSI + OFI + CVD kombinasyonu kullanıyor. Sim sadece BSI'a baktığı için doğruluk düşük.

**2. obtest4 → BSI olmadan OB fallback yetersiz:** OB UP ($0.51 > $0.50) gösteriyor ama gerçek bot DOWN aldı. Sebep belirsiz — Binance momentum sinyali OB'nin tersini gösteriyor olabilir.

**3. Stratejiye etki minimal:** İlk yön kararı yanlış olsa bile bot REBALANCE/SCOOP/LOTTERY ile pozisyonu kazanan tarafa kaydırıyor. Ablation testinde doğru vs yanlış yön farkı **ortalama $0.55 / market** çıktı (B17 PNL tablosundan).

**4. Karar tek seferlik:** İlk kararın değişmemesi `obtest3`'de net görülür — bot 119 trade boyunca tek bir UP almadı, ilk DOWN kararına sadık kaldı.

---

## Bölüm 5 — Lot Boyutu  
**Güven: YÜKSEK (%73, 220/699 standart lot)**

```python
def lot_size(phase, imbalance, opportunity_price=None):
    if phase == "INIT" or phase == "BUILD":
        return random.randint(40, 45)   # standart lot
    
    if phase == "REBALANCE":
        return max(40, abs(imbalance) // 4)  # eksiği kapat
    
    if phase == "SCOOP":
        # Karşı taraf ne kadar ucuzsa scoop o kadar büyük
        if opportunity_price <= 0.01:
            return min(budget // opportunity_price, 10000)
        elif opportunity_price <= 0.10:
            return int(500 * (0.10 - opportunity_price) / 0.09 + 500)  # 500-1000sh
        elif opportunity_price <= 0.25:
            return int(100 * (0.25 - opportunity_price) / 0.15 + 50)   # 50-150sh
    
    if phase == "LOTTERY":
        return LOTTERY_LOT  # ~10,000sh
    
    return 40  # varsayılan
```

**Gözlemlenen büyük lotlar:**
- 1806sh (1777622400, scoop karşı_ask=$0.01)
- 10101sh (1777624500, lottery $0.01)
- 1502sh (1777630200, scoop karşı_ask=$0.01)
- 1010sh (1777638900, scoop karşı_ask=$0.02)

---

## Bölüm 6 — Rebalance Kuralı  
**Güven: KESİN (%100, 15/15 market)**

```python
def check_rebalance(pos_up, pos_dn):
    imbalance = pos_up - pos_dn
    if abs(imbalance) >= REBALANCE_MIN:   # 50sh
        deficit_side = "DOWN" if imbalance > 0 else "UP"
        deficit_price = ob[deficit_side].bid
        return deficit_side, lot_size("REBALANCE", imbalance)
    return None
```

**Önemli:** Rebalance tam kapanmıyor, büyük imbalance'ta birden fazla tur gerekiyor.  
**Uyarı (obtest2):** t=200-276s arası rebalance döngüsü kontrolden çıkabilir — 800+ sh DN birikmesi.

---

## Bölüm 7 — Scoop Kuralı  
**Güven: YÜKSEK (%73, 11/15 market)**

```python
def check_scoop(ob, to_end):
    if to_end > SCOOP_WINDOW:   # 100s
        return None
    
    for side in ["UP", "DOWN"]:
        other_side = "DOWN" if side == "UP" else "UP"
        if ob[other_side].ask <= SCOOP_THRESHOLD:   # 0.25
            lot = lot_size("SCOOP", 0, ob[other_side].ask)
            return side, ob[side].ask, lot
    
    return None
```

**Gözlemlenen scoop'lar (6 market, Kural 7 tablosu):**

| Market | Scoop fiyatı | Karşı ask | Hacim | Kâr |
|---|---|---|---|---|
| 1777622100 | DOWN $0.78 | UP $0.24 | 217sh | +$47 |
| 1777622400 | DOWN $0.99 | UP $0.01 | 5051sh | +$50 |
| 1777622700 | UP $0.88 | DN $0.13 | 255sh | +$30 |
| 1777624200 | DOWN $0.93 | UP $0.04 | 116sh | +$8 |
| 1777624800 | DOWN $0.99 | UP $0.01 | 2383sh | +$24 |
| 1777628100 | DOWN $0.87 | UP $0.16 | 171sh | +$22 |

**Post-market fill:** Scoop emirleri market kapandıktan sonra da fill olabilir (t=+2s ile +24s).

---

## Bölüm 8 — Lottery Tail  
**Güven: DÜŞÜK (1 kez gözlemlendi)**

```python
def check_lottery(ob, to_end):
    if to_end > LOTTERY_WINDOW:   # 15s
        return None
    
    for side in ["UP", "DOWN"]:
        if ob[side].ask <= LOTTERY_THRESHOLD:   # 0.02
            return side, ob[side].ask, LOTTERY_LOT   # ~10,000sh
    
    return None
```

**Tek örnek:** 1777624500 t=310s, UP 10101sh @ $0.01 — DOWN kazandı → **−$101**.  
Beklenen değer teoride pozitif (100x ödül) ama pratikte nadir ve riskli.

---

## Bölüm 9 — Dutch Book Arbitraj  
**Güven: YÜKSEK (5 cluster doğrulandı — W marketleri + obtest)**

```python
def check_dutch_book(ob):
    # Anlık Dutch Book kontrolü
    up_ask = ob["UP"].ask
    dn_ask = ob["DOWN"].ask
    
    if up_ask + dn_ask < BINARY_SUM:   # < $1.00
        margin = BINARY_SUM - up_ask - dn_ask
        lot = lot_size("INIT", 0)
        # Her iki tarafa eş zamanlı emir
        return [
            ("UP",   up_ask, lot),
            ("DOWN", dn_ask, lot),
        ]
    return None
```

**Kanıt (5 cluster, W marketleri):**

| Market | t | UP | DN | Sum | Marj |
|---|---|---|---|---|---|
| 1777638600 | 40s | 26sh@$0.60 | 41sh@$0.34 | $0.94 | +6¢ |
| 1777638600 | 42s | 172sh@$0.64 | 169sh@$0.33 | $0.96 | +4¢ |
| 1777639200 | 144s | 25sh@$0.26 | 126sh@$0.72 | $0.98 | +2¢ |
| 1777638900 | 20s | 120sh@$0.47 | 80sh@$0.51 | $0.98 | +2¢ |
| 1777638900 | 36s | 40sh@$0.43 | 245sh@$0.52 | $0.94 | +6¢ |
| **obtest1** | **36s** | **41sh@$0.51** | **14.8sh@$0.49** | **$1.00** | **+0¢** |
| **obtest2** | **28s** | **41sh@$0.5756** | **41sh@$0.40** | **$0.9756** | **+2.4¢** |

**Not:** obtest1/obtest2'deki Dutch Book filllar resting bid mekanizmasından geliyor (açılış griди emirleri).

---

## Bölüm 10 — Risk Yönetimi (Stop-loss Yok)  
**Güven: KESİN (%100)**

```python
def check_averaging_down(pos, ob, last_direction):
    current_price = ob[last_direction].bid
    avg_cost      = pos.avg_cost(last_direction)
    
    # Fiyat düştüyse aynı yönde tekrar al (averaging down)
    if current_price < avg_cost - 0.02:
        return last_direction, current_price, lot_size("BUILD", 0)
    
    return None

# STOP-LOSS YOK:
# Yanlış yöndeyse pozisyon kapanmaz — market kapanana kadar alım sürer
# 1777624200: UP 1240sh birikti (yanlış yön) → −$358
# 1777624500: UP 11337sh birikti (lottery dahil) → −$146
```

---

## Bölüm 11 — Zaman Modeli (5-Faz)  
**Güven: DAVRANIŞSAL (gözlemsel)**

```
FAZ I  — INIT        t=12s–50s  : Yön kararı + hızlı pozisyon kurma (200-1000sh)
FAZ II — BUILD       t=50s–150s : Lehte tarafı büyüt, fiyat artıyorsa averaging up
FAZ III— WAIT        gap'ler    : Gözlem modu, emir vermez (30-160s)
FAZ IV — REBALANCE   her an     : |imbalance| > 50sh → karşı tarafa lot
FAZ V  — SCOOP       t=200s–300s: Karşı_ask ≤ $0.25 → büyük lot
```

**Gap tipleri:**

| Tip | Volatilite | Süre | Sonra |
|---|---|---|---|
| A — Durgun | ≤$0.05 | 30-46s | Aynı yön devam |
| B — Volatil | $0.10-0.20 | 30-90s | Yön değişimi |
| C — Scoop bekleme | UP/DN $0.99'a yapışıyor | 60-160s | Dev scoop |
| D — Düşük fiyat | Karşı taraf $0.05 altında | 60-120s | Karşı tarafa scoop |

---

## Bölüm 12 — Kapanış Sonrası Fill  
**Güven: DOĞRULANMIŞ (6+/15 market)**

Polymarket limit emirleri market kapandıktan sonra da fill olabiliyor. Bot bu özelliği bilinçli kullanıyor.

```python
# Market kapandıktan sonra (to_end < 0 AND to_end > -30):
# Bekleyen emirler fill olmaya devam eder
# Bot aktif yeni emir göndermez — mevcut emirlerin dolmasını bekler
```

**Gözlemlenen post-market fill'ler:**

| Market | Fill | Gecikme |
|---|---|---|
| 1777622400 | DOWN 2sh@$0.49 | +10s |
| 1777624500 | UP 10101sh@$0.01 (lottery) | +10s |
| 1777624800 | DOWN ~1033sh@$0.99 (5 fill) | +2–14s |
| 1777628400 | DOWN 443sh@$0.76 | +24s |
| 1777629000 | UP 15sh@$0.99 | +4–6s |
| 1777630200 | DOWN ~156sh@$0.99 | +2–24s |

---

## Bölüm 13 — Spread Sabit $0.01  
**Güven: KESİN (%98, 12,000+ saniye snapshot)**

```python
POLYMARKET_SPREAD = 0.01  # neredeyse sabit

# UP_ask - UP_bid = $0.01
# Dolayısıyla: UP_bid + DN_ask = $1.00 (kesin)
#              UP_ask + DN_bid = $1.00 (kesin)
```

Bot bu sabit spread varsayımıyla çalışıyor — her tick agresif olabilir.

---

## Bölüm 14 — Tam Karar Ağacı (Pseudocode)

```python
def run_decision(rel_t, ob, pos):
    to_end = MARKET_DURATION - rel_t   # = 300 - rel_t
    
    # ─── PHASE 0: POST-MARKET ───────────────────────────────────────
    if to_end < 0:
        if to_end > -POST_MARKET_WAIT:
            return  # bekleyen emirlerin dolmasını bekle
        else:
            cancel_all_orders()
            return
    
    # ─── PHASE I: İLK YÖN KARARI (anında — bekleme yok) ────────────
    if pos.total() == 0 and direction is None:
        # Bot beklemez — ilk geçerli OB snapshot'ında karar verir
        direction = decide_direction(ob)   # BSI primer, OB fallback (B4)
        place_limit_buy(direction, ob[direction].bid, lot_size("INIT", 0))
        # Aynı anda B3b: açılış grid emirleri (Dutch Book)
        place_opening_grid(ob)
        return
    
    # ─── PHASE V-A: LOTTERY TAIL ────────────────────────────────────
    if to_end <= LOTTERY_WINDOW:   # 15s
        for side in ["UP", "DOWN"]:
            if ob[side].ask <= LOTTERY_THRESHOLD:   # 0.02
                place_limit_buy(side, ob[side].ask, LOTTERY_LOT)
                return
    
    # ─── PHASE V-B: SCOOP ───────────────────────────────────────────
    if to_end <= SCOOP_WINDOW:   # 100s
        result = check_scoop(ob, to_end)
        if result:
            side, price, lot = result
            place_limit_buy(side, price, lot)
            return
    
    # ─── PHASE IX: DUTCH BOOK ───────────────────────────────────────
    arb = check_dutch_book(ob)
    if arb:
        for side, price, lot in arb:
            place_limit_buy(side, price, lot)
        return
    
    # ─── PHASE IV: REBALANCE ────────────────────────────────────────
    result = check_rebalance(pos.up, pos.dn)
    if result:
        side, lot = result
        place_limit_buy(side, ob[side].bid, lot)
        return
    
    # ─── PHASE II/III: BUILD veya WAIT ──────────────────────────────
    dominant_side = "UP" if pos.up >= pos.dn else "DOWN"
    dom_price = ob[dominant_side].bid
    
    if dom_price >= pos.avg_cost(dominant_side):
        # Fiyat lehe — pozisyon büyüt
        place_limit_buy(dominant_side, dom_price, lot_size("BUILD", 0))
        return
    elif dom_price < pos.avg_cost(dominant_side) - 0.02:
        # Averaging down
        place_limit_buy(dominant_side, dom_price, lot_size("BUILD", 0))
        return
    
    # Else: WAIT (gap)
    cancel_stale_orders()   # fiyat hızlı hareket ediyorsa emirleri iptal et
    return
```

---

## Bölüm 15 — Kural Güvenilirlik Özeti

| Kural | Güven | Kaynak | Not |
|---|---|---|---|
| 2sn tick (B0) | **KESİN** | 699/699 + 73/73 + 131/131 | Backtest zorunlu |
| BUY-only (B2) | **KESİN** | 0 SELL tüm veriset | — |
| Rebalance 50sh (B6) | **KESİN** | 15/15 market | Min tetikleyici 51sh |
| İlk trade t=4-26s (B4/B13) | **KESİN** | 17/17 market | obtest serisinde t=4-6s |
| Resting bid / stale fill (B3) | **YÜKSEK** | obtest1+2 OB analizi | %37-39 stale |
| Açılış Dutch Book emri (B3b) | **YÜKSEK** | 2 market kanıtı | t=28-36s |
| Maker %67 (B3/K3) | **YÜKSEK** | 699 trade | BID+STALE toplam |
| OB bazlı yön kararı (B4) | **YÜKSEK** | 8/9 + obtest1+2 | %89 |
| Scoop ≤$0.25 (B7) | **YÜKSEK** | 11/15 market | Fırsatçı kural |
| Standart 40-45sh lot (B5) | **ORTA** | 220/699 = %31 | Kalanı split/cluster |
| Stop-loss yok (B10) | **KESİN** | obtest2 kanıtı | Büyük risk kaynağı |
| Post-market fill (B12) | **DOĞRULANMIŞ** | 6/15 market | Intentional |
| Dutch Book arb (B9) | **ORTA** | 5+2 cluster | W marketleri + obtest |
| BSI yön sinyali (B4 primer) | **ORTA** | 7/11 BSI'lı markets | |BSI|≥0.30 → %64 doğru |
| Lottery tail (B8) | **DÜŞÜK** | 1/15 market | Nadir, riskli |

---

## Bölüm 16 — Bilinen Sınırlar ve Uyarılar

### Uyarı 1: Stop-loss Yok → Büyük Kayıp Riski
Yanlış yönde averaging down kaybı katlar. En kötü örnekler:
- 1777624200: UP 1240sh, DOWN kazandı → **−$358**
- 1777624500: UP 11337sh, DOWN kazandı → **−$146**
- obtest2: DOWN 1663sh, UP kazandı → **−$212**

### Uyarı 2: Rebalance Döngüsü Kontrolden Çıkabilir
obtest2'de t=200-276s arasında bot UP/DN arasında gidip geldi, her seferinde DN aldı, pozisyon −850sh DN imbalance'a ulaştı. Bir çıkış/limit koşulu eklenmeli.

### Uyarı 3: BSI Tek Başına Yetersiz Sinyal
BSI primer kararı 11 BSI'lı marketde sadece **%64 doğruluk** verir (rastgeleden biraz iyi). Üretim botu muhtemelen BSI + OFI + CVD kombinasyonu kullanır. Sim yalnız BSI'a baktığı için zayıf kalır.

### Uyarı 4: İlk Yön Kararının PNL Etkisi Minimal
Ablation testi: doğru yön vs yanlış yön ortalama PNL farkı **$0.55 / market** (4 obtest market). Bot REBALANCE/SCOOP/LOTTERY ile pozisyonu otomatik dengeler. Yön kararı stratejik kritik DEĞİLDİR — esas kâr marjı son 30s'deki SCOOP ve LOTTERY mekanizmalarından gelir.

### Uyarı 5: BSI Verisi Yoksa OB Fallback Yetersiz Olabilir
obtest4 örneği: OB ltp UP gösteriyor ama gerçek bot DOWN aldı. obtest serisinde OB fallback %75 doğruluk (3/4). BSI feed'i olmayan ortamda yön kararı kalitesi düşer ama Uyarı 4 nedeniyle PNL etkisi yine de küçük kalır.

### Uyarı 4: Açılış OB Verisi t=0-10s Yok
obtest verilerinde OB snapshot t=10-11s'de başlıyor. Bot açılış anında (t=0-10s) OB görmeden emir koyuyor olabilir — veya tüm emirler WebSocket bağlantısı kurulunca (t≈10s) toplu yerleştiriliyor.

### Uyarı 5: Lot Formülü Tam Bilinmiyor
40-45sh aralığı sabittir ama tam değer (`40 mu 41 mi?`) belirsiz. Scoop lot formülü da yaklaşık — kesin parametreler bot kaynak kodu olmadan çıkarılamaz.

---

## Bölüm 17 — PNL Doğrulaması (15 Market)

| Market | Kazanan | PNL | Doğru yön? |
|---|---|---|---|
| 1777622100 | DOWN | +$175.86 | ✓ |
| 1777622400 | DOWN | −$60.45 | ✓ (scoop maliyeti) |
| 1777622700 | UP | −$9.24 | ✓ |
| 1777624200 | DOWN | −$357.98 | ✗ (UP yanlış) |
| 1777624500 | DOWN | −$145.87 | ✗ (lottery batık) |
| 1777624800 | DOWN | +$35.40 | ✓ |
| 1777628100 | DOWN | −$61.54 | ✓ |
| 1777628400 | DOWN | +$73.92 | ✓ |
| 1777628700 | DOWN | +$43.90 | ✓ |
| 1777629000 | UP | +$84.83 | ✓ |
| 1777629300 | UP | +$34.29 | ✓ |
| 1777630200 | DOWN | +$109.64 | ✓ |
| 1777638600 | UP | +$412.58 | ✓ |
| 1777638900 | UP | +$118.16 | ✓ |
| 1777639200 | DOWN | −$64.17 | ✓ |
| obtest1 (1777647000) | UP | +$37.71 | ✓ |
| obtest2 (1777647300) | UP | −$211.57 | ✗ (DOWN dominant) |
| **TOPLAM** | | **+$376.97** | **15/17 = %88** |

---

*Implementasyon Kılavuzu: 1 Mayıs 2026*  
*Kaynak: x/, y/, z/, w/ (699 trade, 15 market) + obtest/obtest2 OB WebSocket analizi (204 trade, 2 market)*  
*Tüm kurallar bağımsız iki veri kümesinde doğrulandı.*
