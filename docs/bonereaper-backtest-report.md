# Bonereaper — Tam Piyasa Simülasyonu ve Sinyal Backtest Raporu

**Üretim:** `scripts/bonereaper_full_sim_backtest.py`  
**Ham çıktı:** `data/bonereaper_sim_backtest.json`  
**Veri:** Yerel `data/baiter.db` (`market_ticks` + `market_sessions`, `bots.strategy = 'bonereaper'`) ve `data/genel.log`, `genel2.log`, `gercekbotlog.log`.

---

## 1. Özet

| Metrik | Değer |
| --- | ---: |
| Simüle edilen oturum | **336** |
| Toplam simüle BUY (tick başına motor) | **10 078** |
| Oturum başına trade (medyan) | **33** |
| Toplam simüle harcama (USDC) | **458 961** |
| Proxy kazanan ile sim PnL toplamı | **−7 225 USDC** (336 piyasa, **203** pozitif oturum) |
| Log REDEEM ile oturum eşleşmesi | **0** (tarih/slug penceresi uyumsuz — bkz. §4) |

Motor mantığı `src/strategy/bonereaper.rs` ile aynı varsayılan sayısal parametreler üzerinden Python’da tekrarlandı (cooldown 8s, late winner 30s / bid≥0.85 / 1000 USDC, `max_avg_sum` 1.30, USDC bucket’ları 15 / 25 / 30, min/max bid 0.05–0.95).

---

## 2. Sinyal backtest tasarımı

### 2.1 Gerçek kazanan (ideal)

Zincir üstü çözüm + `REDEEM` tablosu yerel DB’de boş; loglardaki `activity[type=REDEEM]` ile ise yalnızca **log slug’ları** kazanılabiliyor. Yerel `baiter.db` oturumlarındaki slug’lar şu anki log dosyalarıyla **örtüşmediği** için `truth_redeem_from_logs_*` örneklem sayısı **0** kaldı.

### 2.2 Proxy kazanan (OB tabanlı)

**Son 15 saniye** içindeki **son** `market_ticks` satırında:

- `up_mid = (up_bid + up_ask) / 2`
- `down_mid = (down_bid + down_ask) / 2`
- Proxy: `Up` kazanır, eğer `up_mid ≥ down_mid`; aksi halde `Down`.

Bu, **Polymarket zincir çözümünün yerine** geçmez; yalnızca defterin kapanışa yakın fiyatlamasının hangi tarafı “favori” gösterdiğini ölçer.

### 2.3 Tahmin kuralları (ikili sınıflandırma)

Sinyal alanındaki her satır için:

| Kural | “Up” tahmini |
| --- | --- |
| `score_gt5` | `signal_score > 5` |
| `bsi_gt0` | `bsi > 0` |
| `score_and_bsi` | her iki koşul |
| `ofi_gt0` | `ofi > 0` |
| `cvd_gt0` | `cvd > 0` |

Doğruluk = tahmin yönü ile **proxy kazanan** (veya REDEEM’den türetilen kazanan) uyumu.

### 2.4 Dikkat: “same snapshot” tuzağı

Aynı tick satırındaki `signal_score` ile aynı satırdaki mid fiyatlardan üretilen “truth” kıyaslanırsa, sonuç **tahmin gücü değil**, skor ile OB iç yapısının **eşzamanlı korelasyonu** olur. Bu yüzden raporda `same_snapshot_signal_vs_mid_truth_CAUTION` ayrı etiketlendi.

### 2.5 Gerçek öngörü testleri (proxy truth = son OB)

| Test | Açıklama | n | `score>5` doğruluğu | `bsi>0` | `score∧bsi` |
| --- | --- | ---: | ---: | ---: | ---: |
| **predict_T60s** | Kapanıştan **~60 sn önceki** en yakın tick | 336 | 0.5208 | 0.5000 | 0.4970 |
| **predict_mid_window** | **Pencere ortası** (start+end)/2 zamanına en yakın tick | 336 | **0.5804** | **0.5804** | **0.5804** |

**Yorum:** Pencere ortası skoru, son OB proxy’sine göre **yaklaşık %58** isabet veriyor; T−60s profili **%52** civarında — neredeyse rastgele taban çizgisine yakın. `predict_mid_window` ile tutarlı şekilde OFI/CVD işaretinin de ~%57–58 olduğu görülüyor (birbirine bağlı bileşenler olabilir).

---

## 3. Literatür (sinyal / OFI isabeti)

Kısa vadeli order book ve **order flow imbalance (OFI)** ile fiyat hareketleri arasında literatürde **çok kısa ufuklarda** anlamlı ilişkiler bulunur; ancak:

- **Lee & Ready (1991)** — *Inferring Trade Direction from Intraday Data*: işlem yönünü bid-ask’a göre sınıflandırma; **zaman hizası** ve spread içi işlemler hatalara yol açar. Özet: [ideas.repec.org](https://ideas.repec.org/a/bla/jfinan/v46y1991i2p733-46.html)
- **Cont, Kukanov, Stoikov (2014)** — limit order book’dan türetilen OFI ile fiyat değişimleri arasında güçlü **eşzamanlı** ilişki; ileri dönük tahmin tarafında gecikme ve mikroyapı sürtünmesi kritik. Örnek derleme: [arxiv.org/pdf/1907.06230](https://arxiv.org/pdf/1907.06230)
- Pratik backtest’lerde **latency, slipaj ve kırpılmış quote** OFI sinyalinin dış örneklem performansını düşürür (örn. [hedgefundalpha.com](https://hedgefundalpha.com/strategies/implementation-evaluation-order-flow-imbalance-trading-algorithm/) özetleri).

Bu proje bağlamında `docs/signal.md`, CEX (Binance/OKX) akışından bileşik skor üretimini tarif ediyor; `market_ticks` içindeki `bsi` / `ofi` / `cvd` alanları da bu çizgide yorumlanmalıdır.

---

## 4. Log × DB uyumsuzluğu (karşılaştırma neden boş?)

- Log dosyalarındaki `*-updown-*` slug’ları ile yerel `baiter.db` içindeki session slug’ları **farklı unix pencerelerine** denk geliyor; bu nedenle `compare_summary.n_compare = 0` ve REDEEM tabanlı doğruluk satırı boş.
- **Ne yapılmalı:** Aynı tarih aralığında export edilmiş log + aynı dönemde dolu `baiter.db` (veya VPS DB kopyası) kullanılmalı; script parametreleri değişmeden tekrar çalıştırılır.

---

## 5. Implementasyon notu (`bonereaper.rs`)

Rust yorumunda “signal-driven değildir” ifadesi, **mevcut Rust karar ağacında `signal_score` okunmaması** ile uyumlu. Ancak geçmiş analizlerde (tick + gerçek trade eşlemesi olan örneklemde) **alım yönü ile `signal_score` / `bsi` arasında güçlü uyum** görüldü; bu, gerçek cüzdanın başka katmanda sinyal kullanıyor olabileceğini veya aynı mikroyapıdan türeyen OB tepkisinin sinyalle hizalandığını gösterir. Tam davranış eşlemesi için ya motor genişletilmeli ya da harici cüzdan kodu doğrulanmalıdır.

---

## 6. Komut

```bash
python3 scripts/bonereaper_full_sim_backtest.py
```

Çıktıyı güncellemek için yeterlidir; detaylı satır satır sonuçlar `data/bonereaper_sim_backtest.json` içinde `all_sim_results` altında tutulur.
