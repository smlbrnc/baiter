use std::str::FromStr;

use alloy::primitives::{address, Address, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::{eip712_domain, SolStruct};
use rand::RngExt;
use serde_json::{json, Value};

use crate::config::Credentials;
use crate::error::AppError;
use crate::time::{now_ms, now_secs};
use crate::types::Side;

const TOKEN_DECIMALS: u32 = 6;

const CTF_EXCHANGE: Address = address!("0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E");

const NEG_RISK_CTF_EXCHANGE: Address = address!("0xC5d563A36AE78145C45a50134d48A1215220f80a");

alloy::sol! {
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

pub struct BuildArgs<'a> {
    pub creds: &'a Credentials,
    pub token_id: &'a str,
    pub side: Side,
    pub size: f64,
    pub price: f64,
    pub expiration_secs: u64,
    pub neg_risk: bool,
    pub fee_rate_bps: u32,
    pub tick_size: f64,
}

fn rounding_config(tick_size: f64) -> (u32, u32, u32) {
    if (tick_size - 0.0001).abs() < 1e-9 {
        (4, 2, 6)
    } else if (tick_size - 0.001).abs() < 1e-9 {
        (3, 2, 5)
    } else if (tick_size - 0.1).abs() < 1e-9 {
        (1, 2, 3)
    } else {
        (2, 2, 4)
    }
}

fn verifying_contract(neg_risk: bool) -> Address {
    if neg_risk {
        NEG_RISK_CTF_EXCHANGE
    } else {
        CTF_EXCHANGE
    }
}

pub fn build_order(args: &BuildArgs<'_>) -> Result<Order, AppError> {
    let (price_dec, size_dec, amount_dec) = rounding_config(args.tick_size);
    let size_factor = 10u128.pow(size_dec);
    let price_factor = 10u128.pow(price_dec);
    let amount_to_token = 10u128.pow(TOKEN_DECIMALS - amount_dec);
    let token_per_size = 10u128.pow(TOKEN_DECIMALS - size_dec);

    let size_low = (args.size * size_factor as f64).floor() as u128;
    if size_low == 0 {
        return Err(AppError::Clob(format!(
            "size {} rounds to 0 at tick_size {} (size_decimals={size_dec})",
            args.size, args.tick_size
        )));
    }
    let price_ticks = (args.price * price_factor as f64).round() as u128;
    if price_ticks == 0 || price_ticks >= price_factor {
        return Err(AppError::Clob(format!(
            "price {} out of (0,1) range at tick_size {}",
            args.price, args.tick_size
        )));
    }
    let usdc_units = size_low * price_ticks * amount_to_token;
    let size_units = size_low * token_per_size;

    let (maker_amount, taker_amount, side_byte) = match args.side {
        Side::Buy => (usdc_units, size_units, 0u8),
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
        salt: order_salt(),
        maker: maker_addr,
        signer: signer_addr,
        taker: Address::ZERO,
        tokenId: token_id,
        makerAmount: U256::from(maker_amount),
        takerAmount: U256::from(taker_amount),
        expiration: U256::from(args.expiration_secs),
        nonce: U256::ZERO,
        feeRateBps: U256::from(args.fee_rate_bps),
        side: side_byte,
        signatureType: args.creds.signature_type as u8,
    })
}

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

    let domain = eip712_domain! {
        name: "Polymarket CTF Exchange",
        version: "1",
        chain_id: chain_id,
        verifying_contract: verifying_contract(neg_risk),
    };

    let hash = order.eip712_signing_hash(&domain);
    let sig = signer
        .sign_hash(&hash)
        .await
        .map_err(|e| AppError::Auth(format!("sign: {e}")))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

pub fn order_to_json(order: &Order, signature_hex: &str) -> Value {
    let side_str = if order.side == 0 { "BUY" } else { "SELL" };
    let salt: u64 = order
        .salt
        .try_into()
        .expect("salt fits in u64 by construction (order_salt)");
    json!({
        "salt": salt,
        "maker": format!("{:#x}", order.maker),
        "signer": format!("{:#x}", order.signer),
        "taker": format!("{:#x}", order.taker),
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

fn order_salt() -> U256 {
    let now = now_ms();
    let r: u64 = rand::rng().random_range(0..now.max(1));
    U256::from(r)
}
