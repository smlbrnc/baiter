//! Strateji metrikleri — `StrategyMetrics` + `MarketPnL`.
//!
//! Referans: [docs/bot-platform-mimari.md §11 §17](../../../docs/bot-platform-mimari.md).

use serde::{Deserialize, Serialize};

use crate::types::{Outcome, Side};

/// Anlık strateji durumu — `best_bid_ask` / `trade MATCHED` sonrası güncellenir.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StrategyMetrics {
    pub shares_yes: f64,
    pub shares_no: f64,
    /// `shares_yes - shares_no`.
    pub imbalance: f64,
    /// YES tarafı VWAP (BUY fill'lerinde güncellenir; SELL'de değişmez).
    pub avg_yes: f64,
    /// NO tarafı VWAP.
    pub avg_no: f64,
    /// `avg_yes + avg_no` — Harvest profit-lock eşiğiyle karşılaştırılır.
    pub avg_sum: f64,
    /// Per-side son MATCHED fill fiyatı — UI / debug.
    pub last_fill_price_yes: f64,
    pub last_fill_price_no: f64,
    /// Brüt hacim (tüm stratejilerde).
    pub sum_yes: f64,
    pub sum_no: f64,
    /// Polymarket taker fee toplamı (`Σ size × feeRate × price × (1−price)`).
    /// Maker fill'leri 0 ile gelir (Polymarket policy: makers pay 0%).
    pub fee_total: f64,
}

impl StrategyMetrics {
    /// MATCHED fill event'ini absorbla. `side=Sell` → pozisyondan çıkış:
    /// `shares_*` azalır (0'a clamp), VWAP **değişmez** (kalan pozisyonun
    /// ortalama maliyeti korunur). `size` her zaman pozitif.
    ///
    /// Notional cost takibi (eski `imb_cost_*`) kaldırıldı; PnL hesabı
    /// VWAP × shares formülünden türetilir. Bu nedenle SELL realized profit
    /// `cost_basis`'te gözükmez; sadece `mtm_pnl` üzerinden değerlenir.
    pub fn ingest_fill(
        &mut self,
        outcome: Outcome,
        side: Side,
        price: f64,
        size: f64,
        fee: f64,
    ) {
        let signed = match side {
            Side::Buy => size,
            Side::Sell => -size,
        };
        match outcome {
            Outcome::Up => {
                if matches!(side, Side::Buy) {
                    let new_total = self.shares_yes + size;
                    if new_total > 0.0 {
                        self.avg_yes =
                            (self.avg_yes * self.shares_yes + price * size) / new_total;
                    }
                }
                self.shares_yes = (self.shares_yes + signed).max(0.0);
                self.sum_yes += size;
                self.last_fill_price_yes = price;
            }
            Outcome::Down => {
                if matches!(side, Side::Buy) {
                    let new_total = self.shares_no + size;
                    if new_total > 0.0 {
                        self.avg_no =
                            (self.avg_no * self.shares_no + price * size) / new_total;
                    }
                }
                self.shares_no = (self.shares_no + signed).max(0.0);
                self.sum_no += size;
                self.last_fill_price_no = price;
            }
        }
        self.fee_total += fee;
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
    /// Notional cost (`avg_yes × shares_yes + avg_no × shares_no`); fee hariç.
    /// VWAP × shares formülü olduğu için SELL realized profit burada gözükmez
    /// (avg_*'lar SELL'de değişmediğinden shares azaldıkça cost_basis doğal
    /// olarak düşer). Polymarket UI ile birebir eşleşmeyebilir; gerçek realized
    /// profit takibi için `mtm_pnl` veya tarihsel trade ledger kullanılır.
    pub cost_basis: f64,
    /// Polymarket taker fee toplamı (sadece bilgi/UI için ayrı kart).
    pub fee_total: f64,
    pub shares_yes: f64,
    pub shares_no: f64,
    /// `shares_yes - cost_basis` (YES kazanırsa, fee hariç).
    pub pnl_if_up: f64,
    /// `shares_no - cost_basis` (NO kazanırsa, fee hariç).
    pub pnl_if_down: f64,
    /// Pair-based MTM: kilitli pair `pair_count × $1` redemption,
    /// dengesiz kalan ise `× best_bid` ile değerlenir. Fee hariç.
    pub mtm_pnl: f64,
}

impl MarketPnL {
    /// VWAP × shares formülü ile cost_basis hesaplanır.
    /// `mtm_pnl` kilitli pair'i $1 redemption, imbalance'ı best_bid ile değerler.
    pub fn from_metrics(m: &StrategyMetrics, best_bid_yes: f64, best_bid_no: f64) -> Self {
        let cost_basis = m.avg_yes * m.shares_yes + m.avg_no * m.shares_no;
        let shares_yes = m.shares_yes;
        let shares_no = m.shares_no;
        let pair_count = shares_yes.min(shares_no);
        let imb_yes = shares_yes - pair_count;
        let imb_no = shares_no - pair_count;
        Self {
            cost_basis,
            fee_total: m.fee_total,
            shares_yes,
            shares_no,
            pnl_if_up: shares_yes - cost_basis,
            pnl_if_down: shares_no - cost_basis,
            mtm_pnl: pair_count + imb_yes * best_bid_yes + imb_no * best_bid_no - cost_basis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_updates_vwap_and_imbalance() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.60, 10.0, 0.0);
        assert!((m.avg_yes - 0.55).abs() < 1e-9);
        assert_eq!(m.shares_yes, 20.0);
        assert_eq!(m.imbalance, 20.0);

        m.ingest_fill(Outcome::Down, Side::Buy, 0.40, 15.0, 0.0);
        assert_eq!(m.shares_no, 15.0);
        assert_eq!(m.imbalance, 5.0);
        assert!((m.avg_sum - 0.95).abs() < 1e-9);
    }

