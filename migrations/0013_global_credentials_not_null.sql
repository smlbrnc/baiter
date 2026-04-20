-- 0012'de credential alanları nullable idi; partial-state hiç kullanılmadı
-- (PUT /api/settings/credentials L1 derive ile her zaman tam yazıyor).
-- SQLite `ALTER TABLE ... NOT NULL` desteklemediği için tabloyu yeniden yarat.
--
-- Mevcut satırları (varsa) NULL → '' güvenli dönüşümle koru.
PRAGMA foreign_keys=OFF;

CREATE TABLE global_credentials_new (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    poly_address        TEXT    NOT NULL,
    poly_api_key        TEXT    NOT NULL,
    poly_passphrase     TEXT    NOT NULL,
    poly_secret         TEXT    NOT NULL,
    polygon_private_key TEXT    NOT NULL,
    poly_signature_type INTEGER NOT NULL DEFAULT 0,
    poly_funder         TEXT,
    updated_at_ms       INTEGER NOT NULL
);

INSERT INTO global_credentials_new (
    id, poly_address, poly_api_key, poly_passphrase, poly_secret,
    polygon_private_key, poly_signature_type, poly_funder, updated_at_ms
)
SELECT
    id,
    COALESCE(poly_address, ''),
    COALESCE(poly_api_key, ''),
    COALESCE(poly_passphrase, ''),
    COALESCE(poly_secret, ''),
    COALESCE(polygon_private_key, ''),
    poly_signature_type,
    poly_funder,
    COALESCE(updated_at_ms, 0)
FROM global_credentials;

DROP TABLE global_credentials;
ALTER TABLE global_credentials_new RENAME TO global_credentials;

PRAGMA foreign_keys=ON;
