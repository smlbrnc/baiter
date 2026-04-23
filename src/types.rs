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

/// Bir emrin Polymarket likidite rolü — fee hesabı + strateji intent'i için.
/// Maker fill'leri Polymarket'te 0 fee; taker'lar concave fee öder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderRole {
    Taker,
    Maker,
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

    /// FAK + FOK her zaman taker.
    /// Resmi: <https://docs.polymarket.com/developers/CLOB/orders/order-types>.
    pub fn is_always_taker(self) -> bool {
        matches!(self, Self::Fak | Self::Fok)
    }

    /// `opposing_best`: BUY için karşı best_ask, SELL için karşı best_bid.
    /// GTC/GTD marketable fiyatta taker, aksi halde maker (book boşsa maker).
    pub fn role(self, side: Side, price: f64, opposing_best: f64) -> OrderRole {
        if self.is_always_taker() {
            return OrderRole::Taker;
        }
        let crosses = match side {
            Side::Buy => opposing_best > 0.0 && price >= opposing_best,
            Side::Sell => opposing_best > 0.0 && price <= opposing_best,
        };
        if crosses {
            OrderRole::Taker
        } else {
            OrderRole::Maker
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
    Aras,
}
