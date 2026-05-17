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
    /// DB satırından `BotConfig` üret. `strategy_params` parse hatası `AppError::Config` döner.
    pub fn to_config(&self) -> Result<BotConfig, AppError> {
        let strategy: Strategy = serde_json::from_str(&format!("\"{}\"", self.strategy))
            .map_err(|e| AppError::Config(format!("strategy parse: {e}")))?;
        let run_mode: RunMode = serde_json::from_str(&format!("\"{}\"", self.run_mode))
            .map_err(|e| AppError::Config(format!("run_mode parse: {e}")))?;
        let strategy_params: StrategyParams =
            serde_json::from_str(&self.strategy_params).map_err(|e| {
                AppError::Config(format!("strategy_params parse (bot={}): {e}", self.id))
            })?;
        Ok(BotConfig {
            id: self.id,
            name: self.name.clone(),
            slug_pattern: self.slug_pattern.clone(),
            strategy,
            run_mode,
            order_usdc: self.order_usdc,
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
        "INSERT INTO bots (name, slug_pattern, strategy, run_mode, order_usdc, \
         min_price, max_price, cooldown_threshold, start_offset, strategy_params, \
         state, created_at_ms, updated_at_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'STOPPED', ?, ?) RETURNING id",
    )
    .bind(&cfg.name)
    .bind(&cfg.slug_pattern)
    .bind(&strategy)
    .bind(&run_mode)
    .bind(cfg.order_usdc)
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

const SELECT_BOT: &str = "SELECT id, name, slug_pattern, strategy, run_mode, order_usdc, \
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

// ── İstatistik tipleri ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PositionTypeStats {
    pub position_type: String,
    pub total: i64,
    pub winning: i64,
    pub losing: i64,
    pub winrate_pct: f64,
    pub avg_pnl: f64,
    pub total_pnl: f64,
    pub total_cost: f64,
    pub roi_pct: f64,
}

#[derive(Debug, Serialize)]
pub struct SessionTimelineItem {
    pub session_id: i64,
    pub slug: String,
    pub mtm_pnl: f64,
    pub cost_basis: f64,
    pub roi_pct: f64,
    pub position_type: String,
    pub ts_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct BotStats {
    pub total_sessions: i64,
    pub winning: i64,
    pub losing: i64,
    pub winrate_pct: f64,
    pub total_mtm_pnl: f64,
    pub total_cost_basis: f64,
    pub roi_pct: f64,
    pub total_fee: f64,
    pub avg_session_pnl: f64,
    pub best_session_pnl: f64,
    pub worst_session_pnl: f64,
    pub total_trades: i64,
    pub by_type: Vec<PositionTypeStats>,
    pub sessions_timeline: Vec<SessionTimelineItem>,
}

pub async fn get_bot_stats(pool: &SqlitePool, bot_id: i64) -> Result<BotStats, AppError> {
    // Her session için en son PnL snapshot'unu al (correlated subquery ile kesin son satır)
    let agg_row = sqlx::query(
        r#"
        SELECT
            COUNT(*) AS total_sessions,
            SUM(CASE WHEN p.mtm_pnl > 0 THEN 1 ELSE 0 END) AS winning,
            SUM(CASE WHEN p.mtm_pnl < 0 THEN 1 ELSE 0 END) AS losing,
            COALESCE(SUM(p.mtm_pnl), 0.0) AS total_mtm_pnl,
            COALESCE(SUM(p.cost_basis), 0.0) AS total_cost_basis,
            COALESCE(SUM(p.fee_total), 0.0) AS total_fee,
            COALESCE(AVG(p.mtm_pnl), 0.0) AS avg_session_pnl,
            COALESCE(MAX(p.mtm_pnl), 0.0) AS best_session_pnl,
            COALESCE(MIN(p.mtm_pnl), 0.0) AS worst_session_pnl
        FROM pnl_snapshots p
        WHERE p.bot_id = ?
          AND p.ts_ms = (
              SELECT MAX(ts_ms) FROM pnl_snapshots
              WHERE market_session_id = p.market_session_id AND bot_id = p.bot_id
          )
          AND p.cost_basis > 0
        "#,
    )
    .bind(bot_id)
    .fetch_one(pool)
    .await?;

    let total_sessions: i64 = agg_row.try_get("total_sessions").unwrap_or(0);
    let winning: i64 = agg_row.try_get("winning").unwrap_or(0);
    let losing: i64 = agg_row.try_get("losing").unwrap_or(0);
    let total_mtm_pnl: f64 = agg_row.try_get("total_mtm_pnl").unwrap_or(0.0);
    let total_cost_basis: f64 = agg_row.try_get("total_cost_basis").unwrap_or(0.0);
    let total_fee: f64 = agg_row.try_get("total_fee").unwrap_or(0.0);
    let avg_session_pnl: f64 = agg_row.try_get("avg_session_pnl").unwrap_or(0.0);
    let best_session_pnl: f64 = agg_row.try_get("best_session_pnl").unwrap_or(0.0);
    let worst_session_pnl: f64 = agg_row.try_get("worst_session_pnl").unwrap_or(0.0);

    let winrate_pct = if total_sessions > 0 {
        (winning as f64 / total_sessions as f64) * 100.0
    } else {
        0.0
    };
    let roi_pct = if total_cost_basis > 0.0 {
        (total_mtm_pnl / total_cost_basis) * 100.0
    } else {
        0.0
    };

    // Toplam trade sayısı
    let trade_row = sqlx::query("SELECT COUNT(*) AS cnt FROM trades WHERE bot_id = ?")
        .bind(bot_id)
        .fetch_one(pool)
        .await?;
    let total_trades: i64 = trade_row.try_get("cnt").unwrap_or(0);

    // Pozisyon tipi bazında istatistik (correlated subquery ile kesin son snapshot)
    let type_rows = sqlx::query(
        r#"
        SELECT
            CASE
                WHEN p.up_filled > 0 AND p.down_filled = 0 THEN 'SAF_UP'
                WHEN p.down_filled > 0 AND p.up_filled = 0 THEN 'SAF_DOWN'
                ELSE 'KARMA'
            END AS position_type,
            COUNT(*) AS total,
            SUM(CASE WHEN p.mtm_pnl > 0 THEN 1 ELSE 0 END) AS winning,
            SUM(CASE WHEN p.mtm_pnl < 0 THEN 1 ELSE 0 END) AS losing,
            COALESCE(AVG(p.mtm_pnl), 0.0) AS avg_pnl,
            COALESCE(SUM(p.mtm_pnl), 0.0) AS total_pnl,
            COALESCE(SUM(p.cost_basis), 0.0) AS total_cost
        FROM pnl_snapshots p
        WHERE p.bot_id = ?
          AND p.ts_ms = (
              SELECT MAX(ts_ms) FROM pnl_snapshots
              WHERE market_session_id = p.market_session_id AND bot_id = p.bot_id
          )
          AND p.cost_basis > 0
        GROUP BY position_type
        ORDER BY position_type
        "#,
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;

    let by_type: Vec<PositionTypeStats> = type_rows
        .iter()
        .map(|r| {
            let total: i64 = r.try_get("total").unwrap_or(0);
            let w: i64 = r.try_get("winning").unwrap_or(0);
            let total_cost: f64 = r.try_get("total_cost").unwrap_or(0.0);
            let total_pnl: f64 = r.try_get("total_pnl").unwrap_or(0.0);
            PositionTypeStats {
                position_type: r.try_get("position_type").unwrap_or_default(),
                total,
                winning: w,
                losing: r.try_get("losing").unwrap_or(0),
                winrate_pct: if total > 0 {
                    (w as f64 / total as f64) * 100.0
                } else {
                    0.0
                },
                avg_pnl: r.try_get("avg_pnl").unwrap_or(0.0),
                total_pnl,
                total_cost,
                roi_pct: if total_cost > 0.0 {
                    (total_pnl / total_cost) * 100.0
                } else {
                    0.0
                },
            }
        })
        .collect();

    // Session zaman çizelgesi (en yeni 500 session, sonradan eskiden yeniye sıralanır)
    let timeline_rows = sqlx::query(
        r#"
        SELECT
            ms.id AS session_id,
            ms.slug,
            p.mtm_pnl,
            p.cost_basis,
            p.up_filled,
            p.down_filled,
            p.ts_ms
        FROM market_sessions ms
        JOIN (
            SELECT market_session_id, mtm_pnl, cost_basis, up_filled, down_filled, MAX(ts_ms) AS ts_ms
            FROM pnl_snapshots
            WHERE bot_id = ?
            GROUP BY market_session_id
            HAVING cost_basis > 0
        ) p ON p.market_session_id = ms.id
        WHERE ms.bot_id = ?
        ORDER BY p.ts_ms DESC
        LIMIT 500
        "#,
    )
    .bind(bot_id)
    .bind(bot_id)
    .fetch_all(pool)
    .await?;

    let mut sessions_timeline: Vec<SessionTimelineItem> = timeline_rows
        .iter()
        .map(|r| {
            let mtm_pnl: f64 = r.try_get("mtm_pnl").unwrap_or(0.0);
            let cost_basis: f64 = r.try_get("cost_basis").unwrap_or(0.0);
            let up_filled: f64 = r.try_get("up_filled").unwrap_or(0.0);
            let down_filled: f64 = r.try_get("down_filled").unwrap_or(0.0);
            let position_type = if up_filled > 0.0 && down_filled == 0.0 {
                "SAF_UP"
            } else if down_filled > 0.0 && up_filled == 0.0 {
                "SAF_DOWN"
            } else {
                "KARMA"
            };
            let roi_pct = if cost_basis > 0.0 {
                (mtm_pnl / cost_basis) * 100.0
            } else {
                0.0
            };
            SessionTimelineItem {
                session_id: r.try_get("session_id").unwrap_or(0),
                slug: r.try_get("slug").unwrap_or_default(),
                mtm_pnl,
                cost_basis,
                roi_pct,
                position_type: position_type.to_string(),
                ts_ms: r.try_get("ts_ms").unwrap_or(0),
            }
        })
        .collect();
    // DESC ile çekilen verileri kronolojik (ASC) sıraya döndür
    sessions_timeline.reverse();

    Ok(BotStats {
        total_sessions,
        winning,
        losing,
        winrate_pct,
        total_mtm_pnl,
        total_cost_basis,
        roi_pct,
        total_fee,
        avg_session_pnl,
        best_session_pnl,
        worst_session_pnl,
        total_trades,
        by_type,
        sessions_timeline,
    })
}

/// `update_bot` için editable alanlar. `slug_pattern` ve `strategy` immutable.
#[derive(Debug, Clone)]
pub struct BotUpdate {
    pub name: String,
    pub run_mode: RunMode,
    pub order_usdc: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub cooldown_threshold: u64,
    pub start_offset: u32,
    pub strategy_params: StrategyParams,
}

/// Bot ayarlarını güncelle. Çağıran taraf STOPPED olduğunu garanti etmeli;
/// koşan process'e yansımaz (restart gerekir).
pub async fn update_bot(pool: &SqlitePool, bot_id: i64, upd: &BotUpdate) -> Result<(), AppError> {
    let now = now_ms() as i64;
    let run_mode = serde_json::to_string(&upd.run_mode)?
        .trim_matches('"')
        .to_string();
    let params = serde_json::to_string(&upd.strategy_params)?;

    sqlx::query(
        "UPDATE bots SET name = ?, run_mode = ?, \
         order_usdc = ?, min_price = ?, max_price = ?, \
         cooldown_threshold = ?, start_offset = ?, strategy_params = ?, \
         updated_at_ms = ? WHERE id = ?",
    )
    .bind(&upd.name)
    .bind(&run_mode)
    .bind(upd.order_usdc)
    .bind(upd.min_price)
    .bind(upd.max_price)
    .bind(upd.cooldown_threshold as i64)
    .bind(upd.start_offset as i64)
    .bind(&params)
    .bind(now)
    .bind(bot_id)
    .execute(pool)
    .await?;
    Ok(())
}
