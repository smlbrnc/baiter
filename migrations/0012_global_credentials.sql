-- Global Polymarket kimlik bilgileri (singleton, frontend Settings sayfası).
-- bot_credentials'in bot_id'siz kardeşi: tüm botlar için varsayılan kimlik.
-- id = 1 zorunlu (CHECK), tek satır kuralı.
CREATE TABLE IF NOT EXISTS global_credentials (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    poly_address        TEXT,
    poly_api_key        TEXT,
    poly_passphrase     TEXT,
    poly_secret         TEXT,
    polygon_private_key TEXT,
    poly_signature_type INTEGER NOT NULL DEFAULT 0,
    poly_funder         TEXT,
    updated_at_ms       INTEGER NOT NULL
);
