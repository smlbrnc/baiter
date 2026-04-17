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
| **Frontend** | React + Vite + TypeScript, **düz yapı** | Tek `api.ts`, tek `hooks.ts` |
| **HTTP sunucu** | Axum 0.8.9 | Mevcut [Cargo.toml](../Cargo.toml) zaten içeriyor |
| **WebSocket** | `tokio-tungstenite` | Market + User + Binance aggTrade |
| **EIP-712 imza** | `alloy` 2.0.0 | CLOB order signing + L1 auth |
| **HMAC-SHA256** | `hmac` + `sha2` + `base64` (URL_SAFE) | L2 auth header |
| **CLOB SDK** | **Elle entegrasyon** | Tam kontrol (§⚡ Kural 3) |
| **Kritik event push (bot→supervisor)** | **stdout JSON satırı** (`[[EVENT]] {json}`) | §1 IPC kısıtlamasıyla uyumlu; broadcast channel iki process arası çalışmaz. Gecikme <5 ms (§⚡ Kural 5) |
| **Credential saklama** | SQLite `bot_credentials` tablosunda **plaintext** (lokal disk) | Frontend form'undan giriş; `POLYGON_PRIVATE_KEY` env fallback |
| **Runtime path'ler** | **Env var'lı** (`DB_PATH`, `BOT_BINARY`) + default fallback | `.env` override, Docker/deploy esnekliği |

**Bırakılan / reddedilen alternatifler:**
- ~~Cargo workspace (3 crate)~~ → Tek binary → ayrı PID modeli için yeterli; workspace'in getirdiği `Cargo.toml` x 4 overhead'i yok.
- ~~Her konu için alt-klasör (`polymarket/clob/{client,auth,orders,book}.rs`)~~ → Tek `polymarket.rs` + `clob.rs` + `ws.rs` daha az navigasyon.
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

## 3. Dizin Yapısı (sadeleştirilmiş)

