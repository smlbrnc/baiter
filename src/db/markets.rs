//! `market_resolved` tablosu CRUD'u.

use sqlx::SqlitePool;

use crate::error::AppError;

pub async fn upsert_market_resolved(
    pool: &SqlitePool,
    market: &str,
    winning_outcome: &str,
    winning_asset_id: Option<&str>,
    ts_ms: i64,
    raw: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO market_resolved (market, winning_outcome, winning_asset_id, ts_ms, raw_payload) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(market) DO UPDATE SET winning_outcome = excluded.winning_outcome, \
         ts_ms = excluded.ts_ms",
    )
    .bind(market)
    .bind(winning_outcome)
    .bind(winning_asset_id)
    .bind(ts_ms)
    .bind(raw)
    .execute(pool)
    .await?;
    Ok(())
}
