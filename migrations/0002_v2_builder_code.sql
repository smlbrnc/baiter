-- CLOB V2 migration (28 Nisan 2026 cutover): her order'da bytes32 `builder`
-- alanı zorunlu. Sonradan per-bot override kaldırıldı — kod artık
-- `config::BUILDER_CODE_HEX` sabitini her order'a injekte eder; bu kolonlar
-- yalnızca legacy schema uyumu için duruyor (okunmuyor / yazılmıyor).

ALTER TABLE bot_credentials
    ADD COLUMN poly_builder_code TEXT NOT NULL DEFAULT
    '0x0000000000000000000000000000000000000000000000000000000000000000';

ALTER TABLE global_credentials
    ADD COLUMN poly_builder_code TEXT NOT NULL DEFAULT
    '0x0000000000000000000000000000000000000000000000000000000000000000';
