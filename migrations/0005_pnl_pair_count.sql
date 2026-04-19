-- Delta-neutral PnL doğrulaması için `pair_count = min(shares_yes, shares_no)`
-- (doc §17). DB-side ekleme; PnL snapshot path'i bu sütunu da insert eder.

ALTER TABLE pnl_snapshots ADD COLUMN pair_count REAL NOT NULL DEFAULT 0.0;
