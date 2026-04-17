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
| **Kritik event push (bot→supervisor)** | stdout JSON satırı (`[[EVENT]] {json}`) | Mekanizma + akış diyagramı + `FrontendEvent` varyantları: bkz. [mimari §⚡ Kural 5](bot-platform-mimari.md) |
| **Credential saklama** | SQLite `bot_credentials` tablosunda **plaintext**; `.env` fallback | Politika + güvenlik notu + DDL: bkz. [mimari §1 Kimlik ve cüzdan](bot-platform-mimari.md) + [§9a bot_credentials](bot-platform-mimari.md) |
| **Runtime path'ler** | Çevre değişkenleri (`DB_PATH`, `BOT_BINARY`, `HEARTBEAT_DIR`) + default fallback | Tam anahtar listesi + SIGTERM shutdown: bkz. [mimari §18 Runtime Konfigürasyonu](bot-platform-mimari.md) |

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
├── frontend/                        # React + Vite + TS + shadcn/ui + Tailwind
│   ├── package.json
│   ├── vite.config.ts               # Dev proxy: /api → supervisor
│   ├── tsconfig.json
│   ├── tailwind.config.ts           # shadcn tema
│   ├── postcss.config.js
│   ├── components.json              # shadcn CLI config (style, alias yolları)
│   ├── index.html
│   ├── .env.example                 # VITE_API_BASE_URL
│   └── src/
│       ├── main.tsx
│       ├── App.tsx                  # Router + Theme provider
│       ├── index.css                # Tailwind directives + shadcn CSS değişkenleri
│       ├── api.ts                   # fetch wrapper + EventSource (SSE)
│       ├── types.ts                 # Backend DTO'ları (Bot, Order, Trade, PnL, Log)
│       ├── hooks.ts                 # useBots (1s polling) + useSSE + usePnL
│       ├── lib/
│       │   └── utils.ts             # cn() helper (clsx + tailwind-merge; shadcn standart)
│       ├── components/
│       │   ├── ui/                  # shadcn CLI ile eklenen komponentler
│       │   │   ├── button.tsx
│       │   │   ├── input.tsx
│       │   │   ├── form.tsx
│       │   │   ├── select.tsx
│       │   │   ├── dialog.tsx
│       │   │   ├── table.tsx
│       │   │   ├── badge.tsx
│       │   │   ├── progress.tsx
│       │   │   ├── scroll-area.tsx
│       │   │   ├── sonner.tsx       # toast
│       │   │   ├── card.tsx
│       │   │   └── tabs.tsx
│       │   ├── BotForm.tsx          # shadcn Form; slug, strategy, run_mode, order_usdc, signal_weight
│       │   ├── BotList.tsx          # shadcn Table + Badge (bot durumu)
│       │   ├── LogStream.tsx        # shadcn ScrollArea + SSE live tail
│       │   ├── PriceChart.tsx       # lightweight-charts — tek chart / 4 line: YES bid+ask, NO bid+ask
│       │   ├── SpreadChart.tsx      # lightweight-charts — ayrı chart: YES spread + NO spread (histogram)
│       │   ├── PnLChart.tsx         # lightweight-charts (pnl_if_up/down/mtm çoklu seri)
│       │   ├── SignalChart.tsx      # lightweight-charts (binance_signal 0-10)
│       │   ├── MetricsPanel.tsx     # imbalance, AVG SUM, POSITION Δ özet kartları (Card)
│       │   ├── PnLWidget.tsx        # pnl_if_up/down + mtm_pnl anlık sayısal
│       │   └── ZoneTimeline.tsx     # MarketZone ilerleme göstergesi (Progress)
│
└── data/                            # Runtime artefaktları (gitignore)
    ├── baiter.db                    # SQLite (WAL)
    └── heartbeat/                   # bots/<id>.heartbeat dosyaları
