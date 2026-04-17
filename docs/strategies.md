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

---

## 1. `dutch_book`

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
| `DeepTrade` | 0 – 10 % | TBD |
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
| `up_bid` | `f64` | — | OpenDual YES tarafı limit bid fiyatı (zorunlu) |
| `down_bid` | `f64` | — | OpenDual NO tarafı limit bid fiyatı (zorunlu) |
| `avg_threshold` | `f64` | `0.98` | `avg_YES + avg_NO ≤ avg_threshold` → ProfitLock tetikler |
| `cooldown_ms` | `u64` | `30_000` | İki averaging GTC arasındaki minimum bekleme (ms) |
| `max_position_size` | `f64` | `100.0` | Tek tarafın toplam share limiti (shares) |

> **`tick_size` doğrulaması:** `up_bid` ve `down_bid`, market init'te `GET /book`'tan okunan `tick_size`'ın katı olmalıdır (örn. `tick_size=0.01` ise `0.52` geçerli, `0.525` geçersiz). Uyumsuz fiyat API tarafından `INVALID_ORDER_MIN_TICK_SIZE` hatasıyla reddedilir; bot başlatmada erken doğrulama yapılarak kullanıcı önceden bilgilendirilir. Averaging fiyatı (`first_best_leg`) orderbook'tan geldiği için doğal olarak geçerlidir.

### Tanımlar

| Terim | Anlam |
|---|---|
| `avg_YES` / `avg_NO` | Katalog `avg_up` / `avg_down` — User WS `trade` `MATCHED` event'lerinden hesaplanan taraf bazlı VWAP |
| `first_leg` | SingleLeg ProfitLock koşulundaki kısayol: **dolmuş tarafın** `avg_*` değeri (ör. yalnızca YES dolmuşsa → `avg_YES`) |
| `hedge_leg` | SingleLeg ProfitLock koşulundaki kısayol: **henüz dolmamış (hedef) tarafın** anlık `best_ask` değeri — WS `best_bid_ask`'tan okunur; FAK fiyatı olarak da kullanılır |
| `first_best_leg` | Averaging döngüsündeki kısayol: **dolmuş (tutulan) tarafın** anlık `best_bid` değeri — averaging GTC fiyatı ve düşüş koşulu için kullanılır |
| `last_fill_price` | En son `MATCHED` fill event'inin fiyatı (her kısmi fill dahil güncellenir) |
| `pair_count` | `min(YES_total, NO_total)` — tamamlanan YES+NO çift sayısı; kâr formülünde kullanılır |
| `hedge_token` | Henüz dolmamış (hedge edilecek) tarafın `asset_id`'si (clobTokenId); FAK emrinde `tokenId` alanı olarak kullanılır |
| `signal_multiplier` | `effective_score` ve averaging tarafına göre §14.4 tablosundan okunan boyut çarpanı (ör. `×1.3`); `signal_weight = 0` ise daima `1.0` |
| `imbalance` | `YES_toplam − NO_toplam` (shares) |

### Durum Makinesi

```
[Başlangıç]
    │
    ▼
[OpenDual] ── T=0: POST /orders ile YES@up_bid + NO@down_bid (effective_size her biri)
    │          Fiyat filtresi yok — koşulsuz gönderilir
    │
    ├── Her iki GTC doldu → avg_YES + avg_NO ≤ avg_threshold ?
    │       Evet → imbalance=0 → [ProfitLock] (FAK yok)
    │       Hayır → [SingleLeg] (eşik sağlanmadı)
    │
    └── Yalnızca bir taraf doldu → [SingleLeg]
            │
            │   Her fill (MATCHED) ve her orderbook güncellemesinde:
            │
            ├── first_leg + hedge_leg ≤ avg_threshold   ← ProfitLock öncelikli
            │       → [ProfitLock]: açık GTC'leri iptal (DELETE /orders)
            │                       imbalance > 0 ise FAK@hedge_leg gönderilir
            │
            ├── (cooldown bitti) + (first_best_leg < last_fill_price)
            │   + (pozisyon < max_position_size) + (bölge ≠ StopTrade)
            │       → averaging GTC gönderilir → [SingleLeg] (bekle)
            │
            └── Pencere sona erdi → [Bitti]

[ProfitLock] → pencere sonuna kadar yeni emir yok → [Bitti]
```

### Giriş — OpenDual

