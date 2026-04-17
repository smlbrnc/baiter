# Polymarket + Rust — Güncel Kütüphane Rehberi

> **Amaç:** Gamma (metadata) ve CLOB (emir defteri / trade) entegrasyonu için Rust tarafında kullanılacak crate’leri, **resmi paket deposu ve proje siteleri** üzerinden doğrulanmış sürümlerle listeler.

---

## Teyit yöntemi ve resmi kaynaklar (derin doğrulama)

| Ne | Nereden |
|----|---------|
| **Sürüm numaraları (canonical)** | [crates.io](https://crates.io) — Rust topluluğunun resmi paket kaydı ([Rust projesi](https://www.rust-lang.org/) ile ilişkili; [Cargo book](https://doc.rust-lang.org/cargo/) bağımlılık çözümlemesi burayı kullanır). Her crate için `GET https://crates.io/api/v1/crates/<isim>` JSON alanı `crate.max_version`; belirli sürüm için `.../crates/<isim>/<versiyon>` içinde `rust_version` (MSRV). |
| **API dokümantasyonu (Rust)** | [docs.rs](https://docs.rs/) — crates.io ile entegre, `cargo doc` ile aynı semantik. |
| **Araç zinciri** | [rust-lang.org — Tools](https://www.rust-lang.org/tools) (Cargo, crates.io atfı). |
| **Tokio** | [tokio.rs](https://tokio.rs/) · [docs.rs/tokio](https://docs.rs/tokio). |
| **Serde** | [serde.rs](https://serde.rs/) · [docs.rs/serde](https://docs.rs/serde). |
| **reqwest** | [docs.rs/reqwest](https://docs.rs/reqwest) · [GitHub seanmonstar/reqwest](https://github.com/seanmonstar/reqwest) (releases / changelog). |
| **Axum / Tower** | [crates.io/axum](https://crates.io/crates/axum) · [GitHub tokio-rs/axum](https://github.com/tokio-rs/axum) · [tower-rs/tower](https://github.com/tower-rs/tower). |
| **Alloy** | [alloy.rs](https://alloy.rs/) · [GitHub alloy-rs/alloy](https://github.com/alloy-rs/alloy) · crates.io `alloy` sürüm sayfası (`rust_version` alanı). |
| **rustls** | [docs.rs/rustls](https://docs.rs/rustls) · [GitHub rustls/rustls](https://github.com/rustls/rustls). |
| **Polymarket CLOB SDK** | crates.io [polymarket-client-sdk](https://crates.io/crates/polymarket-client-sdk) — `repository`: [github.com/polymarket/rs-clob-client](https://github.com/polymarket/rs-clob-client) (resmi depo başlığı: *Polymarket Rust CLOB Client*). |

**Son otomatik API taraması:** **17 Nisan 2026** — aşağıdaki `max_version` değerleri bu tarihte `crates.io` API ile çekilmiştir. Üretim öncesi `cargo update` ve `cargo tree --duplicates` çalıştırın; tekrarlanabilirlik için **`Cargo.lock`** kullanın.

---

## İlgili API dokümanları (bu repo)

| Konu | Dosya |
|------|--------|
| Gamma REST (events, markets, tags, …) | [docs/api/polymarket-gamma.md](api/polymarket-gamma.md) |
| CLOB REST + WebSocket (orderbook, emir, imza) | [docs/api/polymarket-clob.md](api/polymarket-clob.md) |
| Bot mimarisi, SQLite, ortak metrik kataloğu, `Strategy` ↔ metrik Rust taslağı | [docs/bot-platform-mimari.md](bot-platform-mimari.md) («Ortak metrik kataloğu», «Rust uygulama taslağı») |

Resmi üst kaynak (API referansı): [docs.polymarket.com](https://docs.polymarket.com/api-reference).

---

## Düşük gecikme + hafif API sunucusu (özet strateji)

Ağ gecikmesi (RTT) ve TLS el sıkışması genelde milisaniyeleri belirler; crate seçimi **CPU ve gereksiz allocations** tarafını sadeleştirir. Resmi dokümantasyona dayalı pratikler:

| Hedef | Öneri | Kaynak |
|--------|--------|--------|
| **Outbound HTTP (Gamma / CLOB)** | Uygulama genelinde **tek paylaşımlı** [`reqwest::Client`](https://docs.rs/reqwest/latest/reqwest/struct.Client.html) — bağlantı havuzu TLS tekrarını azaltır | [reqwest ClientBuilder](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html): `pool_max_idle_per_host`, `pool_idle_timeout`, `tcp_nodelay`, `tcp_keepalive`, `connect_timeout` |
| **Nagle** | Düşük gecikme için `tcp_nodelay(true)` değerlendirin (küçük mesajlarda RTT’yi sıkıştırmaz) | Aynı `ClientBuilder` sayfası |
| **HTTP/2** | Sunucu destekliyorsa `reqwest` içinde `http2` özelliği (çoklu istek için tek bağlantı çoğullaması) | [crates.io reqwest features](https://crates.io/crates/reqwest) |
| **Sıkıştırma** | Küçük JSON cevaplarda CPU maliyeti: gerekiyorsa [`no_gzip()`](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#method.no_gzip) ile otomatik gzip çözümünü kapatın | docs.rs |
| **Async runtime** | `tokio` için **`full` değil**, yalnızca ihtiyaç duyulan feature’lar (daha az derleme süresi ve daha küçük ikili) | [Tokio](https://tokio.rs/) — [docs.rs/tokio](https://docs.rs/tokio) |
| **Gelen HTTP API** | İnce katman: **Axum** (Hyper üzerinde; resmi olarak “hyper ile karşılaştırılabilir” performans iddiası) | [crates.io/axum](https://crates.io/crates/axum), [GitHub tokio-rs/axum](https://github.com/tokio-rs/axum) |
| **Orta katman** | Gecikmeyi ölçmek için `tower`/`tower-http`; sıcak yolda **gereksiz** compression / ağır middleware eklemeyin | [Tower](https://github.com/tower-rs/tower) |
| **JSON** | Varsayılan: **serde_json** (stabil, ekosistem uyumu). Daha agresif parse için isteğe bağlı: [simd-json](https://crates.io/crates/simd-json) **0.17.0**, [sonic-rs](https://crates.io/crates/sonic-rs) **0.5.8** — API farklılıkları ve güvenilirlik testi gerekir | crates.io `max_version` ile teyit |

**İleri seviye (daha az soyutlama):** Doğrudan [hyper](https://crates.io/crates/hyper) **1.9.0** + [hyper-util](https://crates.io/crates/hyper-util) **0.1.20** + [hyper-rustls](https://crates.io/crates/hyper-rustls) **0.27.9** ile istemci yazmak mümkün ([crates.io](https://crates.io) API ile aynı tarihte doğrulandı); **reqwest** bunların üzerinde kolaylık katmanıdır. Bakım maliyeti ve kod hacmi artar.

---

## Resmi Polymarket Rust SDK

| Crate | crates.io `max_version` | Depo (crates.io `repository`) |
|--------|-------------------------|------|
| [polymarket-client-sdk](https://crates.io/crates/polymarket-client-sdk) | **0.4.4** | [github.com/polymarket/rs-clob-client](https://github.com/polymarket/rs-clob-client) |

CLOB auth ve emir akışı için **stabil** seçenek; kendi `reqwest` ayarlarınızı tam kontrol etmek istiyorsanız SDK + özel HTTP katmanı birleşimi veya tamamen elle entegrasyon tercih edilir. SDK bağımlılık ağacı: `cargo tree -p polymarket-client-sdk`. Çevrimiçi API: [docs.rs/polymarket-client-sdk](https://docs.rs/polymarket-client-sdk).

Gamma tarafı genelde **doğrudan REST** (`gamma-api.polymarket.com`) ile **serde** kullanılır.

---

## Rust toolchain (MSRV çakışması)

MSRV, crates.io sürüm nesnesindeki **`rust_version`** alanından okunur (`GET .../crates/<isim>/<versiyon>`).

| Bileşen | Sürüm | `rust_version` (crates.io) |
|---------|--------|----------------------------|
| [alloy](https://crates.io/crates/alloy/2.0.0) | 2.0.0 | **1.91** |
| [axum](https://crates.io/crates/axum/0.8.9) | 0.8.9 | **1.80** |

Projede hem **alloy 2** hem **axum** varsa `rust-version` pratikte **1.91** olmalıdır (en yüksek gereksinim kazanır).

```toml
[package]
rust-version = "1.91"
```

- Alloy: [alloy.rs](https://alloy.rs/) · [docs.rs/alloy](https://docs.rs/alloy/latest/alloy/)

---

## Çekirdek async ve HTTP (`max_version` — crates.io API)

| Crate | Sürüm | Resmi kaynak |
|--------|--------|----------------|
| [tokio](https://crates.io/crates/tokio) | **1.52.1** | [tokio.rs](https://tokio.rs/) |
| [reqwest](https://crates.io/crates/reqwest) | **0.13.2** | [docs.rs/reqwest](https://docs.rs/reqwest/) |
| [serde](https://crates.io/crates/serde) | **1.0.228** | [serde.rs](https://serde.rs/) |
| [serde_json](https://crates.io/crates/serde_json) | **1.0.149** | [docs.rs/serde_json](https://docs.rs/serde_json/) |

**Hafif `reqwest` örneği** (TLS: rustls, JSON, HTTP/2 — `default-features = false` ile varsayılan `default-tls` / sistem proxy vb. siz seçersiniz):

```toml
reqwest = { version = "0.13.2", default-features = false, features = ["json", "rustls", "http2"] }
```

[CHANGELOG / sürüm notları](https://github.com/seanmonstar/reqwest/releases).

---

## Gelen API: Axum + Tower (hafif, güncel)

| Crate | Sürüm | Not |
|--------|--------|-----|
| [axum](https://crates.io/crates/axum) | **0.8.9** | MSRV 1.80; ince routing katmanı, Hyper tabanlı |
| [tower](https://crates.io/crates/tower) | **0.5.3** | `Service` / middleware |
| [tower-http](https://crates.io/crates/tower-http) | **0.6.8** | CORS, trace, compression — **latency odaklı** serviste compression’ı yalnız gerektiğinde açın |

Örnek çalıştırma: [Axum README — `axum::serve`](https://github.com/tokio-rs/axum).

---

## WebSocket (CLOB kanalları)

| Crate | Sürüm |
|--------|--------|
| [tokio-tungstenite](https://crates.io/crates/tokio-tungstenite) | **0.29.0** |
| [tungstenite](https://crates.io/crates/tungstenite) | **0.29.0** |

```toml
tokio-tungstenite = { version = "0.29.0", features = ["rustls-tls-webpki-roots"] }
```

Düşük gecikme: mümkünse **tek uzun ömürlü** WebSocket; gereksiz yeniden bağlanmayı önleyin ([docs.rs](https://docs.rs/tokio-tungstenite)).

---

## İmza ve zincir (EIP-712, Polygon)

| Crate | Sürüm | Not |
|--------|--------|-----|
| [alloy](https://crates.io/crates/alloy) | **2.0.0** | EVM / EIP-712 ekosistemi |
| [k256](https://crates.io/crates/k256) | **0.13.4** önerilir | `max_version` şu an **0.14.0-rc.*** (ön sürüm); kararlı kanalda ilk sürüm **0.13.4** — üretimde `k256 = "=0.13.4"` ile pin’leyin; **0.14** stabil çıktığında crates.io’yu yeniden kontrol edin |

---

## L2 HMAC ve yardımcı kripto

| Crate | Sürüm |
|--------|--------|
| [hmac](https://crates.io/crates/hmac) | **0.13.0** |
| [sha2](https://crates.io/crates/sha2) | **0.11.0** |
| [base64](https://crates.io/crates/base64) | **0.22.1** |
| [hex](https://crates.io/crates/hex) | **0.4.3** |

---

## Serileştirme, zaman, URL, hata

| Crate | Sürüm |
|--------|--------|
| [uuid](https://crates.io/crates/uuid) | **1.23.1** |
| [chrono](https://crates.io/crates/chrono) | **0.4.44** |
| [url](https://crates.io/crates/url) | **2.5.8** |
| [http](https://crates.io/crates/http) | **1.4.0** |
| [thiserror](https://crates.io/crates/thiserror) | **2.0.18** |
| [anyhow](https://crates.io/crates/anyhow) | **1.0.102** |
| [tracing](https://crates.io/crates/tracing) | **0.1.44** |
| [tracing-subscriber](https://crates.io/crates/tracing-subscriber) | **0.3.23** |
| [dotenvy](https://crates.io/crates/dotenvy) | **0.15.7** |

---

## TLS (istemci tarafı)

[rustls](https://crates.io/crates/rustls) — `max_version` bazen **0.24.0-dev.*** gibi ön sürümler gösterebilir; **kararlı** en yüksek sürüm listesinde **0.23.38** (crates.io sürüm listesinde `dev`/`rc`/`alpha` filtrelenerek). `reqwest` TLS’yi kendi [feature](https://crates.io/crates/reqwest) setiyle bağlar (`rustls`). Düşük seviye referans: [docs.rs/rustls](https://docs.rs/rustls).

---

## `Cargo.toml` — hafif API + paylaşımlı HTTP istemcisi (Polymarket’a çıkan servis)

Outbound isteklerde **tek `Client`** kullanımını kodda `OnceLock` veya uygulama durumu ile paylaşın; aşağıda yalnızca bağımlılıklar:

```toml
[package]
name = "baiter-pro"
version = "0.1.0"
edition = "2021"
rust-version = "1.91"

[dependencies]
# Sunucu (gelen API)
axum = { version = "0.8.9", default-features = false, features = ["http1", "json", "tokio", "macros"] }
tower = "0.5.3"
tokio = { version = "1.52.1", features = ["rt-multi-thread", "macros", "net"] }

# Polymarket / HTTP çıkışı
reqwest = { version = "0.13.2", default-features = false, features = ["json", "rustls", "http2"] }
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.149"

thiserror = "2.0.18"
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
```

`axum` default feature set’inden feragat edip `http1` + `json` ile sınırlamak derlemeyi hafifletir; HTTP/2 gerekiyorsa [crates.io axum features](https://crates.io/crates/axum) içinden `http2` ekleyin.

---

## `Cargo.toml` — resmi SDK + Gamma HTTP

```toml
polymarket-client-sdk = "0.4.4"
reqwest = { version = "0.13.2", default-features = false, features = ["json", "rustls", "http2"] }
tokio = { version = "1.52.1", features = ["rt-multi-thread", "macros", "net"] }
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.149"
thiserror = "2.0.18"
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
```

---

## `Cargo.toml` — SDK yok, elle CLOB + imza

`tokio` için **`full` kullanmayın**; WebSocket gerekiyorsa `net` + gerekirse ek feature ekleyin.

```toml
tokio = { version = "1.52.1", features = ["rt-multi-thread", "macros", "net", "time", "sync"] }
reqwest = { version = "0.13.2", default-features = false, features = ["json", "rustls", "http2"] }
tokio-tungstenite = { version = "0.29.0", features = ["rustls-tls-webpki-roots"] }
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.149"
hmac = "0.13.0"
sha2 = "0.11.0"
base64 = "0.22.1"
alloy = { version = "2.0.0", default-features = false, features = ["std", "eip712", "signer-local", "sol-types"] }
k256 = "=0.13.4"
thiserror = "2.0.18"
```

`alloy` için `full` yerine ihtiyaç duyduğunuz feature’ları seçmek derleme süresini ve ikili boyutunu düşürür ([Alloy](https://alloy.rs/)); tam liste: crates.io `alloy` features.

---

## Polymarket işlev → bileşen

| İş | Öneri |
|----|--------|
| Market / event, `clobTokenIds` | Paylaşımlı `reqwest::Client` → Gamma |
| Orderbook / fiyat (public) | Aynı client → CLOB public |
| Emir / trade | `polymarket-client-sdk` veya elle imza + `reqwest` |
| Canlı akış | `tokio-tungstenite`, tek bağlantı |
| Gelen REST API | Axum + mümkün olduğunca ince middleware |

---

## Sürümleri yenileme

```bash
cargo search axum --limit 1
cargo info reqwest
```

[`cargo search`](https://doc.rust-lang.org/cargo/commands/cargo-search.html) ve `cargo info` crates.io indeksini kullanır. İsteğe bağlı: [cargo-edit](https://github.com/killercup/cargo-edit) `cargo upgrade`.

---

## Tek bakışta: crates.io `max_version` (17 Nisan 2026 API teyidi)

| Crate | max_version |
|--------|-------------|
| tokio | 1.52.1 |
| reqwest | 0.13.2 |
| hyper | 1.9.0 |
| hyper-util | 0.1.20 |
| hyper-rustls | 0.27.9 |
| axum | 0.8.9 |
| tower | 0.5.3 |
| tower-http | 0.6.8 |
| tokio-tungstenite | 0.29.0 |
| tungstenite | 0.29.0 |
| serde | 1.0.228 |
| serde_json | 1.0.149 |
| simd-json | 0.17.0 |
| sonic-rs | 0.5.8 |
| polymarket-client-sdk | 0.4.4 |
| alloy | 2.0.0 |
| rustls (kararlı, listeden) | 0.23.38 |
| k256 (kararlı ilk) | 0.13.4 |

*Özet tablo, `https://crates.io/api/v1/crates/<isim>` yanıtlarından üretilmiştir; `rustls` ve `k256` için üstteki filtre notları geçerlidir.*

---

*Güncel canonical sürüm her zaman [crates.io](https://crates.io) üzerindedir; bu dosya belirli bir tarihli API çıktısına dayanır.*
