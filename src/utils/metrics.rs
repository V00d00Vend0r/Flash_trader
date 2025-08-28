//! Collection of simple statistical utility functions.
//!
//! This module provides commonly used metrics for analysing trading and
//! time-series data. These functions intentionally avoid any heavy
//! dependencies and operate purely on slices of `f64`. All functions
//! return `None` if the input slice does not contain enough data to
//! produce a meaningful result.
 
/// Compute the arithmetic mean of a slice of numbers. Returns `None` if
/// the slice is empty.
pub fn mean(data: &[f64]) -> Option<f64> {
    if data.is_empty() { return None; }
    let sum: f64 = data.iter().sum();
    Some(sum / data.len() as f64)
}
 
/// Compute the variance of a slice of numbers. This uses the sample
/// variance formula (dividing by `n - 1`) when `n > 1`. Returns
/// `None` for `n == 0` or `n == 1`.
pub fn variance(data: &[f64]) -> Option<f64> {
    if data.len() < 2 { return None; }
    let m = mean(data)?;
    let mut sum_sq = 0.0;
    for &val in data {
        let diff = val - m;
        sum_sq += diff * diff;
    }
    Some(sum_sq / ((data.len() - 1) as f64))
}
 
/// Compute the standard deviation of a slice of numbers. Uses the
/// sample standard deviation formula. Returns `None` for `n < 2`.
pub fn std_dev(data: &[f64]) -> Option<f64> {
    variance(data).map(|v| v.sqrt())
}
 
/// Compute the exponential moving average (EMA) of the input data with
/// the specified smoothing factor `alpha`. Returns a vector of the
/// same length as `data`, where each element is the EMA up to that
/// index. If the input is empty, an empty vector is returned.
pub fn ema(data: &[f64], alpha: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(data.len());
    let mut prev = None;
    // clamp alpha into (0, 1]; fall back to 1.0 if invalid
    let a = if alpha <= 0.0 || !alpha.is_finite() { 1.0 } else if alpha > 1.0 { 1.0 } else { alpha };
    for &val in data {
        let ema_val = match prev {
            Some(p) => a * val + (1.0 - a) * p,
            None => val,
        };
        out.push(ema_val);
        prev = Some(ema_val);
    }
    out
}
 
/// Convenience: EMA from a period `n` (alpha = 2/(n+1)). If `n < 1`,
/// returns an empty vector.
pub fn ema_from_period(data: &[f64], n: usize) -> Vec<f64> {
    if n < 1 { return Vec::new(); }
    let alpha = 2.0 / ((n as f64) + 1.0);
    ema(data, alpha)
}
 
/// Compute the Sharpe ratio of a series of returns. Uses the sample
/// mean and standard deviation and annualises assuming 252 trading
/// periods per year. Returns `None` if there are fewer than two
/// observations or if the standard deviation is zero.
pub fn sharpe_ratio(returns: &[f64]) -> Option<f64> {
    if returns.len() < 2 { return None; }
    let m = mean(returns)?;
    let sd = std_dev(returns)?;
    if sd == 0.0 { return None; }
    let sqrt_n = (252.0_f64).sqrt();
    Some((m / sd) * sqrt_n)
}
 
/// Sharpe ratio with explicit risk-free rate per period and custom
/// annualisation base. Typically `periods_per_year` is 252 (daily) or 12 (monthly).
/// Returns `None` if fewer than 2 observations or stdev is zero.
pub fn sharpe_ratio_rf(returns: &[f64], risk_free_per_period: f64, periods_per_year: f64) -> Option<f64> {
    if returns.len() < 2 || periods_per_year <= 0.0 { return None; }
    let excess: Vec<f64> = returns.iter().map(|&r| r - risk_free_per_period).collect();
    let m = mean(&excess)?;
    let sd = std_dev(&excess)?;
    if sd == 0.0 { return None; }
    Some((m / sd) * periods_per_year.sqrt())
}
 
#[cfg(test)]
mod tests {
    use super::*;
 
    #[test]
    fn test_mean() {
        assert_eq!(mean(&[]), None);
        assert_eq!(mean(&[1.0, 2.0, 3.0]), Some(2.0));
    }
 
    #[test]
    fn test_variance_std() {
        let v = variance(&[1.0, 2.0, 3.0]).unwrap();
        // sample variance of 1,2,3 is 1.0
        assert!((v - 1.0).abs() < 1e-12);
        let s = std_dev(&[1.0, 2.0, 3.0]).unwrap();
        assert!((s - 1.0_f64.sqrt()).abs() < 1e-12);
    }
 
    #[test]
    fn test_ema() {
        let x = [1.0, 2.0, 3.0];
        let e = ema(&x, 0.5);
        assert_eq!(e.len(), 3);
        assert!((e[0] - 1.0).abs() < 1e-12);
    }
 
    #[test]
    fn test_sharpe() {
        let r = [0.01, 0.01, 0.01, 0.01];
        let sr = sharpe_ratio(&r).unwrap();
        assert!(sr.is_finite());
        let srf = sharpe_ratio_rf(&r, 0.0, 252.0).unwrap();
        assert!((sr - srf).abs() < 1e-9);
    }
}
