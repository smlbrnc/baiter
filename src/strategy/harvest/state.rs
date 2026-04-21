//! Harvest v2 — state makinesi, ctx ve helper'lar.
//!
//! Spesifikasyon: [docs/harvest-v2.md](../../../../docs/harvest-v2.md) §2, §4, §16.

use serde::{Deserialize, Serialize};

use crate::strategy::metrics::StrategyMetrics;
use crate::strategy::OpenOrder;
use crate::time::MarketZone;
use crate::types::{Outcome, Side};

/// OpenPair açılış emri — taker/neutral leg (doc §16).
pub const OPEN_REASON_PREFIX: &str = "harvest_v2:open:";
/// OpenPair hedge emri + re-price edilen tüm hedge'ler.
pub const HEDGE_REASON_PREFIX: &str = "harvest_v2:hedge:";
/// NormalTrade averaging-down GTC (doc §7).
pub const AVG_DOWN_REASON_PREFIX: &str = "harvest_v2:avg_down:";
/// AggTrade/FakTrade pyramiding GTC (doc §8).
pub const PYRAMID_REASON_PREFIX: &str = "harvest_v2:pyramid:";

/// `passive.rs`/`executor.rs` cooldown (last_averaging_ms) tetikleyicisi.
/// Avg-down ve pyramid fill'leri kapsadığı gibi opener fill'lerini de kapsar:
/// session reset / FSM Pending'e dönüş senaryolarında peş peşe opener spam'ini
/// (Bot 4 / btc-updown-5m-1776773400 regresyonu) engellemek için cooldown
/// saati opener matched yanıtında da ileri alınır. Hedge fill'leri pencereyi
/// açmaz çünkü hedge passive olarak market hareketi ile dolar.
pub fn is_averaging_like(reason: &str) -> bool {
    reason.starts_with(AVG_DOWN_REASON_PREFIX)
        || reason.starts_with(PYRAMID_REASON_PREFIX)
        || reason.starts_with(OPEN_REASON_PREFIX)
}

/// Reason string builder'ları — `Outcome` lowercase suffix ile prefix birleştirir.
/// `open_pair.rs` / `position_open.rs` 4 call site bu helper'lara delege eder;
/// inline `format!("{prefix}{outcome}")` tekrarı kaldırılır.
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

/// Harvest v2 FSM (doc §4).
///
/// Persist edilmez — `MarketSession` yeniden oluşturulduğunda `Pending`'ten başlar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HarvestState {
    /// Book quote bekleniyor; hiç emir atılmadı.
    Pending,
    /// İki GTC (opener + hedge) kitapta.
    OpenPair,
    /// Tek leg MATCHED; hedge kitapta; avg-down/pyramid eligible.
    /// Hedge drift veya missing-hedge senaryolarında bu state'te kalınır;
    /// drift atomic `CancelAndPlace` ile yeniden fiyatlandırılır, missing
    /// hedge `PlaceOrders([replacement])` ile re-place edilir.
    PositionOpen { filled_side: Outcome },
    /// Pair kapandı; kalan açık emirler temizlenip `Done`'a geçilir.
    PairComplete,
    Done,
}

/// `HarvestEngine::decide` girdi ctx — ownership `MarketSession::tick` tarafından.
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
    /// Composite skor `[0, 10]`; 5.0 = nötr.
    pub effective_score: f64,
    pub zone: MarketZone,
    pub now_ms: u64,
    /// Son avg-down/pyramid MATCHED zamanı — cooldown referansı.
    pub last_averaging_ms: u64,
    pub tick_size: f64,
    pub open_orders: &'a [OpenOrder],
    /// ProfitLock / hedge eşiği (default 0.98).
    pub avg_threshold: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub cooldown_threshold: u64,
    /// Sinyal hazır mı? RTDS aktif iken `window_open_price.is_some()`,
    /// RTDS pasif iken her zaman `true`. `Pending` opener gate'i (doc §3, §5):
    /// pencere değişiminin ilk 0.5-1 sn'sinde sahte momentum sinyali ile
    /// opener basılmasını engeller.
    pub signal_ready: bool,
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

    pub(super) fn last_fill(&self, side: Outcome) -> f64 {
        match side {
            Outcome::Up => self.metrics.last_fill_price_yes,
            Outcome::Down => self.metrics.last_fill_price_no,
        }
    }

    /// Doc §8: pyramiding hedef tarafı — `yes_bid > 0.5` ise UP, aksi DOWN.
    pub(super) fn rising_side(&self) -> Outcome {
        if self.yes_best_bid > 0.5 {
            Outcome::Up
        } else {
            Outcome::Down
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

    /// Hedge tarafına ait kitapta açık BUY GTC. Hem `harvest_v2:hedge:*`
    /// (re-price edilmiş hedge) hem `harvest_v2:open:*` (OpenPair'de hedge
    /// leg taker fill alıp opener live kalan senaryoda) reason prefix'leri
    /// kapsanır — bot 6 / `btc-updown-5m-1776776400` regresyonu: opener
    /// kitapta dururken `replace_missing_hedge` ikinci hedge basıp aynı
    /// fiyattan çift fill almıştı.
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
}
