//! Polymarket Gamma REST — market keşfi ve metadata.
//!
//! Yalnızca bu taban (`gamma-api.polymarket.com`) üzerinden slug,
//! `clobTokenIds`, `startDate`/`endDate` çekilir (mimari §0).

use reqwest::Client;
use serde::Deserialize;

use crate::error::AppError;

/// Gamma market yanıt DTO'su — yalnızca bot tarafından okunan alanlar.
/// Polymarket Gamma şemasında daha çok alan vardır; serde varsayılan olarak
/// bilinmeyen alanları yutar (deserialize sapması olmaz).
#[derive(Debug, Clone, Deserialize)]
pub struct GammaMarket {
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default, rename = "conditionId")]
    pub condition_id: Option<String>,
    #[serde(default, rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,
    #[serde(default, rename = "orderPriceMinTickSize")]
    pub tick_size: Option<f64>,
    #[serde(default, rename = "orderMinSize")]
    pub minimum_order_size: Option<f64>,
    #[serde(default, rename = "negRisk")]
    pub neg_risk: Option<bool>,
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

}
