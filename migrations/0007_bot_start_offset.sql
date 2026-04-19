-- Bot oluşturulurken seçilen başlangıç penceresi ofseti.
-- 0 = aktif pencere (default), 1 = bir sonraki pencere.
-- Persistent: her fresh start'ta `bot/ctx::load` snap_active + N*interval uygular.

ALTER TABLE bots ADD COLUMN start_offset INTEGER NOT NULL DEFAULT 0;
