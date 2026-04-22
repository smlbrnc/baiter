-- Baiter-Pro consolidated schema (15 migration tek dosyada toplandı).
-- Referans: docs/bot-platform-mimari.md §6-12 + §9a, docs/harvest-v2.md, docs/signal-lightplan.md.
-- WAL mode `db.rs` init'te `PRAGMA journal_mode=WAL` ile etkinleştirilir.
--
-- Önceki migration zinciri (0001..0015) DB'nin temizlenmesiyle yeniden ezildi:
--   * `orders` tablosu hiç okunmuyordu (audit dump) → komple kaldırıldı.
--   * `orderbook_snapshots` Rust tarafında write/read yok → kaldırıldı.
--   * `trades.raw_payload` audit fishing kolonu → tipli `taker_order_id /
--     maker_orders / trader_side` ile değiştirildi, raw kolonu yok.
--   * `bots.signal_weight` artık composite skorunu gate'lemiyor → yok.
--   * `bots.min_price/max_price/cooldown_threshold/start_offset` direkt şemada.
--   * `market_sessions.rtds_window_open_price/ts_ms` direkt şemada.
--   * `pnl_snapshots.pair_count/avg_up/avg_down` direkt şemada.
--   * `global_credentials` tabloları `NOT NULL` constraintler ile yaratılır.

-- Bot tanımı (kullanıcı tarafından oluşturulur).
CREATE TABLE bots (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT NOT NULL,
    slug_pattern        TEXT NOT NULL,                  -- 'btc-updown-5m-*' veya tam slug
    strategy            TEXT NOT NULL,                  -- 'alis' | 'elis' | 'aras'
    run_mode            TEXT NOT NULL,                  -- 'live' | 'dryrun'
    order_usdc          REAL NOT NULL,
    min_price           REAL NOT NULL DEFAULT 0.05,     -- engine reject + strateji clamp lower bound
    max_price           REAL NOT NULL DEFAULT 0.95,     -- upper bound
    cooldown_threshold  INTEGER NOT NULL DEFAULT 30000, -- averaging cooldown (ms); doc §11
    start_offset        INTEGER NOT NULL DEFAULT 0,     -- 0 = aktif pencere, N = N*interval ileri
    strategy_params     TEXT NOT NULL DEFAULT '{}',     -- JSON; StrategyParams
    state               TEXT NOT NULL DEFAULT 'STOPPED',-- STOPPED | RUNNING | FAILED
    last_active_ms      INTEGER,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL
);

CREATE INDEX idx_bots_state ON bots(state);

-- Per-bot Polymarket kimlik bilgileri (§9a). PLAINTEXT — güvenlik notu: §1.
-- Tüm botlar varsayılan olarak `global_credentials`'tan miras alır; bu tablo
-- yalnız bot-spesifik override için kullanılır (frontend "credentials"
-- bölümünde her bot için ayrı tutulur).
CREATE TABLE bot_credentials (
    bot_id              INTEGER PRIMARY KEY REFERENCES bots(id) ON DELETE CASCADE,
    poly_address        TEXT,
    poly_api_key        TEXT,
    poly_passphrase     TEXT,
    poly_secret         TEXT,
    polygon_private_key TEXT,
    poly_signature_type INTEGER NOT NULL DEFAULT 0,
    poly_funder         TEXT,
    updated_at_ms       INTEGER NOT NULL
);

-- Global Polymarket kimlik bilgileri (singleton). id = 1 zorunlu (CHECK).
-- Frontend Settings sayfasından PUT /api/settings/credentials ile yazılır;
-- her zaman tüm alanlar dolu yazıldığı için NOT NULL.
CREATE TABLE global_credentials (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    poly_address        TEXT    NOT NULL,
    poly_api_key        TEXT    NOT NULL,
    poly_passphrase     TEXT    NOT NULL,
    poly_secret         TEXT    NOT NULL,
    polygon_private_key TEXT    NOT NULL,
    poly_signature_type INTEGER NOT NULL DEFAULT 0,
    poly_funder         TEXT,
    updated_at_ms       INTEGER NOT NULL
);

-- Bot × pencere oturumu (§7). Bir sonraki market için ön kayıt T-15'te
-- insert, pencere başlayınca aynı satır güncellenir.
CREATE TABLE market_sessions (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id                  INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    slug                    TEXT NOT NULL,
    condition_id            TEXT,                       -- market (condition) kimliği
    asset_id_up             TEXT,                       -- clobTokenIds[0] (UP outcome)
    asset_id_down           TEXT,                       -- clobTokenIds[1] (DOWN outcome)
    tick_size               REAL,
    min_order_size          REAL,
    start_ts                INTEGER NOT NULL,           -- unix sn, slug'dan
    end_ts                  INTEGER NOT NULL,
    state                   TEXT NOT NULL DEFAULT 'PLANNED', -- PLANNED|ACTIVE|RESOLVED|CLOSED
    cost_basis              REAL NOT NULL DEFAULT 0.0,
    fee_total               REAL NOT NULL DEFAULT 0.0,
    up_filled               REAL NOT NULL DEFAULT 0.0,
    down_filled             REAL NOT NULL DEFAULT 0.0,
    realized_pnl            REAL,                       -- market_resolved sonrası
    rtds_window_open_price  REAL,                       -- RTDS Chainlink ilk tick referans fiyatı
    rtds_window_open_ts_ms  INTEGER,                    -- yakalanma zamanı (ms)
    created_at_ms           INTEGER NOT NULL,
    updated_at_ms           INTEGER NOT NULL
);

