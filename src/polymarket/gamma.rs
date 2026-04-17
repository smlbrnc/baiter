//! Polymarket Gamma REST — market keşfi ve metadata.
//!
//! Yalnızca bu taban (`gamma-api.polymarket.com`) üzerinden slug,
//! `clobTokenIds`, `startDate`/`endDate` çekilir (mimari §0).

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Gamma market yanıt DTO'su — ihtiyaç duyulan alanlar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GammaMarket {
    #[serde(default)]
    pub id: Option<String>,
    pub slug: String,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default, rename = "conditionId")]
    pub condition_id: Option<String>,
    /// JSON string — `["0xYes","0xNo"]` — Polymarket stringleştirir.
    #[serde(default, rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,
    #[serde(default, rename = "orderPriceMinTickSize")]
    pub tick_size: Option<f64>,
    #[serde(default, rename = "orderMinSize")]
    pub minimum_order_size: Option<f64>,
    #[serde(default, rename = "startDate")]
    pub start_date: Option<String>,
    #[serde(default, rename = "endDate")]
    pub end_date: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub closed: Option<bool>,
    #[serde(default, rename = "acceptingOrders")]
    pub accepting_orders: Option<bool>,
}

impl GammaMarket {
    /// `clobTokenIds` JSON string'ini `(yes_id, no_id)` olarak parse eder.
    pub fn parse_token_ids(&self) -> Result<(String, String), AppError> {
        let raw = self
            .clob_token_ids
            .as_ref()
            .ok_or_else(|| AppError::Gamma("clobTokenIds eksik".to_string()))?;
        let ids: Vec<String> = serde_json::from_str(raw)
            .map_err(|e| AppError::Gamma(format!("clobTokenIds parse: {e}")))?;
        if ids.len() != 2 {
            return Err(AppError::Gamma(format!(
                "clobTokenIds 2 öğe beklendi, {} geldi",
                ids.len()
            )));
        }
        Ok((ids[0].clone(), ids[1].clone()))
    }
}

/// Gamma REST istemcisi — paylaşımlı `reqwest::Client`.
pub struct GammaClient {
    http: Client,
    base: String,
}

impl GammaClient {
    pub fn new(http: Client, base: String) -> Self {
        Self { http, base }
    }

    /// Tam slug ile market detayı.
    /// `GET /markets/slug/{slug}`.
    pub async fn get_market_by_slug(&self, slug: &str) -> Result<GammaMarket, AppError> {
        let url = format!("{}/markets/slug/{}", self.base, slug);
        let m: GammaMarket = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(m)
    }

    /// Aktif market listesi (öneki ile filtreli).
    /// `GET /markets?active=true&closed=false` → slug öneki ile filtre.
    pub async fn list_active_by_prefix(
        &self,
        slug_prefix: &str,
    ) -> Result<Vec<GammaMarket>, AppError> {
        let url = format!("{}/markets?active=true&closed=false&limit=500", self.base);
        let markets: Vec<GammaMarket> = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(markets
            .into_iter()
            .filter(|m| m.slug.starts_with(slug_prefix))
            .collect())
    }
}
