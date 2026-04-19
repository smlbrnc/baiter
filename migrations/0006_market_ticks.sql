-- Per-second BBA + Binance signal snapshots.
-- `bot/window.rs::run_trading_loop` içindeki `frontend_timer` (1 sn cadence)
-- ile fire-and-forget yazılır; frontend `/bots/[id]/[slug]` sayfası history
-- fetch + SSE merge için bu tabloyu kullanır.

CREATE TABLE market_ticks (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id              INTEGER NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    market_session_id   INTEGER NOT NULL REFERENCES market_sessions(id) ON DELETE CASCADE,
    yes_best_bid        REAL NOT NULL,
    yes_best_ask        REAL NOT NULL,
    no_best_bid         REAL NOT NULL,
    no_best_ask         REAL NOT NULL,
    signal_score        REAL NOT NULL,
    bsi                 REAL NOT NULL,
    ofi                 REAL NOT NULL,
    cvd                 REAL NOT NULL,
    ts_ms               INTEGER NOT NULL
);

CREATE INDEX idx_market_ticks_session_ts ON market_ticks(market_session_id, ts_ms);
