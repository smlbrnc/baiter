//! SQLite katmanı — tüm upsert/insert/query fonksiyonları.
//!
//! WAL mode init'te açılır. `sqlx::query!` compile-time doğrulama yerine
//! runtime `query_as` kullanılır (SQLX_OFFLINE gereği yok; basitlik için).
//!
//! Referans: [docs/bot-platform-mimari.md §6-12 §9a §17](../../../docs/bot-platform-mimari.md).

use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::config::{BotConfig, Credentials, StrategyParams};
use crate::error::AppError;
use crate::time::now_ms;
use crate::types::{RunMode, Strategy};

/// Veritabanını aç (dosya yoksa oluştur) ve WAL mode etkinleştir.
pub async fn open(db_path: &str) -> Result<SqlitePool, AppError> {
    if let Some(parent) = Path::new(db_path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let opts = SqliteConnectOptions::from_str(db_path)
        .map_err(|e| AppError::Config(format!("sqlite connect options: {e}")))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    Ok(pool)
}

/// `migrations/` klasöründeki SQL dosyalarını sırasıyla çalıştır.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

// --------------------------------------------------------------------------
// Bot CRUD
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRow {
    pub id: i64,
    pub name: String,
    pub slug_pattern: String,
    pub strategy: String,
    pub run_mode: String,
    pub order_usdc: f64,
    pub signal_weight: f64,
    pub strategy_params: String,
    pub state: String,
    pub last_active_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl BotRow {
    /// DB satırından tam BotConfig üret.
    pub fn to_config(&self) -> Result<BotConfig, AppError> {
        let strategy: Strategy = serde_json::from_str(&format!("\"{}\"", self.strategy))
            .map_err(|e| AppError::Config(format!("strategy parse: {e}")))?;
        let run_mode: RunMode = serde_json::from_str(&format!("\"{}\"", self.run_mode))
            .map_err(|e| AppError::Config(format!("run_mode parse: {e}")))?;
        let strategy_params: StrategyParams = serde_json::from_str(&self.strategy_params)
            .unwrap_or_default();
        Ok(BotConfig {
            id: self.id,
            name: self.name.clone(),
            slug_pattern: self.slug_pattern.clone(),
            strategy,
            run_mode,
            order_usdc: self.order_usdc,
            signal_weight: self.signal_weight,
            strategy_params,
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
         strategy_params, state, created_at_ms, updated_at_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'STOPPED', ?, ?) RETURNING id",
    )
    .bind(&cfg.name)
    .bind(&cfg.slug_pattern)
    .bind(&strategy)
    .bind(&run_mode)
    .bind(cfg.order_usdc)
    .bind(cfg.signal_weight)
    .bind(&params)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("id"))
}

pub async fn list_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, AppError> {
    let rows = sqlx::query_as::<_, BotRow>(
        "SELECT id, name, slug_pattern, strategy, run_mode, order_usdc, signal_weight, \
         strategy_params, state, last_active_ms, created_at_ms, updated_at_ms FROM bots \
         ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_bot(pool: &SqlitePool, bot_id: i64) -> Result<Option<BotRow>, AppError> {
    let row = sqlx::query_as::<_, BotRow>(
        "SELECT id, name, slug_pattern, strategy, run_mode, order_usdc, signal_weight, \
         strategy_params, state, last_active_ms, created_at_ms, updated_at_ms FROM bots WHERE id = ?",
    )
    .bind(bot_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn set_bot_state(
    pool: &SqlitePool,
    bot_id: i64,
    state: &str,
) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query(
        "UPDATE bots SET state = ?, updated_at_ms = ?, last_active_ms = ? WHERE id = ?",
    )
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
            strategy_params: row.try_get("strategy_params")?,
            state: row.try_get("state")?,
            last_active_ms: row.try_get("last_active_ms")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

// --------------------------------------------------------------------------
// bot_credentials
// --------------------------------------------------------------------------

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

    Ok(row.map(|r| Credentials {
        poly_address: r.get::<Option<String>, _>("poly_address").unwrap_or_default(),
        poly_api_key: r.get::<Option<String>, _>("poly_api_key").unwrap_or_default(),
        poly_passphrase: r
            .get::<Option<String>, _>("poly_passphrase")
            .unwrap_or_default(),
        poly_secret: r.get::<Option<String>, _>("poly_secret").unwrap_or_default(),
        polygon_private_key: r
            .get::<Option<String>, _>("polygon_private_key")
            .unwrap_or_default(),
        signature_type: r.get::<i32, _>("poly_signature_type"),
        funder: r.get::<Option<String>, _>("poly_funder"),
    }))
}

// --------------------------------------------------------------------------
// market_sessions
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSessionRow {
    pub id: i64,
    pub bot_id: i64,
    pub slug: String,
    pub condition_id: Option<String>,
    pub asset_id_yes: Option<String>,
    pub asset_id_no: Option<String>,
    pub tick_size: Option<f64>,
    pub min_order_size: Option<f64>,
    pub start_ts: i64,
    pub end_ts: i64,
    pub state: String,
    pub cost_basis: f64,
    pub fee_total: f64,
    pub shares_yes: f64,
    pub shares_no: f64,
    pub realized_pnl: Option<f64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for MarketSessionRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            bot_id: row.try_get("bot_id")?,
            slug: row.try_get("slug")?,
            condition_id: row.try_get("condition_id")?,
            asset_id_yes: row.try_get("asset_id_yes")?,
            asset_id_no: row.try_get("asset_id_no")?,
            tick_size: row.try_get("tick_size")?,
            min_order_size: row.try_get("min_order_size")?,
            start_ts: row.try_get("start_ts")?,
            end_ts: row.try_get("end_ts")?,
            state: row.try_get("state")?,
            cost_basis: row.try_get("cost_basis")?,
            fee_total: row.try_get("fee_total")?,
            shares_yes: row.try_get("shares_yes")?,
            shares_no: row.try_get("shares_no")?,
            realized_pnl: row.try_get("realized_pnl")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

pub async fn upsert_market_session(
    pool: &SqlitePool,
    bot_id: i64,
    slug: &str,
    start_ts: i64,
    end_ts: i64,
) -> Result<i64, AppError> {
    let now = now_ms() as i64;
    let row = sqlx::query(
        "INSERT INTO market_sessions (bot_id, slug, start_ts, end_ts, created_at_ms, updated_at_ms) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(bot_id, slug) DO UPDATE SET updated_at_ms = excluded.updated_at_ms \
         RETURNING id",
    )
    .bind(bot_id)
    .bind(slug)
    .bind(start_ts)
    .bind(end_ts)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("id"))
}

pub async fn update_market_session_meta(
    pool: &SqlitePool,
    session_id: i64,
    condition_id: &str,
    asset_id_yes: &str,
    asset_id_no: &str,
    tick_size: f64,
    min_order_size: f64,
) -> Result<(), AppError> {
    let now = now_ms() as i64;
    sqlx::query(
        "UPDATE market_sessions SET condition_id = ?, asset_id_yes = ?, asset_id_no = ?, \
         tick_size = ?, min_order_size = ?, state = 'ACTIVE', updated_at_ms = ? WHERE id = ?",
    )
    .bind(condition_id)
    .bind(asset_id_yes)
    .bind(asset_id_no)
    .bind(tick_size)
    .bind(min_order_size)
    .bind(now)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

// --------------------------------------------------------------------------
// orders
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecord {
    pub order_id: String,
    pub bot_id: i64,
    pub market_session_id: Option<i64>,
    pub source: String,
    pub lifecycle_type: Option<String>,
    pub market: Option<String>,
    pub asset_id: Option<String>,
    pub side: Option<String>,
    pub price: Option<f64>,
    pub outcome: Option<String>,
    pub order_type: Option<String>,
    pub original_size: Option<f64>,
    pub size_matched: Option<f64>,
    pub expiration: Option<i64>,
    pub associate_trades: Option<String>,
    pub post_status: Option<String>,
    pub order_status: Option<String>,
    pub ts_ms: i64,
    pub raw_payload: Option<String>,
}

pub async fn upsert_order(pool: &SqlitePool, r: &OrderRecord) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO orders (order_id, bot_id, market_session_id, source, lifecycle_type, market, \
         asset_id, side, price, outcome, order_type, original_size, size_matched, expiration, \
         associate_trades, post_status, order_status, ts_ms, raw_payload) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(order_id) DO UPDATE SET \
         lifecycle_type = COALESCE(excluded.lifecycle_type, orders.lifecycle_type), \
         size_matched = COALESCE(excluded.size_matched, orders.size_matched), \
         associate_trades = COALESCE(excluded.associate_trades, orders.associate_trades), \
         post_status = COALESCE(excluded.post_status, orders.post_status), \
         order_status = COALESCE(excluded.order_status, orders.order_status), \
         ts_ms = excluded.ts_ms, \
         raw_payload = COALESCE(excluded.raw_payload, orders.raw_payload)",
    )
    .bind(&r.order_id)
    .bind(r.bot_id)
    .bind(r.market_session_id)
    .bind(&r.source)
    .bind(&r.lifecycle_type)
    .bind(&r.market)
    .bind(&r.asset_id)
    .bind(&r.side)
    .bind(r.price)
    .bind(&r.outcome)
    .bind(&r.order_type)
    .bind(r.original_size)
    .bind(r.size_matched)
    .bind(r.expiration)
    .bind(&r.associate_trades)
    .bind(&r.post_status)
    .bind(&r.order_status)
    .bind(r.ts_ms)
    .bind(&r.raw_payload)
    .execute(pool)
    .await?;
    Ok(())
}

// --------------------------------------------------------------------------
// trades
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub trade_id: String,
    pub bot_id: i64,
    pub market_session_id: Option<i64>,
    pub market: Option<String>,
    pub asset_id: Option<String>,
    pub taker_order_id: Option<String>,
    pub maker_orders: Option<String>,
    pub trader_side: Option<String>,
    pub side: Option<String>,
    pub outcome: Option<String>,
    pub size: f64,
    pub price: f64,
    pub status: String,
    pub fee: f64,
    pub ts_ms: i64,
    pub raw_payload: Option<String>,
}

pub async fn upsert_trade(pool: &SqlitePool, r: &TradeRecord) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO trades (trade_id, bot_id, market_session_id, market, asset_id, \
         taker_order_id, maker_orders, trader_side, side, outcome, size, price, status, fee, \
         ts_ms, raw_payload) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(trade_id) DO UPDATE SET \
         status = excluded.status, \
         ts_ms = excluded.ts_ms, \
         raw_payload = COALESCE(excluded.raw_payload, trades.raw_payload)",
    )
    .bind(&r.trade_id)
    .bind(r.bot_id)
    .bind(r.market_session_id)
    .bind(&r.market)
    .bind(&r.asset_id)
    .bind(&r.taker_order_id)
    .bind(&r.maker_orders)
    .bind(&r.trader_side)
    .bind(&r.side)
    .bind(&r.outcome)
    .bind(r.size)
    .bind(r.price)
    .bind(&r.status)
    .bind(r.fee)
    .bind(r.ts_ms)
    .bind(&r.raw_payload)
    .execute(pool)
    .await?;
    Ok(())
}

