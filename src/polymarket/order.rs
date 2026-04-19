//! Polymarket CTF Exchange (ve NegRisk Adapter) için EIP-712 emir imzalama.
//!
//! Doc §13 + [docs/api/polymarket-clob.md §POST /order](../../../docs/api/polymarket-clob.md):
//! - **verifying_contract** seçimi `neg_risk` flag'ine göre değişir.
//! - USDC + Conditional Token miktarları **6 ondalık** sabit nokta sayıdır.
//! - **signature_type** 0/1/2: EOA / POLY_PROXY / POLY_GNOSIS_SAFE.
//!   - 0: `maker == signer == EOA`.
//!   - 1/2: `signer = EOA`, `maker = funder` (proxy / safe).
//!
//! `taker` her zaman `0x0` (public order book).

use std::str::FromStr;

use alloy::primitives::{Address, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::{eip712_domain, SolStruct};
use rand::Rng;
use serde_json::{json, Value};

use crate::config::Credentials;
use crate::error::AppError;
use crate::time::now_secs;
use crate::types::Side;

/// USDC + CTF token decimal'ı (her ikisi de 6).
const TOKEN_DECIMALS: u32 = 6;

/// Polymarket CTF Exchange (standart) verifying_contract — Polygon mainnet.
const CTF_EXCHANGE: &str = "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E";

/// NegRisk CTF Exchange (negRisk markets) verifying_contract — Polygon mainnet.
const NEG_RISK_CTF_EXCHANGE: &str = "0xC5d563A36AE78145C45a50134d48A1215220f80a";

alloy::sol! {
    /// Polymarket CTF Exchange `Order` (EIP-712 typed data).
    struct Order {
        uint256 salt;
        address maker;
        address signer;
        address taker;
        uint256 tokenId;
        uint256 makerAmount;
        uint256 takerAmount;
        uint256 expiration;
        uint256 nonce;
        uint256 feeRateBps;
        uint8 side;
        uint8 signatureType;
    }
}

/// Plan'ın isteğine göre [`Order`] inşa eden parametre paketi.
pub struct BuildArgs<'a> {
    pub creds: &'a Credentials,
    pub token_id: &'a str,
    pub side: Side,
    /// Outcome token miktarı (insan formatı, ör. 5.0 = 5 share).
    pub size: f64,
    /// Birim fiyat 0..1.
    pub price: f64,
    /// 0 = GTC (süresiz). >0 = GTD unix-saniye.
    pub expiration_secs: u64,
    pub neg_risk: bool,
}

/// `neg_risk`'e göre verifying_contract adresi.
pub fn verifying_contract(neg_risk: bool) -> &'static str {
    if neg_risk {
        NEG_RISK_CTF_EXCHANGE
    } else {
        CTF_EXCHANGE
    }
}

/// Plan'ı imzalanabilir [`Order`] yapısına çevirir (6-ondalık math + adres
/// türetimi). Buradan dönen struct, `sign_order` ile imzalanır ve
/// `order_to_json` ile CLOB body'sine serialize edilir.
pub fn build_order(args: &BuildArgs<'_>) -> Result<Order, AppError> {
    // 6-ondalık fixed-point: f64 → u256 (taban USDC/CTF kuralları).
    let scale: f64 = 10f64.powi(TOKEN_DECIMALS as i32);
    let size_units = (args.size * scale).round() as u128;
    let usdc_units = (args.size * args.price * scale).round() as u128;

    let (maker_amount, taker_amount, side_byte) = match args.side {
        // BUY: makerAmount = USDC, takerAmount = CTF.
        Side::Buy => (usdc_units, size_units, 0u8),
        // SELL: makerAmount = CTF, takerAmount = USDC.
        Side::Sell => (size_units, usdc_units, 1u8),
    };

    let signer_addr = signer_address(&args.creds.polygon_private_key)?;
    let maker_addr = match args.creds.signature_type {
        0 => signer_addr,
        1 | 2 => {
            let f = args
                .creds
                .funder
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::Auth("funder zorunlu (sig_type 1|2)".into()))?;
            Address::from_str(f).map_err(|e| AppError::Auth(format!("funder parse: {e}")))?
        }
        other => {
            return Err(AppError::Auth(format!(
                "signature_type {other} desteklenmiyor (0|1|2)"
            )))
        }
    };

    let token_id = U256::from_str_radix(args.token_id, 10)
        .map_err(|e| AppError::Auth(format!("token_id parse: {e}")))?;

    Ok(Order {
        salt: random_salt(),
        maker: maker_addr,
        signer: signer_addr,
        taker: Address::ZERO,
        tokenId: token_id,
        makerAmount: U256::from(maker_amount),
        takerAmount: U256::from(taker_amount),
        expiration: U256::from(args.expiration_secs),
        nonce: U256::ZERO,
        feeRateBps: U256::ZERO,
        side: side_byte,
        signatureType: args.creds.signature_type as u8,
    })
}

