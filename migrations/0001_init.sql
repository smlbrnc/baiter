-- Baiter-Pro initial schema.
-- Referans: docs/bot-platform-mimari.md §6-12 + §9a.
-- WAL mode db.rs init'te `PRAGMA journal_mode=WAL` ile etkinleştirilir.

-- Bot tanımı (kullanıcı tarafından oluşturulur).
CREATE TABLE bots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    slug_pattern    TEXT NOT NULL,          -- 'btc-updown-5m-*' veya tam slug
    strategy        TEXT NOT NULL,          -- 'dutch_book' | 'harvest' | 'prism'
    run_mode        TEXT NOT NULL,          -- 'live' | 'dryrun'
    order_usdc      REAL NOT NULL,
    signal_weight   REAL NOT NULL DEFAULT 0.0,
    strategy_params TEXT NOT NULL DEFAULT '{}',  -- JSON
    state           TEXT NOT NULL DEFAULT 'STOPPED', -- STOPPED | RUNNING | FAILED
    last_active_ms  INTEGER,
    created_at_ms   INTEGER NOT NULL,
    updated_at_ms   INTEGER NOT NULL
);

CREATE INDEX idx_bots_state ON bots(state);

-- Per-bot Polymarket kimlik bilgileri (§9a). PLAINTEXT — güvenlik notu: §1.
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

-- Bot × pencere oturumu (§7). Bir sonraki market için ön kayıt T-15'te
-- insert, pencere başlayınca aynı satır güncellenir.
CREATE TABLE market_sessions (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    slug                TEXT NOT NULL,
    condition_id        TEXT,                  -- market (condition) kimliği
    asset_id_yes        TEXT,                  -- clobTokenIds[0]
    asset_id_no         TEXT,                  -- clobTokenIds[1]
    tick_size           REAL,
    min_order_size      REAL,
    start_ts            INTEGER NOT NULL,      -- unix sn, slug'dan
    end_ts              INTEGER NOT NULL,
    state               TEXT NOT NULL DEFAULT 'PLANNED',  -- PLANNED|ACTIVE|RESOLVED|CLOSED
    cost_basis          REAL NOT NULL DEFAULT 0.0,
    fee_total           REAL NOT NULL DEFAULT 0.0,
    shares_yes          REAL NOT NULL DEFAULT 0.0,
    shares_no           REAL NOT NULL DEFAULT 0.0,
    realized_pnl        REAL,                   -- market_resolved sonrası
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL
);

CREATE INDEX idx_market_sessions_bot ON market_sessions(bot_id);
CREATE UNIQUE INDEX idx_market_sessions_bot_slug ON market_sessions(bot_id, slug);

-- Emir kayıtları (§8). order_id = User WS order.id = REST orderID.
CREATE TABLE orders (
    order_id            TEXT PRIMARY KEY,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id   INTEGER REFERENCES market_sessions(id) ON DELETE SET NULL,
    source              TEXT NOT NULL,         -- 'user_ws' | 'rest_post' | 'rest_delete'
    lifecycle_type      TEXT,                   -- PLACEMENT | UPDATE | CANCELLATION
    market              TEXT,
    asset_id            TEXT,
    side                TEXT,                   -- BUY | SELL
    price               REAL,
    outcome             TEXT,                   -- ham (Yes/No)
    order_type          TEXT,                   -- GTC | GTD | FOK | FAK
    original_size       REAL,
    size_matched        REAL,
    expiration          INTEGER,
    associate_trades    TEXT,                   -- JSON array of trade_id
    post_status         TEXT,                   -- live|matched|delayed|unmatched
    order_status        TEXT,                   -- WS status (LIVE vb.)
    delete_canceled     TEXT,                   -- JSON
    delete_not_canceled TEXT,                   -- JSON
    ts_ms               INTEGER NOT NULL,
    raw_payload         TEXT
);

CREATE INDEX idx_orders_bot ON orders(bot_id);
CREATE INDEX idx_orders_session ON orders(market_session_id);
CREATE INDEX idx_orders_ts ON orders(ts_ms);

-- Market resolved (§9). Resmi WS market_resolved event'inden beslenir.
CREATE TABLE market_resolved (
    market              TEXT PRIMARY KEY,
    winning_outcome     TEXT NOT NULL,         -- 'Yes' | 'No' (ham)
    winning_asset_id    TEXT,
    ts_ms               INTEGER NOT NULL,
    raw_payload         TEXT
);

-- Trade kayıtları (§10). trade_id = User WS trade.id (REST tarafında ayrı yok).
CREATE TABLE trades (
    trade_id            TEXT PRIMARY KEY,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id   INTEGER REFERENCES market_sessions(id) ON DELETE SET NULL,
    market              TEXT,
    asset_id            TEXT,
    taker_order_id      TEXT,
    maker_orders        TEXT,                   -- JSON array
    trader_side         TEXT,                   -- TAKER | MAKER
    side                TEXT,                   -- BUY | SELL
    outcome             TEXT,
    size                REAL NOT NULL,
    price               REAL NOT NULL,
    status              TEXT NOT NULL,         -- MATCHED | MINED | CONFIRMED | RETRYING | FAILED
    fee                 REAL NOT NULL DEFAULT 0.0,
    ts_ms               INTEGER NOT NULL,
    raw_payload         TEXT
);

CREATE INDEX idx_trades_bot ON trades(bot_id);
CREATE INDEX idx_trades_session ON trades(market_session_id);
CREATE INDEX idx_trades_status ON trades(status);

-- Yapısal loglar (supervisor log_tail tarafından [[EVENT]] olmayan satırlar yazılır).
CREATE TABLE logs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id              INTEGER REFERENCES bots(id) ON DELETE CASCADE,
    level               TEXT NOT NULL DEFAULT 'info',  -- info | warn | error
    message             TEXT NOT NULL,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_logs_bot_ts ON logs(bot_id, ts_ms DESC);
