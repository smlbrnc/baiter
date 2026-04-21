//! Polymarket CTF Exchange (ve NegRisk Adapter) için EIP-712 emir imzalama.
//!
//! Doc §13 + [docs/api/polymarket-clob.md §POST /order](../../../docs/api/polymarket-clob.md):
//! - **verifying_contract** seçimi `neg_risk` flag'ine göre değişir.
//! - USDC + Conditional Token miktarları **6 ondalık** sabit nokta sayıdır.
//! - **signature_type** 0/1/2: EOA / POLY_PROXY / POLY_GNOSIS_SAFE.
//!   - 0: `maker == signer == EOA`.
//!   - 1/2: `signer = EOA`, `maker = funder` (proxy / safe).
//! - **feeRateBps** marketten markete değişir; CLOB `GET /fee-rate` ile çekilir
//!   ve `BuildArgs.fee_rate_bps` üzerinden geçirilir. 0 göndermek market'in
//!   beklediği `mbf`'ten farklı olduğu sürece 400 döner.
//!
//! `taker` her zaman `0x0` (public order book).

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

/// USDC + CTF token decimal'ı (her ikisi de 6).
const TOKEN_DECIMALS: u32 = 6;

/// Polymarket CTF Exchange (standart) verifying_contract — Polygon mainnet.
const CTF_EXCHANGE: Address = address!("0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E");

/// NegRisk CTF Exchange (negRisk markets) verifying_contract — Polygon mainnet.
const NEG_RISK_CTF_EXCHANGE: Address = address!("0xC5d563A36AE78145C45a50134d48A1215220f80a");

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
    /// Maker fee rate (basis points). CLOB `GET /fee-rate?token_id=...`'den.
    /// 0 göndermek market'in `mbf`'ine eşit değilse server 400 döner.
    pub fee_rate_bps: u32,
    /// Market tick size (ör. 0.01) — `makerAmount`/`takerAmount` yuvarlaması
    /// (py-clob-client paritesi) için. Server'da derive edilen `price`'ın tick
    /// grid'ini bozmaması için size & USDC fixed-point yuvarlaması bu değere
    /// göre yapılır. Varsayılan tick: 0.01 (binary BTC/ETH markets).
    pub tick_size: f64,
}

/// py-clob-client `ROUNDING_CONFIG` paritesi: tick_size'a göre `(price_decimals,
/// size_decimals, amount_decimals)`. Default tick (0.01) için (2, 2, 4).
/// `size_dec + price_dec == amount_dec` invariant'i her config için sağlanır;
/// integer math `size_units * price_ticks * 10^(6 - amount_dec)` exact USDC
/// 1e6-base sonucu üretir → derived `price = maker/taker` tick grid'inde.
fn rounding_config(tick_size: f64) -> (u32, u32, u32) {
    if (tick_size - 0.0001).abs() < 1e-9 {
        (4, 2, 6)
    } else if (tick_size - 0.001).abs() < 1e-9 {
        (3, 2, 5)
    } else if (tick_size - 0.1).abs() < 1e-9 {
        (1, 2, 3)
    } else {
        // Default: 0.01 (binary markets).
        (2, 2, 4)
    }
}

