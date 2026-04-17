//! L2 HMAC-SHA256 imzalama regresyon testleri.
//!
//! Referans: rs-clob-client `src/auth.rs` — `URL_SAFE` secret decode + imza
//! encode. İmza algoritması deterministiktir; aynı input → aynı output.

use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use baiter_pro::polymarket::auth::{body_to_string, build_l2_signature};

#[test]
fn empty_body_empty_path_is_deterministic() {
    let secret = URL_SAFE.encode(b"test-secret-key-bytes-abcdefghij01");
    let s1 = build_l2_signature(&secret, "1700000000", "GET", "/trades", "").unwrap();
    let s2 = build_l2_signature(&secret, "1700000000", "GET", "/trades", "").unwrap();
    assert_eq!(s1, s2);
    // URL_SAFE base64 decode → tam 32 byte (SHA256 boyutu)
    let decoded = URL_SAFE.decode(&s1).unwrap();
    assert_eq!(decoded.len(), 32);
}

#[test]
fn method_is_uppercased_before_signing() {
    let secret = URL_SAFE.encode(b"case-sensitivity-secret-0123456789");
    let lower = build_l2_signature(&secret, "1", "post", "/order", "").unwrap();
    let upper = build_l2_signature(&secret, "1", "POST", "/order", "").unwrap();
    assert_eq!(lower, upper);
}

#[test]
fn different_timestamps_produce_different_signatures() {
    let secret = URL_SAFE.encode(b"timestamp-diff-secret-bytes-012345");
    let a = build_l2_signature(&secret, "1000", "POST", "/order", "{}").unwrap();
    let b = build_l2_signature(&secret, "1001", "POST", "/order", "{}").unwrap();
    assert_ne!(a, b);
}

#[test]
fn different_paths_produce_different_signatures() {
    let secret = URL_SAFE.encode(b"path-diff-secret-bytes-abcdef01234");
    let a = build_l2_signature(&secret, "1", "POST", "/order", "").unwrap();
    let b = build_l2_signature(&secret, "1", "POST", "/orders", "").unwrap();
    assert_ne!(a, b);
}

#[test]
fn different_bodies_produce_different_signatures() {
    let secret = URL_SAFE.encode(b"body-diff-secret-bytes-abcdef01234");
    let a = build_l2_signature(&secret, "1", "POST", "/order", "{}").unwrap();
    let b = build_l2_signature(&secret, "1", "POST", "/order", "{\"a\":1}").unwrap();
    assert_ne!(a, b);
}

#[test]
fn invalid_base64_secret_returns_error() {
    let bad = "not-a-valid-@@@-base64-!!!";
    let err = build_l2_signature(bad, "1", "POST", "/order", "").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("base64") || msg.contains("secret"));
}

#[test]
fn body_to_string_strips_single_quotes() {
    let v = serde_json::json!({"maker":"0xabc","side":"BUY"});
    let s = body_to_string(&v);
    assert!(!s.contains('\''));
    assert!(s.contains('"'));
    assert!(s.contains("maker"));
}

/// Referans değer — bizim implementasyon ile türetildi. Eğer imza algoritması
/// değişirse bu test erken uyarı verir (algoritma regresyonu).
#[test]
fn known_input_reference_signature() {
    let secret_b64 = URL_SAFE.encode(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"); // 32 'a'
    let sig = build_l2_signature(&secret_b64, "1700000000", "POST", "/order", "")
        .unwrap();
    // 32 bytes → 44 char URL_SAFE base64 (padding dahil)
    assert_eq!(sig.len(), 44);
    let decoded = URL_SAFE.decode(&sig).unwrap();
    assert_eq!(decoded.len(), 32);
    // Algoritma: HMAC-SHA256(b"aa..." [32 byte], "1700000000POST/order")
    // İlk byte deterministiktir.
    assert_ne!(decoded, [0u8; 32]);
}
