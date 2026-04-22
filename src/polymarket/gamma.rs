use reqwest::Client;
use serde::Deserialize;

use crate::error::AppError;

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
    #[serde(default)]
    pub outcomes: Option<String>,
    #[serde(default, rename = "orderPriceMinTickSize")]
    pub tick_size: Option<f64>,
    #[serde(default, rename = "orderMinSize")]
    pub minimum_order_size: Option<f64>,
    #[serde(default, rename = "negRisk")]
    pub neg_risk: Option<bool>,
}

impl GammaMarket {
    /// `outcomes[i] ↔ clobTokenIds[i]` pozisyonel pairing (Gamma şeması).
    /// Bot yalnız UP/DOWN ikili marketleri destekler ("Up" → up_token_id,
    /// "Down" → down_token_id, case-insensitive).
    pub fn parse_token_ids(&self) -> Result<(String, String), AppError> {
        let ids_raw = self
            .clob_token_ids
            .as_ref()
            .ok_or_else(|| AppError::Gamma("clobTokenIds eksik".to_string()))?;
        let outcomes_raw = self
            .outcomes
            .as_ref()
            .ok_or_else(|| AppError::Gamma("outcomes eksik".to_string()))?;
        let ids: Vec<String> = serde_json::from_str(ids_raw)
            .map_err(|e| AppError::Gamma(format!("clobTokenIds parse: {e}")))?;
        let outcomes: Vec<String> = serde_json::from_str(outcomes_raw)
            .map_err(|e| AppError::Gamma(format!("outcomes parse: {e}")))?;
        if ids.len() != 2 || outcomes.len() != 2 {
            return Err(AppError::Gamma(format!(
                "clobTokenIds & outcomes 2 öğe beklendi (got {}/{})",
                ids.len(),
                outcomes.len()
            )));
        }
        let mut up = None;
        let mut down = None;
        for (idx, name) in outcomes.iter().enumerate() {
            match name.trim().to_ascii_uppercase().as_str() {
                "UP" => up = Some(ids[idx].clone()),
                "DOWN" => down = Some(ids[idx].clone()),
                other => {
                    return Err(AppError::Gamma(format!(
                        "tanınmayan outcome '{other}' (UP/DOWN bekleniyor)"
                    )));
                }
            }
        }
        match (up, down) {
            (Some(u), Some(d)) => Ok((u, d)),
            _ => Err(AppError::Gamma(format!(
                "outcomes beklenen UP/DOWN çiftini içermiyor: {outcomes:?}"
            ))),
        }
    }
}

pub struct GammaClient {
    http: Client,
    base: String,
}

impl GammaClient {
    pub fn new(http: Client, base: String) -> Self {
        Self { http, base }
    }

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
