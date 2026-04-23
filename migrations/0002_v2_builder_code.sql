-- CLOB V2 migration (28 Nisan 2026 cutover): her order'da bytes32 `builder`
-- alanı zorunlu. Mevcut credentials satırları zero-hex default ile geçer;
-- yeni input frontend'de zorunlu (api.rs::CredentialsInput.builder_code).

ALTER TABLE bot_credentials
    ADD COLUMN poly_builder_code TEXT NOT NULL DEFAULT
    '0x0000000000000000000000000000000000000000000000000000000000000000';

ALTER TABLE global_credentials
    ADD COLUMN poly_builder_code TEXT NOT NULL DEFAULT
    '0x0000000000000000000000000000000000000000000000000000000000000000';
