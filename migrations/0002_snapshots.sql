-- Orderbook ve PnL snapshot tabloları (§6 + §17).

CREATE TABLE orderbook_snapshots (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    market_session_id   INTEGER NOT NULL REFERENCES market_sessions(id) ON DELETE CASCADE,
    asset_id            TEXT NOT NULL,
    market              TEXT,
    bids                TEXT NOT NULL,          -- JSON
    asks                TEXT NOT NULL,          -- JSON
    hash                TEXT,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_orderbook_session_ts ON orderbook_snapshots(market_session_id, ts_ms DESC);

CREATE TABLE pnl_snapshots (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id   INTEGER NOT NULL REFERENCES market_sessions(id) ON DELETE CASCADE,
    cost_basis          REAL NOT NULL,
    fee_total           REAL NOT NULL,
    shares_yes          REAL NOT NULL,
    shares_no           REAL NOT NULL,
    pnl_if_up           REAL NOT NULL,
    pnl_if_down         REAL NOT NULL,
    mtm_pnl             REAL NOT NULL,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_pnl_bot_session_ts ON pnl_snapshots(bot_id, market_session_id, ts_ms DESC);
