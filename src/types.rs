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

    /// Wire-form ("UP" / "DOWN") — log ve API çıktısında aynı string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Down => "DOWN",
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
}

/// `BotConfig.run_mode` — Live (CLOB REST) veya DryRun (Simulator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    Live,
    Dryrun,
}

/// `BotConfig.strategy` — aktif olan: `Harvest`. Diğerleri DB sözleşmesi için durur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    DutchBook,
    Harvest,
    Prism,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_opposite() {
        assert_eq!(Outcome::Up.opposite(), Outcome::Down);
        assert_eq!(Outcome::Down.opposite(), Outcome::Up);
    }

    #[test]
    fn outcome_serde() {
        let s = serde_json::to_string(&Outcome::Up).unwrap();
        assert_eq!(s, "\"UP\"");
    }

    #[test]
    fn order_type_serde() {
        let s = serde_json::to_string(&OrderType::Gtc).unwrap();
        assert_eq!(s, "\"GTC\"");
    }
}