```
baiter-pro/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml              # rust-version = "1.91" (alloy 2 MSRV)
├── README.md
├── .env.example                     # Mevcut
├── .env                             # .gitignore
├── .gitignore
│
├── docs/                            # Mevcut dokümanlar
│   ├── bot-platform-mimari.md
│   ├── strategies.md
│   ├── rust-polymarket-kutuphaneler.md
│   ├── proje-iskeleti.md            # Bu doküman
│   └── api/
│       ├── polymarket-clob.md
│       └── polymarket-gamma.md
│
├── migrations/                      # sqlx migrations (WAL mode: PRAGMA journal_mode=WAL)
│   ├── 0001_init.sql                # bots, bot_credentials, market_sessions, orders, trades, logs
│   └── 0002_snapshots.sql           # orderbook snapshots, pnl snapshots
│
├── src/
│   ├── lib.rs                       # Modül re-exports (herkese açık yüzey)
│   │
│   ├── bin/
│   │   ├── supervisor.rs            # Ana binary: Axum + spawner
│   │   └── bot.rs                   # Per-bot binary (supervisor tarafından spawn)
│   │
│   ├── config.rs                    # BotConfig, RunMode, StrategyConfig, env yükleme
│   ├── types.rs                     # Outcome, Side, OrderType, OrderStatus, TradeStatus
│   ├── slug.rs                      # {asset}-updown-{interval}-{ts} parse + validate
│   ├── error.rs                     # AppError (thiserror)
│   ├── time.rs                      # T−15, zone_pct, ms helpers
│   │
│   ├── polymarket.rs                # mod bloğu: pub mod gamma; pub mod clob; pub mod ws;
│   ├── polymarket/
│   │   ├── gamma.rs                 # GET /markets/slug + /markets?active=true filtre
│   │   ├── clob.rs                  # REST: POST/DELETE /order(s), /book, heartbeat döngüsü
│   │   ├── auth.rs                  # L1 EIP-712 (alloy) + L2 HMAC URL_SAFE base64
│   │   └── ws.rs                    # Market + User WS (subscribe, PING, reconnect, parse)
│   │
│   ├── binance.rs                   # aggTrade WS + CVD + BSI (Hawkes) + OFI + signal_score
│   │
│   ├── strategy.rs                  # mod bloğu: Strategy enum, MetricMask, MarketZone, ZoneSignalMap
│   ├── strategy/
│   │   ├── metrics.rs               # StrategyMetrics, MarketPnL, effective_score, POSITION Δ
│   │   ├── harvest.rs               # OpenDual → SingleLeg → ProfitLock FSM (tam)
│   │   ├── dutch_book.rs            # TBD ([strategies.md §1](strategies.md))
│   │   └── prism.rs                 # TBD ([strategies.md §3](strategies.md))
│   │
│   ├── engine.rs                    # MarketSession + decision loop + dryrun simulator
│   ├── db.rs                        # sqlx pool + tüm upsert fonksiyonları (tek dosya)
│   ├── api.rs                       # Axum router: bots, logs, pnl, sse (tek dosya)
│   ├── supervisor.rs                # spawn, backoff, health, log-tail, [[EVENT]] parse
│   └── ipc.rs                       # FrontendEvent enum, [[EVENT]] satır formatı, heartbeat I/O
│
├── tests/                           # Integration testler (opsiyonel ama önerilen)
│   ├── slug_parser.rs               # slug validation kenar durumları
│   ├── clob_auth.rs                 # HMAC URL_SAFE vektörleri (resmi rs-clob-client ile eş)
│   └── dryrun_flow.rs               # RunMode::DryRun uçtan uca simülasyon
│
├── frontend/                        # React + Vite + TS (düz yapı)
│   ├── package.json
│   ├── vite.config.ts               # Dev proxy: /api → supervisor
│   ├── tsconfig.json
│   ├── index.html
│   ├── .env.example                 # VITE_API_BASE_URL
│   └── src/
│       ├── main.tsx
│       ├── App.tsx                  # Router
│       ├── api.ts                   # fetch wrapper + EventSource (SSE)
│       ├── types.ts                 # Backend DTO'ları (Bot, Order, Trade, PnL, Log)
│       ├── hooks.ts                 # useBots (1s polling) + useSSE + usePnL
│       ├── pages/
│       │   ├── Dashboard.tsx        # Bot listesi
│       │   ├── NewBot.tsx           # Bot oluştur formu
│       │   └── BotDetail.tsx        # Detay + loglar + PnL
│       └── components/
│           ├── BotForm.tsx          # slug, strategy, run_mode, order_usdc, signal_weight
│           ├── LogStream.tsx        # SSE live tail
│           ├── PnLWidget.tsx        # pnl_if_up/down + mtm_pnl
│           └── MetricsPanel.tsx     # imbalance, AVG SUM, POSITION Δ, signal, zone
│
└── data/                            # Runtime artefaktları (gitignore)
    ├── baiter.db                    # SQLite (WAL)
    └── heartbeat/                   # bots/<id>.heartbeat dosyaları
```

**Toplam kaynak dosya sayısı (Rust):** 19 (+ 3 integration test dosyası) — önceki 60+ dosyalık yapıdan çok daha kolay navigasyon.

---

## 4. Dosya Sorumluluk Özeti

### Backend (Rust)

| Dosya | Sorumluluk | Kullanan |
|---|---|---|
| `src/lib.rs` | Modül re-export, `pub use` | Her iki binary |
| `src/bin/supervisor.rs` | `main()`: Axum serve + spawner init + migrations | supervisor binary |
| `src/bin/bot.rs` | `main()`: bot_id al, config oku, engine çalıştır | bot binary |
| `src/config.rs` | `BotConfig`, `RunMode`, `StrategyConfig`, `.env` yükleme | Hepsi |
| `src/types.rs` | Polymarket ham tipleri (Outcome, Side, OrderType, statuses) | Hepsi |
| `src/slug.rs` | `SlugInfo`, `parse_slug`, desteklenen asset/interval tablosu | supervisor (validate), bot (discovery) |
| `src/error.rs` | `AppError` enum (thiserror) | Hepsi |
| `src/time.rs` | `t_minus_15`, `zone_pct`, UTC ms | Hepsi |
| `src/polymarket.rs` | Modül root; `pub use` | — |
| `src/polymarket/gamma.rs` | Gamma REST client (paylaşımlı `reqwest::Client`) | bot |
| `src/polymarket/clob.rs` | CLOB REST (orders, `/book`, heartbeat döngüsü) | bot |
| `src/polymarket/auth.rs` | EIP-712 + HMAC (URL_SAFE base64) | `clob.rs` |
| `src/polymarket/ws.rs` | Market + User WS (abonelik, PING, reconnect, event parse) | bot |
| `src/binance.rs` | aggTrade WS + sinyal hesabı + `Arc<RwLock<BinanceSignalState>>` | bot |
| `src/strategy.rs` | `Strategy`, `MetricMask`, `MarketZone`, `ZoneSignalMap` | Hepsi |
| `src/strategy/metrics.rs` | `StrategyMetrics`, `MarketPnL`, `effective_score` hesabı | bot, api |
| `src/strategy/harvest.rs` | OpenDual → SingleLeg → ProfitLock FSM | bot |
| `src/strategy/dutch_book.rs` | TBD — doldurulacak | bot |
| `src/strategy/prism.rs` | TBD — doldurulacak | bot |
| `src/engine.rs` | `MarketSession` yaşam döngüsü + karar loop + `Simulator` (dryrun) | bot |
| `src/db.rs` | `SqlitePool` + tüm CRUD/upsert fonksiyonları | supervisor, bot |
| `src/api.rs` | Axum router (bots, logs, pnl, sse) | supervisor |
| `src/supervisor.rs` | Process spawn + backoff + heartbeat check + log-tail + `[[EVENT]]` → `broadcast` | supervisor |
| `src/ipc.rs` | `FrontendEvent` enum, `[[EVENT]] {json}` serialize/parse, heartbeat dosya I/O | Hepsi |