    #[test]
    fn pair_count_min_of_sides() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.5, 10.0, 0.0);
        m.ingest_fill(Outcome::Down, Side::Buy, 0.5, 7.0, 0.0);
        assert_eq!(m.pair_count(), 7.0);
    }

    #[test]
    fn pnl_up_matches_yes_settlement() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.5, 10.0, 0.0);
        m.ingest_fill(Outcome::Down, Side::Buy, 0.48, 10.0, 0.0);
        let pnl = MarketPnL::from_metrics(&m, 0.6, 0.4);
        // cost_basis = 0.5*10 + 0.48*10 = 9.80 (VWAP × shares)
        assert!((pnl.cost_basis - 9.80).abs() < 1e-9);
        assert!((pnl.pnl_if_up - 0.20).abs() < 1e-9);
        assert!((pnl.pnl_if_down - 0.20).abs() < 1e-9);
    }

    /// Pair-locked MTM = pair_count × $1 − notional (fee hariç).
    #[test]
    fn pair_locked_mtm_uses_redemption_value() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.52, 10.0, 0.2496);
        m.ingest_fill(Outcome::Down, Side::Buy, 0.45, 10.0, 0.0);
        let pnl = MarketPnL::from_metrics(&m, 0.51, 0.45);
        assert!((pnl.cost_basis - 9.70).abs() < 1e-9);
        assert!((pnl.fee_total - 0.2496).abs() < 1e-9);
        assert!((pnl.mtm_pnl - 0.30).abs() < 1e-9);
        assert!((pnl.pnl_if_up - 0.30).abs() < 1e-9);
        assert!((pnl.pnl_if_down - 0.30).abs() < 1e-9);
    }

    #[test]
    fn imbalance_marked_at_best_bid() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.52, 10.0, 0.2496);
        m.ingest_fill(Outcome::Down, Side::Buy, 0.45, 10.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
        let pnl = MarketPnL::from_metrics(&m, 0.51, 0.45);
        // cost_basis = avg_yes(0.51) × 20 + avg_no(0.45) × 10 = 10.2 + 4.5 = 14.70
        assert!(
            (pnl.cost_basis - 14.70).abs() < 1e-9,
            "cost_basis={}",
            pnl.cost_basis
        );
        assert!((pnl.fee_total - 0.2496).abs() < 1e-9);
        // mtm = pair_count(10) + imb_yes(10) * 0.51 + imb_no(0) - 14.70 = 10 + 5.1 - 14.70 = 0.40
        assert!((pnl.mtm_pnl - 0.40).abs() < 1e-9);
    }

    /// Maker fee=0 invariant'ı: ingest_fill fee=0 ile çağrılınca fee_total artmaz.
    #[test]
    fn maker_fill_does_not_increase_fee_total() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.5, 10.0, 0.0);
        m.ingest_fill(Outcome::Down, Side::Buy, 0.5, 10.0, 0.0);
        assert_eq!(m.fee_total, 0.0);
        // VWAP × shares = 0.5 × 10 + 0.5 × 10 = 10.0
        let pnl = MarketPnL::from_metrics(&m, 0.5, 0.5);
        assert!((pnl.cost_basis - 10.0).abs() < 1e-9);
    }

    /// SELL VWAP'ı korur; shares düşer. cost_basis = avg × shares formülü olduğu
    /// için satıştan gelen realized profit `cost_basis`'te DEĞİL `mtm_pnl`'de
    /// görünür (eski `imb_cost_*` cash-flow takibi kaldırıldı).
    #[test]
    fn sell_preserves_vwap_reduces_shares() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.30, 20.0, 0.0);
        let avg_before = m.avg_yes;
        let cost_before = m.avg_yes * m.shares_yes; // 6.0

        m.ingest_fill(Outcome::Up, Side::Sell, 0.50, 10.0, 0.0);

        assert!(
            (m.avg_yes - avg_before).abs() < 1e-9,
            "VWAP SELL'de değişmemeli"
        );
        assert!((m.shares_yes - 10.0).abs() < 1e-9);
        let cost_after = m.avg_yes * m.shares_yes; // 3.0
        assert!(cost_after < cost_before, "cost shares ile düştü");
    }

    /// SELL miktar > mevcut shares: shares 0'a clamp; cost_basis 0 olur.
    #[test]
    fn sell_overflow_clamps_shares_to_zero() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.20, 5.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Sell, 0.50, 10.0, 0.0);
        assert_eq!(m.shares_yes, 0.0);
        let pnl = MarketPnL::from_metrics(&m, 0.5, 0.5);
        assert_eq!(pnl.shares_yes, 0.0);
        assert!(pnl.cost_basis.abs() < 1e-9, "shares=0 → cost_basis=0");
    }
}
