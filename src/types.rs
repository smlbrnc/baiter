//! Polymarket ham tipleri — CLOB REST + WS payload'larıyla bire bir uyumlu.

use serde::{Deserialize, Serialize};

/// İkili market outcome'u — Polymarket "Yes/No"yu strateji dilinde "UP/DOWN"a maler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Outcome {
    Up,
    Down,
}

impl Outcome {
    pub fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }

    /// Wire-form `"UP"` / `"DOWN"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Down => "DOWN",
        }
    }

    /// Lowercase form (`alis:open:up` gibi reason etiketleri için).
    pub fn as_lowercase(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    /// Case-insensitive parse; geçersiz → `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "UP" => Some(Self::Up),
            "DOWN" => Some(Self::Down),
            _ => None,
        }
    }
}

/// CLOB emir yönü — REST `side` alanı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }

    /// Case-insensitive parse; geçersiz → `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "BUY" => Some(Self::Buy),
            "SELL" => Some(Self::Sell),
            _ => None,
        }
    }
}

/// CLOB emir tipi — REST `order_type` alanı.
/// `GTC` = Good 'Til Canceled, `GTD` = Good 'Til Date,
/// `FOK` = Fill Or Kill, `FAK` = Fill And Kill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderType {
    Gtc,
    Gtd,
    Fok,
    Fak,
}

impl OrderType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gtc => "GTC",
            Self::Gtd => "GTD",
            Self::Fok => "FOK",
            Self::Fak => "FAK",
        }
    }

    /// Case-insensitive parse (GTC/GTD/FOK/FAK); geçersiz → `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "GTC" => Some(Self::Gtc),
            "GTD" => Some(Self::Gtd),
            "FOK" => Some(Self::Fok),
            "FAK" => Some(Self::Fak),
            _ => None,
        }
    }
}

/// Live (CLOB REST) veya DryRun (Simulator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    Live,
    Dryrun,
}

/// Aktif strateji; FSM'leri `src/strategy/<name>.rs` altında.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    Alis,
    Elis,
    Bonereaper,
    Gravie,
}