- **Tetik:** Market başlangıcı (`T = 0`). Fiyat veya orderbook kontrolü olmaksızın anında gönderilir.
- **Endpoint:** `POST /orders` (batch) — YES ve NO emirleri tek request'te.
- YES token: `side=BUY`, `price=up_bid`, `size=max(⌈order_usdc/up_bid⌉, api_min_order_size)`, `orderType=GTC`
- NO token: `side=BUY`, `price=down_bid`, `size=max(⌈order_usdc/down_bid⌉, api_min_order_size)`, `orderType=GTC`
- `avg_threshold` bu aşamada kontrol edilmez; ProfitLock değerlendirmesi yalnızca fill sonrasında yapılır.

**`POST /orders` kısmi başarısızlık:** Batch yanıtında bir emir `success: false` dönerse:
- Başarılı olan GTC defterde bırakılır, iptal edilmez.
- Başarısız emrin `errorMsg`'i logu yazılır.
- Başarılı emir fill olduğunda bot normal **[SingleLeg]**'e geçer — sanki yalnızca o taraf açılmış gibi davranır.

### Averaging (SingleLeg döngüsü)

Bir taraf doldu, diğer taraf hâlâ açık GTC'de bekliyorsa bot **[SingleLeg]** döngüsünde şu sırayla kontrol yapar:

1. **ProfitLock öncelik kontrolü:** `first_leg + hedge_leg ≤ avg_threshold` → ProfitLock aksiyonuna geç.
2. **Averaging koşulu** (ProfitLock yok ise):
   - Cooldown bitmedi → bekle.
   - Cooldown bitti + `first_best_leg < last_fill_price` + pozisyon < `max_position_size` + bölge ≠ StopTrade → aynı tarafa **GTC bid** emri gönder (`POST /order`, fiyat = `first_best_leg`).
   - **Averaging boyutu** (iki aşamalı):
     ```
     base_size      = max(⌈order_usdc / first_best_leg⌉, api_min_order_size)
     effective_size = max(round(base_size × signal_multiplier), api_min_order_size)
     ```
     Signal çarpanının boyutu `api_min_order_size` altına indirmesi engellenir.
   - Averaging GTC gönderildikten sonra bot [SingleLeg]'de beklemeye devam eder.
3. **Her MATCHED fill event'inde** (kısmi fill dahil): `avg_*` ve `imbalance` güncellenir; `last_fill_price` = bu fill'in fiyatına güncellenir; **cooldown sıfırlanır**.
4. Averaging turu sınırsızdır; `max_position_size` tek durucu kuraldır.

### ProfitLock Aksiyonu

