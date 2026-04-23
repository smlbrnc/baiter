//! Polymarket slug parser: `{asset}-updown-{interval}-{unix_ts_sec}` (§1);
//! eşleşmezse `AppError::InvalidSlug`.

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Asset {
    Btc,
    Eth,
    Sol,
    Xrp,
}

impl Asset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Btc => "btc",
            Self::Eth => "eth",
            Self::Sol => "sol",
            Self::Xrp => "xrp",
        }
    }

    /// Binance USD-M Futures sembol eşlemesi.
    pub fn binance_symbol(self) -> &'static str {
        match self {
            Self::Btc => "btcusdt",
            Self::Eth => "ethusdt",
            Self::Sol => "solusdt",
            Self::Xrp => "xrpusdt",
        }
    }

    /// Polymarket RTDS `crypto_prices_chainlink` filter formatı.
    pub fn rtds_symbol(self) -> &'static str {
        match self {
            Self::Btc => "btc/usd",
            Self::Eth => "eth/usd",
            Self::Sol => "sol/usd",
            Self::Xrp => "xrp/usd",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Interval {
    M5,
    M15,
    H1,
    H4,
}

impl Interval {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::H1 => "1h",
            Self::H4 => "4h",
        }
    }

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

/// Parse edilmiş slug (`ts` = pencere başlangıcı, unix saniye).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlugInfo {
    pub asset: Asset,
    pub interval: Interval,
    pub ts: u64,
}

impl SlugInfo {
    pub fn end_ts(&self) -> u64 {
        self.ts + self.interval.seconds()
    }

    pub fn to_slug(&self) -> String {
        format!(
            "{}-updown-{}-{}",
            self.asset.as_str(),
            self.interval.as_str(),
            self.ts,
        )
    }
}

fn invalid(slug: &str, reason: impl Into<String>) -> AppError {
    AppError::InvalidSlug {
        slug: slug.to_string(),
        reason: reason.into(),
    }
}

/// `{asset}-updown-{interval}-{ts}` parse eder.
pub fn parse_slug(slug: &str) -> Result<SlugInfo, AppError> {
    let parts: Vec<&str> = slug.split('-').collect();
    if parts.len() != 4 {
        return Err(invalid(
            slug,
            format!("beklenen 4 parça, {} bulundu", parts.len()),
        ));
    }
    if parts[1] != "updown" {
        return Err(invalid(
            slug,
            format!("2. parça 'updown' olmalı, '{}' bulundu", parts[1]),
        ));
    }
    let asset = Asset::parse(parts[0]).ok_or_else(|| {
        invalid(
            slug,
            format!("desteklenmeyen asset '{}' (btc/eth/sol/xrp)", parts[0]),
        )
    })?;
    let interval = Interval::parse(parts[2]).ok_or_else(|| {
        invalid(
            slug,
            format!("desteklenmeyen interval '{}' (5m/15m/1h/4h)", parts[2]),
        )
    })?;
    let ts: u64 = parts[3]
        .parse()
        .map_err(|_| invalid(slug, format!("timestamp parse hatası: '{}'", parts[3])))?;
    if ts == 0 {
        return Err(invalid(slug, "timestamp 0 olamaz"));
    }
    if !ts.is_multiple_of(interval.seconds()) {
        return Err(invalid(
            slug,
            format!("timestamp {ts} interval ({}s) katı değil", interval.seconds()),
        ));
    }
    Ok(SlugInfo { asset, interval, ts })
}

/// Tam slug parse edilebilirse onu döner; aksi halde `{asset}-updown-{interval}`
/// önekini `ts = snap_active + start_offset * interval.seconds()` ile tamamlar.
pub fn parse_slug_or_prefix(pattern: &str, start_offset: u32) -> Result<SlugInfo, AppError> {
    if let Ok(info) = parse_slug(pattern) {
        return Ok(info);
    }
    let parts: Vec<&str> = pattern.trim_end_matches('-').split('-').collect();
    if parts.len() < 3 {
        return Err(invalid(
            pattern,
            "tam slug değil; önek de en az 3 parça olmalı",
        ));
    }
    let interval = Interval::parse(parts[2]).ok_or_else(|| {
        invalid(
            pattern,
            format!("önekten interval parse edilemedi: '{}'", parts[2]),
        )
    })?;
    let secs = interval.seconds();
    let ts = (crate::time::now_secs() / secs) * secs + (start_offset as u64) * secs;
    parse_slug(&format!("{}-{}-{}-{ts}", parts[0], parts[1], parts[2]))
}
