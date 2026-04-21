//! Polymarket CLOB taker fee invariant'ı.
//!
//! Spec: <https://docs.polymarket.com/developers/CLOB/fees>
//!
//! - Maker fill'lerde fee 0 (post-only / GTC limit order book'a girip karşı
//!   taraf tarafından alınır).
//! - Taker fill'lerde fee = `size × p × (1 − p) × bps / 10_000`, burada
//!   `bps` market'in `fee_rate_bps` parametresi (CLOB `GET /fee-rate?token_id=`
//!   ile sorgulanır, `MarketSession.fee_rate_bps` alanında saklanır).
//! - Symmetric formül `p × (1 − p)` Polymarket binary outcome fiyat
//!   parametrizasyonunda fee'nin notional'a değil, "her iki taraf için aynı
//!   fee miktarı" garantisine bağlı (UP @ p ile DOWN @ 1−p aynı fee'yi öder).
//!
//! `bps` 0 ise fee 0 — test/dryrun veya fee'siz market'lerde geçerli.
pub fn polymarket_taker_fee(price: f64, size: f64, bps: u32) -> f64 {
    if bps == 0 || size <= 0.0 {
        return 0.0;
    }
    let p = price.clamp(0.0, 1.0);
    size * p * (1.0 - p) * (bps as f64) / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::polymarket_taker_fee;

    #[test]
    fn zero_bps_zero_fee() {
        assert_eq!(polymarket_taker_fee(0.5, 100.0, 0), 0.0);
    }

    #[test]
    fn symmetric_around_half() {
        let up = polymarket_taker_fee(0.4, 10.0, 30);
        let down = polymarket_taker_fee(0.6, 10.0, 30);
        assert!((up - down).abs() < 1e-9);
    }

    #[test]
    fn classic_30bps_at_half() {
        let fee = polymarket_taker_fee(0.5, 100.0, 30);
        assert!((fee - (100.0 * 0.25 * 30.0 / 10_000.0)).abs() < 1e-9);
    }
}
