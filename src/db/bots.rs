//! `bots` tablosu CRUD'u.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::config::{BotConfig, StrategyParams};
use crate::error::AppError;
use crate::time::now_ms;
use crate::types::{RunMode, Strategy};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRow {
    pub id: i64,
    pub name: String,
    pub slug_pattern: String,
    pub strategy: String,
    pub run_mode: String,
    pub order_usdc: f64,
    pub signal_weight: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub cooldown_threshold: i64,
    pub start_offset: i64,
    pub strategy_params: String,
    pub state: String,
    pub last_active_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl BotRow {
    /// DB satırından tam BotConfig üret. `strategy_params` parse hatası
    /// sessizce default'a düşmez; `AppError::Config` döner.
    pub fn to_config(&self) -> Result<BotConfig, AppError> {
        let strategy: Strategy = serde_json::from_str(&format!("\"{}\"", self.strategy))
            .map_err(|e| AppError::Config(format!("strategy parse: {e}")))?;
        let run_mode: RunMode = serde_json::from_str(&format!("\"{}\"", self.run_mode))
            .map_err(|e| AppError::Config(format!("run_mode parse: {e}")))?;
        let strategy_params: StrategyParams = serde_json::from_str(&self.strategy_params)
            .map_err(|e| {
                AppError::Config(format!(
                    "strategy_params parse (bot={}): {e}",
                    self.id
                ))
            })?;
        Ok(BotConfig {
            id: self.id,
            name: self.name.clone(),
            slug_pattern: self.slug_pattern.clone(),
            strategy,
            run_mode,
            order_usdc: self.order_usdc,
            signal_weight: self.signal_weight,
            min_price: self.min_price,
            max_price: self.max_price,
            cooldown_threshold: self.cooldown_threshold as u64,
            start_offset: self.start_offset as u32,
            strategy_params,
        })
    }
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for BotRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            slug_pattern: row.try_get("slug_pattern")?,
            strategy: row.try_get("strategy")?,
            run_mode: row.try_get("run_mode")?,
            order_usdc: row.try_get("order_usdc")?,
            signal_weight: row.try_get("signal_weight")?,
            min_price: row.try_get("min_price")?,
            max_price: row.try_get("max_price")?,
            cooldown_threshold: row.try_get("cooldown_threshold")?,
            start_offset: row.try_get("start_offset")?,
            strategy_params: row.try_get("strategy_params")?,
            state: row.try_get("state")?,
            last_active_ms: row.try_get("last_active_ms")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

/// Yeni bot ekle — id döner.
pub async fn insert_bot(pool: &SqlitePool, cfg: &BotConfig) -> Result<i64, AppError> {
    let now = now_ms() as i64;
    let strategy = serde_json::to_string(&cfg.strategy)?
        .trim_matches('"')
        .to_string();
    let run_mode = serde_json::to_string(&cfg.run_mode)?
        .trim_matches('"')
        .to_string();
    let params = serde_json::to_string(&cfg.strategy_params)?;

    let row = sqlx::query(
        "INSERT INTO bots (name, slug_pattern, strategy, run_mode, order_usdc, signal_weight, \
         min_price, max_price, cooldown_threshold, start_offset, strategy_params, \
         state, created_at_ms, updated_at_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'STOPPED', ?, ?) RETURNING id",
    )
    .bind(&cfg.name)
    .bind(&cfg.slug_pattern)
    .bind(&strategy)
    .bind(&run_mode)
    .bind(cfg.order_usdc)
    .bind(cfg.signal_weight)
    .bind(cfg.min_price)
    .bind(cfg.max_price)
    .bind(cfg.cooldown_threshold as i64)
    .bind(cfg.start_offset as i64)
    .bind(&params)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("id"))
}

const SELECT_BOT: &str =
    "SELECT id, name, slug_pattern, strategy, run_mode, order_usdc, signal_weight, \
     min_price, max_price, cooldown_threshold, start_offset, strategy_params, \
     state, last_active_ms, created_at_ms, updated_at_ms FROM bots";

pub async fn list_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, AppError> {
    let rows = sqlx::query_as::<_, BotRow>(&format!("{SELECT_BOT} ORDER BY id ASC"))
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn get_bot(pool: &SqlitePool, bot_id: i64) -> Result<Option<BotRow>, AppError> {
    let row = sqlx::query_as::<_, BotRow>(&format!("{SELECT_BOT} WHERE id = ?"))
        .bind(bot_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn set_bot_state(pool: &SqlitePool, bot_id: i64, state: &str) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query("UPDATE bots SET state = ?, updated_at_ms = ?, last_active_ms = ? WHERE id = ?")
        .bind(state)
        .bind(now)
        .bind(now)
        .bind(bot_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_bot(pool: &SqlitePool, bot_id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM bots WHERE id = ?")
        .bind(bot_id)
        .execute(pool)
        .await?;
    Ok(())
}
