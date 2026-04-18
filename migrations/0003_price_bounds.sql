-- Bot-level global price guard (engine reject + strateji clamp).
-- Default 0.05 / 0.95 — `[bot.min_price, bot.max_price]` aralığı.

ALTER TABLE bots ADD COLUMN min_price REAL NOT NULL DEFAULT 0.05;
ALTER TABLE bots ADD COLUMN max_price REAL NOT NULL DEFAULT 0.95;
