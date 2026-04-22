//! Harvest v2 — state makinesi, ctx ve helper'lar.

use serde::{Deserialize, Serialize};

use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::OpenOrder;
use crate::time::MarketZone;
use crate::types::{Outcome, Side};

pub const OPEN_REASON_PREFIX: &str = "harvest_v2:open:";
pub const HEDGE_REASON_PREFIX: &str = "harvest_v2:hedge:";
pub const AVG_DOWN_REASON_PREFIX: &str = "harvest_v2:avg_down:";
pub const PYRAMID_REASON_PREFIX: &str = "harvest_v2:pyramid:";

/// Pyramid trend dead zone: `|yes_bid − 0.5| < RISING_EPS` ise yön belirsiz.
pub const RISING_EPS: f64 = 0.05;

/// Cooldown tetikleyicisi — opener / avg-down / pyramid fill'lerini kapsar.
pub fn is_averaging_like(reason: &str) -> bool {
    reason.starts_with(AVG_DOWN_REASON_PREFIX)
        || reason.starts_with(PYRAMID_REASON_PREFIX)
        || reason.starts_with(OPEN_REASON_PREFIX)
}

pub fn open_reason(side: Outcome) -> String {
    format!("{OPEN_REASON_PREFIX}{}", side.as_lowercase())
}

pub fn hedge_reason(side: Outcome) -> String {
    format!("{HEDGE_REASON_PREFIX}{}", side.as_lowercase())
}

pub fn avg_down_reason(side: Outcome) -> String {
    format!("{AVG_DOWN_REASON_PREFIX}{}", side.as_lowercase())
}

pub fn pyramid_reason(side: Outcome) -> String {
    format!("{PYRAMID_REASON_PREFIX}{}", side.as_lowercase())
}

/// Harvest v2 FSM (doc §4). Persist edilmez — `Pending`'ten başlar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HarvestState {
    /// Book quote bekleniyor; hiç emir atılmadı.
    Pending,
    /// İki GTC (opener + hedge) kitapta.
    OpenPair,
    /// Tek leg MATCHED; hedge kitapta; avg-down/pyramid eligible.
    PositionOpen { filled_side: Outcome },
    /// Covered pair + shares parity; HOLD modu, settlement'a kadar yeni emir yok.
    ProfitLocked { filled_side: Outcome },
    /// Pair kapandı; kalan açık emirler temizlenip `Done`'a geçilir.
    PairComplete,
    Done,
}

