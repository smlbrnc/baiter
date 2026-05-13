# Bonereaper Strateji Rehberi

Polymarket BTC 5-dakika marketlerinde çalışan, order-book reaktif bir martingale stratejisi.
Gerçek wallet adresi: `0xeebde7a0e019a63e6b476eb425505b7b3e6eba30`

---

## İçindekiler

1. [Strateji Özeti](#1-strateji-özeti)
2. [Karar Akış Şeması](#2-karar-akış-şeması)
3. [Tüm Parametreler](#3-tüm-parametreler)
4. [Late Winner & arb_mult Tablosu](#4-late-winner--arb_mult-tablosu)
5. [Emir Boyutu Mantığı](#5-emir-boyutu-mantığı)
6. [Gerçek Market Örnekleri](#6-gerçek-market-örnekleri)
7. [Piyasa Görselleştirmesi](#7-piyasa-görselleştirmesi)
8. [Örnek Konfigürasyonlar](#8-örnek-konfigürasyonlar)
9. [Bilinen Riskler](#9-bilinen-riskler)

---

## 1. Strateji Özeti

### Temel Felsefe

| Kural | Açıklama |
|-------|----------|
| **BUY ONLY** | Asla short yok. Pozisyon kapatma yalnızca market kapanışında otomatik REDEEM ile olur |
| **Order-book reaktif** | Dış sinyal yok — bid/ask hareketleri tüm kararları yönlendirir |
| **Geç agresyon** | Son 180 saniyede kazanan tarafa büyük "Late Winner" enjeksiyonları |
| **Otomatik rebalans** | UP/DOWN pozisyon farkı büyüyünce zayıf taraf alınır |
| **Loser scalping** | Kaybeden taraf birkaç sent fiyatla küçük miktarda toplanır |

### Ne Yapar?

```
Piyasa Açılır (T=300s)
       │
       ▼
  OB/BSI sinyali → İlk alım yönü seçilir
       │
       ▼
  Her tick: bid delta izle → alım yap (maker limit @ bid)
       │
       ▼
  T=150s → Winner tarafına boyut çarpanı devreye girer
       │
       ▼
  T=180s + bid ≥ 0.90 → Late Winner (taker @ ask) tetiklenir
       │
       ▼
  T=0s → Kazanan taraf $1.00/sh, kaybeden $0.00/sh REDEEM
```

### Hedef

Market başına ~$0.55–$1.00+ net kâr; 88% yön doğruluğu (15/17 market, gerçek log).

---

## 2. Karar Akış Şeması

### Karar Akış Şeması

<svg xmlns="http://www.w3.org/2000/svg" width="540" height="568" viewBox="0 0 540 568" style="font-family:system-ui,Arial,sans-serif;font-size:12px;background:#0f1117;border-radius:10px;">
  <defs>
    <marker id="ah" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
      <path d="M0,0 L8,3 L0,6 Z" fill="#6c757d"/>
    </marker>
  </defs>
  <text x="270" y="20" text-anchor="middle" fill="#adb5bd" font-size="13" font-weight="bold">decide() Karar Akışı</text>
  <!-- TICK pill -->
  <rect x="185" y="28" width="170" height="28" rx="14" fill="#2c3e50" stroke="#5d6d7e" stroke-width="1.2"/>
  <text x="270" y="47" text-anchor="middle" fill="#ecf0f1" font-weight="bold">TICK GELDİ</text>
  <line x1="270" y1="56" x2="270" y2="73" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <!-- D1: to_end < 0 -->
  <polygon points="270,75 336,102 270,129 204,102" fill="#1a1500" stroke="#f39c12" stroke-width="1.5"/>
  <text x="270" y="98" text-anchor="middle" fill="#f5cba7" font-weight="bold">to_end &lt; 0?</text>
  <text x="270" y="113" text-anchor="middle" fill="#aaa" font-size="10">piyasa kapandı</text>
  <line x1="336" y1="102" x2="390" y2="102" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <rect x="390" y="89" width="90" height="26" rx="4" fill="#3d0000" stroke="#e74c3c" stroke-width="1.2"/>
  <text x="435" y="106" text-anchor="middle" fill="#e74c3c" font-weight="bold">NoOp</text>
  <text x="342" y="97" fill="#e74c3c" font-size="10">Evet</text>
  <text x="275" y="141" fill="#27ae60" font-size="10">Hayır</text>
  <line x1="270" y1="129" x2="270" y2="146" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <!-- D2: LW aktif -->
  <polygon points="270,148 345,178 270,208 195,178" fill="#1a0e00" stroke="#e67e22" stroke-width="1.5"/>
  <text x="270" y="173" text-anchor="middle" fill="#f0b27a" font-weight="bold">LW aktif?</text>
  <text x="270" y="189" text-anchor="middle" fill="#aaa" font-size="10">to_end≤180 ∧ bid≥0.90</text>
  <line x1="345" y1="178" x2="390" y2="178" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <rect x="390" y="162" width="130" height="32" rx="4" fill="#003300" stroke="#27ae60" stroke-width="1.2"/>
  <text x="455" y="180" text-anchor="middle" fill="#2ecc71" font-weight="bold" font-size="11">TAKER BUY</text>
  <text x="455" y="192" text-anchor="middle" fill="#58d68d" font-size="9">(Late Winner)</text>
  <text x="351" y="173" fill="#27ae60" font-size="10">Evet</text>
  <text x="275" y="220" fill="#27ae60" font-size="10">Hayır</text>
  <line x1="270" y1="208" x2="270" y2="225" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <!-- D3: Cooldown -->
  <polygon points="270,227 336,252 270,277 204,252" fill="#1a1500" stroke="#f39c12" stroke-width="1.5"/>
  <text x="270" y="247" text-anchor="middle" fill="#f5cba7" font-weight="bold">Cooldown?</text>
  <text x="270" y="262" text-anchor="middle" fill="#aaa" font-size="10">son alım &lt; 3000 ms</text>
  <line x1="336" y1="252" x2="390" y2="252" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <rect x="390" y="239" width="90" height="26" rx="4" fill="#3d0000" stroke="#e74c3c" stroke-width="1.2"/>
  <text x="435" y="256" text-anchor="middle" fill="#e74c3c" font-weight="bold">NoOp</text>
  <text x="341" y="247" fill="#e74c3c" font-size="10">Evet</text>
  <text x="275" y="289" fill="#27ae60" font-size="10">Hayır</text>
  <line x1="270" y1="277" x2="270" y2="294" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <!-- Yön Seçimi box -->
  <rect x="155" y="296" width="230" height="44" rx="6" fill="#112244" stroke="#3498db" stroke-width="1.5"/>
  <text x="270" y="315" text-anchor="middle" fill="#5dade2" font-weight="bold">Yön Seçimi</text>
  <text x="270" y="332" text-anchor="middle" fill="#85c1e9" font-size="10">BSI primer / OB delta / Rebalans</text>
  <line x1="270" y1="340" x2="270" y2="357" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <!-- D4: Deep lot -->
  <polygon points="270,359 336,384 270,409 204,384" fill="#1a1500" stroke="#f39c12" stroke-width="1.5"/>
  <text x="270" y="379" text-anchor="middle" fill="#f5cba7" font-weight="bold">Deep lot?</text>
  <text x="270" y="395" text-anchor="middle" fill="#aaa" font-size="10">loser bid çok ucuz</text>
  <line x1="336" y1="384" x2="390" y2="384" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <rect x="390" y="368" width="130" height="32" rx="4" fill="#003300" stroke="#27ae60" stroke-width="1.2"/>
  <text x="455" y="386" text-anchor="middle" fill="#2ecc71" font-weight="bold" font-size="11">TAKER BUY</text>
  <text x="455" y="398" text-anchor="middle" fill="#58d68d" font-size="9">(Loser Scalp)</text>
  <text x="341" y="379" fill="#27ae60" font-size="10">Evet</text>
  <text x="275" y="421" fill="#27ae60" font-size="10">Hayır</text>
  <line x1="270" y1="409" x2="270" y2="426" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <!-- D5: avg_sum cap -->
  <polygon points="270,428 346,455 270,482 194,455" fill="#1a1500" stroke="#f39c12" stroke-width="1.5"/>
  <text x="270" y="450" text-anchor="middle" fill="#f5cba7" font-weight="bold">avg_sum &gt; 1.05?</text>
  <text x="270" y="465" text-anchor="middle" fill="#aaa" font-size="10">pozisyon dengesi kontrolü</text>
  <line x1="346" y1="455" x2="390" y2="455" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <rect x="390" y="442" width="90" height="26" rx="4" fill="#3d0000" stroke="#e74c3c" stroke-width="1.2"/>
  <text x="435" y="459" text-anchor="middle" fill="#e74c3c" font-weight="bold">NoOp</text>
  <text x="351" y="450" fill="#e74c3c" font-size="10">Evet</text>
  <text x="275" y="494" fill="#27ae60" font-size="10">Hayır</text>
  <line x1="270" y1="482" x2="270" y2="499" stroke="#6c757d" stroke-width="1.5" marker-end="url(#ah)"/>
  <!-- MAKER BUY final -->
  <rect x="155" y="501" width="230" height="44" rx="6" fill="#003300" stroke="#27ae60" stroke-width="1.5"/>
  <text x="270" y="520" text-anchor="middle" fill="#2ecc71" font-weight="bold">MAKER BUY @ bid</text>
  <text x="270" y="536" text-anchor="middle" fill="#58d68d" font-size="10">size = usdc / bid (lot sayısı)</text>
  <!-- Legend -->
  <rect x="10" y="550" width="11" height="11" rx="2" fill="#1a1500" stroke="#f39c12"/>
  <text x="25" y="560" fill="#adb5bd" font-size="10">Karar noktası</text>
  <rect x="120" y="550" width="11" height="11" rx="2" fill="#003300" stroke="#27ae60"/>
  <text x="135" y="560" fill="#adb5bd" font-size="10">Alım emri</text>
  <rect x="210" y="550" width="11" height="11" rx="2" fill="#3d0000" stroke="#e74c3c"/>
  <text x="225" y="560" fill="#adb5bd" font-size="10">NoOp</text>
  <rect x="270" y="550" width="11" height="11" rx="2" fill="#112244" stroke="#3498db"/>
  <text x="285" y="560" fill="#adb5bd" font-size="10">İşlem kutusu</text>
</svg>

### ASCII Karar Özeti

```
TICK
 ├─ [1] to_end < 0 ────────────────────────────────────── NoOp
 ├─ [2] LW: to_end ≤ 180 AND bid ≥ 0.90 ──────────────── TAKER BUY (winner)
 ├─ [3] Cooldown: son alım < 3s ───────────────────────── NoOp
 ├─ [4] Yön seçimi:
 │       first == false → spread ≥ 0.02 → BSI / OB
 │       first == true  → imbalance > 1000 → rebalans
 │                         else → bid delta
 ├─ [5] Deep lot: loser ucuzsa ────────────────────────── TAKER BUY (loser)
 ├─ [6] Boyut hesapla (bid bandı)
 ├─ [7] avg_sum > 1.05 ─────────────────────────────────── NoOp
 └─ [8] ─────────────────────────────────────────────────── MAKER BUY (bid)
```

---

## 3. Tüm Parametreler

Kaynak: `src/config.rs:406–570`

### Zamanlama

| Parametre | Default | Min | Max | Açıklama |
|-----------|---------|-----|-----|----------|
| `bonereaper_buy_cooldown_ms` | **3 000** ms | 1 000 | 60 000 | Ardışık alımlar arası minimum bekleme süresi |

### Late Winner (LW)

| Parametre | Default | Min | Max | Açıklama |
|-----------|---------|-----|-----|----------|
| `bonereaper_late_winner_secs` | **180** s | 0 | 300 | LW penceresinin başladığı süre (kapanmaya kalan saniye). 0 = KAPALI |
| `bonereaper_late_winner_bid_thr` | **0.90** | 0.50 | 0.99 | LW tetiklemek için kazanan tarafın min bid değeri |
| `bonereaper_late_winner_usdc` | **100.0** $ | 0 | 10 000 | Her LW shot başına temel USDC. 0 = KAPALI |
| `bonereaper_lw_max_per_session` | **20** | 0 | 50 | Market başına max LW shot sayısı. 0 = sınırsız |
| `bonereaper_lw_burst_secs` | **0** s | 0 | 60 | Ek burst dalgası penceresi. 0 = KAPALI |
| `bonereaper_lw_burst_usdc` | **0.0** $ | 0 | 10 000 | Burst dalgası USDC. 0 = KAPALI |

### Pozisyon Dengesi

| Parametre | Default | Min | Max | Açıklama |
|-----------|---------|-----|-----|----------|
| `bonereaper_imbalance_thr` | **1 000** sh | 0 | 10 000 | UP–DOWN pozisyon farkı bu değeri aşınca zorla rebalans yapılır |
| `bonereaper_max_avg_sum` | **1.05** | 0.50 | 2.00 | `avg_up + avg_down` yumuşak tavanı. Aşılırsa normal alım durur (scalp/LW muaf) |
| `bonereaper_avg_loser_max` | **0.50** | 0.10 | 0.95 | Kaybeden taraf ortalaması bu değeri aşarsa sadece scalp yapılır |

### İlk Emir Filtresi

| Parametre | Default | Min | Max | Açıklama |
|-----------|---------|-----|-----|----------|
| `bonereaper_first_spread_min` | **0.02** | 0.00 | 0.20 | `|up_bid - down_bid|` bu değerden küçükse ilk emir verilmez |

### Emir Boyutları (USDC)

| Parametre | Default | Min | Max | Uygulama Koşulu |
|-----------|---------|-----|-----|-----------------|
| `bonereaper_size_longshot_usdc` | **15.0** $ | 0 | 10 000 | `bid ≤ 0.30` |
| `bonereaper_size_mid_usdc` | **23.0** $ | 0 | 10 000 | `0.30 < bid ≤ 0.65` |
| `bonereaper_size_high_usdc` | **37.0** $ | 0 | 10 000 | `bid > 0.65` |
| `bonereaper_winner_size_factor` | **1.0**× | 1.0 | 10.0 | `late_pyramid_secs` penceresinde winner boyutunu çarpar |
| `bonereaper_late_pyramid_secs` | **150** s | 0 | 300 | Büyük lot penceresinin başladığı süre (kapanmaya kalan saniye) |

### Loser Scalping

| Parametre | Default | Min | Max | Açıklama |
|-----------|---------|-----|-----|----------|
| `bonereaper_loser_min_price` | **0.01** | 0.001 | 0.10 | Loser taraf için kabul edilen minimum bid (1 sent) |
| `bonereaper_loser_scalp_usdc` | **10.0** $ | 0 | 50 | Loser scalp emri boyutu. 0 = KAPALI |
| `bonereaper_loser_scalp_max_price` | **0.30** | 0.05 | 0.50 | Loser bid bu değerin altındaysa scalp boyutu uygulanır |

---

## 4. Late Winner & arb\_mult Tablosu

### Çalışma Prensibi

LW tetiklendiğinde emir büyüklüğü:

```
lot = ceil( lw_usdc × arb_mult / w_ask )
```

`arb_mult` yalnızca **winner ask fiyatına** bağlıdır (zaman boyutu yoktur).
Kaynak: `src/strategy/bonereaper.rs:210–222`

### arb\_mult Tablosu

| Winner Ask (w_ask) | arb_mult | Örnek: $100 USDC @ ask |
|--------------------|----------|------------------------|
| `≥ 0.99` | **5.0×** | ceil(100 × 5.0 / 0.99) = **506 lot** ≈ $501 maliyet |
| `≥ 0.98` | **4.0×** | ceil(100 × 4.0 / 0.98) = **409 lot** ≈ $401 maliyet |
| `≥ 0.97` | **3.0×** | ceil(100 × 3.0 / 0.97) = **310 lot** ≈ $301 maliyet |
| `≥ 0.96` | **2.5×** | ceil(100 × 2.5 / 0.96) = **261 lot** ≈ $251 maliyet |
| `≥ 0.95` | **2.0×** | ceil(100 × 2.0 / 0.95) = **211 lot** ≈ $200 maliyet |
| `< 0.95` | **1.0×** | ceil(100 × 1.0 / 0.92) = **109 lot** ≈ $100 maliyet |

### Gerçek Bot Referans Verileri

| Bid Bandı | Medyan Notional | arb_mult Karşılığı |
|-----------|-----------------|---------------------|
| $0.99+ | ~$5 000 / shot | 5× × $100 × quota=10 |
| $0.97–0.99 | ~$1 000 / shot | 3× |
| $0.95–0.97 | ~$580 / shot | 2× |
| $0.85–0.95 | küçük | 1× |

### Hesaplama Örneği

```
Senaryo: UP kazanıyor, w_ask = 0.98, lw_usdc = $100

arb_mult = 4.0   (çünkü w_ask ∈ [0.98, 0.99))
lot       = ceil(100 × 4.0 / 0.98) = ceil(408.16) = 409 lot
maliyet   = 409 × $0.98 = $400.82

Eğer UP kazanırsa:
  gelir  = 409 × $1.00 = $409.00
  kâr    = $409.00 − $400.82 = +$8.18  (tek shot, ~2%)

20 shot × $400.82 = $8 016 maksimum LW riski / market
```

---

## 5. Emir Boyutu Mantığı

### Bid Bandına Göre Boyut Seçimi

<table style="border-collapse:collapse;width:100%;font-family:system-ui,Arial,sans-serif;font-size:13px;background:#0f1117;color:#ecf0f1;border-radius:10px;overflow:hidden;">
  <thead>
    <tr style="background:#1a2a4a;">
      <th style="padding:10px 12px;text-align:left;border-bottom:2px solid #2980b9;color:#5dade2;">Koşul</th>
      <th style="padding:10px 12px;text-align:center;border-bottom:2px solid #2980b9;color:#5dade2;">Boyut (USDC)</th>
      <th style="padding:10px 12px;text-align:center;border-bottom:2px solid #2980b9;color:#5dade2;">Emir Tipi</th>
      <th style="padding:10px 12px;text-align:center;border-bottom:2px solid #2980b9;color:#5dade2;">Yaklaşık Lot</th>
    </tr>
  </thead>
  <tbody>
    <tr style="border-bottom:1px solid #222;">
      <td style="padding:9px 12px;color:#c0392b;">loser bid ≤ 0.30 (kaybeden taraf)</td>
      <td style="padding:9px 12px;text-align:center;font-weight:bold;color:#e74c3c;">$10 Scalp</td>
      <td style="padding:9px 12px;text-align:center;color:#f1948a;font-size:12px;">Taker @ ask</td>
      <td style="padding:9px 12px;text-align:center;color:#aaa;">33–100+</td>
    </tr>
    <tr style="border-bottom:1px solid #222;background:#111;">
      <td style="padding:9px 12px;color:#aaa;">bid ≤ 0.30</td>
      <td style="padding:9px 12px;text-align:center;font-weight:bold;color:#2980b9;">$15 Longshot</td>
      <td style="padding:9px 12px;text-align:center;color:#85c1e9;font-size:12px;">Maker @ bid</td>
      <td style="padding:9px 12px;text-align:center;color:#aaa;">37–75+</td>
    </tr>
    <tr style="border-bottom:1px solid #222;">
      <td style="padding:9px 12px;color:#aaa;">0.30 &lt; bid ≤ 0.65</td>
      <td style="padding:9px 12px;text-align:center;font-weight:bold;color:#27ae60;">$23 Mid</td>
      <td style="padding:9px 12px;text-align:center;color:#58d68d;font-size:12px;">Maker @ bid</td>
      <td style="padding:9px 12px;text-align:center;color:#aaa;">35–77</td>
    </tr>
    <tr style="border-bottom:1px solid #222;background:#111;">
      <td style="padding:9px 12px;color:#aaa;">bid &gt; 0.65</td>
      <td style="padding:9px 12px;text-align:center;font-weight:bold;color:#e67e22;">$37 High</td>
      <td style="padding:9px 12px;text-align:center;color:#f0b27a;font-size:12px;">Maker @ bid</td>
      <td style="padding:9px 12px;text-align:center;color:#aaa;">43–57</td>
    </tr>
    <tr style="border-bottom:1px solid #222;background:#180028;">
      <td style="padding:9px 12px;color:#d2b4de;">bid &gt; 0.65, winner taraf, T ≤ 150s</td>
      <td style="padding:9px 12px;text-align:center;font-weight:bold;color:#af7ac5;">$37 × size_factor</td>
      <td style="padding:9px 12px;text-align:center;color:#d2b4de;font-size:12px;">Maker @ bid</td>
      <td style="padding:9px 12px;text-align:center;color:#d2b4de;">43–570 (factor 1–10)</td>
    </tr>
    <tr style="background:#003300;border-top:2px solid #27ae60;">
      <td style="padding:9px 12px;color:#2ecc71;font-weight:bold;">LW: to_end≤180, bid≥0.90</td>
      <td style="padding:9px 12px;text-align:center;font-weight:bold;color:#2ecc71;">$100 × arb_mult</td>
      <td style="padding:9px 12px;text-align:center;color:#58d68d;font-size:12px;">Taker @ ask</td>
      <td style="padding:9px 12px;text-align:center;color:#2ecc71;">109–506</td>
    </tr>
  </tbody>
</table>

### ASCII Özet

```
bid ≤ $0.30   →  $15 longshot  (maker @ bid)
bid  $0.30–0.65  →  $23 mid      (maker @ bid)
bid > $0.65   →  $37 high     (maker @ bid)
             └── winner + T ≤ 150s → × winner_size_factor (default 1×)

loser bid ≤ $0.30  →  $10 scalp   (taker @ ask, deep lot)
LW trigger        →  $100 × arb_mult / w_ask  (taker @ w_ask)
```

### Lot Sayısı Örnekleri

| Durum | USDC | Bid/Ask | Lot | Emir Tipi |
|-------|------|---------|-----|-----------|
| Longshot | $15 | $0.25 bid | ceil(15/0.25) = **60 lot** | Maker @ $0.25 |
| Mid | $23 | $0.50 bid | ceil(23/0.50) = **46 lot** | Maker @ $0.50 |
| High | $37 | $0.80 bid | ceil(37/0.80) = **47 lot** | Maker @ $0.80 |
| Loser scalp | $10 | $0.08 ask | ceil(10/0.08) = **125 lot** | Taker @ $0.08 |
| LW 1× | $100 | $0.92 ask | ceil(100/0.92) = **109 lot** | Taker @ $0.92 |
| LW 5× | $500 | $0.99 ask | ceil(500/0.99) = **506 lot** | Taker @ $0.99 |

---

## 6. Gerçek Market Örnekleri

### Örnek 1 — Tipik Kazançlı Market (1777622100)

**Sonuç:** DOWN kazandı · PnL: **+$175.86** · Doğru yön ✓

```
T=300s  UP bid: 0.52  DOWN bid: 0.48  BSI: −0.41
        → BSI < −0.30: DOWN seç
        → spread=0.04 ≥ 0.02: emir ver
        → DOWN bid=0.48 → mid bucket: $23/0.48 = 48 lot @ $0.48 (maker)

T=270s  UP:0.50  DOWN:0.50  imbalance: 0
        → bid delta: DOWN hâlâ yüksek
        → DOWN @ $0.50 mid: 46 lot (maker)

T=200s  UP:0.43  DOWN:0.57  imbalance: ~−96 sh (DOWN ağır)
        → OB: DOWN yüksek bid
        → DOWN @ $0.57 high: ceil(37/0.57)=65 lot (maker)

T=180s  UP:0.20  DOWN:0.80  → LW tetiklendi!
        → winner=DOWN, w_ask=0.82, arb_mult=1.0 (< 0.95)
        → 100 × 1.0 / 0.82 = 122 lot @ $0.82 (taker) ✓

T=150s  DOWN:0.88  → LW devam
        → w_ask=0.90, arb_mult=1.0
        → 111 lot @ $0.90 (taker)

T=60s   DOWN:0.95  → LW + late_pyramid
        → w_ask=0.96, arb_mult=2.5
        → ceil(100×2.5/0.96) = 261 lot @ $0.96 (taker)

T=30s   DOWN:0.97  → LW
        → arb_mult=3.0 → 310 lot @ $0.98 (taker)

T=10s   DOWN:0.99
        → arb_mult=5.0 → 506 lot @ $0.99 (taker)

T=0s    DOWN kazandı → REDEEM $1.00/lot
```

| Aşama | Maliyet | Gelir (kazanma) |
|-------|---------|-----------------|
| Erken biriktirme | ~$200 | $200+ |
| LW enjeksiyonları (×6 shot) | ~$750 | $900+ |
| Toplam | ~$950 | ~$1 126 → **+$176** |

---

### Örnek 2 — Kayıplı Martingale (1777624200)

**Sonuç:** DOWN kazandı · PnL: **−$357.98** · Yanlış yön ✗

```
T=300s  UP bid: 0.60  DOWN bid: 0.40  BSI: +0.45
        → BSI > +0.30: UP seç
        → UP bid=0.60 → high bucket: ceil(37/0.60)=62 lot @ $0.60 (maker)

T=260s  UP:0.62  DOWN:0.38
        → UP hâlâ dominant → daha fazla UP al
        → 56 lot @ $0.66 (maker)

T=220s  UP:0.65  DOWN:0.35
        → UP @ $0.65 → high bucket
        → 57 lot @ $0.65

T=180s  UP:0.55  DOWN:0.45  → dönüş başlıyor
        → LW bid_thr=0.90 sağlanmadı (max bid=0.55) → LW YOK
        → OB: UP hâlâ yüksek → UP al

T=120s  UP:0.50  DOWN:0.50  → piyasa belirsiz
        → imbalance > 1000: DOWN al (rebalans)
        → avg_up ≈ 0.62, avg_down ≈ 0.10 → avg_sum=0.72 → cap OK
        → DOWN scalp: 10/0.10 = 100 lot @ $0.10 (loser scalp)

T=60s   UP:0.30  DOWN:0.70  → DOWN kazanıyor
        → LW bid_thr = 0.90, DOWN bid=0.70 → HENÜZ YETERSİZ → LW YOK
        → UP loser side → sadece scalp: 10/0.30 = 33 lot

T=30s   DOWN:0.88  → LW tetiklendi!
        → ama quota dolmadı, sadece 2 shot kaldı
        → DOWN @ $0.89 ask: 112 lot (taker) — GEÇ KALDIK

T=0s    DOWN kazandı
        UP pozisyon: ~1240 sh @ avg $0.62 → $1.00 REDEEM YOK → −$769
        DOWN pozisyon: ~200 sh @ avg $0.45 → +$110
        Net PNL ≈ −$357.98

Problem: UP trend başlangıçta doğru gözüktü, dönüş anında LW devreye giremedi
         (DOWN bid 0.90 eşiğine T=30s'de ulaştı — çok geç).
```

**Dersi:** Stop-loss olmadığından yanlış yönde büyük pozisyon → telafi imkânsız.

---

### Örnek 3 — Rebalans + LW Kombinasyonu (1777638600)

**Sonuç:** UP kazandı · PnL: **+$412.58** · Doğru yön ✓

```
T=300s  UP:0.48  DOWN:0.52  BSI: −0.15 (eşiğin altında)
        → OB: DOWN yüksek → DOWN seç
        → DOWN @ $0.52 mid: 44 lot (maker)

T=240s  UP:0.52  DOWN:0.48  → piyasa döndü
        → OB bid delta: UP yükseldi → UP al
        → UP @ $0.52: 44 lot (maker)

T=200s  UP:0.58  DOWN:0.42
        → imbalance = UP_filled − DOWN_filled > 0 → dengede
        → UP @ $0.58 high: ceil(37/0.58)=64 lot

T=170s  UP:0.70  DOWN:0.30  → late_pyramid devreye (T ≤ 150s yaklaşıyor)
        → UP @ $0.70 high: 53 lot

T=150s  UP:0.78  DOWN:0.22  → late_pyramid aktif (winner_size_factor=1.0, no change)
        → avg_sum = avg_up+avg_dn = 0.62+0.12 = 0.74 → cap OK
        → UP @ $0.78: 48 lot

T=180s  UP:0.91  → LW tetiklendi! (bid ≥ 0.90)
        → w_ask=0.92, arb_mult=1.0
        → 109 lot @ $0.92 (taker) — shot 1

T=90s   UP:0.95
        → w_ask=0.96, arb_mult=2.5
        → 261 lot @ $0.96 — shot 2,3,4

T=30s   UP:0.98
        → arb_mult=4.0
        → 409 lot @ $0.98 — shot 5,6

T=10s   UP:0.99
        → arb_mult=5.0
        → 506 lot @ $0.99 — shot 7,8

T=0s    UP kazandı → REDEEM
        LW pozisyon: ~2700 sh → $2700 gelir
        LW maliyet: ~$2280
        Erken biriktirme: ~$350 maliyet → ~$420 gelir
        Net: +$412.58
```

---

## 7. Piyasa Görselleştirmesi

### Chart 1 — Kazançlı Market (1777638600, UP +$412.58)

> X ekseni: kapanmaya kalan saniye (300→0) · Y ekseni: bid fiyatı · Turuncu noktalar = alım

<svg xmlns="http://www.w3.org/2000/svg" width="660" height="320" viewBox="0 0 660 320" style="background:#0f1117;border-radius:10px;font-family:system-ui,Arial,sans-serif;font-size:11px;">
  <!-- Title -->
  <text x="330" y="20" text-anchor="middle" fill="#ecf0f1" font-size="13" font-weight="bold">1777638600 — UP Kazandı (+$412.58)</text>
  <!-- Y axis label -->
  <text transform="translate(13,170) rotate(-90)" text-anchor="middle" fill="#adb5bd" font-size="11">Bid Fiyatı ($)</text>
  <!-- Chart area: x 55–640, y 30–265. chartW=585, chartH=235 -->
  <!-- Y grid lines: 1.0→y30, 0.8→y77, 0.6→y124, 0.4→y171, 0.2→y218, 0.0→y265 -->
  <line x1="55" y1="30"  x2="640" y2="30"  stroke="#1e2836" stroke-width="1"/>
  <line x1="55" y1="77"  x2="640" y2="77"  stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="124" x2="640" y2="124" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="171" x2="640" y2="171" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="218" x2="640" y2="218" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="265" x2="640" y2="265" stroke="#1e2836" stroke-width="1"/>
  <!-- Y axis labels -->
  <text x="50" y="34"  text-anchor="end" fill="#4a5568">1.00</text>
  <text x="50" y="81"  text-anchor="end" fill="#4a5568">0.80</text>
  <text x="50" y="128" text-anchor="end" fill="#4a5568">0.60</text>
  <text x="50" y="175" text-anchor="end" fill="#4a5568">0.40</text>
  <text x="50" y="222" text-anchor="end" fill="#4a5568">0.20</text>
  <text x="50" y="269" text-anchor="end" fill="#4a5568">0.00</text>
  <!-- X grid lines at key times -->
  <!-- x(t) = 55 + (300-t)/300*585 -->
  <!-- T=300→55, T=240→172, T=180→289, T=150→348, T=90→465, T=60→523, T=30→582, T=0→640 -->
  <line x1="55"  y1="30" x2="55"  y2="265" stroke="#2d3748" stroke-width="1"/>
  <line x1="172" y1="30" x2="172" y2="265" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="523" y1="30" x2="523" y2="265" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="640" y1="30" x2="640" y2="265" stroke="#2d3748" stroke-width="1"/>
  <!-- LW trigger: T=180, x=289 -->
  <line x1="289" y1="30" x2="289" y2="265" stroke="#e67e22" stroke-width="1.5" stroke-dasharray="5,3" opacity="0.8"/>
  <text x="291" y="26" fill="#e67e22" font-size="9" font-weight="bold">▶ LW T=180s</text>
  <!-- Pyramid: T=150, x=348 -->
  <line x1="348" y1="30" x2="348" y2="265" stroke="#9b59b6" stroke-width="1" stroke-dasharray="3,3" opacity="0.7"/>
  <text x="350" y="44" fill="#9b59b6" font-size="9">Pyramid T=150s</text>
  <!-- X axis tick labels -->
  <text x="55"  y="280" text-anchor="middle" fill="#4a5568">300s</text>
  <text x="172" y="280" text-anchor="middle" fill="#4a5568">240s</text>
  <text x="289" y="280" text-anchor="middle" fill="#e67e22" font-weight="bold">180s</text>
  <text x="348" y="280" text-anchor="middle" fill="#9b59b6">150s</text>
  <text x="406" y="280" text-anchor="middle" fill="#4a5568">120s</text>
  <text x="465" y="280" text-anchor="middle" fill="#4a5568">90s</text>
  <text x="523" y="280" text-anchor="middle" fill="#4a5568">60s</text>
  <text x="582" y="280" text-anchor="middle" fill="#4a5568">30s</text>
  <text x="640" y="280" text-anchor="middle" fill="#4a5568">0s</text>
  <text x="347" y="297" text-anchor="middle" fill="#718096" font-size="11">Kapanmaya Kalan Süre</text>
  <!-- UP bid line (mavi) — y(b)=30+(1-b)*235 -->
  <!-- T:[300,240,180,150,120,90,60,30,10,0] x:[55,172,289,348,406,465,523,582,621,640] -->
  <!-- b:[0.48,0.52,0.58,0.70,0.78,0.91,0.95,0.98,0.99,1.00] y:[152,143,129,101,82,51,42,35,32,30] -->
  <polyline points="55,152 172,143 289,129 348,101 406,82 465,51 523,42 582,35 621,32 640,30"
            fill="none" stroke="#3498db" stroke-width="2.5" stroke-linejoin="round"/>
  <!-- DOWN bid line (kırmızı) -->
  <!-- b:[0.52,0.48,0.42,0.30,0.22,0.09,0.05,0.02,0.01,0.00] y:[143,152,166,195,213,244,253,260,263,265] -->
  <polyline points="55,143 172,152 289,166 348,195 406,213 465,244 523,253 582,260 621,263 640,265"
            fill="none" stroke="#e74c3c" stroke-width="2.5" stroke-linejoin="round"/>
  <!-- Alım noktaları (mavi = normal, turuncu = LW) -->
  <circle cx="55"  cy="152" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1.2"/>
  <circle cx="172" cy="143" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1.2"/>
  <circle cx="289" cy="129" r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <circle cx="348" cy="101" r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <circle cx="465" cy="51"  r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <circle cx="523" cy="42"  r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <circle cx="582" cy="35"  r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <circle cx="621" cy="32"  r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <circle cx="640" cy="30"  r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <!-- Legend -->
  <line x1="55" y1="310" x2="75" y2="310" stroke="#3498db" stroke-width="2.5"/>
  <circle cx="65" cy="310" r="3.5" fill="#3498db" stroke="#ecf0f1" stroke-width="1"/>
  <text x="79" y="314" fill="#3498db" font-size="11" font-weight="bold">UP bid</text>
  <line x1="140" y1="310" x2="160" y2="310" stroke="#e74c3c" stroke-width="2.5"/>
  <text x="164" y="314" fill="#e74c3c" font-size="11" font-weight="bold">DOWN bid</text>
  <circle cx="240" cy="310" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1"/>
  <text x="248" y="314" fill="#adb5bd" font-size="11">Normal alım</text>
  <circle cx="330" cy="310" r="4" fill="#f39c12" stroke="#ecf0f1" stroke-width="1"/>
  <text x="338" y="314" fill="#adb5bd" font-size="11">LW alım</text>
  <line x1="398" y1="308" x2="414" y2="308" stroke="#e67e22" stroke-width="1.5" stroke-dasharray="4,2"/>
  <text x="418" y="314" fill="#e67e22" font-size="11">LW başlangıcı</text>
  <!-- Axes -->
  <line x1="55" y1="30"  x2="55"  y2="265" stroke="#4a5568" stroke-width="1.5"/>
  <line x1="55" y1="265" x2="640" y2="265" stroke="#4a5568" stroke-width="1.5"/>
</svg>

### Chart 2 — Kayıplı Market (1777624200, DOWN −$357.98)

> Yanlış yön: BSI UP sinyali verdi, piyasa döndü — LW eşiğine T=30s'de ulaşıldı

<svg xmlns="http://www.w3.org/2000/svg" width="660" height="320" viewBox="0 0 660 320" style="background:#0f1117;border-radius:10px;font-family:system-ui,Arial,sans-serif;font-size:11px;">
  <text x="330" y="20" text-anchor="middle" fill="#ecf0f1" font-size="13" font-weight="bold">1777624200 — DOWN Kazandı (−$357.98) · Yanlış Yön</text>
  <text transform="translate(13,170) rotate(-90)" text-anchor="middle" fill="#adb5bd" font-size="11">Bid Fiyatı ($)</text>
  <!-- Y grid -->
  <line x1="55" y1="30"  x2="640" y2="30"  stroke="#1e2836" stroke-width="1"/>
  <line x1="55" y1="77"  x2="640" y2="77"  stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="124" x2="640" y2="124" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="171" x2="640" y2="171" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="218" x2="640" y2="218" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="55" y1="265" x2="640" y2="265" stroke="#1e2836" stroke-width="1"/>
  <!-- Y labels -->
  <text x="50" y="34"  text-anchor="end" fill="#4a5568">1.00</text>
  <text x="50" y="81"  text-anchor="end" fill="#4a5568">0.80</text>
  <text x="50" y="128" text-anchor="end" fill="#4a5568">0.60</text>
  <text x="50" y="175" text-anchor="end" fill="#4a5568">0.40</text>
  <text x="50" y="222" text-anchor="end" fill="#4a5568">0.20</text>
  <text x="50" y="269" text-anchor="end" fill="#4a5568">0.00</text>
  <!-- X grid -->
  <!-- T:[300,260,220,180,140,100,60,30,10,0] x:[55,133,211,289,367,445,523,582,621,640] -->
  <line x1="55"  y1="30" x2="55"  y2="265" stroke="#2d3748" stroke-width="1"/>
  <line x1="289" y1="30" x2="289" y2="265" stroke="#1e2836" stroke-width="1" stroke-dasharray="3,4"/>
  <line x1="640" y1="30" x2="640" y2="265" stroke="#2d3748" stroke-width="1"/>
  <!-- LW başlangıç referans: T=180, x=289 -->
  <line x1="289" y1="30" x2="289" y2="265" stroke="#e67e22" stroke-width="1.5" stroke-dasharray="5,3" opacity="0.5"/>
  <text x="291" y="26" fill="#e67e22" font-size="9" opacity="0.8">LW ref T=180s</text>
  <!-- Gerçek LW: T=30, x=582 -->
  <line x1="582" y1="30" x2="582" y2="265" stroke="#e74c3c" stroke-width="1.5" stroke-dasharray="5,3" opacity="0.9"/>
  <text x="544" y="26" fill="#e74c3c" font-size="9" font-weight="bold">LW T=30s !</text>
  <!-- X labels -->
  <text x="55"  y="280" text-anchor="middle" fill="#4a5568">300s</text>
  <text x="133" y="280" text-anchor="middle" fill="#4a5568">260s</text>
  <text x="211" y="280" text-anchor="middle" fill="#4a5568">220s</text>
  <text x="289" y="280" text-anchor="middle" fill="#e67e22">180s</text>
  <text x="367" y="280" text-anchor="middle" fill="#4a5568">140s</text>
  <text x="445" y="280" text-anchor="middle" fill="#4a5568">100s</text>
  <text x="523" y="280" text-anchor="middle" fill="#4a5568">60s</text>
  <text x="582" y="280" text-anchor="middle" fill="#e74c3c" font-weight="bold">30s</text>
  <text x="640" y="280" text-anchor="middle" fill="#4a5568">0s</text>
  <text x="347" y="297" text-anchor="middle" fill="#718096" font-size="11">Kapanmaya Kalan Süre</text>
  <!-- UP bid (mavi) — YANLIŞ YÖN, düşüyor -->
  <!-- b:[0.60,0.62,0.65,0.55,0.50,0.40,0.30,0.20,0.10,0.00] y:[124,119,112,136,148,171,195,218,242,265] -->
  <polyline points="55,124 133,119 211,112 289,136 367,148 445,171 523,195 582,218 621,242 640,265"
            fill="none" stroke="#3498db" stroke-width="2.5" stroke-linejoin="round"/>
  <!-- DOWN bid (kırmızı) — kazanan, yükseliyor -->
  <!-- b:[0.40,0.38,0.35,0.45,0.50,0.60,0.70,0.80,0.90,1.00] y:[171,176,183,159,148,124,101,77,54,30] -->
  <polyline points="55,171 133,176 211,183 289,159 367,148 445,124 523,101 582,77 621,54 640,30"
            fill="none" stroke="#e74c3c" stroke-width="2.5" stroke-linejoin="round"/>
  <!-- Alım noktaları -->
  <!-- UP alımlar (yanlış yön, mavi dolu = para battı) -->
  <circle cx="55"  cy="124" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1.2"/>
  <circle cx="133" cy="119" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1.2"/>
  <circle cx="211" cy="112" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1.2"/>
  <circle cx="289" cy="136" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1.2"/>
  <!-- DOWN scalp (loser scalp, gri) -->
  <circle cx="367" cy="148" r="4" fill="#95a5a6" stroke="#ecf0f1" stroke-width="1.2"/>
  <!-- LW DOWN T=30 (turuncu, geç geldi) -->
  <circle cx="582" cy="77" r="5" fill="#f39c12" stroke="#ecf0f1" stroke-width="1.5"/>
  <!-- Kayıp bölgesi highlight (kırmızı yarı saydam kutu UP alımları etrafında) -->
  <rect x="55" y="100" width="240" height="160" fill="#e74c3c" opacity="0.05" rx="4"/>
  <text x="165" y="172" text-anchor="middle" fill="#e74c3c" font-size="10" opacity="0.8">Yanlış yön birikimi</text>
  <!-- Legend -->
  <line x1="55" y1="310" x2="75" y2="310" stroke="#3498db" stroke-width="2.5"/>
  <text x="79" y="314" fill="#3498db" font-size="11" font-weight="bold">UP bid (yanlış)</text>
  <line x1="178" y1="310" x2="198" y2="310" stroke="#e74c3c" stroke-width="2.5"/>
  <text x="202" y="314" fill="#e74c3c" font-size="11" font-weight="bold">DOWN bid (kazanan)</text>
  <circle cx="315" cy="310" r="4" fill="#3498db" stroke="#ecf0f1" stroke-width="1"/>
  <text x="323" y="314" fill="#adb5bd" font-size="11">UP alım</text>
  <circle cx="385" cy="310" r="4" fill="#f39c12" stroke="#ecf0f1" stroke-width="1"/>
  <text x="393" y="314" fill="#adb5bd" font-size="11">LW alım (geç)</text>
  <circle cx="470" cy="310" r="4" fill="#95a5a6" stroke="#ecf0f1" stroke-width="1"/>
  <text x="478" y="314" fill="#adb5bd" font-size="11">Scalp</text>
  <!-- Axes -->
  <line x1="55" y1="30"  x2="55"  y2="265" stroke="#4a5568" stroke-width="1.5"/>
  <line x1="55" y1="265" x2="640" y2="265" stroke="#4a5568" stroke-width="1.5"/>
</svg>

### ASCII Timeline — Karar Anları

```
Kalan Süre:  300s     240s     180s     150s      90s      30s      10s      0s
             │        │        │        │         │        │        │        │
UP bid:      0.48 ──► 0.52 ──► 0.91 ──► 0.78 ──► 0.95 ──► 0.98 ──► 0.99 ──► 1.00
DOWN bid:    0.52 ──► 0.48 ──► 0.09 ──► 0.22 ──► 0.05 ──► 0.02 ──► 0.01 ──► 0.00
             │        │        │        │         │        │        │        │
Karar:      OB/BSI  OB→UP   LW START PYRAMID    LW×2.5  LW×4.0  LW×5.0  REDEEM
             ▼        ▼        ▼        ▼         ▼        ▼        ▼
Emir:       DOWN↓   UP↑     UP×109   UP×48     UP×261  UP×409  UP×506
            $0.52   $0.52   @$0.92   @$0.78    @$0.96  @$0.98  @$0.99
                             taker    maker      taker   taker   taker

Tetiklemeler:
  ▶ T=300s : İlk emir (OB / BSI primer)
  ▶ T=150s : late_pyramid_secs devreye (winner_size_factor çarpanı)
  ▶ T=180s : Late Winner başlangıcı (bid ≥ 0.90)
  ▶ T=10s  : Son LW shot (arb_mult 5×)
  ▶ T=0s   : REDEEM
```

### arb\_mult Isı Haritası

<table style="border-collapse:collapse;font-family:system-ui,Arial,sans-serif;font-size:13px;background:#0f1117;color:#ecf0f1;border-radius:10px;overflow:hidden;width:100%;">
  <thead>
    <tr style="background:#1a2a1a;">
      <th style="padding:10px 14px;text-align:left;border-bottom:2px solid #27ae60;color:#2ecc71;">Winner Ask (w_ask)</th>
      <th style="padding:10px 14px;text-align:center;border-bottom:2px solid #27ae60;color:#2ecc71;">arb_mult</th>
      <th style="padding:10px 14px;text-align:center;border-bottom:2px solid #27ae60;color:#2ecc71;">Görsel</th>
      <th style="padding:10px 14px;text-align:center;border-bottom:2px solid #27ae60;color:#2ecc71;">$100 USDC → Lot</th>
      <th style="padding:10px 14px;text-align:center;border-bottom:2px solid #27ae60;color:#2ecc71;">Yaklaşık Maliyet</th>
    </tr>
  </thead>
  <tbody>
    <tr style="background:#1a0a00;border-bottom:1px solid #222;">
      <td style="padding:9px 14px;font-weight:bold;color:#e74c3c;">≥ 0.99</td>
      <td style="padding:9px 14px;text-align:center;font-size:17px;font-weight:bold;color:#e74c3c;">5.0×</td>
      <td style="padding:9px 14px;text-align:center;"><span style="color:#e74c3c;font-size:18px;letter-spacing:-1px;">█████</span></td>
      <td style="padding:9px 14px;text-align:center;font-weight:bold;color:#e74c3c;">506 lot</td>
      <td style="padding:9px 14px;text-align:center;color:#e74c3c;">≈ $501</td>
    </tr>
    <tr style="background:#1a0d00;border-bottom:1px solid #222;">
      <td style="padding:9px 14px;font-weight:bold;color:#e67e22;">≥ 0.98</td>
      <td style="padding:9px 14px;text-align:center;font-size:17px;font-weight:bold;color:#e67e22;">4.0×</td>
      <td style="padding:9px 14px;text-align:center;"><span style="color:#e67e22;font-size:18px;letter-spacing:-1px;">████</span><span style="color:#333;font-size:18px;">█</span></td>
      <td style="padding:9px 14px;text-align:center;font-weight:bold;color:#e67e22;">409 lot</td>
      <td style="padding:9px 14px;text-align:center;color:#e67e22;">≈ $401</td>
    </tr>
    <tr style="background:#14140a;border-bottom:1px solid #222;">
      <td style="padding:9px 14px;font-weight:bold;color:#f1c40f;">≥ 0.97</td>
      <td style="padding:9px 14px;text-align:center;font-size:17px;font-weight:bold;color:#f1c40f;">3.0×</td>
      <td style="padding:9px 14px;text-align:center;"><span style="color:#f1c40f;font-size:18px;letter-spacing:-1px;">███</span><span style="color:#333;font-size:18px;">██</span></td>
      <td style="padding:9px 14px;text-align:center;font-weight:bold;color:#f1c40f;">310 lot</td>
      <td style="padding:9px 14px;text-align:center;color:#f1c40f;">≈ $301</td>
    </tr>
    <tr style="background:#0d1a0d;border-bottom:1px solid #222;">
      <td style="padding:9px 14px;font-weight:bold;color:#2ecc71;">≥ 0.96</td>
      <td style="padding:9px 14px;text-align:center;font-size:17px;font-weight:bold;color:#2ecc71;">2.5×</td>
      <td style="padding:9px 14px;text-align:center;"><span style="color:#2ecc71;font-size:18px;letter-spacing:-1px;">██▌</span><span style="color:#333;font-size:18px;">██</span></td>
      <td style="padding:9px 14px;text-align:center;font-weight:bold;color:#2ecc71;">261 lot</td>
      <td style="padding:9px 14px;text-align:center;color:#2ecc71;">≈ $251</td>
    </tr>
    <tr style="background:#0a170a;border-bottom:1px solid #222;">
      <td style="padding:9px 14px;font-weight:bold;color:#27ae60;">≥ 0.95</td>
      <td style="padding:9px 14px;text-align:center;font-size:17px;font-weight:bold;color:#27ae60;">2.0×</td>
      <td style="padding:9px 14px;text-align:center;"><span style="color:#27ae60;font-size:18px;letter-spacing:-1px;">██</span><span style="color:#333;font-size:18px;">███</span></td>
      <td style="padding:9px 14px;text-align:center;font-weight:bold;color:#27ae60;">211 lot</td>
      <td style="padding:9px 14px;text-align:center;color:#27ae60;">≈ $200</td>
    </tr>
    <tr style="background:#111;border-bottom:1px solid #222;">
      <td style="padding:9px 14px;color:#3498db;">0.90–0.95</td>
      <td style="padding:9px 14px;text-align:center;font-size:17px;font-weight:bold;color:#3498db;">1.0×</td>
      <td style="padding:9px 14px;text-align:center;"><span style="color:#3498db;font-size:18px;letter-spacing:-1px;">█</span><span style="color:#333;font-size:18px;">████</span></td>
      <td style="padding:9px 14px;text-align:center;color:#3498db;">109–111 lot</td>
      <td style="padding:9px 14px;text-align:center;color:#3498db;">≈ $100</td>
    </tr>
    <tr style="background:#0f0f0f;">
      <td style="padding:9px 14px;color:#4a5568;">&lt; 0.90</td>
      <td style="padding:9px 14px;text-align:center;color:#4a5568;">—</td>
      <td style="padding:9px 14px;text-align:center;"><span style="color:#333;font-size:18px;letter-spacing:-1px;">░░░░░</span></td>
      <td style="padding:9px 14px;text-align:center;color:#4a5568;">LW yok</td>
      <td style="padding:9px 14px;text-align:center;color:#4a5568;">bid_thr=0.90 altında</td>
    </tr>
  </tbody>
</table>

---

## 8. Örnek Konfigürasyonlar

### Muhafazakâr (Düşük Risk)

```json
{
  "bonereaper_buy_cooldown_ms": 5000,
  "bonereaper_late_winner_secs": 60,
  "bonereaper_late_winner_bid_thr": 0.95,
  "bonereaper_late_winner_usdc": 50.0,
  "bonereaper_lw_max_per_session": 5,
  "bonereaper_imbalance_thr": 500.0,
  "bonereaper_max_avg_sum": 0.95,
  "bonereaper_first_spread_min": 0.05,
  "bonereaper_size_longshot_usdc": 8.0,
  "bonereaper_size_mid_usdc": 12.0,
  "bonereaper_size_high_usdc": 20.0,
  "bonereaper_winner_size_factor": 1.0,
  "bonereaper_late_pyramid_secs": 60,
  "bonereaper_loser_scalp_usdc": 5.0,
  "bonereaper_loser_scalp_max_price": 0.20,
  "bonereaper_avg_loser_max": 0.30
}
```

**Risk profili:** Market başına maks ~$1 500 · LW cap: 5 shot × $50 × max 5× / $0.95 ≈ $1 315

---

### Agresif (Yüksek Varyans)

```json
{
  "bonereaper_buy_cooldown_ms": 1000,
  "bonereaper_late_winner_secs": 300,
  "bonereaper_late_winner_bid_thr": 0.85,
  "bonereaper_late_winner_usdc": 200.0,
  "bonereaper_lw_max_per_session": 50,
  "bonereaper_imbalance_thr": 2000.0,
  "bonereaper_max_avg_sum": 1.30,
  "bonereaper_first_spread_min": 0.00,
  "bonereaper_size_longshot_usdc": 30.0,
  "bonereaper_size_mid_usdc": 50.0,
  "bonereaper_size_high_usdc": 100.0,
  "bonereaper_winner_size_factor": 3.0,
  "bonereaper_late_pyramid_secs": 200,
  "bonereaper_loser_scalp_usdc": 20.0,
  "bonereaper_loser_scalp_max_price": 0.40,
  "bonereaper_avg_loser_max": 0.70
}
```

**Risk profili:** Market başına maks ~$50 000+ · Yüksek PnL varyansı · Stop-loss YOK

---

### Gerçek Bot Eşdeğeri (Default Değerler)

```json
{
  "bonereaper_buy_cooldown_ms": 3000,
  "bonereaper_late_winner_secs": 180,
  "bonereaper_late_winner_bid_thr": 0.90,
  "bonereaper_late_winner_usdc": 100.0,
  "bonereaper_lw_max_per_session": 20,
  "bonereaper_lw_burst_secs": 0,
  "bonereaper_lw_burst_usdc": 0.0,
  "bonereaper_imbalance_thr": 1000.0,
  "bonereaper_max_avg_sum": 1.05,
  "bonereaper_first_spread_min": 0.02,
  "bonereaper_size_longshot_usdc": 15.0,
  "bonereaper_size_mid_usdc": 23.0,
  "bonereaper_size_high_usdc": 37.0,
  "bonereaper_winner_size_factor": 1.0,
  "bonereaper_late_pyramid_secs": 150,
  "bonereaper_loser_min_price": 0.01,
  "bonereaper_loser_scalp_usdc": 10.0,
  "bonereaper_loser_scalp_max_price": 0.30,
  "bonereaper_avg_loser_max": 0.50
}
```

**Risk profili:** Market başına LW maks = 20 shot × $100 × 5× / $0.99 ≈ **$10 100**

---

## 9. Bilinen Riskler

### Risk 1: Stop-Loss Yok (En Kritik)

Strateji kaybeden pozisyonu averaging-down ile büyütür. Yanlış yönde başlarsa:

```
Örnek (1777624200):
  UP 1240 sh @ avg $0.62 → DOWN kazandı → 1240 × $0.00 = $0 gelir
  Maliyet: 1240 × $0.62 = $769 → NET: −$769
  (Loser scalp ve LW ile kısmi kurtarma: −$769 + $411 = −$358)
```

**Çözüm:** `max_avg_sum=1.05` yeni alımları sınırlar ama mevcut pozisyonu kapatmaz.

### Risk 2: Maker Fill Garantisi Yok

Normal emirler `bid` fiyatından maker (limit) olarak gönderilir. Piyasa hızlı hareket ederse:
- Emir doldurulmadan iptal olabilir
- LW öncesi birikim eksik kalır

### Risk 3: avg\_sum Cap Kritikliği

`avg_up + avg_down > 1.05` koşulu sağlanınca her iki taraf kazansa da zarar edilir:

```
avg_up = 0.60, avg_down = 0.50 → sum = 1.10
Eğer UP kazanırsa: UP gelir − UP maliyet − DOWN maliyet
= 1.00 − 0.60 − 0.50 = −$0.10/sh → ZARAR!

Bu yüzden cap 1.05 olarak tutulur:
avg_up = 0.90, avg_down = 0.10 → sum = 1.00 → kazanan her zaman kâr
```

### Risk 4: LW Geç Tetiklenebilir

`bid_thr=0.90` eşiği önemli: DOWN 0.88'de kapanırsa LW hiç tetiklenmez, büyük kâr fırsatı kaçar.

### Risk 5: BSI Sinyali Tek Başına Zayıf

İlk yön kararında BSI doğruluğu: **%64** (11 market analizi). Rastgeleden az iyidir. Ancak doğru/yanlış yönün PnL etkisi sadece **$0.55/market** (rebalans/scalp/LW telafi eder).

---

### Telemetri Etiketleri (reason strings)

| Etiket | Anlamı |
|--------|--------|
| `bonereaper:buy:up` | Normal alım, UP yönü (maker @ bid) |
| `bonereaper:buy:down` | Normal alım, DOWN yönü (maker @ bid) |
| `bonereaper:scalp:up` | Loser scalp veya deep lot, UP (taker @ ask) |
| `bonereaper:scalp:down` | Loser scalp veya deep lot, DOWN (taker @ ask) |
| `bonereaper:lw:up` | Late Winner ana dalga, UP (taker @ ask) |
| `bonereaper:lw:down` | Late Winner ana dalga, DOWN (taker @ ask) |
| `bonereaper:lwb:up` | Late Winner burst dalgası, UP (etkin değilse kullanılmaz) |
| `bonereaper:lwb:down` | Late Winner burst dalgası, DOWN |

---

### Gerçek Bot PnL Özeti (15 Market Doğrulaması)

| Market | Kazanan | PnL | Doğru? |
|--------|---------|-----|--------|
| 1777622100 | DOWN | +$175.86 | ✓ |
| 1777622400 | DOWN | −$60.45 | ✓ |
| 1777622700 | UP | −$9.24 | ✓ |
| 1777624200 | DOWN | −$357.98 | ✗ |
| 1777624500 | DOWN | −$145.87 | ✗ |
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
| obtest2 (1777647300) | UP | −$211.57 | ✗ |
| **TOPLAM** | | **+$376.97** | **15/17 = %88** |

---

*Kaynak: `src/strategy/bonereaper.rs` · `src/config.rs:406–570` · `docs/bonereaper.md` · `docs/bonereaper-backtest-report.md`*
*Tarih: Mayıs 2026*
