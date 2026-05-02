//! Polymarket CLOB V2 REST istemcisi (POST /order, DELETE /order|/orders,
//! /cancel-all, /clob-markets, /heartbeats).
//!
//! Hot-path tasarımı:
//! * `auth_request` body'yi tek seferde `Vec<u8>` olarak üretir; HMAC
//!   imzası bu byte slice'ından `&str` üzerinden alınır (alloc yok),
//!   HTTP gövdesine de aynı bytes gönderilir.
//! * `method` parametresi `&'static str` literal (`"POST"`, `"DELETE"`,
//!   …); runtime'da `to_uppercase` allocation'ı yapılmaz.
//! * Response gövdesi doğrudan `serde_json::from_str::<T>` ile hedef
//!   tipe parse edilir; ara `Value` adımı yok.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::config::Credentials;
use crate::error::AppError;
use crate::polymarket::auth::make_l2_headers;
use crate::time::now_secs;

/// `DELETE /orders` resmi batch tavanı.
const CANCEL_ORDERS_MAX_PER_REQ: usize = 3000;

/// Taker fee parametreleri (`/clob-markets/{condition_id}.fd`).
#[derive(Debug, Clone, Copy)]
pub struct TakerFee {
    pub rate: f64,
    pub taker_only: bool,
}

