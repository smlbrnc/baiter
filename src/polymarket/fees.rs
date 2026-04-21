pub fn fee_for_role(price: f64, size: f64, bps: u32, is_taker: bool) -> f64 {
    if !is_taker || bps == 0 || size <= 0.0 {
        return 0.0;
    }
    let p = price.clamp(0.0, 1.0);
    size * p * (1.0 - p) * (bps as f64) / 10_000.0
}
