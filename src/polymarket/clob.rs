use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Method};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::config::Credentials;
use crate::error::AppError;
use crate::polymarket::auth::make_l2_headers;
use crate::time::now_secs;

/// `POST /orders` resmi batch tavanı.
pub const POST_ORDERS_MAX_PER_REQ: usize = 15;
/// `DELETE /orders` resmi batch tavanı.
pub const CANCEL_ORDERS_MAX_PER_REQ: usize = 3000;

/// Taker fee parametreleri (`/clob-markets/{condition_id}.fd`).
#[derive(Debug, Clone, Copy)]
pub struct TakerFee {
    pub rate: f64,
    pub taker_only: bool,
}

pub fn shared_http_client() -> Client {
    Client::builder()
        .pool_max_idle_per_host(16)
        .tcp_nodelay(true)
        .http2_prior_knowledge()
        .timeout(Duration::from_secs(10))
        .user_agent("baiter-pro/0.1")
        .build()
        .expect("reqwest client build")
}

#[derive(Debug, Clone)]
pub struct ClobClient {
    http: Client,
    base: String,
    creds: Option<Arc<Credentials>>,
}

impl ClobClient {
    pub fn new(http: Client, base: String, creds: Option<Credentials>) -> Self {
        Self {
            http,
            base,
            creds: creds.map(Arc::new),
        }
    }

    fn creds(&self) -> Result<&Credentials, AppError> {
        self.creds
            .as_deref()
            .ok_or_else(|| AppError::Auth("credentials eksik (dry run? env?)".to_string()))
    }

    /// CLOB V2 public GET — auth gerektirmez.
    async fn public_get_json(&self, path: &str) -> Result<Value, AppError> {
        let url = format!("{}{}", self.base, path);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json::<Value>().await?)
    }

    /// `GET /clob-markets/{condition_id}` → `fd.r` (rate) + `fd.to` (taker_only).
    pub async fn get_taker_fee(&self, condition_id: &str) -> Result<TakerFee, AppError> {
        let v = self
            .public_get_json(&format!("/clob-markets/{condition_id}"))
            .await?;
        let fd = v
            .get("fd")
            .ok_or_else(|| AppError::Clob(format!("clob-markets/{condition_id}: 'fd' missing")))?;
        let rate = fd.get("r").and_then(Value::as_f64).ok_or_else(|| {
            AppError::Clob(format!("clob-markets/{condition_id}: 'fd.r' missing or not number"))
        })?;
        let taker_only = fd.get("to").and_then(Value::as_bool).ok_or_else(|| {
            AppError::Clob(format!("clob-markets/{condition_id}: 'fd.to' missing or not bool"))
        })?;
        Ok(TakerFee { rate, taker_only })
    }

    async fn auth_request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, AppError> {
        let creds = self.creds()?;
        let ts = now_secs().to_string();
        let body_str = body.as_ref().map(Value::to_string).unwrap_or_default();
        let headers = make_l2_headers(creds, &ts, method.as_str(), path, &body_str)?;

        let url = format!("{}{}", self.base, path);
        let mut req = self
            .http
            .request(method.clone(), &url)
            .header("Content-Type", "application/json");
        if !body_str.is_empty() {
            req = req.body(body_str);
        }
        let resp = headers.apply(req).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            tracing::warn!(
                method = method.as_str(),
                path,
                status = status.as_u16(),
                body = %text,
                "clob non-2xx"
            );
            return Err(AppError::Clob(format!(
                "{} {} → HTTP {}: {}",
                method,
                path,
                status.as_u16(),
                text
            )));
        }
        if text.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| {
            AppError::Clob(format!("{} {} → parse: {} (body={})", method, path, e, text))
        })
    }

    /// Batch `POST /orders` — max 15/req, üstüyse chunk'lar; giriş sırası korunur.
    pub async fn post_orders(
        &self,
        items: Vec<PostOrderItem>,
    ) -> Result<Vec<PostOrderResponse>, AppError> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(items.len());
        for chunk in items.chunks(POST_ORDERS_MAX_PER_REQ) {
            let body = Value::Array(
                chunk
                    .iter()
                    .map(|i| {
                        serde_json::json!({
                            "order": i.order,
                            "owner": i.owner,
                            "orderType": i.order_type,
                        })
                    })
                    .collect(),
            );
            let resp = self.auth_request(Method::POST, "/orders", Some(body)).await?;
            let parsed: Vec<PostOrderResponse> = serde_json::from_value(resp).map_err(|e| {
                AppError::Clob(format!("POST /orders parse: {e}"))
            })?;
            out.extend(parsed);
        }
        Ok(out)
    }

    /// Batch `DELETE /orders` — max 3000/req; `canceled` ve `not_canceled` map'lerini birleştirir.
    pub async fn cancel_orders(&self, ids: &[String]) -> Result<CancelResponse, AppError> {
        if ids.is_empty() {
            return Ok(CancelResponse {
                canceled: Vec::new(),
                not_canceled: serde_json::json!({}),
            });
        }
        let mut canceled = Vec::with_capacity(ids.len());
        let mut not_canceled_merged = serde_json::Map::new();
        for chunk in ids.chunks(CANCEL_ORDERS_MAX_PER_REQ) {
            let body = Value::Array(chunk.iter().map(|s| Value::String(s.clone())).collect());
            let resp = self
                .auth_request(Method::DELETE, "/orders", Some(body))
                .await?;
            let parsed: CancelResponse = serde_json::from_value(resp).map_err(|e| {
                AppError::Clob(format!("DELETE /orders parse: {e}"))
            })?;
            canceled.extend(parsed.canceled);
            if let Some(map) = parsed.not_canceled.as_object() {
                for (k, v) in map {
                    not_canceled_merged.insert(k.clone(), v.clone());
                }
            }
        }
        Ok(CancelResponse {
            canceled,
            not_canceled: Value::Object(not_canceled_merged),
        })
    }

    pub async fn cancel_all(&self) -> Result<CancelResponse, AppError> {
        let resp = self
            .auth_request(Method::DELETE, "/cancel-all", None)
            .await?;
        Ok(serde_json::from_value(resp)?)
    }

    pub async fn heartbeat_once(&self) -> Result<(), AppError> {
        self.auth_request(Method::POST, "/heartbeats", None).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostOrderStatus {
    Matched,
    Live,
    Delayed,
    Unmatched,
}

impl PostOrderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Live => "live",
            Self::Delayed => "delayed",
            Self::Unmatched => "unmatched",
        }
    }

    pub fn is_filled(self) -> bool {
        matches!(self, Self::Matched)
    }
}

impl<'de> Deserialize<'de> for PostOrderStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        match raw.trim().to_ascii_lowercase().as_str() {
            "matched" => Ok(Self::Matched),
            "live" => Ok(Self::Live),
            "delayed" => Ok(Self::Delayed),
            "unmatched" => Ok(Self::Unmatched),
            other => Err(serde::de::Error::custom(format!(
                "unknown PostOrderStatus: {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostOrderResponse {
    pub success: bool,
    #[serde(default, rename = "orderID")]
    pub order_id: String,
    pub status: PostOrderStatus,
    #[serde(default, rename = "errorMsg")]
    pub error_msg: String,
}

/// `POST /orders` body item.
#[derive(Debug, Clone, Serialize)]
pub struct PostOrderItem {
    pub order: Value,
    pub owner: String,
    pub order_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CancelResponse {
    #[serde(default)]
    pub canceled: Vec<String>,
    #[serde(default)]
    pub not_canceled: Value,
}