#[derive(Debug, Clone)]
pub struct HarvestContext<'a> {
    pub metrics: &'a StrategyMetrics,
    pub yes_token_id: &'a str,
    pub no_token_id: &'a str,
    pub yes_best_bid: f64,
    pub yes_best_ask: f64,
    pub no_best_bid: f64,
    pub no_best_ask: f64,
    pub api_min_order_size: f64,
    pub order_usdc: f64,
    pub effective_score: f64,
    pub zone: MarketZone,
    pub now_ms: u64,
    pub last_averaging_ms: u64,
    pub tick_size: f64,
    pub open_orders: &'a [OpenOrder],
    pub avg_threshold: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub cooldown_threshold: u64,
    pub signal_ready: bool,
    pub avg_min_score: f64,
    pub max_position_usdc: Option<f64>,
    pub opposite_pyramid_enabled: bool,
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

    pub(super) fn spread(&self, side: Outcome) -> f64 {
        (self.best_ask(side) - self.best_bid(side)).max(0.0)
    }

    /// Doc §3: `delta(side) = (score − 5) / 5 × spread(side)`.
    pub(super) fn delta(&self, side: Outcome) -> f64 {
        (self.effective_score - 5.0) / 5.0 * self.spread(side)
    }

    pub(super) fn shares(&self, side: Outcome) -> f64 {
        match side {
            Outcome::Up => self.metrics.shares_yes,
            Outcome::Down => self.metrics.shares_no,
        }
    }

    pub(super) fn avg_filled(&self, side: Outcome) -> f64 {
        match side {
            Outcome::Up => self.metrics.avg_yes,
            Outcome::Down => self.metrics.avg_no,
        }
    }

    pub(super) fn cost_filled(&self, side: Outcome) -> f64 {
        self.avg_filled(side) * self.shares(side)
    }

    /// ProfitLock: covered pair + shares parity (`|shares_yes − shares_no| < api_min`).
    pub(super) fn profit_locked(&self) -> bool {
        let m = self.metrics;
        if m.shares_yes <= 0.0 || m.shares_no <= 0.0 {
            return false;
        }
        (m.shares_yes - m.shares_no).abs() < self.api_min_order_size
    }

    /// Doc §8: pyramiding hedef tarafı; 0.5 ± `RISING_EPS` dead zone'da `None`.
    pub(super) fn rising_side(&self) -> Option<Outcome> {
        let bid = self.yes_best_bid;
        if bid > 0.5 + RISING_EPS {
            Some(Outcome::Up)
        } else if bid < 0.5 - RISING_EPS {
            Some(Outcome::Down)
        } else {
            None
        }
    }

    /// Pozisyon ağırlığına göre hedge tarafı; dust imbalance'ta `None`.
    pub(super) fn majority_side(&self) -> Option<Outcome> {
        let diff = self.metrics.shares_yes - self.metrics.shares_no;
        if diff.abs() < self.api_min_order_size {
            return None;
        }
        if diff > 0.0 {
            Some(Outcome::Up)
        } else {
            Some(Outcome::Down)
        }
    }

    /// `tick_size` grid'ine snap + `[min_price, max_price]` clamp.
    pub(super) fn snap_clamp(&self, price: f64) -> f64 {
        let snapped = (price / self.tick_size).round() * self.tick_size;
        snapped.clamp(self.min_price, self.max_price)
    }

    pub(super) fn cooldown_ok(&self) -> bool {
        self.now_ms.saturating_sub(self.last_averaging_ms) >= self.cooldown_threshold
    }

    pub(super) fn price_in_band(&self, price: f64) -> bool {
        price >= self.min_price && price <= self.max_price
    }

    /// Hedge tarafına ait kitapta açık BUY GTC — hem `hedge:*` hem `open:*` reason'lar.
    pub(super) fn hedge_order(&self, hedge_side: Outcome) -> Option<&OpenOrder> {
        self.open_orders.iter().find(|o| {
            o.outcome == hedge_side
                && o.side == Side::Buy
                && (o.reason.starts_with(HEDGE_REASON_PREFIX)
                    || o.reason.starts_with(OPEN_REASON_PREFIX))
        })
    }

    pub(super) fn has_open_avg(&self, side: Outcome) -> bool {
        self.open_orders
            .iter()
            .any(|o| o.outcome == side && o.reason.starts_with(AVG_DOWN_REASON_PREFIX))
    }

    pub(super) fn has_open_pyramid(&self, side: Outcome) -> bool {
        self.open_orders
            .iter()
            .any(|o| o.outcome == side && o.reason.starts_with(PYRAMID_REASON_PREFIX))
    }

    pub(super) fn stale_avg_or_pyramid_ids(&self) -> Vec<String> {
        self.open_orders
            .iter()
            .filter(|o| {
                (o.reason.starts_with(AVG_DOWN_REASON_PREFIX)
                    || o.reason.starts_with(PYRAMID_REASON_PREFIX))
                    && self.now_ms.saturating_sub(o.placed_at_ms) >= self.cooldown_threshold
            })
            .map(|o| o.id.clone())
            .collect()
    }

    /// P0: `effective_score` `side` tarafını destekliyor mu?
    /// UP için skor `avg_min_score` ≥ eşik, DOWN için `10 − avg_min_score` ≤ eşik.
    /// `avg_min_score == 0.0` → guard kapalı, her zaman `true` döner.
    pub(super) fn signal_supports(&self, side: Outcome) -> bool {
        if self.avg_min_score == 0.0 {
            return true;
        }
        match side {
            Outcome::Up => self.effective_score >= self.avg_min_score,
            Outcome::Down => self.effective_score <= 10.0 - self.avg_min_score,
        }
    }

    /// P1: Signal'ın artık desteklemediği açık avg/pyramid emirlerinin ID listesi.
    /// Bir sonraki tick'te cancel edilmek üzere `handle()` tarafından sorgulanır.
    pub(super) fn signal_opposed_avg_ids(&self) -> Vec<String> {
        if self.avg_min_score == 0.0 {
            return vec![];
        }
        self.open_orders
            .iter()
            .filter(|o| {
                (o.reason.starts_with(AVG_DOWN_REASON_PREFIX)
                    || o.reason.starts_with(PYRAMID_REASON_PREFIX))
                    && !self.signal_supports(o.outcome)
            })
            .map(|o| o.id.clone())
            .collect()
    }

    /// P2: Toplam notional (cost_basis) pencere başına cap'i aştı mı?
    pub(super) fn position_cap_reached(&self) -> bool {
        match self.max_position_usdc {
            Some(cap) => {
                let total = self.metrics.avg_yes * self.metrics.shares_yes
                    + self.metrics.avg_no * self.metrics.shares_no;
                total >= cap
            }
            None => false,
        }
    }
}