/// EIP-712 hash üzerinde ECDSA imzası — `0x...` hex döner.
pub async fn sign_order(
    order: &Order,
    creds: &Credentials,
    chain_id: u64,
    neg_risk: bool,
) -> Result<String, AppError> {
    let signer: PrivateKeySigner = creds
        .polygon_private_key
        .trim_start_matches("0x")
        .parse()
        .map_err(|e| AppError::Auth(format!("private key parse: {e}")))?;

    let verifying = Address::from_str(verifying_contract(neg_risk))
        .map_err(|e| AppError::Auth(format!("verifying_contract parse: {e}")))?;

    let domain = eip712_domain! {
        name: "Polymarket CTF Exchange",
        version: "1",
        chain_id: chain_id,
        verifying_contract: verifying,
    };

    let hash = order.eip712_signing_hash(&domain);
    let sig = signer
        .sign_hash(&hash)
        .await
        .map_err(|e| AppError::Auth(format!("sign: {e}")))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

/// CLOB `POST /order` body'sinin `order` alanı için JSON serileştirme.
pub fn order_to_json(order: &Order, signature_hex: &str) -> Value {
    let side_str = if order.side == 0 { "BUY" } else { "SELL" };
    json!({
        "salt": order.salt.to_string(),
        "maker": format!("{:?}", order.maker),
        "signer": format!("{:?}", order.signer),
        "taker": format!("{:?}", order.taker),
        "tokenId": order.tokenId.to_string(),
        "makerAmount": order.makerAmount.to_string(),
        "takerAmount": order.takerAmount.to_string(),
        "expiration": order.expiration.to_string(),
        "nonce": order.nonce.to_string(),
        "feeRateBps": order.feeRateBps.to_string(),
        "side": side_str,
        "signatureType": order.signatureType,
        "signature": signature_hex,
    })
}

/// `expiration` hesabı: `GTC` ise 0, `GTD` ise `now + timeout_secs`.
pub fn expiration_for(order_type: &str, timeout_secs: u64) -> u64 {
    if order_type.eq_ignore_ascii_case("GTD") {
        now_secs() + timeout_secs
    } else {
        0
    }
}

fn signer_address(private_key_hex: &str) -> Result<Address, AppError> {
    let signer: PrivateKeySigner = private_key_hex
        .trim_start_matches("0x")
        .parse()
        .map_err(|e| AppError::Auth(format!("private key parse: {e}")))?;
    Ok(signer.address())
}

fn random_salt() -> U256 {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    U256::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_creds() -> Credentials {
        // Test-only deterministic key (anvil default 0).
        Credentials {
            poly_address: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".into(),
            poly_api_key: "k".into(),
            poly_passphrase: "p".into(),
            poly_secret: "cw==".into(),
            polygon_private_key:
                "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .into(),
            signature_type: 0,
            funder: None,
        }
    }

    #[test]
    fn verifying_contract_swaps_on_neg_risk() {
        assert_ne!(verifying_contract(true), verifying_contract(false));
    }

    #[test]
    fn build_order_buy_uses_usdc_for_maker_amount() {
        let creds = fake_creds();
        let order = build_order(&BuildArgs {
            creds: &creds,
            token_id: "1",
            side: Side::Buy,
            size: 10.0,
            price: 0.50,
            expiration_secs: 0,
            neg_risk: false,
        })
        .unwrap();
        // 10 * 0.50 = 5 USDC = 5_000_000 (6 decimals).
        assert_eq!(order.makerAmount, U256::from(5_000_000u64));
        assert_eq!(order.takerAmount, U256::from(10_000_000u64));
        assert_eq!(order.side, 0);
    }

    #[test]
    fn build_order_sell_swaps_amounts() {
        let creds = fake_creds();
        let order = build_order(&BuildArgs {
            creds: &creds,
            token_id: "1",
            side: Side::Sell,
            size: 10.0,
            price: 0.50,
            expiration_secs: 0,
            neg_risk: false,
        })
        .unwrap();
        assert_eq!(order.makerAmount, U256::from(10_000_000u64));
        assert_eq!(order.takerAmount, U256::from(5_000_000u64));
        assert_eq!(order.side, 1);
    }

    #[tokio::test]
    async fn sign_order_returns_hex_signature() {
        let creds = fake_creds();
        let order = build_order(&BuildArgs {
            creds: &creds,
            token_id: "1",
            side: Side::Buy,
            size: 10.0,
            price: 0.50,
            expiration_secs: 0,
            neg_risk: false,
        })
        .unwrap();
        let sig = sign_order(&order, &creds, 137, false).await.unwrap();
        assert!(sig.starts_with("0x"));
        assert_eq!(sig.len(), 2 + 65 * 2); // 0x + 65 byte hex
    }

    #[test]
    fn expiration_gtc_is_zero() {
        assert_eq!(expiration_for("GTC", 5_000), 0);
    }

    #[test]
    fn expiration_gtd_is_now_plus_timeout() {
        let exp = expiration_for("GTD", 60);
        assert!(exp >= now_secs());
    }
}
