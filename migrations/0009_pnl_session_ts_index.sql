-- list_sessions_for_bot / session_by_bot_slug içindeki
-- "SELECT MAX(ts_ms) FROM pnl_snapshots WHERE market_session_id = ?"
-- subquery'si için yardımcı index. Mevcut idx_pnl_bot_session_ts'in
-- leading kolonu bot_id olduğundan bu sorgu o index'i kullanamıyordu →
-- her session için full table scan oluyordu.
CREATE INDEX IF NOT EXISTS idx_pnl_session_ts
    ON pnl_snapshots(market_session_id, ts_ms DESC);
