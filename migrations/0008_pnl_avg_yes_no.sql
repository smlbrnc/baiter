-- Taraf bazlı VWAP (avg_up / avg_down) — PnL geçmişi ile aynı cadence snapshot.

ALTER TABLE pnl_snapshots ADD COLUMN avg_yes REAL NOT NULL DEFAULT 0.0;
ALTER TABLE pnl_snapshots ADD COLUMN avg_no REAL NOT NULL DEFAULT 0.0;
