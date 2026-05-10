# Bonereaper Strategy — Tüm Kurallar (Uygulamadaki Davranış)

Bu doküman, repodaki **Bonereaper** stratejisinin (gerçekte çalışan kodun) tüm karar kurallarını tek yerde toplar.

Yetkili kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs) ve parametre default/limitleri: [config.rs](file:///workspace/src/config.rs#L329-L421).

Ek not: [bonereaper.md](file:///workspace/docs/bonereaper.md) dosyası daha çok saha gözlemi / reverse-engineering içerir; burada anlatılanlar ise **kodun birebir uyguladığı** kurallardır.

---

## 1) Genel Yaklaşım

- Strateji **signal-driven değildir**; temel olarak orderbook’a bakıp BUY emirleri üretir.
- Emirler **taker BUY @ ask** olacak şekilde GTC limit olarak gönderilir (ask fiyatına limit koyulduğu için pratikte anında fill hedeflenir).
- Strateji iki ana davranışa ayrılır:
  - **Normal akış (minimal scalp)**: küçük notional alımlar (bid seviyesine göre bucket).
  - **Late Winner Injection**: pencerenin sonuna yakın, “winner” görünen tarafa büyük notional tek seferlik/limitli taker BUY.

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L1-L31)

---

## 2) Girdi Verileri (StrategyContext)

Strateji aşağıdaki context alanlarını kullanır:

- UP/DOWN top-of-book: `up_best_bid`, `up_best_ask`, `down_best_bid`, `down_best_ask`
- Pozisyon metrikleri: `metrics.up_filled`, `metrics.down_filled`, `metrics.avg_up`, `metrics.avg_down`
- Pencere süresi: `market_remaining_secs` (end - now)
- Bot konfigürasyonu: `min_price`, `max_price`
- API minimum order notional: `api_min_order_size` (Gamma’dan gelir)
- Strateji parametreleri: `strategy_params` (DB’de JSON; accessor’lar config’te)

Strateji dispatch’i: [strategy.rs](file:///workspace/src/strategy.rs#L28-L43)

---

## 3) Durum Makinesi (FSM)

Bonereaper state’i:

- `Idle`: orderbook hazır değilse bekler.
- `Active`: karar üretir.
- `Done`: geriye uyumluluk için var; yeni akışta üretilmez.

Geçişler:

- `Idle → Active`: `up_best_bid/ask` ve `down_best_bid/ask` hepsi `> 0.0` olunca.
- `Active`: pencere boyunca devam eder (bu state içindeyken yapılan her karar NoOp veya emir üretimidir).

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L53-L106)

---

## 4) Parametreler (StrategyParams) ve Varsayılanlar

Tüm defaultlar ve güvenli aralıklar config accessor’larından gelir:

- `bonereaper_buy_cooldown_ms`: default `15000`, clamp `500..=60000`
- `bonereaper_late_winner_secs`: default `30`, clamp `0..=300`
- `bonereaper_late_winner_bid_thr`: default `0.90`, clamp `0.50..=0.99`
- `bonereaper_late_winner_usdc`: default `2000.0`, clamp `0..=10000`
- `bonereaper_lw_max_per_session`: default `1`, clamp `0..=20` (0 = sınırsız; spam riski)
- `bonereaper_imbalance_thr`: default `200.0`, clamp `0..=10000`
- `bonereaper_max_avg_sum`: default `1.10`, clamp `0.50..=2.00`
- `bonereaper_size_longshot_usdc`: default `5.0`, clamp `0..=10000`
- `bonereaper_size_mid_usdc`: default `10.0`, clamp `0..=10000`
- `bonereaper_size_high_usdc`: default `15.0`, clamp `0..=10000`

Kaynak: [config.rs](file:///workspace/src/config.rs#L355-L420)

---

## 5) Karar Zinciri (Öncelik Sırası)

Aşağıdaki sıra **tam olarak** uygulanır:

### 5.1 Guard’lar

- `to_end < 0.0` ise NoOp (pencere kapanmış gibi davranır).
- `up_best_bid <= 0.0` veya `down_best_bid <= 0.0` ise NoOp.

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L108-L115)

### 5.2 Late Winner Injection (Cooldown’ı Bypass Eder)

Bu blok **cooldown kontrolünden önce** çalışır; yani tetiklenirse normal cooldown beklenmez.

Koşulların hepsi sağlanmalı:

- `lw_usdc > 0.0`
- `lw_secs > 0.0`
- `0.0 < to_end <= lw_secs`
- Session limiti: `lw_max == 0 || lw_injections < lw_max`
- Winner seçimi: `up_best_bid >= down_best_bid` ise winner=UP, değilse DOWN
- `winner_bid >= lw_thr`
- `winner_ask > 0.0`

Emir boyutu:

- `size = ceil(lw_usdc / winner_ask)`

Emir üretilebilmesi için ayrıca `make_buy` koşulları da geçmelidir (bkz. §6).

Tetiklenince:

- `last_buy_ms = now_ms`
- `lw_injections += 1`
- `last_up_bid/last_dn_bid` güncellenir
- `Decision::PlaceOrders([order])` döner

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L116-L149)