// --------------------------------------------------------------------------
// logs
// --------------------------------------------------------------------------

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
        Ok(Self {
            id: row.try_get("id")?,
            bot_id: row.try_get("bot_id")?,
            level: row.try_get("level")?,
            message: row.try_get("message")?,
            ts_ms: row.try_get("ts_ms")?,
        })
    }
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
                 ORDER BY ts_ms DESC LIMIT ?",
            )
            .bind(id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, LogRow>(
                "SELECT id, bot_id, level, message, ts_ms FROM logs \
                 ORDER BY ts_ms DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

// --------------------------------------------------------------------------
// market_resolved
// --------------------------------------------------------------------------

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

// --------------------------------------------------------------------------
// snapshots
// --------------------------------------------------------------------------

pub async fn insert_pnl_snapshot(
    pool: &SqlitePool,
    bot_id: i64,
    market_session_id: i64,
    cost_basis: f64,
    fee_total: f64,
    shares_yes: f64,
    shares_no: f64,
    pnl_if_up: f64,
    pnl_if_down: f64,
    mtm_pnl: f64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO pnl_snapshots (bot_id, market_session_id, cost_basis, fee_total, \
         shares_yes, shares_no, pnl_if_up, pnl_if_down, mtm_pnl, ts_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bot_id)
    .bind(market_session_id)
    .bind(cost_basis)
    .bind(fee_total)
    .bind(shares_yes)
    .bind(shares_no)
    .bind(pnl_if_up)
    .bind(pnl_if_down)
    .bind(mtm_pnl)
    .bind(now_ms() as i64)
    .execute(pool)
    .await?;
    Ok(())
}
