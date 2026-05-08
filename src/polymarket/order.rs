use std::str::FromStr;

use alloy::primitives::{address, Address, FixedBytes, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::{eip712_domain, Eip712Domain, SolStruct};
use rand::RngExt;
use serde_json::{json, Value};

use crate::config::Credentials;
use crate::error::AppError;
use crate::time::{now_ms, now_secs};
use crate::types::Side;

const TOKEN_DECIMALS: u32 = 6;

/// CLOB V2 CTF Exchange (binary markets).
const CTF_EXCHANGE: Address = address!("0xE111180000d2663C0091e4f400237545B87B996B");

/// CLOB V2 NegRisk CTF Exchange (multi-outcome markets).
const NEG_RISK_CTF_EXCHANGE: Address = address!("0xe2222d279d744050d28e00520010520000310F59");

alloy::sol! {
    struct Order {
        uint256 salt;
        address maker;
        address signer;
        uint256 tokenId;
        uint256 makerAmount;
        uint256 takerAmount;
        uint8 side;
        uint8 signatureType;
        uint256 timestamp;
        bytes32 metadata;
        bytes32 builder;
    }
}

/// Statik metadata = `bytes32(0)` hex prefiks'li yazımı; her order'da alloc'sız kullanılır.
const METADATA_HEX: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";

/// Boot'ta bir kez kurulan imza materyali; her order için signer parse + domain inşası eler.
pub struct SignerCache {
    pub signer: PrivateKeySigner,
    pub signer_addr: Address,
    /// `signature_type=0`'da `signer_addr`; `1|2`'de `funder`.
    pub maker_addr: Address,
    pub signature_type: u8,
    pub ctf_domain: Eip712Domain,
    pub neg_risk_domain: Eip712Domain,
    pub builder_bytes: FixedBytes<32>,
    /// `0x...` lower-case hex; her order JSON'unda allocation kaybeder.
    pub maker_addr_hex: String,
    pub signer_addr_hex: String,
    pub builder_hex: String,
}

impl SignerCache {
    /// `Credentials` + `chain_id`'den boot anında bir kez kur.
    pub fn from_creds(creds: &Credentials, chain_id: u64) -> Result<Self, AppError> {
        let signer: PrivateKeySigner =
            creds
                .polygon_private_key
                .trim_start_matches("0x")
                .parse()
                .map_err(|e| AppError::Auth(format!("private key parse: {e}")))?;
        let signer_addr = signer.address();
        let maker_addr = match creds.signature_type {
            0 => signer_addr,
            1 | 2 => {
                let f = creds
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
        let ctf_domain = eip712_domain! {
            name: "Polymarket CTF Exchange",
            version: "2",
            chain_id: chain_id,
            verifying_contract: CTF_EXCHANGE,
        };
        let neg_risk_domain = eip712_domain! {
            name: "Polymarket CTF Exchange",
            version: "2",
            chain_id: chain_id,
            verifying_contract: NEG_RISK_CTF_EXCHANGE,
        };
        let builder_bytes = parse_bytes32(crate::config::BUILDER_CODE_HEX)?;
        let maker_addr_hex = format!("{maker_addr:#x}");
        let signer_addr_hex = format!("{signer_addr:#x}");
        let builder_hex = format!("0x{}", hex::encode(builder_bytes));
        Ok(Self {
            signer,
            signer_addr,
            maker_addr,
            signature_type: creds.signature_type as u8,
            ctf_domain,
            neg_risk_domain,
            builder_bytes,
            maker_addr_hex,
            signer_addr_hex,
            builder_hex,
        })
    }

    fn domain(&self, neg_risk: bool) -> &Eip712Domain {
        if neg_risk {
            &self.neg_risk_domain
        } else {
            &self.ctf_domain
        }
    }
}

pub struct BuildArgs<'a> {
    pub cache: &'a SignerCache,
    pub token_id: &'a str,
    pub side: Side,
    pub size: f64,
    pub price: f64,
    pub tick_size: f64,
}

fn rounding_config(tick_size: f64) -> Result<(u32, u32, u32), AppError> {
    if (tick_size - 0.0001).abs() < 1e-9 {
        Ok((4, 2, 6))
    } else if (tick_size - 0.001).abs() < 1e-9 {
        Ok((3, 2, 5))
    } else if (tick_size - 0.01).abs() < 1e-9 {
        Ok((2, 2, 4))
    } else if (tick_size - 0.1).abs() < 1e-9 {
        Ok((1, 2, 3))
    } else {
        Err(AppError::Clob(format!(
            "unsupported tick_size {tick_size} (allowed: 0.0001, 0.001, 0.01, 0.1)"
        )))
    }
}

pub fn build_order(args: &BuildArgs<'_>) -> Result<Order, AppError> {
    let (price_dec, size_dec, amount_dec) = rounding_config(args.tick_size)?;
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

    let token_id = U256::from_str_radix(args.token_id, 10)
        .map_err(|e| AppError::Auth(format!("token_id parse: {e}")))?;

    Ok(Order {
        salt: order_salt(),
        maker: args.cache.maker_addr,
        signer: args.cache.signer_addr,
        tokenId: token_id,
        makerAmount: U256::from(maker_amount),
        takerAmount: U256::from(taker_amount),
        side: side_byte,
        signatureType: args.cache.signature_type,
        timestamp: U256::from(now_ms()),
        metadata: FixedBytes::<32>::ZERO,
        builder: args.cache.builder_bytes,
    })
}

pub async fn sign_order(
    order: &Order,
    cache: &SignerCache,
    neg_risk: bool,
) -> Result<String, AppError> {
    let hash = order.eip712_signing_hash(cache.domain(neg_risk));
    let sig = cache
        .signer
        .sign_hash(&hash)
        .await
        .map_err(|e| AppError::Auth(format!("sign: {e}")))?;
    Ok(format!("0x{}", hex::encode(sig.as_bytes())))
}

/// V2 wire body: 11 imzalı alan + `expiration` (GTD için unix-secs, aksi `0`) + `signature`.
///
/// `cache` üzerinden `maker`/`signer`/`builder` hex'leri ve sabit `metadata`
/// alanı doğrudan referansla kullanılır; her order'da yeniden allocate
/// edilmez.
pub fn order_to_json(
    cache: &SignerCache,
    order: &Order,
    expiration_secs: u64,
    signature_hex: &str,
) -> Value {
    let side_str = if order.side == 0 { "BUY" } else { "SELL" };
    let salt: u64 = order
        .salt
        .try_into()
        .expect("salt fits in u64 by construction (order_salt)");
    json!({
        "salt": salt,
        "maker": &cache.maker_addr_hex,
        "signer": &cache.signer_addr_hex,
        "tokenId": order.tokenId.to_string(),
        "makerAmount": order.makerAmount.to_string(),
        "takerAmount": order.takerAmount.to_string(),
        "side": side_str,
        "signatureType": order.signatureType,
        "expiration": expiration_secs.to_string(),
        "timestamp": order.timestamp.to_string(),
        "metadata": METADATA_HEX,
        "builder":  &cache.builder_hex,
        "signature": signature_hex,
    })
}

/// V2 GTD: protocol +60s buffer; effective N için `expiration = now + 60 + N`.
pub fn expiration_for(order_type: &str, timeout_secs: u64) -> u64 {
    if order_type.eq_ignore_ascii_case("GTD") {
        now_secs() + 60 + timeout_secs
    } else {
        0
    }
}

fn order_salt() -> U256 {
    let now = now_ms();
    let r: u64 = rand::rng().random_range(0..now.max(1));
    U256::from(r)
}

/// `0x` + tam 64 hex char → `FixedBytes<32>`. Geçersiz → `AppError::Auth`.
fn parse_bytes32(hex_str: &str) -> Result<FixedBytes<32>, AppError> {
    let stripped = hex_str
        .strip_prefix("0x")
        .ok_or_else(|| AppError::Auth(format!("bytes32 must start with 0x: '{hex_str}'")))?;
    if stripped.len() != 64 {
        return Err(AppError::Auth(format!(
            "bytes32 must be 64 hex chars (got {})",
            stripped.len()
        )));
    }
    let bytes =
        hex::decode(stripped).map_err(|e| AppError::Auth(format!("bytes32 hex decode: {e}")))?;
    Ok(FixedBytes::<32>::from_slice(&bytes))
}