CREATE INDEX idx_market_sessions_bot ON market_sessions(bot_id);
CREATE UNIQUE INDEX idx_market_sessions_bot_slug ON market_sessions(bot_id, slug);

-- Market resolved (§9). Resmi WS market_resolved event'inden beslenir.
CREATE TABLE market_resolved (
    market              TEXT PRIMARY KEY,
    winning_outcome     TEXT NOT NULL,                  -- 'Yes' | 'No' (ham)
    winning_asset_id    TEXT,
    ts_ms               INTEGER NOT NULL,
    raw_payload         TEXT
);

-- Trade kayıtları (§10). trade_id = User WS trade.id. DryRun fill'leri için
-- `trade_id = format!("dryrun:{order_id}")`.
CREATE TABLE trades (
    trade_id            TEXT PRIMARY KEY,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id   INTEGER REFERENCES market_sessions(id) ON DELETE SET NULL,
    market              TEXT,
    asset_id            TEXT,
    taker_order_id      TEXT,                           -- TradePayload.taker_order_id
    maker_orders        TEXT,                           -- JSON; TradePayload.maker_orders serialize
    trader_side         TEXT,                           -- TAKER | MAKER (DryRun: TAKER/MAKER literal)
    side                TEXT,                           -- BUY | SELL
    outcome             TEXT,                           -- Up | Down
    size                REAL NOT NULL,
    price               REAL NOT NULL,
    status              TEXT NOT NULL,                  -- MATCHED | MINED | CONFIRMED | RETRYING | FAILED
    fee                 REAL NOT NULL DEFAULT 0.0,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_trades_bot ON trades(bot_id);
CREATE INDEX idx_trades_session ON trades(market_session_id);
CREATE INDEX idx_trades_status ON trades(status);

-- PnL snapshot (§17). 1 sn cadence; cost_basis + filled + (avg_up, avg_down) +
-- pair_count = min(up_filled, down_filled) dahil delta-neutral doğrulama
-- alanları DB-side.
CREATE TABLE pnl_snapshots (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id   INTEGER NOT NULL REFERENCES market_sessions(id) ON DELETE CASCADE,
    cost_basis          REAL NOT NULL,
    fee_total           REAL NOT NULL,
    up_filled           REAL NOT NULL,
    down_filled         REAL NOT NULL,
    pnl_if_up           REAL NOT NULL,
    pnl_if_down         REAL NOT NULL,
    mtm_pnl             REAL NOT NULL,
    pair_count          REAL NOT NULL DEFAULT 0.0,
    avg_up              REAL NOT NULL DEFAULT 0.0,
    avg_down            REAL NOT NULL DEFAULT 0.0,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_pnl_bot_session_ts ON pnl_snapshots(bot_id, market_session_id, ts_ms DESC);
-- list_sessions_for_bot / session_by_bot_slug için "MAX(ts_ms) WHERE
-- market_session_id = ?" subquery'sinin index üzerinden döndürülmesini sağlar
-- (idx_pnl_bot_session_ts'in leading kolonu bot_id olduğundan bu sorgu için
-- yetersiz; full table scan'ı engellemek için ikinci composite).
CREATE INDEX idx_pnl_session_ts ON pnl_snapshots(market_session_id, ts_ms DESC);

-- Per-second BBA + composite sinyal snapshot'ları. `bot/window.rs::run_trading_loop`
-- içindeki frontend_timer (1 sn cadence) ile fire-and-forget yazılır; frontend
-- `/bots/[id]/[slug]` sayfası history fetch + SSE merge için bu tabloyu kullanır.
CREATE TABLE market_ticks (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id   INTEGER NOT NULL REFERENCES market_sessions(id) ON DELETE CASCADE,
    up_best_bid         REAL NOT NULL,
    up_best_ask         REAL NOT NULL,
    down_best_bid       REAL NOT NULL,
    down_best_ask       REAL NOT NULL,
    signal_score        REAL NOT NULL,                  -- composite ∈ [0, 10]
    bsi                 REAL NOT NULL,
    ofi                 REAL NOT NULL,
    cvd                 REAL NOT NULL,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_market_ticks_session_ts ON market_ticks(market_session_id, ts_ms);

-- Yapısal loglar (supervisor log_tail tarafından [[EVENT]] olmayan satırlar yazılır).
CREATE TABLE logs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id              INTEGER REFERENCES bots(id) ON DELETE CASCADE,
    level               TEXT NOT NULL DEFAULT 'info',   -- info | warn | error
    message             TEXT NOT NULL,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_logs_bot_ts ON logs(bot_id, ts_ms DESC);