/// `neg_risk`'e göre verifying_contract adresi.
fn verifying_contract(neg_risk: bool) -> Address {
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
    // Bot 1 / btc-updown-5m-1776791700 regresyonu: naive `(size*price*1e6).round()`
    // hedge re-place 30+ kez 400 alıyordu çünkü `usdc/size` server-derived price
    // tick 0.01'i bozuyordu (size 28.996743, price 0.60 → 0.6000000068973264).
    //
    // Çözüm: integer math (py-clob-client paritesi). Önce tick_size'a göre
    // decimals config çek; size DOWN-round, price tick'e snap; amount = size_2dec
    // × price_ticks (her ikisi de tamsayı) → 6-dec USDC base'e tek bir tamsayı
    // çarpımı. `size_dec + price_dec == amount_dec` invariant'i sayesinde
    // `usdc_units / size_units` derived price daima tick grid'inde.
    let (price_dec, size_dec, amount_dec) = rounding_config(args.tick_size);
    let size_factor = 10u128.pow(size_dec);
    let price_factor = 10u128.pow(price_dec);
    let amount_to_token = 10u128.pow(TOKEN_DECIMALS - amount_dec); // 6 - amount_dec ≥ 0
    let token_per_size = 10u128.pow(TOKEN_DECIMALS - size_dec);

    let size_low = (args.size * size_factor as f64).floor() as u128; // round_down
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
    // `usdc_amount_units` = size_2dec × price_ticks (10^amount_dec base).
    // 6-dec USDC base'e çevrim için × amount_to_token.
    let usdc_units = size_low * price_ticks * amount_to_token;
    let size_units = size_low * token_per_size;

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

/// CLOB `POST /order` body'sinin `order` alanı için JSON serileştirme.
///
/// Şema (resmi): [docs.polymarket.com/api-reference/trade/post-a-new-order](https://docs.polymarket.com/api-reference/trade/post-a-new-order).
/// - `salt` **integer** — küçük (int64'e sığar) — `order_salt()` üretir.
/// - Adresler **lowercase hex** (`{:#x}`) — py-clob-client paritesi.
/// - Tutarlar (`makerAmount`/`takerAmount`/`tokenId`/`expiration`/`nonce`/`feeRateBps`) string — uint256 fixed-math.
/// - `side` "BUY"/"SELL" string, `signatureType` int (0|1|2).
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

/// Polymarket SDK paritesi: küçük, ms-tabanlı, çağrı başına benzersiz salt.
///
/// - TS `clob-order-utils`: `Math.round(Math.random() * Date.now())` — int64'e sığar.
/// - python-order-utils: `round(time.time())`.
///
/// 256-bit random salt göndermek Polymarket Go server'ında int64 overflow yapar
/// (400 Bad Request). Burada `now_ms() ⊗ random()` formülüyle 64-bit'e sığan
/// yarı-rastgele bir değer üretiyoruz.
fn order_salt() -> U256 {
    let now = now_ms();
    let r: u64 = rand::rng().random_range(0..now.max(1));
    U256::from(r)
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

    fn args(creds: &Credentials, side: Side) -> BuildArgs<'_> {
        BuildArgs {
            creds,
            token_id: "1",
            side,
            size: 10.0,
            price: 0.50,
            expiration_secs: 0,
            neg_risk: false,
            fee_rate_bps: 30,
            tick_size: 0.01,
        }
    }

    #[test]
    fn verifying_contract_swaps_on_neg_risk() {
        assert_ne!(verifying_contract(true), verifying_contract(false));
    }

    #[test]
    fn build_order_buy_uses_usdc_for_maker_amount() {
        let creds = fake_creds();
        let order = build_order(&args(&creds, Side::Buy)).unwrap();
        // 10 * 0.50 = 5 USDC = 5_000_000 (6 decimals).
        assert_eq!(order.makerAmount, U256::from(5_000_000u64));
        assert_eq!(order.takerAmount, U256::from(10_000_000u64));
        assert_eq!(order.side, 0);
        assert_eq!(order.feeRateBps, U256::from(30u64));
    }

    #[test]
    fn build_order_sell_swaps_amounts() {
        let creds = fake_creds();
        let order = build_order(&args(&creds, Side::Sell)).unwrap();
        assert_eq!(order.makerAmount, U256::from(10_000_000u64));
        assert_eq!(order.takerAmount, U256::from(5_000_000u64));
        assert_eq!(order.side, 1);
    }

    #[tokio::test]
    async fn sign_order_returns_hex_signature() {
        let creds = fake_creds();
        let order = build_order(&args(&creds, Side::Buy)).unwrap();
        let sig = sign_order(&order, &creds, 137, false).await.unwrap();
        assert!(sig.starts_with("0x"));
        assert_eq!(sig.len(), 2 + 65 * 2); // 0x + 65 byte hex
    }

    /// Bot 1 / `btc-updown-5m-1776791700` regresyonu: hedge size=28.996743,
    /// price=0.60 → eski naive math `usdc_units / size_units = 0.60000000689…`
    /// → CLOB 400 "breaks minimum tick size 0.01". Yeni rounding (size→2dec,
    /// amount→4dec UP) tick grid'i bozmamalı.
    #[test]
    fn build_order_buy_amount_snaps_to_tick() {
        let creds = fake_creds();
        let mut a = args(&creds, Side::Buy);
        a.size = 28.996743;
        a.price = 0.60;
        a.tick_size = 0.01;
        let order = build_order(&a).unwrap();
        let maker = u128::from_le_bytes(order.makerAmount.to_le_bytes::<32>()[..16].try_into().unwrap());
        let taker = u128::from_le_bytes(order.takerAmount.to_le_bytes::<32>()[..16].try_into().unwrap());
        assert_eq!(taker, 28_990_000, "size 28.996743 → round_down 28.99 → 28_990_000");
        assert_eq!(maker, 17_394_000, "28.99 * 0.60 = 17.394 → 17_394_000");
        // Derived price (server-side validation): maker * 100 == taker * price_ticks(60).
        assert_eq!(maker * 100, taker * 60);
    }

    /// Tick 0.001 → size 2 dec, amount 5 dec. 33.33 * 0.123 = 4.09959.
    #[test]
    fn build_order_buy_supports_finer_tick() {
        let creds = fake_creds();
        let mut a = args(&creds, Side::Buy);
        a.size = 33.337;
        a.price = 0.123;
        a.tick_size = 0.001;
        let order = build_order(&a).unwrap();
        let maker = u128::from_le_bytes(order.makerAmount.to_le_bytes::<32>()[..16].try_into().unwrap());
        let taker = u128::from_le_bytes(order.takerAmount.to_le_bytes::<32>()[..16].try_into().unwrap());
        assert_eq!(taker, 33_330_000);
        // 33.33 * 0.123 = 4.09959 → 4_099_590 (5 dec exact).
        assert_eq!(maker, 4_099_590);
        // Server-side: maker * 1000 == taker * price_ticks(123).
        assert_eq!(maker * 1000, taker * 123);
    }

    /// SELL: maker (CTF) = size, taker (USDC) = round_down(size*price).
    #[test]
    fn build_order_sell_rounds_taker_amount_down() {
        let creds = fake_creds();
        let mut a = args(&creds, Side::Sell);
        a.size = 28.996743;
        a.price = 0.60;
        a.tick_size = 0.01;
        let order = build_order(&a).unwrap();
        let maker = u128::from_le_bytes(order.makerAmount.to_le_bytes::<32>()[..16].try_into().unwrap());
        let taker = u128::from_le_bytes(order.takerAmount.to_le_bytes::<32>()[..16].try_into().unwrap());
        assert_eq!(maker, 28_990_000);
        // Integer math: size_2dec(2899) * price_ticks(60) = 173940 → × 100 (tick→6dec).
        assert_eq!(taker, 17_394_000);
        // SELL derived price = taker / maker; tick rule (taker * 100 == maker * 60).
        assert_eq!(taker * 100, maker * 60);
    }

    /// `size` size_decimals altında ise `round_down` 0 üretir → açık hata.
    #[test]
    fn build_order_rejects_subtick_size() {
        let creds = fake_creds();
        let mut a = args(&creds, Side::Buy);
        a.size = 0.001; // round_down to 2 dec → 0.00
        let err = build_order(&a).err().expect("subtick size must be rejected");
        assert!(matches!(err, AppError::Clob(msg) if msg.contains("rounds to 0")));
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

    /// Polymarket OpenAPI: `salt` JSON **integer**, `side` "BUY"/"SELL" string,
    /// `signatureType` int. Diğer uint256 alanları string. 256-bit random salt
    /// göndermek Go server'ında int64 overflow yapar — bu testle korunur.
    /// `feeRateBps` "30" (string) — `BuildArgs.fee_rate_bps` doğru geçiyor mu?
    #[test]
    fn order_to_json_matches_openapi_shape() {
        let creds = fake_creds();
        let order = build_order(&args(&creds, Side::Buy)).unwrap();
        let body = order_to_json(&order, "0xdeadbeef");
        assert!(body["salt"].is_number(), "salt must be JSON number");
        assert!(body["salt"].as_u64().unwrap() <= i64::MAX as u64);
        assert_eq!(body["side"], "BUY");
        assert_eq!(body["signatureType"], 0);
        assert_eq!(body["feeRateBps"], "30");
        for k in [
            "tokenId",
            "makerAmount",
            "takerAmount",
            "expiration",
            "nonce",
            "feeRateBps",
        ] {
            assert!(body[k].is_string(), "{k} must be JSON string");
        }
        // Adresler lowercase hex (py-clob-client paritesi).
        let maker = body["maker"].as_str().unwrap();
        assert!(maker.starts_with("0x"));
        assert_eq!(maker, maker.to_lowercase());
        assert_eq!(body["signature"], "0xdeadbeef");
    }
}