### Frontend (React)

| Dosya | Sorumluluk |
|---|---|
| `src/main.tsx` | React kök |
| `src/App.tsx` | Router |
| `src/api.ts` | `fetch` wrapper + `EventSource` (SSE) — tüm API çağrıları |
| `src/types.ts` | Backend DTO ile aynı tipler |
| `src/hooks.ts` | `useBots`, `useSSE`, `usePnL`, `useLogs` — tek dosyada |
| `src/pages/Dashboard.tsx` | Bot listesi + özet |
| `src/pages/NewBot.tsx` | Bot oluşturma |
| `src/pages/BotDetail.tsx` | Bot detay (loglar + PnL + metrikler) |
| `src/components/BotForm.tsx` | Strateji bazlı dinamik form |
| `src/components/LogStream.tsx` | SSE canlı log akışı |
| `src/components/PnLWidget.tsx` | pnl_if_up / pnl_if_down / mtm_pnl |
| `src/components/MetricsPanel.tsx` | imbalance, AVG SUM, POSITION Δ, signal, zone badge |

---

## 5. §⚡ Kural 1-6 → Dosya Eşlemesi

| Kural | Nerede uygulanır |
|---|---|
| **Kural 1** — Emir yolu sıfır blok | `src/engine.rs` karar → `src/polymarket/clob.rs` `post_order` → hemen sonra `tokio::spawn` DB/log/SSE |
| **Kural 2** — State önceden hazır | `src/strategy/metrics.rs` her WS event'inde `Arc<RwLock<StrategyMetrics>>` günceller |
| **Kural 3** — Connection pooling | `src/polymarket/clob.rs` tek paylaşımlı `reqwest::Client` (pool_max_idle_per_host, tcp_nodelay) |
| **Kural 4** — Fire-and-forget | `src/bin/bot.rs` emir sonrası `tokio::spawn` ile DB/log + `println!("[[EVENT]] ...")` |
| **Kural 5** — Anlık push | **stdout JSON akışı**: bot `[[EVENT]] {json}` → supervisor `log_tail` parse → `broadcast::channel` → `src/api.rs` SSE handler → frontend `EventSource`. Gecikme: <5 ms |
| **Kural 6** — WS okuyucu önceliği | `src/polymarket/ws.rs` mpsc ile engine'e; heartbeat ayrı `tokio::interval` task |

---

## 6. Cargo.toml Taslağı (sadeleştirilmiş)

