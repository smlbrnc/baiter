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
    /// Per-side notional maliyet (`Σ price × size`); fee dahil DEĞİL.
    pub imb_cost_up: f64,
    pub imb_cost_down: f64,
    /// Polymarket taker fee toplamı (`Σ size × feeRate × price × (1−price)`).
    /// Maker fill'leri 0 ile gelir (Polymarket policy: makers pay 0%).
    pub fee_total: f64,
}

impl StrategyMetrics {
    /// MATCHED fill event'ini absorbla. `side=Sell` → pozisyondan çıkış:
    /// `shares_*` azalır (0'a clamp), `imb_cost_*` notional cash-in kadar
    /// düşer (negatife inebilir = realized profit), VWAP **değişmez** (kalan
    /// pozisyonun ortalama maliyeti korunur). `size` her zaman pozitif.
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
                self.imb_cost_up += signed * price;
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
                self.imb_cost_down += signed * price;
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
    /// Notional cost (`Σ price × size`); fee hariç. Polymarket UI ile birebir.
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
    /// Polymarket'in profit-lock mantığıyla uyumlu PnL:
    /// - `cost_basis` notional only (fee ayrı satırda).
    /// - `mtm_pnl` kilitli pair'i $1 redemption, imbalance'ı best_bid ile değerler.
    pub fn from_metrics(m: &StrategyMetrics, best_bid_yes: f64, best_bid_no: f64) -> Self {
        // `StrategyMetrics::ingest_fill` shares'i 0'a clamp ediyor (SELL
        // overflow garantisi); burada ek `.max(0.0)` çağrısı gereksiz.
        let cost_basis = m.imb_cost_up + m.imb_cost_down;
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
        // cost_basis = 0.5*10 + 0.48*10 = 9.80 (notional only)
        assert!((pnl.cost_basis - 9.80).abs() < 1e-9);
        assert!((pnl.pnl_if_up - 0.20).abs() < 1e-9);
        assert!((pnl.pnl_if_down - 0.20).abs() < 1e-9);
    }

    /// Pair-locked MTM = pair_count × $1 − notional (fee hariç).
    /// Bot 52 senaryosu ilk pair: UP @ 0.52 × 10 + DOWN @ 0.45 × 10.
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

    /// İmbalance kısmı best_bid ile değerlenir; pair_count kısmı $1 redemption.
    /// Bot 52 senaryosu üç trade sonrası: UP=20, DOWN=10.
    #[test]
    fn imbalance_marked_at_best_bid() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.52, 10.0, 0.2496);
        m.ingest_fill(Outcome::Down, Side::Buy, 0.45, 10.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.50, 10.0, 0.0);
        let pnl = MarketPnL::from_metrics(&m, 0.51, 0.45);
        // cost_basis = 5.20 + 5.00 + 4.50 = 14.70
        assert!((pnl.cost_basis - 14.70).abs() < 1e-9);
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
        assert!((m.imb_cost_up + m.imb_cost_down - 10.0).abs() < 1e-9);
    }

    /// Bot 2 senaryosu: 9 BUY UP (toplam 98.99 share, ~25.29 cost) sonra
    /// taker SELL UP 98.41 @ 0.92 → shares_yes ≈ 0.58, cost_basis negatife
    /// iner (realized profit), VWAP korunur.
    #[test]
    fn sell_reduces_shares_and_cost_realized_profit() {
        let mut m = StrategyMetrics::default();
        // Yığılmış BUY pozisyonu (Bot 2 history'sinden basitleştirilmiş).
        m.ingest_fill(Outcome::Up, Side::Buy, 0.45, 11.0, 0.27225);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.30, 17.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.17, 10.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.17, 6.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.17, 0.68, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.17, 3.31, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.17, 10.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.15, 34.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Buy, 0.72, 7.0, 0.14112);
        let buy_shares = 11.0 + 17.0 + 10.0 + 6.0 + 0.68 + 3.31 + 10.0 + 34.0 + 7.0;
        let buy_cost = 0.45 * 11.0
            + 0.30 * 17.0
            + 0.17 * 10.0
            + 0.17 * 6.0
            + 0.17 * 0.68
            + 0.17 * 3.31
            + 0.17 * 10.0
            + 0.15 * 34.0
            + 0.72 * 7.0;
        assert!((m.shares_yes - buy_shares).abs() < 1e-9);
        assert!((m.imb_cost_up - buy_cost).abs() < 1e-9);
        let avg_yes_before_sell = m.avg_yes;

        // Manuel taker SELL.
        m.ingest_fill(Outcome::Up, Side::Sell, 0.92, 98.41, 0.7242976);

        let expected_remaining = buy_shares - 98.41;
        let expected_cost = buy_cost - 0.92 * 98.41;
        assert!(
            (m.shares_yes - expected_remaining).abs() < 1e-9,
            "shares_yes={} vs {}",
            m.shares_yes,
            expected_remaining
        );
        assert!(
            (m.imb_cost_up - expected_cost).abs() < 1e-9,
            "cost_basis={} (negatif olmalı)",
            m.imb_cost_up
        );
        assert!(m.imb_cost_up < 0.0, "SELL > BUY notional ⇒ realized profit");
        assert!(
            (m.avg_yes - avg_yes_before_sell).abs() < 1e-9,
            "VWAP SELL'de değişmemeli"
        );
        assert!(m.shares_yes >= 0.0);
    }

    /// SELL miktar > mevcut shares: shares 0'a clamp; cost negatife iner;
    /// PnL formülü `max(0)` koruması ile sağlam.
    #[test]
    fn sell_overflow_clamps_shares_to_zero() {
        let mut m = StrategyMetrics::default();
        m.ingest_fill(Outcome::Up, Side::Buy, 0.20, 5.0, 0.0);
        m.ingest_fill(Outcome::Up, Side::Sell, 0.50, 10.0, 0.0);
        assert_eq!(m.shares_yes, 0.0);
        // imb_cost = 1.00 - 5.00 = -4.00 (realized profit korunur).
        assert!((m.imb_cost_up + 4.0).abs() < 1e-9);
        let pnl = MarketPnL::from_metrics(&m, 0.5, 0.5);
        assert_eq!(pnl.shares_yes, 0.0);
        assert!((pnl.cost_basis + 4.0).abs() < 1e-9);
        // mtm = 0 + 0 + 0 - (-4.0) = 4.0
        assert!((pnl.mtm_pnl - 4.0).abs() < 1e-9);
    }
}
