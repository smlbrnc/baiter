-- RTDS Chainlink pencere açılış snapshot'u (docs/signal-lightplan.md §6.2).
--
-- Her market window için ilk RTDS tick yakalandığında bir kereye mahsus
-- yazılır; `window_delta_bps = (current − open) / open × 10_000` hesabının
-- referans noktası. Mevcut `market_ticks` şeması değiştirilmez (fire-and-forget
-- 1/sn yükü eklemiyoruz).
--
-- Idempotent: SQLite ALTER TABLE ADD COLUMN kolon mevcutsa hata verir, bu
-- yüzden migration bir kez çalışır. Rollback için migrations/0010_down.sql
-- (manuel) — sqlx migrate rollback desteklemediğinden elle DROP COLUMN gerekir
-- (SQLite 3.35+).

ALTER TABLE market_sessions ADD COLUMN rtds_window_open_price REAL;
ALTER TABLE market_sessions ADD COLUMN rtds_window_open_ts_ms INTEGER;