```toml
[package]
name = "baiter-pro"
version = "0.1.0"
edition = "2021"
rust-version = "1.91"
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
# Async + HTTP
tokio = { version = "1.52.1", features = ["rt-multi-thread", "macros", "net", "time", "sync", "process", "signal"] }
reqwest = { version = "0.13.2", default-features = false, features = ["json", "rustls", "http2"] }
axum = { version = "0.8.9", default-features = false, features = ["http1", "json", "tokio", "macros"] }
tower = "0.5.3"
tower-http = { version = "0.6.8", features = ["cors", "trace"] }

# WebSocket
tokio-tungstenite = { version = "0.29.0", features = ["rustls-tls-webpki-roots"] }
futures-util = "0.3"

# Serde
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.149"

# Kripto + imza
alloy = { version = "2.0.0", default-features = false, features = ["std", "eip712", "signer-local", "sol-types"] }
k256 = "=0.13.4"
hmac = "0.13.0"
sha2 = "0.11.0"
base64 = "0.22.1"
hex = "0.4.3"

# DB
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio-rustls", "sqlite", "macros", "migrate", "chrono"] }

# Yardımcı
uuid = { version = "1.23.1", features = ["v4", "serde"] }
chrono = { version = "0.4.44", features = ["serde"] }
thiserror = "2.0.18"
anyhow = "1.0.102"
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
dotenvy = "0.15.7"
```

Tek `Cargo.toml`; workspace yok. `cargo run --bin supervisor` ve `cargo run --bin bot` ile çalıştırılır.

---

## 7. Kritik Event Push Akışı (§⚡ Kural 5)

`tokio::sync::broadcast` iki **ayrı PID** arasında çalışmadığı için ve §1 IPC kısıtı UDS'yi yasakladığı için kritik olaylar **stdout üzerinden structured log satırı** olarak akar:

```
┌─────────────────────────────────────────────────────────────────┐
│  bot PID                                                        │
│    strategy decision → POST /order → yanıt alındı               │
│         │                                                       │
│         ├── tokio::spawn(db.upsert_order(...))       (Kural 4)  │
│         └── println!("[[EVENT]] {\"type\":\"OrderPlaced\",...")  │
│                             │                                   │
└─────────────────────────────┼───────────────────────────────────┘
                              │ stdout pipe (ChildStdout)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  supervisor PID — log_tail task                                 │
│    satır in: `[[EVENT]] {...}` prefix ise                       │
│      → parse FrontendEvent                                      │
│      → app_state.event_tx.send(ev)                              │
│        (tokio::sync::broadcast, process-local)                  │
│    değilse → SQLite logs tablosuna yaz                          │
└─────────────────────────────┬───────────────────────────────────┘
                              │ broadcast subscribe
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  src/api.rs SSE handler (GET /api/events)                       │
│    event_rx.recv() → sse Event::data(json) → frontend           │
└─────────────────────────────┬───────────────────────────────────┘
                              │ EventSource
                              ▼
                          frontend useSSE hook
```

