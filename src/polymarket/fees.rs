//! Polymarket CLOB fee politikası — tek kaynak.
//!
//! Spec: <https://docs.polymarket.com/trading/fees>
//!
//! Kural (rol-bazlı, order type'tan **bağımsız**):
//! - **Maker** (post-only ya da kitapta passive bekleyip karşı taraf
//!   tarafından alınan GTC) → fee 0.
//! - **Taker** (FOK/FAK ve kitabı geçen GTC/GTD dâhil her aktif fill) →
//!   konkav formül: `size × p × (1 − p) × bps / 10_000`.
//!
//! Symmetric `p × (1 − p)` faktörü Polymarket binary outcome'larda UP@p ile
//! DOWN@(1−p) trade'lerinin **aynı fee** miktarını ödemesini garanti eder.
//!
//! `bps == 0` (test/dryrun, fee'siz market) ya da `size <= 0` → 0.

/// Fee'yi rol'e göre hesapla. **Tek public fee API.**
///
/// `bps` market'in `fee_rate_bps` parametresi (CLOB `GET /fee-rate?token_id=`
/// ile sorgulanır, `MarketSession.fee_rate_bps` alanında saklanır).
pub fn fee_for_role(price: f64, size: f64, bps: u32, is_taker: bool) -> f64 {
    if !is_taker || bps == 0 || size <= 0.0 {
        return 0.0;
    }
    let p = price.clamp(0.0, 1.0);
    size * p * (1.0 - p) * (bps as f64) / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::fee_for_role;

    #[test]
    fn maker_returns_zero() {
        assert_eq!(fee_for_role(0.5, 100.0, 1000, false), 0.0);
        assert_eq!(fee_for_role(0.4, 10.0, 30, false), 0.0);
    }

    #[test]
    fn taker_zero_bps_zero_fee() {
        assert_eq!(fee_for_role(0.5, 100.0, 0, true), 0.0);
    }

    #[test]
    fn taker_zero_size_zero_fee() {
        assert_eq!(fee_for_role(0.5, 0.0, 30, true), 0.0);
    }

    #[test]
    fn taker_concave_formula_at_half() {
        let fee = fee_for_role(0.5, 100.0, 30, true);
        assert!((fee - (100.0 * 0.25 * 30.0 / 10_000.0)).abs() < 1e-9);
    }

    #[test]
    fn taker_symmetric_around_half() {
        let up = fee_for_role(0.4, 10.0, 30, true);
        let down = fee_for_role(0.6, 10.0, 30, true);
        assert!((up - down).abs() < 1e-9);
    }
}
