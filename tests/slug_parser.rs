//! `parse_slug` kenar durum entegrasyon testleri (iskelet §1 destek tablosu).

use baiter_pro::slug::{parse_slug, Asset, Interval};

#[test]
fn btc_5m_valid() {
    // 5m = 300 sn, 1776420900 / 300 = 5921403 (tam bölünür)
    let info = parse_slug("btc-updown-5m-1776420900").unwrap();
    assert_eq!(info.asset, Asset::Btc);
    assert_eq!(info.interval, Interval::M5);
    assert_eq!(info.ts, 1776420900);
    assert_eq!(info.end_ts(), 1776420900 + 300);
}

#[test]
fn eth_15m_valid() {
    let info = parse_slug("eth-updown-15m-1776420000").unwrap();
    assert_eq!(info.asset, Asset::Eth);
    assert_eq!(info.interval, Interval::M15);
}

#[test]
fn sol_1h_valid() {
    // 1h = 3600 sn, 1776420000 / 3600 = 493450 (tam)
    let info = parse_slug("sol-updown-1h-1776420000").unwrap();
    assert_eq!(info.asset, Asset::Sol);
    assert_eq!(info.interval, Interval::H1);
}

#[test]
fn xrp_4h_valid() {
    // 4h = 14400 sn, 1776412800 / 14400 = 123362 (tam)
    let info = parse_slug("xrp-updown-4h-1776412800").unwrap();
    assert_eq!(info.asset, Asset::Xrp);
    assert_eq!(info.interval, Interval::H4);
}

#[test]
fn unsupported_asset() {
    let err = parse_slug("doge-updown-5m-1776420900").unwrap_err();
    assert!(format!("{err}").contains("desteklenmeyen asset"));
}

#[test]
fn unsupported_interval() {
    let err = parse_slug("btc-updown-30m-1776420900").unwrap_err();
    assert!(format!("{err}").contains("desteklenmeyen interval"));
}

#[test]
fn wrong_middle_literal() {
    let err = parse_slug("btc-binary-5m-1776420900").unwrap_err();
    assert!(format!("{err}").contains("updown"));
}

#[test]
fn too_few_parts() {
    assert!(parse_slug("btc-updown-5m").is_err());
    assert!(parse_slug("").is_err());
}

#[test]
fn too_many_parts() {
    assert!(parse_slug("btc-updown-5m-1776420900-extra").is_err());
}

#[test]
fn non_numeric_timestamp() {
    let err = parse_slug("btc-updown-5m-abcdefg").unwrap_err();
    assert!(format!("{err}").contains("timestamp"));
}

#[test]
fn zero_timestamp() {
    let err = parse_slug("btc-updown-5m-0").unwrap_err();
    assert!(format!("{err}").contains("timestamp 0"));
}

#[test]
fn timestamp_not_multiple_of_interval() {
    let err = parse_slug("btc-updown-5m-1776420901").unwrap_err();
    assert!(format!("{err}").contains("katı değil"));
}

#[test]
fn slug_roundtrip() {
    let original = "btc-updown-5m-1776420900";
    let info = parse_slug(original).unwrap();
    assert_eq!(info.to_slug(), original);
}
