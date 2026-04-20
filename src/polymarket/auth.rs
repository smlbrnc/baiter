//! Polymarket L1 (EIP-712 ClobAuth) + L2 (HMAC-SHA256) imzalama.
//!
//! L2 secret URL_SAFE base64 ile decode/encode edilir (rs-clob-client uyumu).
//! Referans: [docs/api/polymarket-clob.md §Authentication](../../../docs/api/polymarket-clob.md).

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

/// L2 HMAC-SHA256 imzası — mesaj: `timestamp + METHOD + request_path + body`.
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

/// CLOB body'sini HMAC için kullanılacak forma çevirir.
///
/// `serde_json::Value::to_string()` zaten kompakt JSON üretir
/// (`{"a":1}`, boşluksuz, `ensure_ascii=False` davranışı). py-clob-client'in
/// `json.dumps(..., separators=(",", ":"), ensure_ascii=False)` ile birebir aynı
/// — başka dönüşüm gerekmez. **Önemli:** signed body == sent body olmalı,
/// çağıran iki yerde de aynı stringi kullansın.
pub fn body_to_string(value: &serde_json::Value) -> String {
    value.to_string()
}

/// L2 header bundle.
pub struct L2Headers {
    address: String,
    api_key: String,
    passphrase: String,
    timestamp: String,
    signature: String,
}

impl L2Headers {
    /// `reqwest::RequestBuilder`'a 5 zorunlu L2 header'ı ekle.
    pub fn apply(self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header("POLY_ADDRESS", self.address)
            .header("POLY_API_KEY", self.api_key)
            .header("POLY_PASSPHRASE", self.passphrase)
            .header("POLY_TIMESTAMP", self.timestamp)
            .header("POLY_SIGNATURE", self.signature)
    }
}

/// Verilen credentials + timestamp + method + path + body için L2 header üretir.
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

// =====================================================================
// L1 EIP-712 — `ClobAuth` ile API key türetme.
// =====================================================================
//
// `GET /auth/derive-api-key` aynı `(EOA_address, nonce)` çiftiyle
// önceden yaratılmış L2 credential'ı geri döner. POLY_ADDRESS her zaman
// EOA — signature_type ne olursa olsun (py-clob-client paritesi).

alloy::sol! {
    /// Polymarket ClobAuth EIP-712 typed data.
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

/// L1 EIP-712 ClobAuth imzala — `(signer_address, signature_hex)` döner.
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

    // py-clob-client paritesi: header lowercase hex (`0xabc...`).
    // alloy `{:?}` EIP-55 checksummed, server case-insensitive parse etse de
    // canonical lowercase'e standardize ediyoruz.
    Ok((
        format!("{address:#x}"),
        format!("0x{}", hex::encode(sig.as_bytes())),
    ))
}

/// `derive_api_key` sonucu. `signer_address` = EOA (sig_type fark etmez).
pub struct DeriveResult {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
    pub signer_address: String,
}

/// `GET /auth/derive-api-key`: aynı `(EOA, nonce)` ile mevcut L2 credential'ı getir.
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE;

    #[test]
    fn hmac_is_deterministic() {
        let secret_b64 = URL_SAFE.encode(b"my-test-secret-32-bytes-length!!");
        let a = build_l2_signature(&secret_b64, "1700000000", "POST", "/order", "{}").unwrap();
        let b = build_l2_signature(&secret_b64, "1700000000", "POST", "/order", "{}").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn hmac_output_length_is_32_bytes() {
        let secret_b64 = URL_SAFE.encode(b"example-secret-bytes-abcdefghij01");
        let sig = build_l2_signature(&secret_b64, "1700000000", "GET", "/trades", "").unwrap();
        let decoded = URL_SAFE.decode(&sig).unwrap();
        assert_eq!(decoded.len(), 32);
    }

    #[test]
    fn hmac_method_uppercase() {
        let secret_b64 = URL_SAFE.encode(b"another-test-secret-bytes-012345");
        let a = build_l2_signature(&secret_b64, "1", "post", "/x", "").unwrap();
        let b = build_l2_signature(&secret_b64, "1", "POST", "/x", "").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn hmac_different_inputs_produce_different_signatures() {
        let secret_b64 = URL_SAFE.encode(b"another-test-secret-bytes-987654");
        let a = build_l2_signature(&secret_b64, "1700000000", "POST", "/order", "").unwrap();
        let b = build_l2_signature(&secret_b64, "1700000001", "POST", "/order", "").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn body_to_string_is_compact_json() {
        let v = serde_json::json!({"a": 1, "b": "x"});
        assert_eq!(body_to_string(&v), r#"{"a":1,"b":"x"}"#);
    }

    #[tokio::test]
    async fn clob_auth_signature_is_hex_and_address_matches() {
        let pk = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let (addr, sig) = sign_clob_auth(pk, "1700000000", 0).await.unwrap();
        assert!(sig.starts_with("0x"));
        // Anvil[0] adresi — lowercase hex (py-clob-client paritesi).
        assert_eq!(addr, "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266");
    }
}
