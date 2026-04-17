//! Polymarket slug parser.
//!
//! Desteklenen kalıp: `{asset}-updown-{interval}-{unix_timestamp_saniye}`.
//! Eşleşmeyen slug → bot başlatma reddi (ürün hatası).
//!
//! Referans: [docs/bot-platform-mimari.md §1](../../../docs/bot-platform-mimari.md).

use crate::error::AppError;

/// Desteklenen crypto asset'ler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Asset {
    Btc,
    Eth,
    Sol,
    Xrp,
}

impl Asset {
    pub const ALL: [Asset; 4] = [Asset::Btc, Asset::Eth, Asset::Sol, Asset::Xrp];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Btc => "btc",
            Self::Eth => "eth",
            Self::Sol => "sol",
            Self::Xrp => "xrp",
        }
    }

    /// Binance USD-M Futures sembol eşlemesi (`binance_signal` için).
    pub fn binance_symbol(self) -> &'static str {
        match self {
            Self::Btc => "btcusdt",
            Self::Eth => "ethusdt",
            Self::Sol => "solusdt",
            Self::Xrp => "xrpusdt",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "btc" => Some(Self::Btc),
            "eth" => Some(Self::Eth),
            "sol" => Some(Self::Sol),
            "xrp" => Some(Self::Xrp),
            _ => None,
        }
    }
}

/// Desteklenen pencere aralıkları.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Interval {
    M5,
    M15,
    H1,
    H4,
}

impl Interval {
    pub const ALL: [Interval; 4] = [Interval::M5, Interval::M15, Interval::H1, Interval::H4];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::H1 => "1h",
            Self::H4 => "4h",
        }
    }

    /// Pencere uzunluğu saniye cinsinden.
    pub fn seconds(self) -> u64 {
        match self {
            Self::M5 => 5 * 60,
            Self::M15 => 15 * 60,
            Self::H1 => 60 * 60,
            Self::H4 => 4 * 60 * 60,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "5m" => Some(Self::M5),
            "15m" => Some(Self::M15),
            "1h" => Some(Self::H1),
            "4h" => Some(Self::H4),
            _ => None,
        }
    }
}

/// Parse edilmiş slug bilgisi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlugInfo {
    pub asset: Asset,
    pub interval: Interval,
    /// Pencere başlangıcı, unix saniye.
    pub ts: u64,
}

impl SlugInfo {
    /// Pencere bitişi, unix saniye.
    pub fn end_ts(&self) -> u64 {
        self.ts + self.interval.seconds()
    }

    /// Slug string'e geri serialize.
    pub fn to_slug(&self) -> String {
        format!(
            "{}-updown-{}-{}",
            self.asset.as_str(),
            self.interval.as_str(),
            self.ts,
        )
    }

    /// Aktif market filtresi için slug öneki (ör. "btc-updown-5m-").
    pub fn prefix(asset: Asset, interval: Interval) -> String {
        format!("{}-updown-{}-", asset.as_str(), interval.as_str())
    }
}

/// `{asset}-updown-{interval}-{ts}` formatını parse eder.
///
/// Tam 4 parça beklenir; 2. parça literal `"updown"` olmak zorundadır.
pub fn parse_slug(slug: &str) -> Result<SlugInfo, AppError> {
    let parts: Vec<&str> = slug.split('-').collect();
    if parts.len() != 4 {
        return Err(AppError::InvalidSlug {
            slug: slug.to_string(),
            reason: format!("beklenen 4 parça, {} bulundu", parts.len()),
        });
    }
    if parts[1] != "updown" {
        return Err(AppError::InvalidSlug {
            slug: slug.to_string(),
            reason: format!("2. parça 'updown' olmalı, '{}' bulundu", parts[1]),
        });
    }
    let asset = Asset::parse(parts[0]).ok_or_else(|| AppError::InvalidSlug {
        slug: slug.to_string(),
        reason: format!("desteklenmeyen asset '{}' (btc/eth/sol/xrp)", parts[0]),
    })?;
    let interval = Interval::parse(parts[2]).ok_or_else(|| AppError::InvalidSlug {
        slug: slug.to_string(),
        reason: format!(
            "desteklenmeyen interval '{}' (5m/15m/1h/4h)",
            parts[2]
        ),
    })?;
    let ts: u64 = parts[3].parse().map_err(|_| AppError::InvalidSlug {
        slug: slug.to_string(),
        reason: format!("timestamp parse hatası: '{}'", parts[3]),
    })?;
    if ts == 0 {
        return Err(AppError::InvalidSlug {
            slug: slug.to_string(),
            reason: "timestamp 0 olamaz".to_string(),
        });
    }
    if ts % interval.seconds() != 0 {
        return Err(AppError::InvalidSlug {
            slug: slug.to_string(),
            reason: format!(
                "timestamp {} interval ({}s) katı değil",
                ts,
                interval.seconds()
            ),
        });
    }
    Ok(SlugInfo {
        asset,
        interval,
        ts,
    })
}
