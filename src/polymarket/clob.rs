//! Polymarket CLOB REST istemcisi — orderbook, emir, fee, heartbeat.
//!
//! Referans: [docs/api/polymarket-clob.md](../../../docs/api/polymarket-clob.md).
//!
//! §⚡ Kural 3: Paylaşımlı `reqwest::Client` (http2 + rustls + tcp_nodelay,
//! connection pool). Engine per-request client oluşturmaz.

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Method};
use serde::{Deserialize, Deserializer};
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

    /// `GET /fee-rate?token_id=...` — token bazlı maker base fee (basis points).
    ///
    /// Polymarket fee'leri marketten markete değişir; hardcoded 0 göndermek
    /// `invalid fee rate (0), current market's maker fee: ...` 400 hatasına
    /// yol açar. Pencere açılırken bir kez fetch edip `MarketSession`'a yazıyoruz.
    ///
    /// Doc: <https://docs.polymarket.com/api-reference/market-data/get-fee-rate>
    /// Response şeması: `{ "base_fee": <int_bps> }`.
    pub async fn fetch_fee_rate_bps(&self, token_id: &str) -> Result<u32, AppError> {
        #[derive(Deserialize)]
        struct FeeRate {
            base_fee: u32,
        }
        // token_id daima büyük decimal int (CTF token id) — URL-safe; manuel
        // query string yeterli, `serde_urlencoded` (reqwest `query` feature)
        // bağımlılığını çekmiyor.
        let url = format!("{}/fee-rate?token_id={token_id}", self.base);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json::<FeeRate>().await?.base_fee)
    }

    // ---------------------------- Authenticated (L2) ----------------------------

    /// Generic authenticated request — L2 imza + 5 header ekler.
    ///
    /// `body=None` → boş gövdeli istek (heartbeat, cancel-all). HMAC mesajı
    /// `ts + METHOD + path + ""` olur — py-clob-client paritesi. `Some(v)` ise
    /// kompakt JSON serialize edilir; **signed body == sent body** garanti.
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
            // Polymarket genelde JSON `{"error": "..."}` veya plain text döner.
            // İmza/expiry/order yapısı hatalarını hızla görmek için body'yi log'a
            // ve hatanın kendisine ekle.
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

    /// `POST /order` — tek emir.
    ///
    /// Şema: [post-a-new-order](https://docs.polymarket.com/api-reference/trade/post-a-new-order).
    /// Zorunlu alanlar: `order`, `owner`. `orderType` default `GTC`, `deferExec` default `false` —
    /// göndermek istemediğimiz default'ları es geçeriz (gereksiz alan = imza body byte'ları büyür,
    /// hata mesajı bulanıklaşır).
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

    /// `DELETE /order` — tek iptal.
    pub async fn cancel_order(&self, order_id: &str) -> Result<CancelResponse, AppError> {
        let body = serde_json::json!({"orderID": order_id});
        let resp = self
            .auth_request(Method::DELETE, "/order", Some(body))
            .await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// `DELETE /cancel-all` — tüm açık emirleri iptal et.
    pub async fn cancel_all(&self) -> Result<CancelResponse, AppError> {
        let resp = self
            .auth_request(Method::DELETE, "/cancel-all", None)
            .await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// `POST /heartbeats` — auth'lu session-keepalive ping'i.
    ///
    /// Polymarket heartbeats not received within ~10s ⇒ kullanıcının tüm açık
    /// emirleri **otomatik iptal edilir**. Path **çoğul `/heartbeats`** —
    /// `/heartbeat` (tekil) 404 döner.
    ///
    /// Doc: <https://docs.polymarket.com/api-reference/trade/send-heartbeat>
    pub async fn heartbeat_once(&self) -> Result<(), AppError> {
        self.auth_request(Method::POST, "/heartbeats", None).await?;
        Ok(())
    }
}

// -------------------------- DTO'lar --------------------------

/// Polymarket CLOB `POST /order` response status enum.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/orders/create-an-order>
///
/// - `Matched` — karşı taraf REST anında bulundu, **kısmi veya tam** fill.
///   GTC için kısmi match'te de `status=matched` döner; kalan miktar kitapta
///   canlı kalır. Yanıt gerçek fill miktarını taşımadığı için
///   `LiveExecutor::place` `open_orders`'a `size_matched = 0` push eder; gerçek
///   fill_size + price + prune User WS `trade MATCHED` event'inin **tek**
///   sorumluluğudur (`record_fill_and_prune_if_full`).
/// - `Live` — kitaba (orderbook) girdi, passive bekliyor →
///   `open_orders`'a `size_matched = 0` push.
/// - `Delayed` — CLOB asenkron eşleştirme kuyruğunda; sonuç User WS
///   `trade MATCHED` ile gelir → `open_orders`'a `size_matched = 0` push.
/// - `Unmatched` — reject; `PostOrderResponse.success=false` ile birlikte
///   `error_msg` doldurulur ve `LiveExecutor::place` `AppError::Clob` döndürür.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostOrderStatus {
    Matched,
    Live,
    Delayed,
    Unmatched,
}

impl PostOrderStatus {
    /// Canonical lowercase string — DB persist ve hata mesajı için.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Live => "live",
            Self::Delayed => "delayed",
            Self::Unmatched => "unmatched",
        }
    }

    /// REST anında dolan emir mi?
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

/// Polymarket CLOB `POST /order` response.
///
/// Spec: <https://docs.polymarket.com/developers/CLOB/orders/create-an-order>
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

#[cfg(test)]
mod tests {
    use super::{PostOrderResponse, PostOrderStatus};

    #[test]
    fn parse_known_variants_case_insensitive() {
        for (raw, want) in [
            ("matched", PostOrderStatus::Matched),
            ("LIVE", PostOrderStatus::Live),
            ("Delayed", PostOrderStatus::Delayed),
            ("  unmatched\n", PostOrderStatus::Unmatched),
        ] {
            let resp: PostOrderResponse = serde_json::from_value(serde_json::json!({
                "success": true,
                "orderID": "0xabc",
                "status": raw,
            }))
            .unwrap();
            assert_eq!(resp.status, want, "raw={raw}");
        }
    }

    #[test]
    fn parse_unknown_status_errors() {
        let err = serde_json::from_value::<PostOrderResponse>(serde_json::json!({
            "success": true,
            "orderID": "0xabc",
            "status": "weird",
        }))
        .unwrap_err();
        assert!(
            err.to_string().contains("unknown PostOrderStatus"),
            "err={err}"
        );
    }

    #[test]
    fn is_filled_only_for_matched() {
        assert!(PostOrderStatus::Matched.is_filled());
        assert!(!PostOrderStatus::Live.is_filled());
        assert!(!PostOrderStatus::Delayed.is_filled());
        assert!(!PostOrderStatus::Unmatched.is_filled());
    }

    #[test]
    fn as_str_round_trips() {
        for s in [
            PostOrderStatus::Matched,
            PostOrderStatus::Live,
            PostOrderStatus::Delayed,
            PostOrderStatus::Unmatched,
        ] {
            assert_eq!(PostOrderStatus::deserialize_from_str(s.as_str()), s);
        }
    }

    impl PostOrderStatus {
        fn deserialize_from_str(s: &str) -> Self {
            serde_json::from_value(serde_json::Value::String(s.into())).unwrap()
        }
    }
}
