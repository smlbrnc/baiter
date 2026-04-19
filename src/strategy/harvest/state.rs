//! Harvest FSM durumu, context ve sabitler.

use serde::{Deserialize, Serialize};

use crate::config::StrategyParams;
use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::{OpenOrder, ZoneSignalMap};
use crate::time::MarketZone;
use crate::types::Outcome;

/// SingleLeg averaging tek tarafta tutulabilir maksimum share. Doc §17.
pub const MAX_POSITION_SIZE: f64 = 100.0;

/// Harvest FSM durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarvestState {
    /// OpenDual henüz gönderilmedi.
    Pending,
    /// İki GTC kitapta; `deadline_ms`'e kadar fill bekleniyor.
    OpenDual { deadline_ms: u64 },
    /// Bir taraf doldu; averaging döngüsünde.
    SingleLeg { filled_side: Outcome },
    /// Kâr kilitlendi — yeni emir yok.
    ProfitLock,
    Done,
}

#[derive(Debug, Clone)]
pub struct HarvestContext<'a> {
    pub params: &'a StrategyParams,
    pub metrics: &'a StrategyMetrics,
    pub yes_token_id: &'a str,
    pub no_token_id: &'a str,
    pub yes_best_bid: f64,
    pub yes_best_ask: f64,
    pub no_best_bid: f64,
    pub no_best_ask: f64,
    pub api_min_order_size: f64,
    pub order_usdc: f64,
    pub signal_weight: f64,
    pub effective_score: f64,
    pub zone: MarketZone,
    pub now_ms: u64,
    /// Son averaging turu zamanı (ms); ilk turda 0.
    pub last_averaging_ms: u64,
    /// En son MATCHED fill fiyatı (averaging kuralı için).
    pub last_fill_price: f64,
    /// Tick boyutu — OpenDual fiyatı snap için.
    pub tick_size: f64,
    /// OpenDual fill bekleme süresi (ms).
    pub dual_timeout: u64,
    /// MarketSession'daki açık emirler — timeout cancel + LIVE notional pos hesabı.
    pub open_orders: &'a [OpenOrder],
    /// SingleLeg ProfitLock eşiği (örn. 0.98).
    pub avg_threshold: f64,
    /// SingleLeg averaging tek tarafta tutulabilir maksimum share.
    pub max_position_size: f64,
    /// Global emir taban fiyatı — strateji içi proaktif clamp.
    pub min_price: f64,
    /// Global emir tavan fiyatı — strateji içi proaktif clamp.
    pub max_price: f64,
    /// Averaging cooldown (ms) — bot config'den gelir; iki rolü vardır:
    /// (1) iki averaging emri arası min süre,
    /// (2) açık averaging GTC max yaş.
    pub cooldown_threshold: u64,
}

impl<'a> HarvestContext<'a> {
    /// Outcome → token_id (book sözlüğü).
    pub(super) fn token_id(&self, side: Outcome) -> &'a str {
        match side {
            Outcome::Up => self.yes_token_id,
            Outcome::Down => self.no_token_id,
        }
    }

    /// Outcome → en iyi alış (kendi tarafı).
    pub(super) fn best_bid(&self, side: Outcome) -> f64 {
        match side {
            Outcome::Up => self.yes_best_bid,
            Outcome::Down => self.no_best_bid,
        }
    }

    /// Outcome → en iyi satış (kendi tarafı). Hedge fiyatı için karşı tarafın
    /// `best_ask`'ı istenirse `ctx.best_ask(side.opposite())` çağrılır.
    pub(super) fn best_ask(&self, side: Outcome) -> f64 {
        match side {
            Outcome::Up => self.yes_best_ask,
            Outcome::Down => self.no_best_ask,
        }
    }

    /// `signal_multiplier` (§14.4 harvest tablosu) — averaging size çarpanı.
    pub(super) fn signal_multiplier(&self, averaging_side: Outcome) -> f64 {
        if !ZoneSignalMap::HARVEST.is_active(self.zone) || self.signal_weight <= 0.0 {
            return 1.0;
        }
        let s = self.effective_score;
        match averaging_side {
            Outcome::Up => {
                if s >= 8.0 {
                    1.0
                } else if s >= 6.0 {
                    0.9
                } else if s >= 4.0 {
                    1.0
                } else if s >= 2.0 {
                    1.1
                } else {
                    1.2
                }
            }
            Outcome::Down => {
                if s >= 8.0 {
                    1.3
                } else if s >= 6.0 {
                    1.1
                } else if s >= 4.0 {
                    1.0
                } else if s >= 2.0 {
                    0.9
                } else {
                    0.7
                }
            }
        }
    }
}
