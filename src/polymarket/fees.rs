/// V2 protocol fee formülü (resmi `trading/fees.md`):
///   `fee = C × feeRate × p × (1-p)`
/// Maker hiçbir markette ücret ödemez (`taker_only` daima true).
pub struct FeeParams {
    pub rate: f64,
    pub taker_only: bool,
}

pub fn fee_for_role(price: f64, size: f64, params: &FeeParams, is_taker: bool) -> f64 {
    if params.rate <= 0.0 || size <= 0.0 {
        return 0.0;
    }
    if params.taker_only && !is_taker {
        return 0.0;
    }
    let p = price.clamp(0.0, 1.0);
    size * params.rate * p * (1.0 - p)
}