### 5.3 Cooldown (Normal Akış Gate)

- `st.last_buy_ms > 0` ve `(now_ms - last_buy_ms) < bonereaper_buy_cooldown_ms` ise NoOp.
- Bu durumda bile `last_up_bid/last_dn_bid` güncellenir.

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L151-L157)

### 5.4 Yön Seçimi (Dir)

Önce “inventory imbalance” kontrol edilir:

- `imb = metrics.up_filled - metrics.down_filled`
- `abs(imb) > bonereaper_imbalance_thr` ise:
  - `imb > 0` (UP daha fazla dolu) → `dir = DOWN`
  - aksi → `dir = UP`

İmbalance küçükse OB-driven seçim:

- `d_up = abs(up_best_bid - last_up_bid)`
- `d_dn = abs(down_best_bid - last_dn_bid)`
- Her ikisi de `0.0` ise: “momentum” fallback
  - `up_best_bid >= down_best_bid` → UP, aksi DOWN
- Aksi halde:
  - `d_up >= d_dn` → UP, aksi DOWN

Not: Her tick sonunda `last_up_bid/last_dn_bid` güncellenir.

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L159-L187)

### 5.5 Fiyat ve Notional Gate’leri

Dir belirlendikten sonra:

- `bid = best_bid(dir)`, `ask = best_ask(dir)`
- `bid <= 0.0` veya `ask <= 0.0` ise NoOp
- `bid < min_price` veya `bid > max_price` ise NoOp

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L188-L197)

### 5.6 Dinamik USDC Bucket ve Size

Notional seçimi (USDC):

- `bid <= 0.30` → `bonereaper_size_longshot_usdc`
- `0.30 < bid <= 0.85` → `bonereaper_size_mid_usdc`
- `bid > 0.85` → `bonereaper_size_high_usdc`

- `usdc <= 0.0` ise NoOp
- `size = ceil(usdc / ask)`

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L199-L211)

### 5.7 avg_sum Soft Cap

Amaç: Yeni alımdan sonra “mevcut tarafın yeni avg’i + karşı taraf avg’i” aşırı büyümesin.

- `max_avg_sum = bonereaper_max_avg_sum`
- Eğer karşı tarafta fill varsa (`opp_filled > 0.0`):
  - `new_avg` hesaplanır:
    - `cur_filled > 0.0` ise:
      - `new_avg = (cur_avg * cur_filled + ask * size) / (cur_filled + size)`
    - değilse:
      - `new_avg = ask`
  - `new_avg + opp_avg > max_avg_sum` ise NoOp

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L212-L227)

### 5.8 Emir Üretimi

Tüm gate’ler geçilirse:

- `make_buy(ctx, dir, ask, size, reason_buy(dir))`
- Başarılıysa `last_buy_ms = now_ms` ve `Decision::PlaceOrders([order])`

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L229-L236)

---

## 6) Emir Şekli ve Geçerlilik Kuralları (make_buy)

Üretilen emir:

- `side = BUY`
- `order_type = GTC`
- `price = ask` (dir tarafının ask’i)
- `size = ceil(usdc / ask)`

Emir üretimi için zorunlu şartlar:

- `price > 0.0`
- `size > 0.0`
- `size * price >= api_min_order_size` (Gamma’dan gelen minimum notional)

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L242-L265)

---

## 7) Reason Etiketleri

Bot log/telemetri için `reason` alanını sabit etiketlerle doldurur:

- Normal BUY:
  - `bonereaper:buy:up`
  - `bonereaper:buy:down`
- Late Winner Injection:
  - `bonereaper:lw:up`
  - `bonereaper:lw:down`

Kaynak: [bonereaper.rs](file:///workspace/src/strategy/bonereaper.rs#L37-L51)

---

## 8) Kısa Pseudocode (Tek Parça)

```text
if state == Idle:
  if book_ready: state = Active(init last bids); return NoOp
  else: return NoOp

if state == Active:
  if to_end < 0 or bids invalid: return NoOp

  if late_winner_enabled and to_end in (0, lw_secs] and quota_ok:
    winner = argmax(up_bid, down_bid) (tie -> UP)
    if winner_bid >= lw_thr:
      size = ceil(lw_usdc / winner_ask)
      if make_buy(...): last_buy_ms=now; lw_injections++; return Place(order)

  if cooldown_active: update last bids; return NoOp

  dir = if abs(up_filled-down_filled) > imb_thr then weaker_side else ob_driven_delta
  update last bids

  bid, ask = best(dir)
  if bid/ask invalid or bid outside [min_price,max_price]: return NoOp

  usdc = bucket(bid)
  size = ceil(usdc / ask)
  if avg_sum_cap_violated: return NoOp

  if make_buy(...): last_buy_ms=now; return Place(order)
  else return NoOp
```

---

## 9) Uygulama Notları

- Bu strateji **SELL üretmez**; pozisyon kapanışı Polymarket resolution/redeem mekanizmasına dayanır (strateji kodu içinde SELL kararı yok).
- Late winner bloğu, normal cooldown’ı bypass ettiği için “son saniye büyük emir” davranışı baskındır.

