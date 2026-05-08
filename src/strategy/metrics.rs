//! Strateji metrikleri — `StrategyMetrics` + `MarketPnL`.
//!
//! Adlandırma sözleşmesi: tüm pozisyon/VWAP alanları `_up`/`_down` (Polymarket
//! "Yes/No" wire dilinden bağımsız strateji dili). `last_filled_*` her MATCHED
//! fill'de güncellenir (BUY/SELL fark etmez).

use serde::{Deserialize, Serialize};

use crate::types::{Outcome, Side};

/// Anlık pozisyon/PNL özeti — `trade MATCHED` sonrası güncellenir.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StrategyMetrics {
    pub up_filled: f64,
    pub down_filled: f64,
    /// UP tarafı VWAP (BUY fill'lerinde güncellenir; SELL'de değişmez).
    pub avg_up: f64,
    /// DOWN tarafı VWAP.
    pub avg_down: f64,
    /// Son UP MATCHED fill price'ı (BUY/SELL fark etmez). Henüz fill yoksa `0.0`.
    pub last_filled_up: f64,
    /// Son DOWN MATCHED fill price'ı.
    pub last_filled_down: f64,
    /// Polymarket taker fee toplamı (`Σ size × feeRate × price × (1−price)`).
    /// Maker fill'leri 0 ile gelir (Polymarket policy: makers pay 0%).
    pub fee_total: f64,
}

impl StrategyMetrics {
    /// MATCHED fill event'ini absorbla. `side=Sell` → pozisyondan çıkış:
    /// `*_filled` azalır (0'a clamp), VWAP **değişmez** (kalan pozisyonun
    /// ortalama maliyeti korunur). `size` her zaman pozitif.
    pub fn ingest_fill(&mut self, outcome: Outcome, side: Side, price: f64, size: f64, fee: f64) {
        let signed = match side {
            Side::Buy => size,
            Side::Sell => -size,
        };
        match outcome {
            Outcome::Up => {
                if matches!(side, Side::Buy) {
                    let new_total = self.up_filled + size;
                    if new_total > 0.0 {
                        self.avg_up = (self.avg_up * self.up_filled + price * size) / new_total;
                    }
                }
                self.up_filled = (self.up_filled + signed).max(0.0);
                self.last_filled_up = price;
            }
            Outcome::Down => {
                if matches!(side, Side::Buy) {
                    let new_total = self.down_filled + size;
                    if new_total > 0.0 {
                        self.avg_down =
                            (self.avg_down * self.down_filled + price * size) / new_total;
                    }
                }
                self.down_filled = (self.down_filled + signed).max(0.0);
                self.last_filled_down = price;
            }
        }
        self.fee_total += fee;
    }

    /// `min(up_filled, down_filled)` — kilitli pair sayısı.
    pub fn pair_count(&self) -> f64 {
        self.up_filled.min(self.down_filled)
    }

    /// `up_filled − down_filled`. Pozitif → UP dominant, negatif → DOWN dominant.
    pub fn imbalance(&self) -> f64 {
        self.up_filled - self.down_filled
    }

    /// `avg_up + avg_down` — profit-lock check için. Stored field değil
    /// (gereksiz invariant); her okumada toplam.
    pub fn avg_sum(&self) -> f64 {
        self.avg_up + self.avg_down
    }

    /// Pozisyonun cost basis'i (notional, fee hariç).
    pub fn cost_basis(&self) -> f64 {
        self.avg_up * self.up_filled + self.avg_down * self.down_filled
    }

    /// Profit-lock garantisi: her iki tarafta da fill olmalı (pair > 0)
    /// **ve** `avg_up + avg_down ≤ avg_threshold`. `avg_threshold` config'den
    /// (`StrategyParams::avg_threshold()`, default 0.98) gelir. Sınır durumda
    /// (eşit) lock geçerli sayılır — Alis hedge formülü hedef avg = threshold −
    /// best_ask_opp olduğundan tam eşit denk gelebiliyor.
    pub fn profit_locked(&self, avg_threshold: f64) -> bool {
        self.pair_count() > 0.0 && self.avg_sum() <= avg_threshold
    }
}

/// Market × bot PnL snapshot.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MarketPnL {
    /// Notional cost (`avg_up × up_filled + avg_down × down_filled`); fee hariç.
    pub cost_basis: f64,
    /// Polymarket taker fee toplamı (sadece bilgi/UI için ayrı kart).
    pub fee_total: f64,
    pub up_filled: f64,
    pub down_filled: f64,
    /// `up_filled − cost_basis` (UP kazanırsa, fee hariç).
    pub pnl_if_up: f64,
    /// `down_filled − cost_basis` (DOWN kazanırsa, fee hariç).
    pub pnl_if_down: f64,
    /// Pair-based MTM: kilitli pair `pair_count × $1` redemption,
    /// dengesiz kalan ise `× best_bid` ile değerlenir. Fee hariç.
    pub mtm_pnl: f64,
}

impl MarketPnL {
    /// VWAP × shares formülü ile cost_basis hesaplanır.
    /// `mtm_pnl` kilitli pair'i $1 redemption, imbalance'ı best_bid ile değerler.
    pub fn from_metrics(m: &StrategyMetrics, up_best_bid: f64, down_best_bid: f64) -> Self {
        let cost_basis = m.cost_basis();
        let up_filled = m.up_filled;
        let down_filled = m.down_filled;
        let pair_count = m.pair_count();
        let imb_up = up_filled - pair_count;
        let imb_down = down_filled - pair_count;
        Self {
            cost_basis,
            fee_total: m.fee_total,
            up_filled,
            down_filled,
            pnl_if_up: up_filled - cost_basis,
            pnl_if_down: down_filled - cost_basis,
            mtm_pnl: pair_count + imb_up * up_best_bid + imb_down * down_best_bid - cost_basis,
        }
    }
}