pub fn shared_http_client() -> Client {
    Client::builder()
        // Connection pool — TLS handshake amortize için canlı tut.
        .pool_max_idle_per_host(16)
        .pool_idle_timeout(Duration::from_secs(300))
        // TCP optimizations.
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(60))
        // HTTP/2 zorunlu + keep-alive ping (idle bile olsa connection canlı).
        // Polymarket Cloudflare HTTP/2 — bu sayede TLS handshake bir kez yapılır.
        .http2_prior_knowledge()
        .http2_keep_alive_interval(Duration::from_secs(20))
        .http2_keep_alive_timeout(Duration::from_secs(5))
        .http2_keep_alive_while_idle(true)
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
            base: base.trim_end_matches('/').to_string(),
            creds: creds.map(Arc::new),
        }
    }

    fn creds(&self) -> Result<&Credentials, AppError> {
        self.creds
            .as_deref()
            .ok_or_else(|| AppError::Auth("credentials eksik (dry run? env?)".to_string()))
    }

    /// Public CLOB GET — auth gerektirmez. Yalnızca `get_taker_fee` tarafından kullanılır.
    async fn public_get_typed<T: DeserializeOwned>(&self, path: &str) -> Result<T, AppError> {
        let url = format!("{}{}", self.base, path);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let text = resp.text().await?;
        serde_json::from_str::<T>(&text)
            .map_err(|e| AppError::Clob(format!("GET {path} → parse: {e} (body={text})")))
    }

    /// `GET /clob-markets/{condition_id}` → `fd.r` (rate) + `fd.to` (taker_only).
    pub async fn get_taker_fee(&self, condition_id: &str) -> Result<TakerFee, AppError> {
        let v: Value = self
            .public_get_typed(&format!("/clob-markets/{condition_id}"))
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

    /// HMAC + body single-serialization L2 request. `T = IgnoredAny` ise gövde yok sayılır.
    async fn auth_request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: &'static str,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, AppError> {
        let creds = self.creds()?;
        let ts = now_secs().to_string();

        let body_bytes = match body {
            Some(b) => serde_json::to_vec(b)
                .map_err(|e| AppError::Clob(format!("body serialize: {e}")))?,
            None => Vec::new(),
        };
        let body_str = std::str::from_utf8(&body_bytes)
            .map_err(|e| AppError::Clob(format!("body utf-8: {e}")))?;
        let headers = make_l2_headers(creds, ts, method, path, body_str)?;

        let url = format!("{}{}", self.base, path);
        let req_method = match method {
            "POST" => reqwest::Method::POST,
            "GET" => reqwest::Method::GET,
            "DELETE" => reqwest::Method::DELETE,
            _ => unreachable!("auth_request method literali destekli olmalı: {method}"),
        };
        let mut req = self
            .http
            .request(req_method, &url)
            .header("Content-Type", "application/json");
        if !body_bytes.is_empty() {
            req = req.body(body_bytes);
        }
        let resp = headers.apply(req).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            tracing::warn!(
                method,
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
            return serde_json::from_str::<T>("null").map_err(|e| {
                AppError::Clob(format!("{method} {path} → empty body parse: {e}"))
            });
        }
        serde_json::from_str::<T>(&text).map_err(|e| {
            AppError::Clob(format!("{method} {path} → parse: {e} (body={text})"))
        })
    }

    /// Tek V2 `POST /order` çağrısı. Hot-path = bir HTTP round-trip.
    ///
    /// 4xx yanıtları (örn. "not enough balance") hard `Err` değil, `success=false`
    /// `PostOrderResponse` olarak döner; `place_many` bunu `continue` ile atlar.
    /// 5xx veya ağ hataları hâlâ `Err` olarak propagate edilir.
    pub async fn post_order(
        &self,
        order: &Value,
        owner: &str,
        order_type: &str,
    ) -> Result<PostOrderResponse, AppError> {
        let creds = self.creds()?;
        let ts = now_secs().to_string();
        let body = PostOrderBody { order, owner, order_type };
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| AppError::Clob(format!("post_order serialize: {e}")))?;
        let body_str = std::str::from_utf8(&body_bytes)
            .map_err(|e| AppError::Clob(format!("post_order utf-8: {e}")))?;
        let headers = make_l2_headers(creds, ts, "POST", "/order", body_str)?;
        let url = format!("{}/order", self.base);
        let resp = headers
            .apply(
                self.http
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .body(body_bytes),
            )
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;

        if status.is_success() {
            return serde_json::from_str::<PostOrderResponse>(&text).map_err(|e| {
                AppError::Clob(format!("POST /order → parse: {e} (body={text})"))
            });
        }

        // 4xx = rejected order (balance, price bounds, vb.) → soft rejection.
        // 5xx = sunucu hatası → hard error.
        if status.is_client_error() {
            let error_msg = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| text.clone());
            tracing::debug!(
                status = status.as_u16(),
                error = %error_msg,
                "post_order 4xx (soft)"
            );
            return Ok(PostOrderResponse {
                success: false,
                order_id: String::new(),
                status: PostOrderStatus::Unmatched,
                error_msg,
            });
        }

        // 5xx veya diğer beklenmedik kodlar → hard error.
        tracing::warn!(
            method = "POST",
            path = "/order",
            status = status.as_u16(),
            body = %text,
            "clob non-2xx"
        );
        Err(AppError::Clob(format!(
            "POST /order → HTTP {}: {}",
            status.as_u16(),
            text
        )))
    }

    /// Tekil `DELETE /order` veya batch `DELETE /orders` (>1).
    pub async fn cancel_orders(&self, ids: &[String]) -> Result<CancelResponse, AppError> {
        if ids.is_empty() {
            return Ok(CancelResponse::default());
        }
        if ids.len() == 1 {
            let body = CancelOneBody { order_id: &ids[0] };
            return self.auth_request("DELETE", "/order", Some(&body)).await;
        }
        let mut canceled = Vec::with_capacity(ids.len());
        let mut not_canceled_merged = serde_json::Map::new();
        for chunk in ids.chunks(CANCEL_ORDERS_MAX_PER_REQ) {
            let parsed: CancelResponse = self
                .auth_request("DELETE", "/orders", Some(&chunk))
                .await?;
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
        self.auth_request::<(), CancelResponse>("DELETE", "/cancel-all", None)
            .await
    }

    pub async fn heartbeat_once(&self) -> Result<(), AppError> {
        self.auth_request::<(), IgnoredAny>("POST", "/heartbeats", None)
            .await?;
        Ok(())
    }
}

#[derive(Serialize)]
struct PostOrderBody<'a> {
    order: &'a Value,
    owner: &'a str,
    #[serde(rename = "orderType")]
    order_type: &'a str,
}

#[derive(Serialize)]
struct CancelOneBody<'a> {
    #[serde(rename = "orderID")]
    order_id: &'a str,
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

impl Default for CancelResponse {
    fn default() -> Self {
        Self {
            canceled: Vec::new(),
            not_canceled: Value::Object(serde_json::Map::new()),
        }
    }
}
