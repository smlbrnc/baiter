use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Method};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::config::Credentials;
use crate::error::AppError;
use crate::polymarket::auth::{body_to_string, make_l2_headers};
use crate::time::now_secs;

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

    pub async fn fetch_fee_rate_bps(&self, token_id: &str) -> Result<u32, AppError> {
        #[derive(Deserialize)]
        struct FeeRate {
            base_fee: u32,
        }
        let url = format!("{}/fee-rate?token_id={token_id}", self.base);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json::<FeeRate>().await?.base_fee)
    }

    async fn auth_request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, AppError> {
        let creds = self.creds()?;
        let ts = now_secs().to_string();
        let body_str = body.as_ref().map(body_to_string).unwrap_or_default();
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

    pub async fn post_order(
        &self,
        order: Value,
        order_type: &str,
        owner: &str,
    ) -> Result<PostOrderResponse, AppError> {
        let body = serde_json::json!({
            "order": order,
            "owner": owner,
            "orderType": order_type,
        });
        let resp = self.auth_request(Method::POST, "/order", Some(body)).await?;
        Ok(serde_json::from_value(resp)?)
    }

    pub async fn cancel_order(&self, order_id: &str) -> Result<CancelResponse, AppError> {
        let body = serde_json::json!({"orderID": order_id});
        let resp = self
            .auth_request(Method::DELETE, "/order", Some(body))
            .await?;
        Ok(serde_json::from_value(resp)?)
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

#[derive(Debug, Clone, Deserialize)]
pub struct CancelResponse {
    #[serde(default)]
    pub canceled: Vec<String>,
    #[serde(default)]
    pub not_canceled: Value,
}
