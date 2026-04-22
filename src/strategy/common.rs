//! Strateji <-> engine arasındaki ortak veri tipleri.
//!
//! `Decision` her stratejinin `decide()` çıktısı; `PlannedOrder` ve `OpenOrder`
//! place/cancel akışında kullanılır. `StrategyContext` ise tüm stratejilerin
//! tick başına okuduğu paylaşımlı snapshot — yeni strateji eklerken context'e
//! sadece o stratejinin ihtiyacı olan alan eklenir, mevcutlar bozulmaz.

use crate::strategy::metrics::StrategyMetrics;
use crate::time::MarketZone;
use crate::types::{Outcome, OrderType, Side};

/// `decide()` sonucu — engine bu envelope'u sırayla yürütür.
#[derive(Debug, Clone)]
pub enum Decision {
    NoOp,
    PlaceOrders(Vec<PlannedOrder>),
    CancelOrders(Vec<String>),
    /// Önce iptal, sonra yerleştir — atomic re-price (örn. hedge drift).
    CancelAndPlace {
        cancels: Vec<String>,
        places: Vec<PlannedOrder>,
    },
}

/// Strateji tarafından planlanan emir; engine bunu CLOB POST veya DryRun fill'e çevirir.
#[derive(Debug, Clone)]
pub struct PlannedOrder {
    pub outcome: Outcome,
    pub token_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub order_type: OrderType,
    /// Strateji-spesifik etiket (örn. `"alis:open:up"`). Cooldown ve log için.
    pub reason: String,
}

/// Açık emir snapshot'u — REST POST cevabından / DryRun simülatöründen yazılır,
/// WS `order` ve `trade` event'lerinde update/prune edilir.
#[derive(Debug, Clone)]
pub struct OpenOrder {
    pub id: String,
    pub outcome: Outcome,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub reason: String,
    pub placed_at_ms: u64,
    pub size_matched: f64,
}

impl OpenOrder {
    /// Emrin ne kadar süredir kitapta olduğu (ms). Cooldown / GTC max-age
    /// kontrolleri için stratejilerin ortak helper'ı.
    pub fn age_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.placed_at_ms)
    }
}

/// Stratejilerin tick başına okuduğu salt-okunur snapshot.
/// Yeni strateji bu yapıya yeni alan **eklemekte** serbesttir; mevcut alanlar
/// kararlı public API gibi davranır (tüm stratejiler okuyabilmeli).
pub struct StrategyContext<'a> {
    pub metrics: &'a StrategyMetrics,
    pub up_token_id: &'a str,
    pub down_token_id: &'a str,
    pub up_best_bid: f64,
    pub up_best_ask: f64,
    pub down_best_bid: f64,
    pub down_best_ask: f64,
    pub api_min_order_size: f64,
    pub order_usdc: f64,
    /// Composite skor `[0, 10]`; 5.0 nötr (long/short eşit).
    pub effective_score: f64,
    pub zone: MarketZone,
    pub now_ms: u64,
    pub last_averaging_ms: u64,
    pub tick_size: f64,
    pub open_orders: &'a [OpenOrder],
    pub min_price: f64,
    pub max_price: f64,
    /// Averaging cooldown (ms) + GTC max age. `BotConfig.cooldown_threshold`.
    pub cooldown_threshold: u64,
    /// Profit-lock eşiği — `StrategyParams::avg_threshold()` (default 0.98).
    /// `metrics.profit_locked()` ve `hedge_price()` bu değerle çalışır.
    pub avg_threshold: f64,
    pub signal_ready: bool,
}

impl StrategyContext<'_> {
    pub fn token_id(&self, outcome: Outcome) -> &str {
        match outcome {
            Outcome::Up => self.up_token_id,
            Outcome::Down => self.down_token_id,
        }
    }

    pub fn best_bid(&self, outcome: Outcome) -> f64 {
        match outcome {
            Outcome::Up => self.up_best_bid,
            Outcome::Down => self.down_best_bid,
        }
    }

    pub fn best_ask(&self, outcome: Outcome) -> f64 {
        match outcome {
            Outcome::Up => self.up_best_ask,
            Outcome::Down => self.down_best_ask,
        }
    }

    /// Anlık fiyat bandı tabanlı yön (kullanıcı tanımı, doc canonical):
    /// son UP MATCHED price `[0.55, max_price]` aralığındaysa **UP yükselişte**;
    /// `[min_price, 0.45]` aralığındaysa **DOWN yükselişte**; aksi halde
    /// nötr bant (`None`). `last_filled_up == 0.0` → henüz fill yok → `None`.
    pub fn rising_side(&self) -> Option<Outcome> {
        let p = self.metrics.last_filled_up;
        if p <= 0.0 {
            return None;
        }
        if p >= 0.55 && p <= self.max_price {
            Some(Outcome::Up)
        } else if p >= self.min_price && p <= 0.45 {
            Some(Outcome::Down)
        } else {
            None
        }
    }
}

/// Profit-lock için karşı tarafa basılacak hedge fiyatı:
/// `avg_threshold − metrics.avg_dominant()`'i `[min_price, max_price]` aralığına clamp'ler.
pub fn hedge_price(
    metrics: &StrategyMetrics,
    avg_threshold: f64,
    min_price: f64,
    max_price: f64,
) -> f64 {
    (avg_threshold - metrics.avg_dominant()).clamp(min_price, max_price)
}

/// Pair tamamlamak için hedge size: `|imbalance|` (= eksik tarafa basılacak share).
/// İki taraf eşit ise 0.
pub fn hedge_size(metrics: &StrategyMetrics) -> f64 {
    metrics.imbalance().abs()
}
