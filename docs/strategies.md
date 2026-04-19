# Strateji Tanımları

Bu doküman, `dutch_book`, `harvest` ve `prism` stratejilerinin **giriş koşulları, aksiyonları ve çıkış kurallarını** tanımlar. Ortak metrik kataloğu (`imbalance`, `imbalance_cost`, `avgsum`, `profit`, `sum_volume`, `POSITION Δ`, `binance_signal`) ve Rust `MetricMask` / `BotConfig` yapıları için bkz. [bot-platform-mimari.md](bot-platform-mimari.md).

---

## Emir Boyutu — Genel Platform Kuralı (tüm stratejiler)

Tüm stratejilerde emir boyutu tek bir config parametresiyle (`order_usdc`) kontrol edilir:

```
size = max(⌈order_usdc / price⌉, api_min_order_size)
```

| Değişken | Kaynak | Açıklama |
|---|---|---|
| `order_usdc` | `BotConfig` | Emir başına harcanacak USDC miktarı (ör. `5.0`) |
| `price` | Emrin limit fiyatı | GTC için bid fiyatı; FAK için best_ask |
| `api_min_order_size` | `GET /book → min_order_size` | Market başlangıcında bir kez okunur, share cinsinden minimum |

**Kural:** `order_usdc` arttıkça `size` artar. FAK boyutu bu formülle hesaplanmaz — her zaman `imbalance` (share farkı) kadardır.

---

