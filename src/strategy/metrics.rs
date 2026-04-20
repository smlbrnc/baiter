//! Strateji metrikleri — `StrategyMetrics` + `MarketPnL`.
//!
//! Referans: [docs/bot-platform-mimari.md §11 §17](../../../docs/bot-platform-mimari.md).

use serde::{Deserialize, Serialize};

use crate::types::Outcome;

/// Anlık strateji durumu — `best_bid_ask` / `trade MATCHED` sonrası güncellenir.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StrategyMetrics {
    pub shares_yes: f64,
    pub shares_no: f64,
    /// `shares_yes - shares_no`.
    pub imbalance: f64,
    /// YES tarafı VWAP.
    pub avg_yes: f64,
    /// NO tarafı VWAP.
    pub avg_no: f64,
    /// `avg_yes + avg_no` — Harvest PairComplete profit-lock eşiğiyle karşılaştırılır.
    pub avg_sum: f64,
    /// Per-side son MATCHED fill fiyatı — Harvest averaging "price_fell" kaynağı.
    pub last_fill_price_yes: f64,
    pub last_fill_price_no: f64,
    /// Brüt hacim (tüm stratejilerde).
    pub sum_yes: f64,
    pub sum_no: f64,
    /// `imbalance_cost` (per-side maliyet).
    pub imb_cost_up: f64,
    pub imb_cost_down: f64,
}

impl StrategyMetrics {
    /// MATCHED fill event'ini absorbla.
    pub fn ingest_fill(&mut self, outcome: Outcome, price: f64, size: f64, fee: f64) {
        match outcome {
            Outcome::Up => {
                let new_total = self.shares_yes + size;
                if new_total > 0.0 {
                    self.avg_yes = (self.avg_yes * self.shares_yes + price * size) / new_total;
                }
                self.shares_yes = new_total;
                self.sum_yes += size;
                self.imb_cost_up += price * size + fee;
                self.last_fill_price_yes = price;
            }
            Outcome::Down => {
                let new_total = self.shares_no + size;
                if new_total > 0.0 {
                    self.avg_no = (self.avg_no * self.shares_no + price * size) / new_total;
                }
                self.shares_no = new_total;
                self.sum_no += size;
                self.imb_cost_down += price * size + fee;
                self.last_fill_price_no = price;
            }
        }
        self.imbalance = self.shares_yes - self.shares_no;
        self.avg_sum = self.avg_yes + self.avg_no;
    }

    /// `min(shares_yes, shares_no)` — kâr formülünde kullanılır.
    pub fn pair_count(&self) -> f64 {
        self.shares_yes.min(self.shares_no)
    }
}

/// Market × bot PnL snapshot (§17).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MarketPnL {
    pub cost_basis: f64,
    /// Daima 0.0 — komisyon `imb_cost_*` üzerinden cost_basis'e dahil. UI/PnL
    /// şema uyumluluğu için saklanır.
    pub fee_total: f64,
    pub shares_yes: f64,
    pub shares_no: f64,
    /// `shares_yes - cost_basis` (YES kazanırsa).
    pub pnl_if_up: f64,
    /// `shares_no - cost_basis` (NO kazanırsa).
    pub pnl_if_down: f64,
    /// `shares_yes * best_bid_yes + shares_no * best_bid_no - cost_basis`.
    pub mtm_pnl: f64,
}

impl MarketPnL {
    pub fn from_metrics(m: &StrategyMetrics, best_bid_yes: f64, best_bid_no: f64) -> Self {
        let cost_basis = m.imb_cost_up + m.imb_cost_down;
        let shares_yes = m.shares_yes;
        let shares_no = m.shares_no;
        Self {
            cost_basis,
            fee_total: 0.0,
            shares_yes,
            shares_no,
            pnl_if_up: shares_yes - cost_basis,
            pnl_if_down: shares_no - cost_basis,
            mtm_pnl: shares_yes * best_bid_yes + shares_no * best_bid_no - cost_basis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_updates_vwap_and_imbalance() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, 0.50, 10.0, 0.0);
        m.ingest_fill(Outcome::Up, 0.60, 10.0, 0.0);
        assert!((m.avg_yes - 0.55).abs() < 1e-9);
        assert_eq!(m.shares_yes, 20.0);
        assert_eq!(m.imbalance, 20.0);

        m.ingest_fill(Outcome::Down, 0.40, 15.0, 0.0);
        assert_eq!(m.shares_no, 15.0);
        assert_eq!(m.imbalance, 5.0);
        assert!((m.avg_sum - 0.95).abs() < 1e-9);
    }

    #[test]
    fn pair_count_min_of_sides() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, 0.5, 10.0, 0.0);
        m.ingest_fill(Outcome::Down, 0.5, 7.0, 0.0);
        assert_eq!(m.pair_count(), 7.0);
    }

    #[test]
    fn pnl_up_matches_yes_settlement() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, 0.5, 10.0, 0.0);
        m.ingest_fill(Outcome::Down, 0.48, 10.0, 0.0);
        let pnl = MarketPnL::from_metrics(&m, 0.6, 0.4);
        assert!((pnl.pnl_if_up - 0.2).abs() < 1e-9);
        assert!((pnl.pnl_if_down - 0.2).abs() < 1e-9);
    }
}
