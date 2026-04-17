//! Polymarket L1 (EIP-712 ClobAuth) + L2 (HMAC-SHA256) imzalama.
//!
//! L2 imzasında `secret` URL_SAFE base64 decode edilir ve imza URL_SAFE base64
//! olarak döner — STANDARD alfabe KULLANILMAZ (rs-clob-client uyumu).
//!
//! Referans: [docs/api/polymarket-clob.md §Authentication](../../../docs/api/polymarket-clob.md).

use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::error::AppError;

type HmacSha256 = Hmac<Sha256>;

/// L2 HMAC-SHA256 imzası.
///
/// Mesaj: `timestamp + METHOD + request_path + body`.
/// `body_json` `serde_json::Value` olarak geçilirse Python/Rust davranışı
/// (tek/çift tırnak) için `body_to_string` normalizasyonu yapılır.
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

/// rs-clob-client `body_to_string` eşdeğeri: JSON'da tek tırnak → çift tırnak
/// normalizasyonu (Python sunucu tarafıyla aynı string üretmek için).
pub fn body_to_string(value: &serde_json::Value) -> String {
    let raw = value.to_string();
    raw.replace('\'', "\"")
}

/// L1 header bundle (EIP-712 ClobAuth imzalanmış).
#[derive(Debug, Clone)]
pub struct L1Headers {
    pub address: String,
    pub signature: String,
    pub timestamp: String,
    pub nonce: String,
}

/// L2 header bundle.
#[derive(Debug, Clone)]
pub struct L2Headers {
    pub address: String,
    pub api_key: String,
    pub passphrase: String,
    pub timestamp: String,
    pub signature: String,
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
    let signature =
        build_l2_signature(&creds.poly_secret, timestamp, method, path, body)?;
    Ok(L2Headers {
        address: creds.poly_address.clone(),
        api_key: creds.poly_api_key.clone(),
        passphrase: creds.poly_passphrase.clone(),
        timestamp: timestamp.to_string(),
        signature,
    })
}

/// EIP-712 ClobAuth imzalı L1 header üret (yalnızca `/auth/*` endpoint'leri için).
///
/// Alloy 2.0 `signer-local` feature'i ile cüzdan yüklenir; `sign_typed_data`
/// ile imza alınır. Gerçek zincir domain'i: `chainId=137`, `name=ClobAuthDomain`.
pub async fn make_l1_headers(
    private_key_hex: &str,
    chain_id: u64,
    timestamp: &str,
    nonce: &str,
) -> Result<L1Headers, AppError> {
    use alloy::signers::local::PrivateKeySigner;
    use alloy::signers::Signer;
    use alloy::sol_types::{eip712_domain, SolStruct};

    let signer: PrivateKeySigner = private_key_hex
        .trim_start_matches("0x")
        .parse()
        .map_err(|e| AppError::Auth(format!("private key parse: {e}")))?;
    let address = format!("{:?}", signer.address());

    // EIP-712 struct'ı
    alloy::sol! {
        struct ClobAuth {
            address wallet;
            string timestamp;
            uint256 nonce;
            string message;
        }
    }

    let domain = eip712_domain! {
        name: "ClobAuthDomain",
        version: "1",
        chain_id: chain_id,
    };

    let nonce_u256 = alloy::primitives::U256::from_str_radix(nonce, 10)
        .map_err(|e| AppError::Auth(format!("nonce parse: {e}")))?;

    let payload = ClobAuth {
        wallet: signer.address(),
        timestamp: timestamp.to_string(),
        nonce: nonce_u256,
        message: "This message attests that I control the given wallet".to_string(),
    };

    let hash = payload.eip712_signing_hash(&domain);
    let sig = signer
        .sign_hash(&hash)
        .await
        .map_err(|e| AppError::Auth(format!("sign: {e}")))?;

    Ok(L1Headers {
        address,
        signature: format!("0x{}", hex::encode(sig.as_bytes())),
        timestamp: timestamp.to_string(),
        nonce: nonce.to_string(),
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
    fn body_to_string_normalizes_single_quotes() {
        let v = serde_json::json!({"foo": "bar"});
        let s = body_to_string(&v);
        assert!(s.contains('"'));
        assert!(!s.contains('\''));
    }
}
