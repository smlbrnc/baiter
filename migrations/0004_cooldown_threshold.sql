-- Per-bot averaging cooldown (ms). Tüm stratejiler için engine + strateji ctx
-- bu değeri kullanır. Default 30000 = 30 sn (eski COOLDOWN_THRESHOLD sabiti).

ALTER TABLE bots ADD COLUMN cooldown_threshold INTEGER NOT NULL DEFAULT 30000;
