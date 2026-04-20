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

> **Final implementasyon dokümanı:** [harvest-v2.md](harvest-v2.md). Bu bölüm yüksek-seviye özettir; akış kuralları, fiyat formülleri, hedge update protokolü, senaryolar ve migration notları için v2 dokümanına bakın.

### Temel Kavram

`harvest`, **bölge bazlı dual davranışlı** stratejidir. Erken fazda riski kilitler (DeepTrade — sadece açılış), orta fazda hâlâ uygun fiyatla pair tamamlamayı dener (NormalTrade — averaging-down ile `avg_filled_side` düşür), son fazda trend varsa sırtlar (AggTrade/FakTrade — yükselen tarafa pyramiding), pencere kapanırken durdurur (StopTrade — tüm açıklar iptal).

**Pair tamamlanırsa** (her iki tarafta da fill varsa) profit:

```
pair_count    = min(shares_yes, shares_no)
pair_avg_cost = (avg_yes + avg_no) / 2
profit        = (1 − pair_avg_cost) × pair_count
```

### Konfigürasyon

| Parametre | Tip | Varsayılan | Rol |
|---|---|---|---|
| `order_usdc` | `f64` | 5.0 | Emir başına notional (size = `max(⌈order_usdc/price⌉, api_min_order_size)`) |
| `avg_threshold` | `f64` | 0.98 | Pair maliyet tavanı; OpenPair'de hedge fiyatı (`avg_threshold − open_price`) ve NormalTrade hedge update formülünün (`avg_threshold − avg_filled_side`) tek kaynağı |
| `cooldown_threshold` | `u64` (ms) | 30_000 | (a) iki avg arası min süre, (b) açık avg GTC max yaşı, (c) bölgeler arası paylaşımlı tek `last_averaging_ms` |
| `signal_weight` | `f64` (0–10) | 0 | Composite skor + Binance harmanı; `delta` ölçeği |
| `min_price` / `max_price` | `f64` | 0.05 / 0.95 | Tüm emirler bu aralıkta clamp + tick snap |

Kaldırılan v1 parametreleri: `dual_timeout`, `max_position_size`. Eski persist payload'larda varsa Serde `default = ...` ile yutulur.

### Bölge davranış özeti

| Bölge | `zone_pct` | OpenPair | NormalTrade avg-down | Pyramiding | Hedge update | StopTrade iptal |
|---|---|:-:|:-:|:-:|:-:|:-:|
| `DeepTrade` | 0–10 % | ✓ (ilk tick) | ✗ | ✗ | ✓ | ✗ |
| `NormalTrade` | 10–50 % | ✓ (geç açılış) | ✓ | ✗ | ✓ | ✗ |
| `AggTrade` | 50–90 % | ✓ (geç açılış) | ✗ | ✓ | ✓ | ✗ |
| `FakTrade` | 90–97 % | ✓ (geç açılış) | ✗ | ✓ | ✓ | ✗ |
| `StopTrade` | 97–100 % | ✗ | ✗ | ✗ | ✗ | ✓ (tüm açıklar) |

**State machine** (özet, ayrıntı için bkz. [harvest-v2.md §4](harvest-v2.md#4-state-machine)):

```
Pending → OpenPair → PositionOpen ↔ HedgeUpdating → PairComplete → Done
         │                                              ↑
         └─→ PairComplete (iki taraf aynı tick'te taker)─┘
```

### Sinyal etkisi (özet)

`composite_score ∈ [0, 10]` (RTDS + Binance harmanı; `signal_weight=0` → 5.0 nötr). `delta = (composite − 5)/5 × spread`.

- **OpenPair:** skor > 5 → açılan taraf = UP @ `yes_ask + delta`, hedge DOWN @ `avg_threshold − open_price`. Skor < 5 simetrik. Nötr → her iki tarafa `best_bid` (delta=0).
- **NormalTrade avg-down:** sinyal-bağımsız (sadece `best_bid`).
- **Pyramiding:** `best_ask(rising_side) + |delta|` (yön taraf seçimiyle, `|delta|` agresiflik).

`signal_weight = 0` davranışı: nötr OpenPair + saf taker pyramiding.

### Bölge × `binance_signal` aktiflik haritası

| Bölge | `binance_signal` aktif |
|---|:---:|
| `DeepTrade` | Evet (`ZoneSignalMap::HARVEST.0[0]`) |
| `NormalTrade` | Evet |
| `AggTrade` | Evet |
| `FakTrade` | Evet |
| `StopTrade` | Hayır (`src/strategy.rs`) |

> **Detaylı senaryolar, hedge update'in 9 adımlı sequential-safe akışı, partial hedge fill imbalance perspektifi ve migration notları:** [harvest-v2.md](harvest-v2.md).

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