```

**Toplam kaynak dosya sayısı (Rust):** 19 (+ 3 integration test dosyası) — önceki 60+ dosyalık yapıdan çok daha kolay navigasyon.
**Frontend:** 11 app dosyası + ~12 shadcn UI komponenti (CLI ile eklenir, elle yazılmaz).

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

### Frontend (React + shadcn/ui + Tailwind + lightweight-charts)

| Dosya | Sorumluluk |
|---|---|
| `src/main.tsx` | React kök + StrictMode |
| `src/App.tsx` | Router + ThemeProvider (dark mode) + `<Toaster />` (Sonner) |
| `src/index.css` | Tailwind directives + shadcn CSS custom properties |
| `src/lib/utils.ts` | `cn()` — shadcn standart helper (clsx + tailwind-merge) |
| `src/api.ts` | `fetch` wrapper + `EventSource` (SSE) — tüm API çağrıları |
| `src/types.ts` | Backend DTO ile aynı tipler; `Market { start_date_ms, end_date_ms, condition_id, asset_id_yes, asset_id_no, tick_size, min_order_size }` ve `BestBidAskEvent { kind: "best_bid_ask", bot_id, side: "YES"\|"NO", best_bid, best_ask, spread, ts_ms }` dahil |
| `src/hooks.ts` | `useBots` (1 sn polling), `useSSE`, `usePnL`, `useLogs`, `usePriceStream` |
| `src/pages/Dashboard.tsx` | `BotList` + global PnL özeti |
| `src/pages/NewBot.tsx` | `BotForm` full-page |
| `src/pages/BotDetail.tsx` | Tabs: Overview (PriceChart + MetricsPanel) / Logs / PnL / Orders |
| `src/components/ui/*` | shadcn CLI komponentleri (`button`, `input`, `form`, `select`, `dialog`, `table`, `badge`, `progress`, `scroll-area`, `sonner`, `card`, `tabs`) |
| `src/components/BotForm.tsx` | Strateji bazlı dinamik form; Zod + react-hook-form |
| `src/components/BotList.tsx` | Bot tablosu + status `Badge` + aksiyon menüsü |
| `src/components/LogStream.tsx` | `ScrollArea` içinde SSE canlı log |
| `src/components/PriceChart.tsx` | **lightweight-charts** — **tek chart / 4 Line**: `YES best_bid` (yeşil kalın), `YES best_ask` (yeşil ince), `NO best_bid` (kırmızı kalın), `NO best_ask` (kırmızı ince). X-ekseni Gamma `startDate`→`endDate`; tick = WS `timestamp`. `side` propu yok; her iki asset_id akışını tek komponent dinler |
| `src/components/SpreadChart.tsx` | **lightweight-charts** — **ayrı chart / 2 Histogram**: `YES spread` (yeşil bar), `NO spread` (kırmızı bar). Aynı zaman ekseni (startDate→endDate). PriceChart'ın altında render edilir |
| `src/components/PnLChart.tsx` | **lightweight-charts**: `pnl_if_up` / `pnl_if_down` / `mtm_pnl` çoklu line |
| `src/components/SignalChart.tsx` | **lightweight-charts**: `binance_signal` 0-10 (area) |
| `src/components/MetricsPanel.tsx` | `imbalance`, `AVG SUM`, `POSITION Δ` — shadcn `Card` grid |
| `src/components/PnLWidget.tsx` | Anlık `pnl_if_up`/`down`/`mtm` sayısal gösterim |
| `src/components/ZoneTimeline.tsx` | MarketZone ilerleme + 5 bölge `Badge`'i |

---

## 5. §⚡ Kural 1-6 → Dosya Eşlemesi

| Kural | Nerede uygulanır |
|---|---|
| **Kural 1** — Emir yolu sıfır blok | `src/engine.rs` karar → `src/polymarket/clob.rs` `post_order` → hemen sonra `tokio::spawn` DB/log/SSE |
| **Kural 2** — State önceden hazır | `src/strategy/metrics.rs` her WS event'inde `Arc<RwLock<StrategyMetrics>>` günceller |
| **Kural 3** — Connection pooling | `src/polymarket/clob.rs` tek paylaşımlı `reqwest::Client` (pool_max_idle_per_host, tcp_nodelay) |
| **Kural 4** — Fire-and-forget | `src/bin/bot.rs` emir sonrası `tokio::spawn` ile DB/log + `println!("[[EVENT]] ...")` |
| **Kural 5** — Anlık push | Bot `println!("[[EVENT]] {json}")` → supervisor `src/supervisor.rs::log_tail` parse → `broadcast::channel` → `src/api.rs` SSE handler → frontend `EventSource`. Tam mekanizma + akış diyagramı + `FrontendEvent` varyantları: [mimari §⚡ Kural 5](bot-platform-mimari.md) |
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

## 7. Frontend Stack (shadcn/ui + lightweight-charts)

### 7.1 `frontend/package.json` Taslağı

```json
{
  "name": "baiter-frontend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-router-dom": "^6.28.0",
    "react-hook-form": "^7.54.0",
    "@hookform/resolvers": "^3.9.1",
    "zod": "^3.24.0",
    "@tanstack/react-query": "^5.62.0",
    "lightweight-charts": "^5.0.0",
    "lucide-react": "^0.468.0",
    "class-variance-authority": "^0.7.1",
    "clsx": "^2.1.1",
    "tailwind-merge": "^2.5.5",
    "tailwindcss-animate": "^1.0.7",
    "sonner": "^1.7.1",
    "next-themes": "^0.4.4"
  },
  "devDependencies": {
    "@types/react": "^18.3.12",
    "@types/react-dom": "^18.3.1",
    "@vitejs/plugin-react": "^4.3.4",
    "typescript": "^5.7.2",
    "vite": "^6.0.0",
    "tailwindcss": "^3.4.15",
    "postcss": "^8.4.49",
    "autoprefixer": "^10.4.20"
  }
}
```

> **shadcn/ui komponentleri** runtime dep değildir — `npx shadcn@latest add <component>` komutu `src/components/ui/*.tsx` dosyasını kopyalar ve gerekli Radix paketlerini (`@radix-ui/react-dialog`, `@radix-ui/react-slot` vb.) devDeps'e ekler. Hiçbir shadcn paketi kendi bağımlılık listenize gelmez.

### 7.2 Kurulum Sırası

```bash
# 1. Vite + React + TS iskeleti
npm create vite@latest frontend -- --template react-ts

# 2. Tailwind kurulumu
npm install -D tailwindcss postcss autoprefixer
npx tailwindcss init -p

# 3. shadcn init (components.json oluşturur)
npx shadcn@latest init

# 4. İhtiyaç duyulan komponentleri ekle (tek tek)
npx shadcn@latest add button input form select dialog table badge \
                       progress scroll-area sonner card tabs

# 5. Diğer bağımlılıklar
npm install lightweight-charts react-router-dom react-hook-form @hookform/resolvers zod \
            @tanstack/react-query sonner next-themes lucide-react
```

### 7.3 shadcn Teması ve Dark Mode

- `next-themes` + shadcn'in varsayılan CSS değişkenleri = class tabanlı dark mode (`<html class="dark">`).
- `lightweight-charts` renklerini CSS custom property'lere bağlayarak tema otomatik adapte olur (`var(--border)`, `var(--foreground)`).
- shadcn default renk paleti (`slate` / `zinc` / `neutral`) — `components.json`'da seçilir.

### 7.4 Chart Mimarisi — Hangi Kütüphane Nerede?

| Grafik | Veri kaynağı | Güncelleme sıklığı | Kütüphane | Neden |
|---|---|---|---|---|
| **PriceChart** (4 line: YES bid/ask + NO bid/ask) | Market WS `best_bid_ask` → SSE (iki `asset_id` = YES & NO, ayrı akış); x-eksen domain'i Gamma `startDate`/`endDate`; tick = WS `timestamp` (ms → UTCTimestamp) | ~100 ms – 1 s | `lightweight-charts` (`LineSeries` × 4) | Tek chart'ta YES↔NO komplementerliği (toplam ≈ 1) net görünür |
| **SpreadChart** (YES spread + NO spread Histogram) | Aynı `best_bid_ask` event'inden `spread` alanı | ~100 ms – 1 s | `lightweight-charts` (`HistogramSeries` × 2) | Fiyattan ayrı ölçek — çubuk yüksekliği likidite göstergesi |
| **PnLChart** (`pnl_if_up` / `pnl_if_down`) | User WS `trade MATCHED` → SSE | Her fill | `lightweight-charts` multi-series line | Sık güncelleme + birden fazla seri |
| **PnLChart → mtm_pnl** | `best_bid_ask` → 1 sn polling | 1 sn | `lightweight-charts` (aynı chart 3. seri) | Yüksek frekans canvas avantajı |
| **SignalChart** (binance_signal 0-10) | Binance task → SSE (her trade) | Yüksek | `lightweight-charts` area chart | Yoğun veri |
| **MetricsPanel** kartları | Hooks (polling + SSE) | 1 sn + push | shadcn `Card` (grafik değil) | Özet sayı |
| **ZoneTimeline** | Client-side hesap (`zone_pct`) | 1 sn | shadcn `Progress` + `Badge` | Bölge ilerleme |
| **Günlük/Haftalık PnL özeti** (ileride) | Backend özet endpoint | Statik | shadcn `Chart` (Recharts) | Düşük frekans, güzel tema |

### 7.5 Polymarket `best_bid_ask` → Frontend DTO

Resmi `best_bid_ask` WS event şeması — alanlar (`event_type`, `market`, `asset_id`, `best_bid`, `best_ask`, `spread`, `timestamp`) ve YES/NO'nun iki ayrı `asset_id` akışı olduğu kuralı için bkz. [api/polymarket-clob.md §Market Channel](api/polymarket-clob.md). Bot `asset_id`'yi Gamma `clobTokenIds` konvansiyonuna göre YES/NO'ya map'ler ve supervisor'a aşağıdaki normalize DTO'yu gönderir (SSE payload'ı):

```jsonc
// ipc.rs FrontendEvent::BestBidAsk  → GET /api/events
{
  "kind":     "best_bid_ask",
  "bot_id":   7,
  "side":     "YES",          // "YES" | "NO"
  "best_bid": 0.73,
  "best_ask": 0.77,
  "spread":   0.04,
  "ts_ms":    1766789469958   // WS `timestamp` string → u64
}
```

Frontend bu DTO'yu `Math.floor(ts_ms/1000)` ile `UTCTimestamp`'e çevirip chart serilerine yazar.

### 7.6 PriceChart — Tek Chart, 4 Line

**Seriler** (tek `createChart` içinde, tek fiyat ekseni `0 – 1`):

| # | Seri | Tür | Renk / kalınlık | Kaynak |
|---|---|---|---|---|
| 1 | `YES best_bid` | `LineSeries` | Yeşil (`#16a34a`), **kalın** (`lineWidth: 3`) | SSE `side="YES"` → `best_bid` |
| 2 | `YES best_ask` | `LineSeries` | Yeşil (`#86efac`), **ince** (`lineWidth: 1`) | SSE `side="YES"` → `best_ask` |
| 3 | `NO best_bid` | `LineSeries` | Kırmızı (`#dc2626`), **kalın** (`lineWidth: 3`) | SSE `side="NO"` → `best_bid` |
| 4 | `NO best_ask` | `LineSeries` | Kırmızı (`#fca5a5`), **ince** (`lineWidth: 1`) | SSE `side="NO"` → `best_ask` |

**X-ekseni** — market'in `startDate` → `endDate` aralığı (Gamma'dan `ISO 8601` → saniye çevrilip `setVisibleRange` ile **bir kez** uygulanır). `fixLeftEdge: true`, `fixRightEdge: true` ile domain sabitlenir; market canlıyken "şimdi" ile `endDate` arasında boş alan kalır — bu "ne kadar süre kaldı" hissini verir.

**`PriceChart.tsx`**

```typescript
import {
  createChart, LineSeries,
  type IChartApi, type ISeriesApi, type UTCTimestamp,
} from 'lightweight-charts';
import { useEffect, useRef } from 'react';
import { useSSE } from '@/hooks';
import type { BestBidAskEvent } from '@/types';

interface Props {
  botId: number;
  startDateIso: string; // Gamma market.startDate
  endDateIso: string;   // Gamma market.endDate
}

export function PriceChart({ botId, startDateIso, endDateIso }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const chart = useRef<IChartApi | null>(null);
  const yesBid = useRef<ISeriesApi<'Line'> | null>(null);
  const yesAsk = useRef<ISeriesApi<'Line'> | null>(null);
  const noBid  = useRef<ISeriesApi<'Line'> | null>(null);
  const noAsk  = useRef<ISeriesApi<'Line'> | null>(null);

  useEffect(() => {
    if (!ref.current) return;

    chart.current = createChart(ref.current, {
      layout: { background: { color: 'transparent' }, textColor: 'var(--muted-foreground)' },
      grid: {
        vertLines: { color: 'var(--border)' },
        horzLines: { color: 'var(--border)' },
      },
      rightPriceScale: {
        borderVisible: false,
        scaleMargins: { top: 0.08, bottom: 0.08 },
      },
      timeScale: {
        timeVisible: true,
        secondsVisible: true,
        fixLeftEdge: true,
        fixRightEdge: true,
        rightBarStaysOnScroll: true,
      },
    });

    yesBid.current = chart.current.addSeries(LineSeries, {
      color: '#16a34a', title: 'YES bid', lineWidth: 3,
    });
    yesAsk.current = chart.current.addSeries(LineSeries, {
      color: '#86efac', title: 'YES ask', lineWidth: 1,
    });
    noBid.current = chart.current.addSeries(LineSeries, {
      color: '#dc2626', title: 'NO bid',  lineWidth: 3,
    });
    noAsk.current = chart.current.addSeries(LineSeries, {
      color: '#fca5a5', title: 'NO ask',  lineWidth: 1,
    });

    const from = Math.floor(new Date(startDateIso).getTime() / 1000) as UTCTimestamp;
    const to   = Math.floor(new Date(endDateIso).getTime()   / 1000) as UTCTimestamp;
    chart.current.timeScale().setVisibleRange({ from, to });

    return () => chart.current?.remove();
  }, [startDateIso, endDateIso]);

  useSSE<BestBidAskEvent>(botId, 'best_bid_ask', (ev) => {
    const time = Math.floor(ev.ts_ms / 1000) as UTCTimestamp;
    if (ev.side === 'YES') {
      yesBid.current?.update({ time, value: ev.best_bid });
      yesAsk.current?.update({ time, value: ev.best_ask });
    } else {
      noBid.current?.update({  time, value: ev.best_bid });
      noAsk.current?.update({  time, value: ev.best_ask });
    }
  });

  return <div ref={ref} className="h-[340px] w-full" />;
}
```

**Tasarım kararları**

- Tek chart'ta 4 line → YES↔NO komplementerliği (`price(YES) + price(NO) ≈ 1`) görsel olarak denetlenebilir.
- Bid **kalın** / ask **ince** konvansiyonu: Bid → satışın karşılığı olan fiyat (pozisyon kapatma referansı); ask → alışın karşılığı (pozisyon açma referansı). Kalın çizgi "bizim çıkış fiyatımız" olarak vurgulanır.
- Renk yarı-tonu (`#86efac` / `#fca5a5`) tema uyumlu pastel — dark mode'da da okunur.
- YES ve NO farklı `asset_id` event akışı olsa bile tek `PriceChart` komponenti dinler; backend DTO `side` alanı ile yönlendirilir.

### 7.7 SpreadChart — Ayrı Chart, 2 Histogram

**Seriler** (tek `createChart` içinde, tek fiyat ekseni `0 – ~0.10`):

| # | Seri | Tür | Renk | Kaynak |
|---|---|---|---|---|
| 1 | `YES spread` | `HistogramSeries` | Yeşil (`#16a34a`) | SSE `side="YES"` → `spread` |
| 2 | `NO spread` | `HistogramSeries` | Kırmızı (`#dc2626`) | SSE `side="NO"` → `spread` |

X-ekseni **PriceChart ile aynı** domain (startDate → endDate). İki chart dikey olarak hizalanır; `BotDetail` sayfasında PriceChart (yüksek) → SpreadChart (alçak) şeklinde üst üste render edilir.

**`SpreadChart.tsx`**

```typescript
import {
  createChart, HistogramSeries,
  type IChartApi, type ISeriesApi, type UTCTimestamp,
} from 'lightweight-charts';
import { useEffect, useRef } from 'react';
import { useSSE } from '@/hooks';
import type { BestBidAskEvent } from '@/types';

interface Props {
  botId: number;
  startDateIso: string;
  endDateIso: string;
}

export function SpreadChart({ botId, startDateIso, endDateIso }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const chart = useRef<IChartApi | null>(null);
  const yesSpread = useRef<ISeriesApi<'Histogram'> | null>(null);
  const noSpread  = useRef<ISeriesApi<'Histogram'> | null>(null);

  useEffect(() => {
    if (!ref.current) return;

    chart.current = createChart(ref.current, {
      layout: { background: { color: 'transparent' }, textColor: 'var(--muted-foreground)' },
      grid: {
        vertLines: { color: 'var(--border)' },
        horzLines: { color: 'var(--border)' },
      },
      rightPriceScale: {
        borderVisible: false,
        scaleMargins: { top: 0.1, bottom: 0.05 },
      },
      timeScale: {
        timeVisible: true,
        secondsVisible: true,
        fixLeftEdge: true,
        fixRightEdge: true,
        rightBarStaysOnScroll: true,
      },
    });

    yesSpread.current = chart.current.addSeries(HistogramSeries, {
      color: '#16a34a',
      title: 'YES spread',
      priceFormat: { type: 'price', precision: 4, minMove: 0.0001 },
    });
    noSpread.current = chart.current.addSeries(HistogramSeries, {
      color: '#dc2626',
      title: 'NO spread',
      priceFormat: { type: 'price', precision: 4, minMove: 0.0001 },
    });

    const from = Math.floor(new Date(startDateIso).getTime() / 1000) as UTCTimestamp;
    const to   = Math.floor(new Date(endDateIso).getTime()   / 1000) as UTCTimestamp;
    chart.current.timeScale().setVisibleRange({ from, to });

    return () => chart.current?.remove();
  }, [startDateIso, endDateIso]);

  useSSE<BestBidAskEvent>(botId, 'best_bid_ask', (ev) => {
    const time = Math.floor(ev.ts_ms / 1000) as UTCTimestamp;
    if (ev.side === 'YES') {
      yesSpread.current?.update({ time, value: ev.spread });
    } else {
      noSpread.current?.update({  time, value: ev.spread });
    }
  });

  return <div ref={ref} className="h-[140px] w-full" />;
}
```

### 7.8 BotDetail'de Yerleşim

```tsx
// pages/BotDetail.tsx (ilgili kısım)
<div className="space-y-2">
  <PriceChart  botId={bot.id} startDateIso={market.startDate} endDateIso={market.endDate} />
  <SpreadChart botId={bot.id} startDateIso={market.startDate} endDateIso={market.endDate} />
</div>
```

İki chart'ın **zaman eksenlerini senkronize** etmek istersen (bir chart'ta kaydırınca diğeri de kaysın) `lightweight-charts` v5 `timeScale().subscribeVisibleTimeRangeChange(...)` ile kolayca bağlanır; opsiyonel olarak v1'de eklenebilir.

### 7.9 Notlar

- `lightweight-charts` canvas tabanlı olduğu için React re-render'ı tetiklemez; `series.update()` imperative çağrısı milisaniye altı yenileme sağlar (§⚡ Kural 5 SSE push ile uyumlu).
- Aynı `timestamp` değerine sahip iki event geldiğinde `update()` noktayı overwrite eder (hata vermez) — Polymarket saniye altı çoklu event yayınlayabildiği için önemlidir.
- `startDate`/`endDate` alanları Gamma `GET /markets/{slug}` cevabından (`docs/api/polymarket-gamma.md:210-211, 378-379`) çekilir; `BotDetail` sayfası bu değerleri bot meta-verisi ile birlikte alır.

---

## 8. Neden Sade?

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

## 9. İmplementasyon Sırası (sade)

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
12. **Frontend iskelet:** Vite + React + TS + Tailwind + `shadcn init` + temel UI komponentleri (`button`, `form`, `input`, `table`, `card`, `tabs`, `sonner`)
13. **Frontend data katmanı:** `api.ts` + `hooks.ts` (React Query + EventSource) + `types.ts`
14. **Frontend sayfalar:** `Dashboard` (BotList) → `NewBot` (BotForm + Zod) → `BotDetail` (Tabs)
15. **Chart komponentleri:** `PriceChart`, `PnLChart`, `SignalChart` — lightweight-charts + SSE `update()`
16. **Dutch book + Prism:** TBD bölümleri netleştikten sonra
17. **Entegrasyon testi:** `clob-staging.polymarket.com`

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