**Sınıflandırma** (`src/ipc.rs`'de `FrontendEvent` enum):
- `OrderPlaced { bot_id, order_id, order_type, side, price, size }`
- `Fill { bot_id, trade_id, size, price, outcome, status }` (MATCHED + sonraki status güncellemeleri)
- `PnLUpdate { bot_id, session_id, pnl_if_up, pnl_if_down }` (yalnız MATCHED sonrası)
- `BotStateChanged { bot_id, state }` (RUNNING/STOPPED/FAILED)

`mtm_pnl` değişimleri (`best_bid_ask` her event) push edilmez — §17 kuralına göre 1 sn polling ile okunur.

---

## 8. Runtime Konfigürasyonu

`.env` anahtarları ([.env.example](../.env.example) ile uyumlu; bu iskelet yeni anahtarları ekler):

| Anahtar | Varsayılan | Kullanım |
|---|---|---|
| `PORT` | `3000` | Supervisor Axum HTTP port |
| `RUST_LOG` | `info` | `tracing-subscriber` filter |
| `DB_PATH` | `./data/baiter.db` | SQLite dosya yolu (WAL mode) |
| `BOT_BINARY` | `./target/release/bot` (release) / `./target/debug/bot` (debug) | Supervisor'un spawn edeceği bot binary'si |
| `HEARTBEAT_DIR` | `./data/heartbeat` | `bots/<id>.heartbeat` dosyalarının dizini |
| `GAMMA_BASE_URL` | `https://gamma-api.polymarket.com` | [.env.example](../.env.example)'den |
| `CLOB_BASE_URL` | `https://clob.polymarket.com` | [.env.example](../.env.example)'den |
| `POLYGON_CHAIN_ID` | `137` | EIP-712 domain |
| `POLY_ADDRESS` / `POLY_API_KEY` / `POLY_PASSPHRASE` / `POLY_SECRET` | — | **Fallback** credentials; per-bot override SQLite `bot_credentials`'tan |
| `POLYGON_PRIVATE_KEY` | — | **Fallback** L1 signing key |

**Kural (§1 "Kimlik çözümleme" ile uyumlu):** Önce SQLite `bot_credentials` tablosundan bot-spesifik kimlik okunur; yoksa `.env` fallback kullanılır.

**Credential saklama (plaintext SQLite):**
```sql
CREATE TABLE bot_credentials (
    bot_id           INTEGER PRIMARY KEY REFERENCES bots(id) ON DELETE CASCADE,
    poly_address     TEXT,
    poly_api_key     TEXT,
    poly_passphrase  TEXT,
    poly_secret      TEXT,
    polygon_private_key  TEXT,     -- L1 signing key (plaintext)
    poly_signature_type  INTEGER DEFAULT 0,
    poly_funder      TEXT,
    updated_at       INTEGER NOT NULL
);
```

> **Güvenlik notu:** `polygon_private_key` plaintext — disk/FS düzeyinde koruma önerilir (`chmod 600 data/baiter.db`, kullanıcı izolasyonu). Tehdit modeli değişirse ileride AES-GCM ile şifreleme eklenebilir.

**Graceful shutdown (SIGTERM):** `src/bin/bot.rs` `tokio::signal::unix::SignalKind::terminate()` dinler:
1. Market WS ve User WS aboneliklerini kapat
2. `DELETE /orders` ile açık GTC'leri temizle (§1 "temiz durdurma")
3. REST heartbeat döngüsünü durdur
4. `stdout` flush + exit code 0 (supervisor crash loop sayacına eklemez)

---

## 9. Neden Sade?

| Karmaşık yapı | Sade yapı | Fark |
|---|---|---|
| 3 crate × 4 `Cargo.toml` | 1 `Cargo.toml` | Daha az versiyon senkronizasyonu |
| `polymarket/clob/{mod,client,auth,orders,book,heartbeat,models}.rs` (7 dosya) | `polymarket/clob.rs` + `polymarket/auth.rs` (2 dosya) | Navigasyon hızı |
| `strategy/` 10 dosya | `strategy/` 4 dosya + `strategy.rs` modül root | Paralel editi kolay |
| Frontend: `api/` + `hooks/` + `types/` dizinleri | `api.ts` + `hooks.ts` + `types.ts` | Tek tıklama erişim |
| `baiter-core` + `baiter-supervisor` + `baiter-bot` isim çakışmaları | Hepsi `crate::` | Import çözümleme basit |

**Trade hızına etkisi:** Sıfır. Runtime performansını belirleyen:
- WS event → strateji kararı: `< 1 ms` (fonksiyon çağrısı zinciri)
- Karar → POST /order: `< 1 ms` (async `reqwest` pool'dan hazır bağlantı)
- Fill → SSE push: `< 50 ms` (`broadcast::channel`)

Bunlar **kod layout'undan bağımsız**; dizin yapısı derleme zamanında çözülür.

---

## 10. İmplementasyon Sırası (sade)

1. **Workspace temizliği:** Yeni Cargo.toml, `src/lib.rs`, `src/bin/{supervisor,bot}.rs` iskeletleri, migrations dizini
2. **Temel tipler:** `config.rs`, `types.rs`, `slug.rs`, `error.rs`, `time.rs`
3. **DB katmanı:** `db.rs` + `migrations/0001_init.sql`
4. **Polymarket REST:** `polymarket/gamma.rs`, `polymarket/auth.rs`, `polymarket/clob.rs`
5. **Polymarket WS:** `polymarket/ws.rs` (market + user kanalları)
6. **Binance sinyal:** `binance.rs`
7. **Strateji temeli:** `strategy.rs` + `strategy/metrics.rs`
8. **Harvest FSM:** `strategy/harvest.rs`
9. **Engine:** `engine.rs` (market session + decision loop + simulator)
10. **Bot binary:** `src/bin/bot.rs` runtime
11. **Supervisor + API:** `supervisor.rs` + `api.rs` + `ipc.rs` + `src/bin/supervisor.rs`
12. **Frontend:** React iskelet + `api.ts` + `hooks.ts` + sayfalar
13. **Dutch book + Prism:** TBD bölümleri netleştikten sonra
14. **Entegrasyon testi:** `clob-staging.polymarket.com`

---

## 11. Doküman Haritası

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
