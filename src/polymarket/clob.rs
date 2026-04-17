//! Polymarket CLOB REST istemcisi — orderbook, emir, heartbeat.
//!
//! Referans: [docs/api/polymarket-clob.md](../../../docs/api/polymarket-clob.md).
//!
//! §⚡ Kural 3: Paylaşımlı `reqwest::Client` (http2 + rustls + tcp_nodelay,
//! connection pool). Engine per-request client oluşturmaz.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Credentials;
use crate::error::AppError;
use crate::polymarket::auth::{body_to_string, make_l2_headers};
use crate::time::now_secs;

/// Paylaşımlı HTTP client — tek tip (uzun süreli pool).
pub fn shared_http_client() -> Client {
    Client::builder()
        .pool_max_idle_per_host(16)
        .tcp_nodelay(true)
        .http2_prior_knowledge() // §⚡ Kural 3
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

    // ---------------------------- Public (no auth) ----------------------------

    /// `GET /time` — sunucu UNIX timestamp (HMAC saat senkronizasyonu için).
    pub async fn get_time(&self) -> Result<u64, AppError> {
        #[derive(Deserialize)]
        struct Resp {
            #[serde(rename = "unixtime")]
            unix_time: Option<u64>,
            #[serde(rename = "serverTime")]
            server_time: Option<u64>,
        }
        let url = format!("{}/time", self.base);
        let resp: Value = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Ok(r) = serde_json::from_value::<Resp>(resp.clone()) {
            if let Some(v) = r.unix_time.or(r.server_time) {
                return Ok(v);
            }
        }
        if let Some(n) = resp.as_u64() {
            return Ok(n);
        }
        Err(AppError::Clob(format!("/time parse hatası: {resp}")))
    }

    /// `GET /book?token_id=...` — tek bir outcome için orderbook snapshot.
    pub async fn get_book(&self, token_id: &str) -> Result<BookResponse, AppError> {
        let url = format!("{}/book?token_id={}", self.base, token_id);
        let book: BookResponse = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(book)
    }

    // ---------------------------- Authenticated (L2) ----------------------------

    /// Generic authenticated POST — imza + 5 header ekler.
    async fn auth_post(&self, path: &str, body: Value) -> Result<Value, AppError> {
        let creds = self.creds()?;
        let ts = now_secs().to_string();
        let body_str = body_to_string(&body);
        let headers = make_l2_headers(creds, &ts, "POST", path, &body_str)?;

        let url = format!("{}{}", self.base, path);
        let req = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body_str);
        let resp = headers
            .apply(req)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    async fn auth_delete(&self, path: &str, body: Value) -> Result<Value, AppError> {
        let creds = self.creds()?;
        let ts = now_secs().to_string();
        let body_str = body_to_string(&body);
        let headers = make_l2_headers(creds, &ts, "DELETE", path, &body_str)?;

        let url = format!("{}{}", self.base, path);
        let req = self
            .http
            .delete(&url)
            .header("Content-Type", "application/json")
            .body(body_str);
        let resp = headers
            .apply(req)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    /// `POST /order` — tek emir.
    pub async fn post_order(
        &self,
        order: Value,
        order_type: &str,
        owner: &str,
    ) -> Result<PostOrderResponse, AppError> {
        let body = serde_json::json!({
            "order": order,
            "orderType": order_type,
            "owner": owner,
            "deferExec": false,
        });
        let resp = self.auth_post("/order", body).await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// `DELETE /order` — tek iptal.
    pub async fn cancel_order(&self, order_id: &str) -> Result<CancelResponse, AppError> {
        let body = serde_json::json!({"orderID": order_id});
        let resp = self.auth_delete("/order", body).await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// `DELETE /cancel-all` — tüm açık emirleri iptal et.
    pub async fn cancel_all(&self) -> Result<CancelResponse, AppError> {
        let resp = self.auth_delete("/cancel-all", Value::Null).await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// `GET /heartbeat` veya `POST /postHeartbeat` — 5 sn aralıkla.
    /// Resmi örnek POST olarak tanımlar.
    pub async fn heartbeat_once(&self) -> Result<(), AppError> {
        let _ = self.auth_post("/postHeartbeat", Value::Null).await?;
        Ok(())
    }
}

// -------------------------- DTO'lar --------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookResponse {
    #[serde(default)]
    pub market: Option<String>,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub bids: Vec<PriceLevel>,
    #[serde(default)]
    pub asks: Vec<PriceLevel>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub tick_size: Option<String>,
    #[serde(default)]
    pub min_order_size: Option<String>,
    #[serde(default)]
    pub neg_risk: Option<bool>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: String,
    pub size: String,
}

impl PriceLevel {
    pub fn price_f64(&self) -> f64 {
        self.price.parse().unwrap_or(0.0)
    }
    pub fn size_f64(&self) -> f64 {
        self.size.parse().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostOrderResponse {
    pub success: bool,
    #[serde(default, rename = "orderID")]
    pub order_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "errorMsg")]
    pub error_msg: String,
    #[serde(default, rename = "transactionsHashes")]
    pub transactions_hashes: Vec<String>,
    #[serde(default, rename = "tradeIDs")]
    pub trade_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelResponse {
    #[serde(default)]
    pub canceled: Vec<String>,
    #[serde(default)]
    pub not_canceled: Value,
}