**Koşul:**
- **SingleLeg** (bir taraf dolmuş): `first_leg + hedge_leg ≤ avg_threshold`
  - `first_leg` = dolmuş tarafın VWAP (örn. yalnızca YES dolmuşsa → `avg_YES`)
  - `hedge_leg` = henüz dolmamış tarafın anlık `best_ask` (WS `best_bid_ask`'tan; FAK fiyatı olarak kullanılır)
- **Her iki taraf dolmuş** (OpenDual tam dolum): `avg_YES + avg_NO ≤ avg_threshold`
  - `avg_YES` ve `avg_NO` = her iki tarafın MATCHED fill VWAP'ı

Adımlar:

1. Dolu olmayan taraftaki tüm açık GTC'lerin ID'leri toplanır → `DELETE /orders` (order ID listesi) ile iptal edilir.
2. **imbalance > 0** ise: karşı tarafa `POST /order` ile **FAK** gönderilir.
   - `side=BUY`, `tokenId=hedge_token`, `price=hedge_leg`, `size=imbalance`, `orderType=FAK`
   - FAK kısmi dolumda: `pair_count = min(YES_total, NO_total)`; kalan imbalance aynı pencerede işlemsiz bırakılır.
3. **imbalance = 0** (her iki OpenDual eşit doldu) ise: FAK gönderilmez, doğrudan ProfitLock.
4. ProfitLock sonrası aynı pencerede yeni GTC veya averaging başlatılmaz.

### Binance Sinyali Etkisi

`harvest` delta-nötr stratejisidir; sinyal **yön filtresi uygulamaz**, yalnızca averaging GTC boyutunu etkiler. Averaging yapılan tarafın fiyat düşüşünü Binance sinyali de teyit ediyorsa boyut büyütülür; sinyal zıt yöndeyse standart boyut korunur.

| `effective_score` | Averaging YES tarafı | Averaging NO tarafı |
|---|---|---|
| `8–10` (güçlü alış) | `× 1.0` (zıt — standart) | `× 1.3` (teyit) |
| `6–8` (hafif alış) | `× 0.9` | `× 1.1` |
| `4–6` (nötr) | `× 1.0` | `× 1.0` |
| `2–4` (hafif satış) | `× 1.1` (teyit) | `× 0.9` |
| `0–2` (güçlü satış) | `× 1.3` (teyit) | `× 1.0` (zıt — standart) |

- `OpenDual` emirleri ve FAK boyutu sinyal tarafından değiştirilmez.
- `signal_weight = 0` → çarpan her zaman `× 1.0`.

### Bölge Haritası — `binance_signal` aktif mi?

| Bölge | `zone_pct` aralığı | `binance_signal` aktif |
|---|---|:---:|
| `DeepTrade` | 0 – 10 % | TBD |
| `NormalTrade` | 10 – 50 % | TBD |
| `AggTrade` | 50 – 90 % | TBD |
| `FakTrade` | 90 – 97 % | TBD |
| `StopTrade` | 97 – 100 % | — |

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
Konfigürasyon: avg_threshold=0.98, order_usdc=2.0, cooldown_ms=30_000
               up_bid=0.52, down_bid=0.44
Market init:   api_min_order_size=5  (GET /book)

YES size = max(⌈2.0/0.52⌉, 5) = 5
NO  size = max(⌈2.0/0.44⌉, 5) = 5

T=0    [OpenDual]    POST /orders → GTC YES@0.52(5) + GTC NO@0.44(5)

T+2s   [Fill]        YES GTC doldu: avg_YES=0.52, imbalance=+5
                     hedge_leg=0.47 → ProfitLock: 0.52+0.47=0.99 > 0.98 ✗ → [SingleLeg]

T+32s  [Cooldown]    first_best_leg=0.45 < 0.52 ✓ → size=max(⌈2.0/0.45⌉,5)=5
                     ProfitLock: 0.99 > 0.98 ✗
                     → POST /order GTC YES@0.45(5)

T+37s  [Fill]        YES GTC@0.45 doldu: avg_YES=0.485, imbalance=+10
                     ProfitLock: 0.485+0.47=0.955 ≤ 0.98 ✓ → [ProfitLock]
                     → DELETE /orders [NO GTC@0.44 iptal]
                     → POST /order FAK NO@0.47(10)  [boyut=imbalance]

T+37.1s [Fill]       pair_count=10, AVG_SUM=0.955
                     profit=(1.0−0.955)×10=0.45 USDC → [Bitti]

─── Kısmi FAK ───
                     FAK NO@0.47 → 7 doldu
                     pair_count=7, kalan 3 YES pencerede işlemsiz → [Bitti]

─── Her iki OpenDual doldu ───
T+1s   [Fill×2]      imbalance=0, avg_YES=0.52, avg_NO=0.44 → AVG_SUM=0.96
                     avg_threshold kontrol: 0.96 ≤ 0.98 ✓ → [ProfitLock] (FAK yok)
                     pair_count=5, profit=(1.0−0.96)×5=0.20 USDC → [Bitti]

─── Her iki OpenDual doldu ama eşik sağlanmadı ───
T+1s   [Fill×2]      avg_YES=0.58, avg_NO=0.45 → AVG_SUM=1.03 > 0.98 ✗
                     imbalance=0 olsa da eşik sağlanmadı → [SingleLeg]
                     (Averaging yalnızca imbalance>0 ise mümkün; imbalance=0 ise pencere sonuna kadar beklenir)

─── order_usdc=10.0 ile ───
                     YES size=max(⌈10/0.52⌉,5)=20  NO size=max(⌈10/0.44⌉,5)=23
```

---

## 3. `prism`

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
| `DeepTrade` | 0 – 10 % | TBD |
| `NormalTrade` | 10 – 50 % | TBD |
| `AggTrade` | 50 – 90 % | TBD |
| `FakTrade` | 90 – 97 % | TBD |
| `StopTrade` | 97 – 100 % | — (emir yok) |

<!-- Giriş koşulları, aksiyonlar ve çıkış kuralları burada -->

---

*Strateji parametreleri (`scale_up`, eşik değerleri, `signal_weight` varsayılanları), bölge haritası `true`/`false` değerleri ve tam formüller implementasyon sırasında netleştirilir.*
