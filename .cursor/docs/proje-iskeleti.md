# Proje İskeleti ve Dizin Yapısı

> Bu doküman, [bot-platform-mimari.md](bot-platform-mimari.md) ve [strategies.md](strategies.md) ile uyumlu **hafif, hızlı ve sade** Rust backend + React frontend iskeletini tanımlar.
>
> **Tasarım felsefesi:** Trading botunda runtime performansı **kod organizasyonuna değil**, event loop sıkılığına, connection pooling'e ve tek WS→karar→emir yoluna bağlıdır. Bu nedenle iskelet **sığ hiyerarşi + büyük modüller** ilkesiyle tasarlandı — az dosya, hızlı geliştirme, düşük bilişsel yük.
>
> **Çelişki durumunda** her zaman [bot-platform-mimari.md](bot-platform-mimari.md) (ana mimari) ve [docs.polymarket.com](https://docs.polymarket.com/) (resmi API) geçerlidir.

---

## 1. Tasarım Kararları

| Karar | Seçim | Gerekçe |
|---|---|---|
| **Rust düzeni** | **Tek crate, 2 binary** (`src/bin/supervisor.rs`, `src/bin/bot.rs`) + paylaşılan `src/lib.rs` | Workspace overhead'i yok; aynı lib her iki binary için tek derlemede hazır. Kod ayrımı dizin yerine **binary dosyaları** ile sağlanır. |
| **Modül granülerliği** | **Geniş modüller** (1 konu = 1 dosya; `mod.rs` zinciri yok) | Az dosya = hızlı navigasyon; küçük helper'lar ayrı dosyaya taşınmaz |
| **SQLite** | `sqlx` (async, compile-time query, migrations) | Fire-and-forget yazım (§Kural 4) + async uyumu |
| **Frontend** | React + Vite + TypeScript + **shadcn/ui** + Tailwind | Form/tablo/modal = shadcn'in güçlü alanı; copy-paste ownership (runtime bloat yok); dark mode ücretsiz |
| **Chart stack** | **lightweight-charts** (TradingView) canlı akış + shadcn `Chart` özet | Canvas tabanlı; 1000+ nokta real-time; kriptonun standart kütüphanesi (Apache 2.0) |
| **PriceChart yerleşimi** | Tek chart / 4 Line: `YES bid` (yeşil kalın), `YES ask` (yeşil ince), `NO bid` (kırmızı kalın), `NO ask` (kırmızı ince). X-ekseni = Gamma `startDate`→`endDate`; tick = WS `timestamp` | YES↔NO komplementerliği (toplam ≈ 1) tek bakışta; bid/ask kalınlık konvansiyonu |
| **SpreadChart yerleşimi** | Ayrı chart / 2 Histogram: YES spread (yeşil), NO spread (kırmızı) | Fiyat ölçeğinden (`0-1`) ayrı eksen; likidite göstergesi PriceChart'ı kalabalıklaştırmaz |
| **Polymarket WS → chart map** | Her `best_bid_ask` event'i **tek `asset_id`** içindir; YES ve NO iki ayrı event akışı; backend `side: YES\|NO` olarak normalize eder | Ham API alanları: `best_bid`, `best_ask`, `spread`, `timestamp` (ms) — "up_bid" gibi alanlar yok |
| **HTTP sunucu** | Axum 0.8.9 | Mevcut [Cargo.toml](../Cargo.toml) zaten içeriyor |
| **WebSocket** | `tokio-tungstenite` | Market + User + Binance aggTrade |
| **EIP-712 imza** | `alloy` 2.0.0 | CLOB order signing + L1 auth |
| **HMAC-SHA256** | `hmac` + `sha2` + `base64` (URL_SAFE) | L2 auth header |
| **CLOB SDK** | **Elle entegrasyon** | Tam kontrol (§⚡ Kural 3) |
| **Kritik event push (bot→supervisor)** | stdout JSON satırı (`[[EVENT]] {json}`), `ipc::emit` | Mekanizma + akış + `FrontendEvent`: bkz. [mimari §⚡ Kural 5](bot-platform-mimari.md), kod `src/ipc.rs` |
| **Credential saklama** | SQLite `bot_credentials` **plaintext**; Live'da **yalnız** bu tablo | `.env` `POLY_*` bot binary tarafından okunmaz; bkz. [mimari §1](bot-platform-mimari.md) + [§9a](bot-platform-mimari.md) |
| **Runtime path'ler** | Çevre değişkenleri (`DB_PATH`, `BOT_BINARY`, `HEARTBEAT_DIR`) + default fallback | Tam anahtar listesi + SIGTERM shutdown: bkz. [mimari §18 Runtime Konfigürasyonu](bot-platform-mimari.md) |

**Bırakılan / reddedilen alternatifler:**
- ~~Cargo workspace (3 crate)~~ → Tek binary → ayrı PID modeli için yeterli; workspace'in getirdiği `Cargo.toml` x 4 overhead'i yok.
- ~~Tek dosyalık `engine.rs` / `db.rs`~~ → Üretim kodu `src/engine/`, `src/db/`, `src/bot/` alt modüllere ayrıldı (okunabilirlik); kritik yol davranışı aynı §⚡ kurallarıyla korunur.
- ~~Ayrı `mod.rs` + `models.rs` dosyaları~~ → Tipler kendi kullanıldıkları dosyada.
- ~~Unix Domain Socket~~ → §1 IPC kuralını ihlal eder.
- ~~SQLite events tablosu + 50 ms polling~~ → §⚡ Kural 5 "anında" şartını karşılamaz.
- ~~tokio::sync::broadcast (process-arası)~~ → Tokio broadcast channel yalnızca process-local.

---

## 2. Yüksek Seviye Topoloji

```
┌───────────────────────────────────────────────────────────────┐
│                  React Frontend (Vite, düz yapı)              │
│             1 sn polling + SSE push (EventSource)             │
└────────────────────────┬──────────────────────────────────────┘
                         │ HTTP / SSE (tek port)
                         ▼
┌───────────────────────────────────────────────────────────────┐
│         supervisor binary  (src/bin/supervisor.rs)            │
│   Axum API + SSE broker + process spawner + log-tail          │
│                         │                                     │
│                    SQLite (WAL)                               │
└───────────────┬─────────┴──────────┬─────────────┬────────────┘
                │ spawn      heartbeat / logs
                ▼                    ▼             ▼
         ┌────────────┐       ┌────────────┐ ┌────────────┐
         │   bot-A    │       │   bot-B    │ │   bot-C    │
         │ (bot bin)  │       └────────────┘ └────────────┘
         └─────┬──────┘
               │
               ├── Gamma REST        (market keşfi)
               ├── CLOB REST         (POST /order, DELETE, heartbeat)
               ├── Market WS         (book, price_change, best_bid_ask, resolved)
               ├── User WS           (order, trade)            [live only]
               └── Binance aggTrade  (signal: CVD/BSI/OFI → 0-10)
```

İki binary aynı kodu paylaşır (`src/lib.rs`); sadece girişleri farklı.

---

## 3. Dizin Yapısı (mevcut repo)

```
baiter-pro/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml              # channel = "1.91"
├── README.md
├── .env.example
├── .gitignore
│
├── docs/
│   ├── bot-platform-mimari.md
│   ├── strategies.md
│   ├── staging-runbook.md
│   ├── rust-polymarket-kutuphaneler.md
│   ├── proje-iskeleti.md            # Bu doküman
│   └── api/
│       ├── polymarket-clob.md
│       └── polymarket-gamma.md
│
├── migrations/
│   ├── 0001_init.sql
│   ├── 0002_snapshots.sql
│   ├── 0003_price_bounds.sql
│   ├── 0004_cooldown_threshold.sql
│   └── 0005_pnl_pair_count.sql
│
├── src/
│   ├── lib.rs                       # crate kök modülleri (re-export minimal)
│   ├── bin/supervisor.rs            # Axum + migrate + restart_previously_running
│   ├── bin/bot.rs                   # İnce kabuk → bot::run()
│   ├── config.rs                    # BotConfig, Credentials, RuntimeEnv, StrategyParams
│   ├── types.rs                     # Outcome, Side, OrderType, Strategy, RunMode, …
│   ├── slug.rs                      # {asset}-updown-{interval}-{ts}
│   ├── error.rs
│   ├── time.rs                      # now_ms, zone_pct, MarketZone
│   ├── ipc.rs                       # FrontendEvent, [[EVENT]], log_line (ET timestamp)
│   ├── api.rs                       # /api/health, /api/bots, /api/bots/:id/{logs,pnl,session}, /api/events (SSE)
│   ├── supervisor.rs                # spawn, stdout tail, broadcast, backoff
│   ├── binance.rs
│   ├── polymarket.rs                # facade + gamma/clob/ws/order/auth
│   ├── polymarket/
│   ├── strategy.rs                  # MetricMask, ZoneSignalMap, Decision, PlannedOrder, …
│   ├── strategy/
│   │   ├── metrics.rs
│   │   ├── order.rs
│   │   ├── harvest/                 # dual, single, profit_lock, state, tests
│   │   ├── dutch_book.rs            # yer tutucu
│   │   └── prism.rs                 # yer tutucu
│   ├── engine/
│   │   ├── mod.rs                   # MarketSession::tick (Harvest)
│   │   ├── executor.rs              # LiveExecutor + Simulator, db::spawn_db persist
│   │   └── passive.rs               # DryRun passive fill
│   ├── db/
│   │   └── mod.rs + bots, credentials, logs, markets, orders, pnl, sessions, trades
│   └── bot/                         # ctx, window, tick, event, zone, tasks, persist, shutdown
│
├── tests/
│   ├── slug_parser.rs
│   ├── clob_auth.rs
│   └── dryrun_flow.rs
│
├── frontend/                        # Next.js 16 (App Router) + Tailwind 4 + shadcn + Recharts
│   ├── app/                         # sayfa rotaları (/, /bots/new, /bots/[id])
│   ├── components/
│   └── lib/                         # api.ts, types.ts, hooks.ts, chart-utils, …
│
└── data/                            # runtime (gitignore)
    ├── baiter.db
    └── {bot_id}.heartbeat           # HEARTBEAT_DIR kökünde düz dosya
```

**Rust:** çekirdek iş mantığı `src/bot/`, `src/engine/`, `src/strategy/harvest/` altında; tek dosya sayısı ~35+ kaynak (testler hariç).
**Frontend:** Vite şablonu değil — **Next.js** (`frontend/package.json`, dev port varsayılan **5173**).

---

## 4. Dosya Sorumluluk Özeti

### Backend (Rust)

| Modül / dosya | Sorumluluk |
|---|---|
| `src/lib.rs` | Crate mod ağacı; sık kullanılan `AppError`, `Outcome`, `RunMode`, `Strategy` re-export |
| `src/bin/supervisor.rs` | `RuntimeEnv`, DB migrate, `restart_previously_running`, `axum::serve` |
| `src/bin/bot.rs` | rustls + tracing init, `bot::run()` |
| `src/config.rs` | `BotConfig`, `Credentials`, `RuntimeEnv` (PORT, DB_PATH, CLOB/Gamma URL, …) |
| `src/types.rs`, `slug.rs`, `error.rs`, `time.rs` | Ortak tipler, slug parse, hata, zaman / `MarketZone` |
| `src/polymarket/*` | Gamma, CLOB REST, WS, EIP-712 + HMAC, order JSON |
| `src/binance.rs` | USD-M sembolüne aggTrade tabanlı skor state |
| `src/strategy.rs` + `strategy/*` | `Decision`, `MetricMask`, `ZoneSignalMap`, Harvest FSM alt modülleri |
| `src/engine/*` | `MarketSession`, `execute` / `LiveExecutor` / `Simulator`, dryrun passive |
| `src/db/*` | Pool, WAL, `spawn_db`, tablo başına upsert/list helpers |
| `src/bot/*` | `Ctx::load`, pencere döngüsü, WS dispatch, 500 ms tick, zone/signal emit, shutdown |
| `src/api.rs` | Axum router |
| `src/supervisor.rs` | Child spawn, stdout `[[EVENT]]` → broadcast, crash backoff |
| `src/ipc.rs` | `FrontendEvent`, `emit`, `log_line` |

### Frontend (Next.js + shadcn + Recharts)

| Konum | Sorumluluk |
|---|---|
| `frontend/app/` | App Router sayfaları |
| `frontend/lib/api.ts` | REST + SSE (`EventSource`) |
| `frontend/lib/types.ts` | `FrontendEvent` ve DTO eşlemesi (`BestBidAsk` birleşik YES/NO kotasyonu) |
| `frontend/lib/hooks.ts` | Bot listesi, SSE, PnL polling, … |
| `frontend/components/charts/*` | Recharts tabanlı fiyat / spread / sinyal / PnL grafikleri |
| `frontend/components/bots/*` | Form, liste, log akışı, metrik paneli |

---

## 5. §⚡ Kural 1-6 → Dosya Eşlemesi

| Kural | Nerede uygulanır |
|---|---|
| **Kural 1** — Emir yolu sıfır blok (DB) | `src/engine/mod.rs` karar → `engine/executor.rs::LiveExecutor::place` içinde `POST /order`; DB `persist_place` → `db::spawn_db` (emir coroutine'i DB'yi beklemez) |
| **Kural 2** — State önceden hazır | `MarketSession` alanları + `strategy/metrics.rs`; tick `bot/tick.rs` |
| **Kural 3** — Connection pooling | `src/polymarket/clob.rs` + `ctx::shared_http_client` |
| **Kural 4** — Fire-and-forget | `db::spawn_db`, trade/order persist `bot/event.rs` içinde benzer pattern |
| **Kural 5** — Anlık push | `ipc::emit` / `log_line` stdout → supervisor `handle_stdout_line` → `broadcast` → `GET /api/events` SSE ([mimari §5](bot-platform-mimari.md)) |
| **Kural 6** — WS / yan görevler | WS okuma `polymarket/ws.rs`; CLOB + dosya heartbeat `bot/tasks.rs` ayrı `tokio::spawn` |

---

## 6. Cargo.toml Taslağı (sadeleştirilmiş)

```toml
[package]
name = "baiter-pro"
version = "0.1.0"
edition = "2021"
rust-version = "1.91"
default-run = "supervisor"
description = "Polymarket trading bot platform (supervisor + bot)"

[[bin]]
name = "supervisor"
path = "src/bin/supervisor.rs"

[[bin]]
name = "bot"
path = "src/bin/bot.rs"

[lib]
path = "src/lib.rs"

[dependencies]
# Async + HTTP (tam liste: repo kökü Cargo.toml)
tokio = { version = "1.52.1", features = [ "rt-multi-thread", "macros", "net", "time", "sync", "process", "signal", "io-util", "fs" ] }
reqwest = { version = "0.13.2", default-features = false, features = ["json", "rustls", "http2"] }
axum = { version = "0.8.9", default-features = false, features = ["http1", "json", "tokio", "macros", "query"] }
tower = "0.5.3"
tower-http = { version = "0.6.8", features = ["cors", "trace"] }
tokio-tungstenite = { version = "0.29.0", features = ["rustls-tls-webpki-roots"] }
rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }
futures-util = "0.3"
tokio-stream = { version = "0.1", features = ["sync"] }
# … serde, serde_json, alloy, k256, hmac, sha2, base64, hex, sqlx, uuid,
# chrono, thiserror, anyhow, tracing, tracing-subscriber, dotenvy,
# chrono-tz, rand, async-trait — tam sürümler için repo kökü Cargo.toml.
```

`default-run = "supervisor"`. Workspace yok. `cargo run --bin supervisor` ve `cargo run --bin bot -- --bot-id <id>` (veya `BAITER_BOT_ID`).

---

## 7. Frontend (Next.js)

Gerçek uygulama `frontend/` altındadır — **Vite + react-router** şablonu yerine **Next.js 16** (App Router), **Tailwind CSS 4**, **shadcn** bileşenleri ve grafikler için **Recharts** kullanılır (`frontend/package.json`).

- **API tabanı:** Supervisor `PORT` (varsayılan 3000). Next dev sunucusu varsayılan **5173**; `frontend/lib/api.ts` içindeki taban URL ortam değişkeniyle ayarlanır.
- **SSE:** `GET /api/events` → `EventSource`; `FrontendEvent` tipi `frontend/lib/types.ts` içinde `src/ipc.rs` ile uyumludur.
- **BestBidAsk:** Backend tek eventte dört kotasyon gönderir (`yes_best_bid/ask`, `no_best_bid/ask`); grafik bileşenleri buna göre (`frontend/components/charts/price-chart.tsx`, `spread-signal-chart.tsx`).
- **PnL:** Anlık özet `GET /api/bots/{id}/pnl` (polling); tam zamanlı PnL SSE kanalında yoktur (bkz. [bot-platform-mimari.md §17](bot-platform-mimari.md)).

Kurulum: `cd frontend && npm install && npm run dev`.

---

## 8. Neden Sade?

| Karmaşık yapı | Sade yapı | Fark |
|---|---|---|
| 3 crate × 4 `Cargo.toml` | 1 `Cargo.toml` | Daha az versiyon senkronizasyonu |
| Tek devasa `bot.rs` / `engine.rs` | `src/bot/*`, `src/engine/*`, `src/strategy/harvest/*` | Davranışı izole etmek |
| `strategy/` tek dosya harvest | `strategy/harvest/` alt modülleri | FSM okunabilirliği |
| Frontend çerçevesi | Next App Router + `frontend/lib/*` | Üretim yığını repo ile uyumlu |
| `baiter-core` + `baiter-supervisor` + `baiter-bot` isim çakışmaları | Hepsi `crate::` | Import çözümleme basit |

**Trade hızına etkisi:** Sıfır. Runtime performansını belirleyen:
- WS event → strateji kararı: `< 1 ms` (fonksiyon çağrısı zinciri)
- Karar → POST /order: `< 1 ms` (async `reqwest` pool'dan hazır bağlantı)
- Fill → SSE push: `< 50 ms` (`broadcast::channel`)

Bunlar **kod layout'undan bağımsız**; dizin yapısı derleme zamanında çözülür.

---

## 9. Geliştirme sırası (mevcut koda göre referans)

1. **DB + migrations** — `src/db/*`, `migrations/0001`…`0005`
2. **Polymarket** — `gamma`, `clob`, `auth`, `order`, `ws`
3. **Strateji iskeleti** — `strategy.rs`, `metrics`, `harvest/*`
4. **Engine** — `MarketSession`, `executor`, `passive`
5. **Bot döngüsü** — `ctx`, `window`, `tick`, `event`, `zone`, `tasks`, `shutdown`
6. **Supervisor + API + IPC**
7. **Frontend** — Next.js sayfalar + `lib/api` + SSE + Recharts grafikleri
8. **Staging E2E** — [staging-runbook.md](staging-runbook.md)

---

## 10. Doküman Haritası

| Konu | Kaynak |
|---|---|
| Ana mimari + SQLite + metrik kataloğu + §⚡ kurallar | [bot-platform-mimari.md](bot-platform-mimari.md) |
| Strateji detayları | [strategies.md](strategies.md) |
| Polymarket CLOB REST + WS | [api/polymarket-clob.md](api/polymarket-clob.md) |
| Polymarket Gamma REST | [api/polymarket-gamma.md](api/polymarket-gamma.md) |
| Rust crate ve sürümler | [rust-polymarket-kutuphaneler.md](rust-polymarket-kutuphaneler.md) |
| **Proje iskeleti** (bu doküman) | [proje-iskeleti.md](proje-iskeleti.md) |

---

*Sade iskelet = hızlı geliştirme. Runtime hızı §⚡ Kural 1-6 + async event loop + connection pooling ile sağlanır, dizin derinliğiyle değil. Modül adları implementasyon sırasında ihtiyaca göre netleşir.*