**Binance sinyal notu (tüm stratejiler):** Desteklenen bir kripto market (`btc/eth/sol/xrp-updown-*`) üzerinde çalışan her bot, `signal_weight` (0–10, bot config'den) parametresiyle ağırlıklandırılmış `effective_score` değerini emir kararı öncesinde okur. Sinyal hesabının detayı için bkz. [bot-platform-mimari.md §14](bot-platform-mimari.md). Her stratejinin sinyal mekanizması aşağıda ayrı ayrı belirtilmiştir.

**Market bölge notu (tüm stratejiler):** Market penceresi 5 bölgeye ayrılır; her bölgede `binance_signal` aktifliği aşağıdaki **Bölge Haritası** tablolarıyla tanımlanır. Bölge hesabı ve `ZoneSignalMap` Rust yapısı için bkz. [bot-platform-mimari.md §15](bot-platform-mimari.md). `StopTrade` bölgesinde (`zone_pct ≥ 97 %`) tüm stratejilerde sinyal durumundan bağımsız olarak **yeni emir üretilmez**.

**Global price guard (tüm stratejiler):** Tüm planlanan emir fiyatları
`[bot.min_price, bot.max_price]` aralığında olmak zorundadır (varsayılan
`0.05` / `0.95`). Bu aralığın dışındaki emirler `engine` katmanında reject
edilir ve `🚧 Order rejected: price=… outside [min, max] reason=…` log'u
basılır. Strateji modülleri (örn. harvest averaging) sınır aşımını proaktif
olarak da kontrol edip emri hiç üretmemeyi tercih eder. Bkz.
[bot-platform-mimari.md §11.5](bot-platform-mimari.md).

**Ortak averaging cooldown (tüm stratejiler):** `BotConfig.cooldown_threshold`
(default `30_000 ms`, frontend "Ek ayarlar" → "Cooldown threshold") iki rolü
karşılar: (1) iki averaging emri **arası** minimum bekleme, (2) **kitapta
açık** averaging GTC'sinin maksimum yaşam süresi (geçen iptal edilir).
`last_averaging_ms` emrin **gönderildiği** anda set edilir. Bkz.
[bot-platform-mimari.md §15.4](bot-platform-mimari.md).

---

## 1. `dutch_book`

> **Kod durumu:** `src/strategy/dutch_book.rs` şu an yer tutucu; bot başlatma `Strategy::Harvest` dışını `src/bot/ctx.rs` içinde reddeder. Aşağıdaki tablolar tasarım notudur.

### Binance Sinyali Etkisi

`dutch_book`'ta sinyal **hem pozisyon boyutunu hem yön doğrulamasını** etkiler.

| `effective_score` | Yorumlama | Pozisyon çarpanı | Yön filtresi |
|---|---|---|---|
| `8–10` | Güçlü alış baskısı | UP boyut `× scale_up` (ör. `1.5`) | UP emir tercih edilir |
| `6–8` | Hafif alış | UP boyut `× 1.2` | UP emir normal |
| `4–6` | Nötr | `× 1.0` | Sinyal etkisiz |
| `2–4` | Hafif satış | DOWN boyut `× 1.2`; UP emirde `× 0.8` | UP yavaşlatılır |
| `0–2` | Güçlü satış baskısı | DOWN boyut `× scale_up`; UP **atlanır** | UP emir üretilmez |

- `scale_up` ve alt eşikler `BotConfig::signal_weight` ile ölçeklenir.
- `signal_weight = 0` → çarpan her zaman `1.0`; yön filtresi devre dışı.

### Bölge Haritası — `binance_signal` aktif mi?

| Bölge | `zone_pct` aralığı | `binance_signal` aktif |
|---|---|:---:|
| `DeepTrade` | 0 – 10 % | TBD (strateji yok) |
| `NormalTrade` | 10 – 50 % | TBD |
| `AggTrade` | 50 – 90 % | TBD |
| `FakTrade` | 90 – 97 % | TBD |
| `StopTrade` | 97 – 100 % | — (emir yok) |

<!-- Giriş koşulları, aksiyonlar ve çıkış kuralları burada -->

---

## 2. `harvest`

### Temel Kavram

`harvest`, **delta-nötr arbitraj** stratejisidir. YES ve NO tarafının **birleşik ortalama maliyetini** (`avg_YES + avg_NO`, katalogda `AVG SUM`) konfigürasyondaki `avg_threshold` değerinin altında tutarak garanti kâr kilitler.

```
profit = (1.0 − AVG_SUM) × pair_count
```

Piyasa çözümlendikten sonra (YES ya da NO kazanır) `pair_count` tam çifti olan hesap `pair_count × $1` alır.

### Fiyat Referansları

Fiyatlar **WS `best_bid_ask` event'inden** anlık okunur (Market Channel):

| Değişken | Kullanım |
|---|---|
| `best_bid` (YES/NO token) | Averaging GTC bid fiyatı |
| `hedge_leg` (hedef tokenın `best_ask`) | ProfitLock koşulu (`first_leg + hedge_leg`) + FAK gönderim fiyatı |

### Konfigürasyon Parametreleri

Emir boyutu `order_usdc` ile kontrol edilir — platform geneli kuralı bkz. yukarıdaki §.

| Parametre | Tip | Varsayılan | Açıklama |
|---|---|---|---|
| `dual_timeout` | `u64` (ms) | `5_000` | OpenDual fill bekleme süresi; sonunda fill olmayan GTC'ler iptal edilir |
| `avg_threshold` | `f64` | `0.98` | ProfitLock eşiği: SingleLeg'de `first_leg + hedge_leg`, DoubleLeg'de `avg_yes + avg_no` (eşik aşıldığında imbalance kapatma GTC'si devreye girer) |
| `max_position_size` | `f64` | `100.0` | Tek tarafın toplam share limiti (shares) — DoubleLeg'de bir tarafın doyması diğerini durdurmaz |

> **OpenDual fiyatı sinyalden + anlık book mid + market spread'inden türetilir** (aşağıda *Giriş — OpenDual* bölümüne bakın). `up_bid` / `down_bid` ve fiyat-kayma genişliği artık konfigürasyonda yer almaz; sinyal kayması her tarafın anlık `(best_ask − best_bid)` market spread'ine eşittir, böylece **tight market'te küçük, wide market'te büyük** signal effect oluşur. Çıktı `tick_size` katına snap edilir ve `[min_price, max_price]` ile clamp'lenir. Averaging GTC fiyatı `first_best_leg` orderbook'tan geldiği için doğal olarak geçerlidir.
>
> **`cooldown_threshold`** (averaging GTC'leri arasındaki minimum bekleme + GTD süresi kaynağı) tüm stratejiler için **`BotConfig` alanıdır**; API varsayılanı `30_000 ms` (`POST /api/bots` `cooldown_threshold`). bkz. [bot-platform-mimari.md §15](bot-platform-mimari.md#15).

### Tanımlar

| Terim | Anlam |
|---|---|
| `avg_YES` / `avg_NO` | Katalog `avg_up` / `avg_down` — User WS `trade` `MATCHED` event'lerinden hesaplanan taraf bazlı VWAP |
| `first_leg` | SingleLeg ProfitLock koşulundaki kısayol: **dolmuş tarafın** `avg_*` değeri (ör. yalnızca YES dolmuşsa → `avg_YES`) |
| `hedge_leg` | SingleLeg ProfitLock koşulundaki kısayol: **henüz dolmamış (hedef) tarafın** anlık `best_ask` değeri — WS `best_bid_ask`'tan okunur; FAK fiyatı olarak da kullanılır |
| `first_best_leg` | Averaging döngüsündeki kısayol: **dolmuş (tutulan) tarafın** anlık `best_bid` değeri — averaging GTC fiyatı ve düşüş koşulu için kullanılır |
| `last_fill_price_yes` / `last_fill_price_no` | Per-side en son `MATCHED` fill fiyatı (her kısmi fill dahil güncellenir). Averaging "price_fell" kontrolünün tek kaynağıdır. Eski tek-değerli `MarketSession.last_fill_price` field'ı kaldırıldı; metrics içinden okunur. |
| `pair_count` | `min(YES_total, NO_total)` — tamamlanan YES+NO çift sayısı; kâr formülünde kullanılır |
| `hedge_token` | Henüz dolmamış (hedge edilecek) tarafın `asset_id`'si (clobTokenId); FAK emrinde `tokenId` alanı olarak kullanılır |
| `signal_multiplier` | `effective_score` ve averaging tarafına göre §14.4 tablosundan okunan boyut çarpanı (ör. `×1.3`); `signal_weight = 0` ise daima `1.0` |
| `imbalance` | `YES_toplam − NO_toplam` (shares) |

### Durum Makinesi

```
[Başlangıç]
    │
    ▼
[Pending]
    │   T=0 (her tick): sinyali oku → up_bid/down_bid hesapla
    │                    → POST /orders (YES + NO GTC) → deadline = now + dual_timeout
    ▼
[OpenDual{deadline}]
    │
    ├── Her iki taraf MATCHED (deadline beklemeden veya sonra)
    │       → açık emirleri iptal et → [DoubleLeg]
    │
    ├── Tek taraf MATCHED + now ≥ deadline
    │       → diğer GTC iptal → [SingleLeg{filled_side, entered_at_ms = now}]
    │
    └── Hiçbir taraf MATCHED + now ≥ deadline
            → 2 GTC iptal → [Pending] (sonraki tick yeni sinyalle yeniden açılır)

[SingleLeg{filled_side, entered_at_ms}]
    │   Her fill (MATCHED) ve her orderbook güncellemesinde:
    │
    ├── (now − entered_at_ms ≥ cooldown_threshold)               ← warmup
    │   ∧ first_leg + hedge_leg ≤ avg_threshold                  ← SingleLeg ProfitLock
    │       → FAK@hedge_leg (imbalance > 0 ise) → [Done]
    │
    ├── (cooldown_threshold bitti) + (first_best_leg < last_fill_price_<side>)
    │   + (pozisyon < max_position_size) + (bölge ≠ StopTrade)
    │       → averaging GTC (size = base × signal_multiplier) → [SingleLeg]
    │
    └── Pencere sona erdi → [Done]

[DoubleLeg]
    │   Her fill ve her orderbook güncellemesinde:
    │
    ├── avg_yes > 0 ∧ avg_no > 0 ∧ avg_sum ≤ avg_threshold ∧ |imbalance| < api_min_order_size
    │       → [Done] (saf VWAP teslim alınır, FAK yok)
    │
    ├── avg_sum ≤ avg_threshold ∧ |imbalance| ≥ api_min_order_size      ← imbalance kapatma
    │       → eksik tarafa GTC: side = (imbalance<0 ? Up : Down),
    │         price = best_bid(eksik), size = min(|imbalance|, max−pos_held)
    │         (price_fell BYPASS, multiplier = 1.0)
    │       → [DoubleLeg] (fill alınca shares eşitlenir → sonraki tick Done)
    │
    ├── avg_sum > avg_threshold → per-side bağımsız normal averaging:
    │     YES + NO için handle_open_avg + try_averaging (mult=1.0, price_fell zorunlu)
    │     → birleşik karar: NoOp / PlaceOrders / CancelOrders / Batch{cancel,place}
    │     → bir taraf max_position_size'a ulaşırsa o taraf donar, diğer taraf devam eder
    │
    └── Pencere sona erdi → [Done]
```

> **Not (legacy):** `HarvestState::ProfitLock` enum varyantı eski persist dosyaları
> için saklanır; yeni kod üretmez. `decide` katmanı bu state'i bir tick içinde
> doğrudan `Done`'a evolve eder.

### Giriş — OpenDual (sinyal güdümlü, market spread'i ölçeklenmiş)

- **Tetik:** `Pending` durumunda her tick. **Önkoşul:** `yes_best_bid > 0 && no_best_bid > 0` (market book quote'u gelmiş olmalı, DryRun passive-fill simulator best_ask isteyecek). Quote yoksa `Pending` korunur, log basılmaz.
- **Endpoint:** İki ayrı **`POST /order`** — `Decision::PlaceOrders` iki `PlannedOrder` döner; `engine::executor::place_batch` bunları **sırayla** yürütür (CLOB batch `/orders` kullanılmaz).
- **Fiyatlama** (`s = effective_score ∈ [0, 10]`, nötr 5; sinyal kayması = anlık market spread):

  ```
  delta      = (s − 5) / 5                                  // [-1, +1]

  yes_spread = max(0, yes_best_ask − yes_best_bid)          // Polymarket WS BestBidAsk.spread
  no_spread  = max(0, no_best_ask  − no_best_bid)

  up_raw     = yes_best_ask + delta · yes_spread            // sinyal UP iken ask'ı geçer
  down_raw   = no_best_ask  − delta · no_spread             // sinyal UP iken no_bid'e iner

  up_bid     = clamp( snap(up_raw),   min_price, max_price )
  down_bid   = clamp( snap(down_raw), min_price, max_price )
  ```

  Örnek 1 — likit market (yes_bid=0.50, yes_ask=0.52 → yes_spread=0.02; no_bid=0.46, no_ask=0.48 → no_spread=0.02):

  | `effective_score` | `delta` | `up_bid` | `down_bid` | not |
  |---|---|---|---|---|
  | 0.0 (max düşüş) | −1.0 | `0.50` | `0.50` | up=yes_bid (maker), down=no_ask+spread (agresif taker) |
  | 5.0 (nötr / `signal_weight=0`) | 0.0 | `0.52` | `0.48` | her iki bid kendi ask'ında — taker eşiği |
  | 10.0 (max yükseliş) | +1.0 | `0.54` | `0.46` | up=yes_ask+spread (agresif taker), down=no_bid (maker) |

  Örnek 2 — illikit market (yes_bid=0.40, yes_ask=0.60 → yes_spread=0.20):

  | `effective_score` | `delta` | `up_bid` | not |
  |---|---|---|---|
  | 5.0 (nötr) | 0.0 | `0.60` | yes_ask'da, taker eşiği |
  | 10.0 | +1.0 | `0.80` | yes_ask=0.60'ı tam 0.20 geçer; çok agresif taker |

- **Anahtar özellik:** Signal effect, market'in anlık likiditesine **otomatik ölçeklenir**. Tight book (`spread = 0.01`) → sinyal etkisi minik; wide book (`spread = 0.30`) → sinyal etkisi büyük. Bu sayede ek bir konfigürasyon parametresi gerekmez; bot adaptif davranır.
- **Maker / Taker davranışı (asimetrik):**
  - `delta = 0` (nötr): her iki bid kendi ask'ında → ikisi de **taker** (anlık taker fill).
  - `delta = +1`: `up_bid = yes_ask + yes_spread` → agresif taker; `down_bid = no_bid` → pasif maker.
  - `delta = −1`: `up_bid = yes_bid` → pasif maker; `down_bid = no_ask + no_spread` → agresif taker.
  - Ara delta'larda bir taraf taker, diğer taraf maker bölgesine geçer.
- **1−up simetrisi YOKTUR**: iki taraf bağımsız hesaplanır. Toplam (`up_bid + down_bid`) Polymarket'te ≈ `1.00` olur ama garanti değildir.
- **Spread = 0** (bid = ask) → signal effect 0; her taraf kendi ask'ında (= bid) durur. `(ask − bid).max(0)` negatif spread'e karşı korur.
- Sadece global `[min_price, max_price]` (default `0.05–0.95`) sınırı uygulanır; tick'e snap edilir.
- **İki taraflı taker fill** → `DoubleLeg`'e geçilir; orada `avg_sum ≤ avg_threshold` eşiği sağlandığında imbalance kapatma GTC'si devreye girer ve shares dengesi sağlandığında `[Done]`'a geçilir. Tek taraflı fill + timeout → `SingleLeg{filled_side, entered_at_ms = now}`; warmup (`cooldown_threshold` ms) sonrasında `first_leg + hedge_leg` ProfitLock değerlendirmesi devreye girer (yapay tetikten korunur).

**DryRun davranışı:** Yeni formülde `up_bid ≥ yes_best_ask` durumu mümkün — `Simulator::fill` koşulu (`price ≥ best_ask`) bu emirleri **anlık doldurur** (kitap dokunduğunda taker simülasyonu). Daha düşük bid'li (passive maker) emirler ise market hareket edene kadar `live` kalır; her market book update'inde `engine::simulate_passive_fills(session)` (bot.rs içinde tetiklenir) açık emirleri yeni quote ile karşılaştırır ve dolduğunda `📥 passive_fill` logu basar.

**Boyut:**
- YES: `size = max(⌈order_usdc/up_bid⌉, api_min_order_size)`, `orderType=GTC`
- NO:  `size = max(⌈order_usdc/down_bid⌉, api_min_order_size)`, `orderType=GTC`

**`dual_timeout` sayacı:** Emirler gönderildiği tick'te `deadline_ms = now_ms + dual_timeout` saklanır. Sonraki her tick'te fill durumu ve `now_ms ≥ deadline_ms` kontrolü yapılır (yukarıdaki *Durum Makinesi* dallarına bakın).

**Kısmi başarısızlık (ilk emir OK, ikinci `POST /order` hata):** İkinci çağrı `AppError::Clob` ile döngüyü kesebilir; ilk bacakta oluşan açık GTC defterde kalır. Operasyonel olarak log + User WS `order` ile durum izlenir; bir sonraki tick'te FSM yeniden değerlendirilir.

### Averaging (SingleLeg döngüsü)

Tek taraf doldu, diğer taraf timeout sonrası iptal edildiyse bot **[SingleLeg{filled_side, entered_at_ms}]** durumunda şu sırayla kontrol yapar:

1. **ProfitLock öncelik kontrolü (warmup'lı):**
   - `now − entered_at_ms ≥ cooldown_threshold` (warmup) **ve**
   - `hedge_leg > 0` ve `first_leg + hedge_leg ≤ avg_threshold` → FAK + `[Done]` (aşağıda).
   - Warmup süresince ProfitLock pas geçilir; dual fill anındaki book gürültüsünün
     yapay tetikleme yapması engellenir.
2. **Açık averaging GTC kontrolü:** Aynı taraf için `open_orders` listesinde `reason="harvest:averaging:*"` kayıtları varsa:
   - En yaşlı emrin yaşı `< cooldown_threshold` → `Decision::NoOp`, kitapta beklemeye devam.
   - Yaşı `≥ cooldown_threshold` → `Decision::CancelOrders([..])` döner; emir kaldırılır. Bir sonraki tick'te koşullar uygunsa yenisi gönderilir.
3. **Averaging koşulu** (kitap aynı tarafta boşsa):
   - `now − last_averaging_ms < cooldown_threshold` → bekle. (`last_averaging_ms` emrin **gönderildiği** anda set edilir; live emirler de bu sayacı tetikler.)
   - Cooldown bitti + `first_best_leg < last_fill_price_<side>` (per-side metrics) + `pos_held < max_position_size` + bölge ≠ StopTrade + `first_best_leg ∈ [min_price, max_price]` → aynı tarafa **GTC bid** emri gönder (`POST /order`, fiyat = `first_best_leg`).
   - **`pos_held` formülü:** `pos_held = filled_shares + Σ(open_orders.size where outcome == filled_side && side == BUY)`. LIVE notional dahil olduğundan birikmiş averaging GTC'ler `max_position_size` korumasından kaçamaz.
   - **Averaging boyutu** (iki aşamalı):
     ```
     base_size      = max(⌈order_usdc / first_best_leg⌉, api_min_order_size)
     effective_size = max(round(base_size × signal_multiplier), api_min_order_size)
     ```
     Signal çarpanının boyutu `api_min_order_size` altına indirmesi engellenir.
4. **Her MATCHED fill event'inde** (kısmi fill dahil): `avg_*`, `imbalance`, `last_fill_price_yes/_no` güncellenir; `last_averaging_ms` fill anında da set edilir.
5. Averaging turu sınırsızdır; `max_position_size` tek durucu kuraldır.

### Averaging (DoubleLeg döngüsü)

İki taraf da OpenDual'da dolduğunda bot **[DoubleLeg]** durumuna geçer. Davranış `avg_sum` eşiği etrafında iki kola ayrılır:

#### 1) `avg_sum > avg_threshold` — normal per-side averaging

Her iki tarafa bağımsız averaging GTC üretilir:

- `handle_open_averaging_for_side`: aynı taraftaki açık averaging GTC yaşı `< cooldown_threshold` → bekle; yaşı `≥` → cancel listesine ekle.
- `try_averaging_for_side(side, apply_signal_mult=false)`: aynı taraf için `cooldown` + `price_fell` + `pos_held < max_position_size` + book sınırları sağlanırsa yeni GTC place listesine eklenir.
- **Decision birleştirme:** boş+boş → `NoOp`; place→`PlaceOrders`; cancel→`CancelOrders`; ikisi de → `Batch{cancel, place}`.
- **`signal_multiplier` uygulanmaz** (`apply_signal_mult=false`, çarpan `1.0`) — sinyal etkisi açılış fiyatında zaten gömülü; averaging'de tekrar boyutlandırılırsa double-count olur.
- **`max_position_size` tek taraflı freeze:** bir taraf max'a ulaşırsa `pos_held` kontrolünden geçemez; diğer taraf devam eder. State `DoubleLeg` kalır.

#### 2) `avg_sum ≤ avg_threshold` — imbalance kapatma yolu

Eşik sağlandığında **doğrudan Done değil**; önce shares dengesi aranır. `imbalance = shares_yes − shares_no` farkına göre:

- **`|imbalance| < api_min_order_size`** (dengeli) → `[Done] + NoOp`. Daha küçük bir imbalance API minimum altında zaten kapatılamaz.
- **`|imbalance| ≥ api_min_order_size`** → eksik tarafa **tek bir** GTC açılır:
  - `short_side = if imbalance < 0 { Up } else { Down }`
  - `price = best_bid(short_side)` — book sınırları (`min_price`, `max_price`) içinde olmalı.
  - `size = min(|imbalance|, max_position_size − pos_held(short_side))` — `api_min_order_size` altına düşerse `NoOp`.
  - `cooldown_ok` (averaging cooldown'una saygı) sağlanmalı.
  - **`price_fell` BYPASS** — bu yol VWAP eşiği zaten geçildiği için fiyat düşüşü beklemez; amaç imbalance'ı kapatmak.
  - **`signal_multiplier` uygulanmaz** (çarpan `1.0`).
  - Açık averaging GTC: `handle_open_averaging_for_side` cancel-eligible ise `Decision::Batch{ cancel, place: [order] }`, fresh ise `NoOp` (mevcut emir bekleniyor).
  - Reason etiketi `harvest:averaging:{side}` — mevcut takip mantığıyla uyumlu.
- Eksik tarafa fill girip `|imbalance|` `api_min_order_size` altına düştüğünde sonraki tick `[Done]`'a geçer.

> **StopTrade bölgesi** (her iki kolda): `[DoubleLeg] + NoOp` — yeni emir basılmaz, mevcut emirler dokunulmaz.

### ProfitLock Aksiyonu (SingleLeg yolu)

**Yalnızca `SingleLeg` döngüsünde tetiklenir** (DoubleLeg ProfitLock için yukarıya bakın). Warmup `cooldown_threshold` boyunca pas geçilir.

**Koşul:** `now − entered_at_ms ≥ cooldown_threshold` ∧ `hedge_leg > 0` ∧ `first_leg + hedge_leg ≤ avg_threshold`
- `first_leg` = dolmuş (tutulan) tarafın VWAP (averaging'le birlikte aşağı çekilir)
- `hedge_leg` = henüz dolmamış (hedge edilecek) tarafın anlık `best_ask` (FAK fiyatı olarak da kullanılır)

Adımlar:

1. **imbalance ≠ 0** ise: karşı tarafa `POST /order` ile **FAK** gönderilir.
   - `side=BUY`, `tokenId=hedge_token`, `price=hedge_leg`, `size=|imbalance|`, `orderType=FAK`
   - FAK kısmi dolumda: `pair_count = min(YES_total, NO_total)`; kalan imbalance aynı pencerede işlemsiz bırakılır.
2. State doğrudan `[Done]`'a geçer; aynı pencerede yeni GTC veya averaging başlatılmaz. (Eski transient `ProfitLock` ara state'i kaldırıldı; legacy persist'ler bir tick'te `Done`'a evolve eder.)

### Binance Sinyali Etkisi

Sinyal iki yerde etkilidir:

1. **OpenDual fiyatı** — *yukarıdaki market-spread güdümlü formül.* Her taraf kendi ask'ından ±`market_spread` kayar (likit market'te küçük, illikit market'te büyük). delta=0 nötr durumda her iki bid kendi ask'ında oturur (taker eşiği); delta saturasyonunda bir taraf agresif taker, diğer taraf pasif maker olur. Toplam ≈ `1.00` ama **garanti değil**.
2. **SingleLeg averaging boyutu** (`signal_multiplier`) — averaging yapılan tarafın fiyat düşüşünü sinyal de teyit ediyorsa boyut büyütülür. **DoubleLeg averaging'de uygulanmaz** (sinyal etkisi açılış fiyatında zaten gömülü).

| `effective_score` | Averaging YES tarafı | Averaging NO tarafı |
|---|---|---|
| `8–10` (güçlü alış) | `× 1.0` (zıt — standart) | `× 1.3` (teyit) |
| `6–8` (hafif alış) | `× 0.9` | `× 1.1` |
| `4–6` (nötr) | `× 1.0` | `× 1.0` |
| `2–4` (hafif satış) | `× 1.1` (teyit) | `× 0.9` |
| `0–2` (güçlü satış) | `× 1.3` (teyit) | `× 1.0` (zıt — standart) |

- ProfitLock FAK boyutu sinyal tarafından değiştirilmez (`size = |imbalance|`).
- `signal_weight = 0` → `effective_score = 5.0` (nötr) → OpenDual her taraf kendi `ask`'ında durur (taker eşiği), averaging çarpan `× 1.0`.

### Bölge Haritası — `binance_signal` aktif mi?

| Bölge | `zone_pct` aralığı | `binance_signal` aktif |
|---|---|:---:|
| `DeepTrade` | 0 – 10 % | Evet (`ZoneSignalMap::HARVEST.0[0]`) |
| `NormalTrade` | 10 – 50 % | Evet |
| `AggTrade` | 50 – 90 % | Evet |
| `FakTrade` | 90 – 97 % | Evet |
| `StopTrade` | 97 – 100 % | Hayır (`src/strategy.rs`) |

### Bölge Bazlı Emir Kısıtları

| Bölge | Yeni GTC (OpenDual / Averaging) | FAK (ProfitLock) |
|---|:---:|:---:|
| `DeepTrade` | ✓ | ✓ |
| `NormalTrade` | ✓ | ✓ |
| `AggTrade` | ✓ | ✓ |
| `FakTrade` | ✓ | ✓ |
| `StopTrade` | ✗ | ✓ |

### Örnek Senaryo

```
Konfigürasyon: avg_threshold=0.98, order_usdc=2.0, dual_timeout=5_000 ms
               cooldown_threshold=30_000 ms (sabit), signal_weight=10
               effective_score=8.0 → delta=+0.6
               Book: yes_bid=0.51, yes_ask=0.53 → yes_spread=0.02
                     no_bid=0.43,  no_ask=0.45  → no_spread=0.02
               → up_bid   = clamp(snap(0.53 + 0.6·0.02), 0.05, 0.95) = snap(0.542) = 0.54
                 down_bid = clamp(snap(0.45 − 0.6·0.02), 0.05, 0.95) = snap(0.438) = 0.44
Market init:   api_min_order_size=5  (GET /book)

YES size = max(⌈2.0/0.54⌉, 5) = 5
NO  size = max(⌈2.0/0.44⌉, 5) = 5

T=0    [OpenDual]    POST /orders → GTC YES@0.54(5) + GTC NO@0.44(5)
                     up_bid=0.54 > yes_ask=0.53 → YES anlık agresif taker, fill_price=0.53
                     down_bid=0.44 < no_ask=0.45 → NO maker olarak kitapta bekler

T+2s   [Fill]        YES taker doldu (fill_price=0.53): avg_YES=0.53, imbalance=+5
                     hedge_leg=0.47 → ProfitLock: 0.53+0.47=1.00 > 0.98 ✗ → [SingleLeg]

T+32s  [Cooldown]    first_best_leg=0.45 < 0.53 ✓ → size=max(⌈2.0/0.45⌉,5)=5
                     ProfitLock: 1.00 > 0.98 ✗
                     → POST /order GTC YES@0.45(5)

T+37s  [Fill]        YES GTC@0.45 doldu: avg_YES=0.49, imbalance=+10
                     ProfitLock: 0.49+0.47=0.96 ≤ 0.98 ✓ → [ProfitLock]
                     → DELETE /orders [NO GTC@0.44 iptal]
                     → POST /order FAK NO@0.47(10)  [boyut=imbalance]

T+37.1s [Fill]       pair_count=10, AVG_SUM=0.96
                     profit=(1.0−0.96)×10=0.40 USDC → [Bitti]

─── Kısmi FAK ───
                     FAK NO@0.47 → 7 doldu
                     pair_count=7, kalan 3 YES pencerede işlemsiz → [Bitti]

─── Her iki OpenDual doldu ───
T+1s   [Fill×2]      imbalance=0, avg_YES=0.53, avg_NO=0.45 → AVG_SUM=0.98
                     avg_threshold kontrol: 0.98 ≤ 0.98 ✓ → [ProfitLock] (FAK yok)
                     pair_count=5, profit=(1.0−0.98)×5=0.10 USDC → [Bitti]

─── Her iki OpenDual doldu ama eşik sağlanmadı ───
T+1s   [Fill×2]      avg_YES=0.58, avg_NO=0.45 → AVG_SUM=1.03 > 0.98 ✗
                     imbalance=0 olsa da eşik sağlanmadı → [SingleLeg]
                     (Averaging yalnızca imbalance>0 ise mümkün; imbalance=0 ise pencere sonuna kadar beklenir)

─── order_usdc=10.0 ile ───
                     YES size=max(⌈10/0.53⌉,5)=19  NO size=max(⌈10/0.45⌉,5)=23
```

---

## 3. `prism`

> **Kod durumu:** `src/strategy/prism.rs` yer tutucu; bot başlatmada yalnız `harvest` seçilebilir (`src/bot/ctx.rs`).

### Binance Sinyali Etkisi

`prism`'de sinyal **giriş zamanlamasını ve eşiğini** etkiler; pozisyon boyutunu doğrudan ölçeklemez.

| `effective_score` | Yorumlama | Giriş davranışı |
|---|---|---|
| `8–10` | Güçlü alış baskısı | Giriş eşiği düşürülür → erken pozisyon |
| `6–8` | Hafif alış | Eşik normale yakın |
| `4–6` | Nötr | Normal zamanlama |
| `2–4` | Hafif satış | Giriş eşiği yükseltilir → gecikmiş giriş |
| `0–2` | Güçlü satış baskısı | Giriş pencere içinde engellenir |

- `prism` `imbalance` / `imbalance_cost` kullanmaz; `avgsum` + `profit` ile çalışır. Sinyal bu metriklere bağımsız olarak giriş kararına eklenir.
- `signal_weight = 0` → giriş eşiği değişmez (sinyal devre dışı).

### Bölge Haritası — `binance_signal` aktif mi?

| Bölge | `zone_pct` aralığı | `binance_signal` aktif |
|---|---|:---:|
| `DeepTrade` | 0 – 10 % | TBD (strateji yok) |
| `NormalTrade` | 10 – 50 % | TBD |
| `AggTrade` | 50 – 90 % | TBD |
| `FakTrade` | 90 – 97 % | TBD |
| `StopTrade` | 97 – 100 % | — (emir yok) |

<!-- Giriş koşulları, aksiyonlar ve çıkış kuralları burada -->

---

*Strateji parametreleri (`scale_up`, eşik değerleri, `signal_weight` varsayılanları) ve `dutch_book` / `prism` bölge haritaları uygulama ilerledikçe netleştirilir. `harvest` için bölge→sinyal matrisi kodda sabittir: `ZoneSignalMap::HARVEST` (`src/strategy.rs`).*
