//! `bot_credentials` tablosu CRUD'u.
//!
//! Sessiz fallback yok: zorunlu alanlar `NULL` ise `AppError::MissingCredentials`
//! döner; opsiyonel `funder` `Option<String>` olarak kalır.

use sqlx::{Row, SqlitePool};

use crate::config::Credentials;
use crate::error::AppError;
use crate::time::now_ms;

pub async fn upsert_credentials(
    pool: &SqlitePool,
    bot_id: i64,
    creds: &Credentials,
) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query(
        "INSERT INTO bot_credentials (bot_id, poly_address, poly_api_key, poly_passphrase, \
         poly_secret, polygon_private_key, poly_signature_type, poly_funder, updated_at_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(bot_id) DO UPDATE SET \
         poly_address = excluded.poly_address, \
         poly_api_key = excluded.poly_api_key, \
         poly_passphrase = excluded.poly_passphrase, \
         poly_secret = excluded.poly_secret, \
         polygon_private_key = excluded.polygon_private_key, \
         poly_signature_type = excluded.poly_signature_type, \
         poly_funder = excluded.poly_funder, \
         updated_at_ms = excluded.updated_at_ms",
    )
    .bind(bot_id)
    .bind(&creds.poly_address)
    .bind(&creds.poly_api_key)
    .bind(&creds.poly_passphrase)
    .bind(&creds.poly_secret)
    .bind(&creds.polygon_private_key)
    .bind(creds.signature_type)
    .bind(&creds.funder)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_credentials(
    pool: &SqlitePool,
    bot_id: i64,
) -> Result<Option<Credentials>, AppError> {
    let row = sqlx::query(
        "SELECT poly_address, poly_api_key, poly_passphrase, poly_secret, \
         polygon_private_key, poly_signature_type, poly_funder FROM bot_credentials \
         WHERE bot_id = ?",
    )
    .bind(bot_id)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else { return Ok(None) };

    let need = |field: &'static str, v: Option<String>| -> Result<String, AppError> {
        v.filter(|s| !s.is_empty()).ok_or_else(|| {
            tracing::warn!(bot_id, field, "credentials missing required field");
            AppError::MissingCredentials { bot_id }
        })
    };

    let poly_address = need("poly_address", r.try_get("poly_address")?)?;
    let poly_api_key = need("poly_api_key", r.try_get("poly_api_key")?)?;
    let poly_passphrase = need("poly_passphrase", r.try_get("poly_passphrase")?)?;
    let poly_secret = need("poly_secret", r.try_get("poly_secret")?)?;
    let polygon_private_key = need(
        "polygon_private_key",
        r.try_get("polygon_private_key")?,
    )?;
    let signature_type: i32 = r.try_get("poly_signature_type")?;
    let funder: Option<String> = r.try_get("poly_funder")?;

    Ok(Some(Credentials {
        poly_address,
        poly_api_key,
        poly_passphrase,
        poly_secret,
        polygon_private_key,
        signature_type,
        funder,
    }))
}
