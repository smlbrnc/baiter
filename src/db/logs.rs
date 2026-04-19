//! `logs` tablosu — supervisor `log_tail` tarafından yazılır.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::time::now_ms;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRow {
    pub id: i64,
    pub bot_id: Option<i64>,
    pub level: String,
    pub message: String,
    pub ts_ms: i64,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for LogRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row as _;
        Ok(Self {
            id: row.try_get("id")?,
            bot_id: row.try_get("bot_id")?,
            level: row.try_get("level")?,
            message: row.try_get("message")?,
            ts_ms: row.try_get("ts_ms")?,
        })
    }
}

pub async fn insert_log(
    pool: &SqlitePool,
    bot_id: Option<i64>,
    level: &str,
    message: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO logs (bot_id, level, message, ts_ms) VALUES (?, ?, ?, ?)")
        .bind(bot_id)
        .bind(level)
        .bind(message)
        .bind(now_ms() as i64)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn recent_logs(
    pool: &SqlitePool,
    bot_id: Option<i64>,
    limit: i64,
) -> Result<Vec<LogRow>, AppError> {
    let rows = match bot_id {
        Some(id) => {
            sqlx::query_as::<_, LogRow>(
                "SELECT id, bot_id, level, message, ts_ms FROM logs WHERE bot_id = ? \
                 ORDER BY ts_ms DESC, id DESC LIMIT ?",
            )
            .bind(id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, LogRow>(
                "SELECT id, bot_id, level, message, ts_ms FROM logs \
                 ORDER BY ts_ms DESC, id DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}
