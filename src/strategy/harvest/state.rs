//! Harvest FSM durumu, context ve sabitler.

use serde::{Deserialize, Serialize};

use crate::config::StrategyParams;
use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::{OpenOrder, ZoneSignalMap};
use crate::time::MarketZone;
use crate::types::Outcome;

/// SingleLeg averaging tek tarafta tutulabilir maksimum share. Doc §17.
pub const MAX_POSITION_SIZE: f64 = 100.0;

/// `signal_multiplier` katmanları: `(min_score, up_mult, down_mult)`.
/// Yüksek eşik önce, ilk eşleşme kazanır.
const SIGNAL_TIERS: [(f64, f64, f64); 5] = [
    (8.0, 1.0, 1.3),
    (6.0, 0.9, 1.1),
    (4.0, 1.0, 1.0),
    (2.0, 1.1, 0.9),
    (f64::NEG_INFINITY, 1.2, 0.7),
];

/// Harvest FSM durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarvestState {
    /// OpenDual henüz gönderilmedi.
    Pending,
    /// İki GTC kitapta; `deadline_ms`'e kadar fill bekleniyor.
    OpenDual { deadline_ms: u64 },
    /// Tek taraf doldu; averaging döngüsünde. `entered_at_ms` ProfitLock warmup için
    /// — ilk `cooldown_threshold` boyunca FAK kontrolü pas geçilir.
    SingleLeg {
        filled_side: Outcome,
        #[serde(default)]
        entered_at_ms: u64,
    },
    /// İki taraf da doldu; `avg_yes + avg_no ≤ avg_threshold` + balanced → Done.
    DoubleLeg,
    /// Legacy — yeni kod üretmez. `decide` Done'a evolve eder; eski persist'ler
    /// için Deserialize uyumluluğu.
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
    /// Composite skor (RTDS + Binance harmanı; 5.0 = nötr).
    pub effective_score: f64,
    pub zone: MarketZone,
    pub now_ms: u64,
    /// Son averaging turu zamanı (ms).
    pub last_averaging_ms: u64,
    /// OpenDual fiyatı snap için tick boyutu.
    pub tick_size: f64,
    /// OpenDual fill bekleme süresi (ms).
    pub dual_timeout: u64,
    /// LIVE açık emirler — timeout cancel + notional pos hesabı.
    pub open_orders: &'a [OpenOrder],
    /// ProfitLock eşiği (örn. 0.98).
    pub avg_threshold: f64,
    /// Tek tarafta tutulabilir maksimum share.
    pub max_position_size: f64,
    /// Global emir taban / tavan fiyatı.
    pub min_price: f64,
    pub max_price: f64,
    /// Averaging cooldown (ms): (1) iki averaging arası min süre,
    /// (2) açık averaging GTC max yaş, (3) SingleLeg ProfitLock warmup.
    pub cooldown_threshold: u64,
}

impl<'a> HarvestContext<'a> {
    pub(super) fn token_id(&self, side: Outcome) -> &'a str {
        match side {
            Outcome::Up => self.yes_token_id,
            Outcome::Down => self.no_token_id,
        }
    }

    pub(super) fn best_bid(&self, side: Outcome) -> f64 {
        match side {
            Outcome::Up => self.yes_best_bid,
            Outcome::Down => self.no_best_bid,
        }
    }

    pub(super) fn best_ask(&self, side: Outcome) -> f64 {
        match side {
            Outcome::Up => self.yes_best_ask,
            Outcome::Down => self.no_best_ask,
        }
    }

    /// Averaging size çarpanı (§14.4 harvest tablosu). Harvest zone aktif değilse 1.0.
    pub(super) fn signal_multiplier(&self, averaging_side: Outcome) -> f64 {
        if !ZoneSignalMap::HARVEST.is_active(self.zone) {
            return 1.0;
        }
        let s = self.effective_score;
        let tier = SIGNAL_TIERS
            .iter()
            .find(|(min, _, _)| s >= *min)
            .expect("NEG_INFINITY tier matches all");
        match averaging_side {
            Outcome::Up => tier.1,
            Outcome::Down => tier.2,
        }
    }
}
