use alloy::primitives::U256;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::{eip712_domain, SolStruct};
use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::error::AppError;
use crate::time::now_secs;

type HmacSha256 = Hmac<Sha256>;

pub fn build_l2_signature(
    secret_b64: &str,
    timestamp: &str,
    method: &str,
    request_path: &str,
    body: &str,
) -> Result<String, AppError> {
    let secret = URL_SAFE
        .decode(secret_b64)
        .map_err(|e| AppError::Auth(format!("secret base64 decode: {e}")))?;

    let message = format!(
        "{}{}{}{}",
        timestamp,
        method.to_uppercase(),
        request_path,
        body,
    );

    let mut mac = HmacSha256::new_from_slice(&secret)
        .map_err(|e| AppError::Auth(format!("hmac init: {e}")))?;
    mac.update(message.as_bytes());
    let tag = mac.finalize().into_bytes();

    Ok(URL_SAFE.encode(tag))
}

pub fn body_to_string(value: &serde_json::Value) -> String {
    value.to_string()
}

pub struct L2Headers {
    address: String,
    api_key: String,
    passphrase: String,
    timestamp: String,
    signature: String,
}

impl L2Headers {
    pub fn apply(self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header("POLY_ADDRESS", self.address)
            .header("POLY_API_KEY", self.api_key)
            .header("POLY_PASSPHRASE", self.passphrase)
            .header("POLY_TIMESTAMP", self.timestamp)
            .header("POLY_SIGNATURE", self.signature)
    }
}

pub fn make_l2_headers(
    creds: &crate::config::Credentials,
    timestamp: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<L2Headers, AppError> {
    let signature = build_l2_signature(&creds.poly_secret, timestamp, method, path, body)?;
    Ok(L2Headers {
        address: creds.poly_address.clone(),
        api_key: creds.poly_api_key.clone(),
        passphrase: creds.poly_passphrase.clone(),
        timestamp: timestamp.to_string(),
        signature,
    })
}

alloy::sol! {
    struct ClobAuth {
        address address;
        string timestamp;
        uint256 nonce;
        string message;
    }
}

const CLOB_AUTH_MESSAGE: &str = "This message attests that I control the given wallet";
const POLYGON_CHAIN_ID: u64 = 137;

#[derive(Deserialize)]
struct DerivedApiKey {
    #[serde(rename = "apiKey")]
    api_key: String,
    secret: String,
    passphrase: String,
}

async fn sign_clob_auth(
    private_key_hex: &str,
    timestamp: &str,
    nonce: u64,
) -> Result<(String, String), AppError> {
    let signer: PrivateKeySigner = private_key_hex
        .trim_start_matches("0x")
        .parse()
        .map_err(|e| AppError::Auth(format!("private key parse: {e}")))?;

    let address = signer.address();
    let typed = ClobAuth {
        address,
        timestamp: timestamp.to_string(),
        nonce: U256::from(nonce),
        message: CLOB_AUTH_MESSAGE.to_string(),
    };
    let domain = eip712_domain! {
        name: "ClobAuthDomain",
        version: "1",
        chain_id: POLYGON_CHAIN_ID,
    };

    let hash = typed.eip712_signing_hash(&domain);
    let sig = signer
        .sign_hash(&hash)
        .await
        .map_err(|e| AppError::Auth(format!("clob auth sign: {e}")))?;

    Ok((
        format!("{address:#x}"),
        format!("0x{}", hex::encode(sig.as_bytes())),
    ))
}

pub struct DeriveResult {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
    pub signer_address: String,
}

pub async fn derive_api_key(
    http: &reqwest::Client,
    clob_base_url: &str,
    private_key_hex: &str,
    nonce: u64,
) -> Result<DeriveResult, AppError> {
    let timestamp = now_secs().to_string();
    let (signer_address, signature) =
        sign_clob_auth(private_key_hex, &timestamp, nonce).await?;

    let url = format!("{}/auth/derive-api-key", clob_base_url.trim_end_matches('/'));
    let resp = http
        .get(&url)
        .header("POLY_ADDRESS", &signer_address)
        .header("POLY_SIGNATURE", &signature)
        .header("POLY_TIMESTAMP", &timestamp)
        .header("POLY_NONCE", nonce.to_string())
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Clob(format!("derive-api-key {status}: {body}")));
    }

    let parsed: DerivedApiKey = resp.json().await?;
    Ok(DeriveResult {
        api_key: parsed.api_key,
        secret: parsed.secret,
        passphrase: parsed.passphrase,
        signer_address,
    })
}
