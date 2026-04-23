//! `global_credentials` singleton tablosu CRUD'u (frontend Settings sayfası).
//!
//! Frontend tek atışta tam credential yazar (PUT /api/settings/credentials →
//! L1 EIP-712 derive). Partial state imkânsız; bu yüzden 5 zorunlu alan
//! `String` (NOT NULL — bkz. migration 0013). `funder` yalnızca sig_type
//! ∈ {1,2} için anlamlı, opsiyonel kalır.

use sqlx::{Row, SqlitePool};

use crate::config::Credentials;
use crate::error::AppError;
use crate::time::now_ms;

#[derive(Debug, Clone)]
pub struct GlobalCredentials {
    pub poly_address: String,
    pub poly_api_key: String,
    pub poly_passphrase: String,
    pub poly_secret: String,
    pub polygon_private_key: String,
    pub signature_type: i32,
    pub funder: Option<String>,
    pub builder_code: String,
    pub updated_at_ms: i64,
}

/// `bot_credentials` ile aynı şekle dönüşüm — fallback chain `From` ile inline
/// olur (`bot/ctx.rs::load_validated_creds`).
impl From<GlobalCredentials> for Credentials {
    fn from(g: GlobalCredentials) -> Self {
        Self {
            poly_address: g.poly_address,
            poly_api_key: g.poly_api_key,
            poly_passphrase: g.poly_passphrase,
            poly_secret: g.poly_secret,
            polygon_private_key: g.polygon_private_key,
            signature_type: g.signature_type,
            funder: g.funder,
            builder_code: g.builder_code,
        }
    }
}

pub async fn get_global_credentials(
    pool: &SqlitePool,
) -> Result<Option<GlobalCredentials>, AppError> {
    let row = sqlx::query(
        "SELECT poly_address, poly_api_key, poly_passphrase, poly_secret, \
         polygon_private_key, poly_signature_type, poly_funder, poly_builder_code, \
         updated_at_ms FROM global_credentials WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else { return Ok(None) };

    Ok(Some(GlobalCredentials {
        poly_address: r.try_get("poly_address")?,
        poly_api_key: r.try_get("poly_api_key")?,
        poly_passphrase: r.try_get("poly_passphrase")?,
        poly_secret: r.try_get("poly_secret")?,
        polygon_private_key: r.try_get("polygon_private_key")?,
        signature_type: r.try_get("poly_signature_type")?,
        funder: r.try_get("poly_funder")?,
        builder_code: r.try_get("poly_builder_code")?,
        updated_at_ms: r.try_get("updated_at_ms")?,
    }))
}

pub async fn upsert_global_credentials(
    pool: &SqlitePool,
    creds: &GlobalCredentials,
) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query(
        "INSERT INTO global_credentials (id, poly_address, poly_api_key, poly_passphrase, \
         poly_secret, polygon_private_key, poly_signature_type, poly_funder, \
         poly_builder_code, updated_at_ms) \
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
         poly_address = excluded.poly_address, \
         poly_api_key = excluded.poly_api_key, \
         poly_passphrase = excluded.poly_passphrase, \
         poly_secret = excluded.poly_secret, \
         polygon_private_key = excluded.polygon_private_key, \
         poly_signature_type = excluded.poly_signature_type, \
         poly_funder = excluded.poly_funder, \
         poly_builder_code = excluded.poly_builder_code, \
         updated_at_ms = excluded.updated_at_ms",
    )
    .bind(&creds.poly_address)
    .bind(&creds.poly_api_key)
    .bind(&creds.poly_passphrase)
    .bind(&creds.poly_secret)
    .bind(&creds.polygon_private_key)
    .bind(creds.signature_type)
    .bind(&creds.funder)
    .bind(&creds.builder_code)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}
