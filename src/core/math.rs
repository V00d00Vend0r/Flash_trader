//! Mathematical utilities for trading calculations.
//!
//! This module provides essential financial math functions for:
//! - Price calculations and conversions
//! - Slippage and fee computations
//! - PnL and risk metrics
//! - CPMM (Constant Product Market Maker) formulas
//! - Basis point conversions

use std::f64::consts::E;

/// Clamp a value to the range [0.0, 1.0]
pub fn clamp01(x: f32) -> f32 {
    x.max(0.0).min(1.0)
}

/// Clamp a value to the range [0.0, 1.0] for f64
pub fn clamp01_f64(x: f64) -> f64 {
    x.max(0.0).min(1.0)
}

/// Convert basis points to decimal (e.g., 100 bps = 0.01)
pub fn bps_to_decimal(bps: u32) -> f64 {
    bps as f64 / 10_000.0
}

/// Convert decimal to basis points (e.g., 0.01 = 100 bps)
pub fn decimal_to_bps(decimal: f64) -> u32 {
    (decimal * 10_000.0).round() as u32
}

/// Calculate percentage change between two values
pub fn pct_change(old_value: f64, new_value: f64) -> f64 {
    if old_value == 0.0 {
        return 0.0;
    }
    (new_value - old_value) / old_value
}

/// Calculate bid-ask spread in basis points
pub fn spread_bps(bid: f64, ask: f64) -> u32 {
    if bid <= 0.0 || ask <= 0.0 || ask <= bid {
        return 0;
    }
    let mid = (bid + ask) / 2.0;
    decimal_to_bps((ask - bid) / mid)
}

/// Calculate mid price from bid and ask
pub fn mid_price(bid: f64, ask: f64) -> f64 {
    if bid <= 0.0 || ask <= 0.0 {
        return 0.0;
    }
    (bid + ask) / 2.0
}

/// CPMM: Calculate output amount for exact input swap
/// Formula: dy = (r_out * dx * fee) / (r_in + dx * fee)
pub fn cpmm_out_for_in(r_in: f64, r_out: f64, dx: f64, fee_rate: f64) -> f64 {
    if r_in <= 0.0 || r_out <= 0.0 || dx <= 0.0 || fee_rate < 0.0 || fee_rate > 1.0 {
        return 0.0;
    }
    let fee_multiplier = 1.0 - fee_rate;
    let dx_after_fee = dx * fee_multiplier;
    (r_out * dx_after_fee) / (r_in + dx_after_fee)
}

/// CPMM: Calculate input amount needed for exact output swap
/// Formula: dx = (r_in * dy) / ((r_out - dy) * fee)
pub fn cpmm_in_for_out(r_in: f64, r_out: f64, dy: f64, fee_rate: f64) -> f64 {
    if r_in <= 0.0 || r_out <= 0.0 || dy <= 0.0 || dy >= r_out || fee_rate < 0.0 || fee_rate > 1.0 {
        return 0.0;
    }
    let fee_multiplier = 1.0 - fee_rate;
    (r_in * dy) / ((r_out - dy) * fee_multiplier)
}

/// Calculate price impact for a trade in basis points
pub fn price_impact_bps(r_in: f64, r_out: f64, dx: f64, fee_rate: f64) -> u32 {
    if r_in <= 0.0 || r_out <= 0.0 || dx <= 0.0 {
        return 0;
    }
    
    let price_before = r_out / r_in;
    let dy = cpmm_out_for_in(r_in, r_out, dx, fee_rate);
    
    if dy <= 0.0 {
        return 0;
    }
    
    let effective_price = dy / dx;
    let impact = (price_before - effective_price) / price_before;
    decimal_to_bps(impact.abs())
}

/// Calculate slippage between expected and actual price
pub fn slippage_bps(expected_price: f64, actual_price: f64) -> u32 {
    if expected_price <= 0.0 {
        return 0;
    }
    let slippage = (expected_price - actual_price) / expected_price;
    decimal_to_bps(slippage.abs())
}

/// Calculate PnL for a position
pub fn calculate_pnl(entry_price: f64, current_price: f64, quantity: f64, is_long: bool) -> f64 {
    if is_long {
        (current_price - entry_price) * quantity
    } else {
        (entry_price - current_price) * quantity
    }
}

/// Calculate position value at current market price
pub fn position_value(price: f64, quantity: f64) -> f64 {
    price * quantity.abs()
}

/// Calculate required margin for a position (simplified)
pub fn required_margin(position_value: f64, margin_rate: f64) -> f64 {
    position_value * margin_rate.max(0.0).min(1.0)
}

/// Calculate compound interest
pub fn compound_interest(principal: f64, rate: f64, periods: f64) -> f64 {
    if rate <= -1.0 {
        return 0.0;
    }
    principal * (1.0 + rate).powf(periods)
}

/// Calculate exponential moving average
pub fn ema(current_ema: f64, new_value: f64, alpha: f64) -> f64 {
    let alpha_clamped = clamp01_f64(alpha);
    alpha_clamped * new_value + (1.0 - alpha_clamped) * current_ema
}

/// Calculate simple moving average from a slice of values
pub fn sma(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Calculate standard deviation
pub fn std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    
    let mean = sma(values);
    let variance = values.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / (values.len() - 1) as f64;
    
    variance.sqrt()
}

/// Calculate Sharpe ratio (simplified)
pub fn sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }
    
    let mean_return = sma(returns);
    let std_return = std_dev(returns);
    
    if std_return == 0.0 {
        return 0.0;
    }
    
    (mean_return - risk_free_rate) / std_return
}

/// Calculate maximum drawdown from a series of values
pub fn max_drawdown(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    
    let mut peak = values[0];
    let mut max_dd = 0.0;
    
    for &value in values.iter().skip(1) {
        if value > peak {
            peak = value;
        } else {
            let drawdown = (peak - value) / peak;
            max_dd = max_dd.max(drawdown);
        }
    }
    
    max_dd
}

/// Safe division that returns 0.0 if denominator is zero
pub fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

/// Round to specified decimal places
pub fn round_to_decimals(value: f64, decimals: u32) -> f64 {
    let multiplier = 10_f64.powi(decimals as i32);
    (value * multiplier).round() / multiplier
}

/// Check if two floating point numbers are approximately equal
pub fn approx_eq(a: f64, b: f64, epsilon: f64) -> bool {
    (a - b).abs() < epsilon
}

/// Calculate Kelly criterion for position sizing
pub fn kelly_criterion(win_prob: f64, win_loss_ratio: f64) -> f64 {
    if win_prob <= 0.0 || win_prob >= 1.0 || win_loss_ratio <= 0.0 {
        return 0.0;
    }
    
    let lose_prob = 1.0 - win_prob;
    let fraction = (win_prob * win_loss_ratio - lose_prob) / win_loss_ratio;
    
    clamp01_f64(fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp01() {
        assert_eq!(clamp01(-0.5), 0.0);
        assert_eq!(clamp01(0.5), 0.5);
        assert_eq!(clamp01(1.5), 1.0);
    }

    #[test]
    fn test_bps_conversion() {
        assert_eq!(bps_to_decimal(100), 0.01);
        assert_eq!(decimal_to_bps(0.01), 100);
        assert_eq!(bps_to_decimal(1), 0.0001);
    }

    #[test]
    fn test_cpmm_calculations() {
        // Test basic CPMM swap: 1000/1000 pool, swap 100 in, 3% fee
        let r_in = 1000.0;
        let r_out = 1000.0;
        let dx = 100.0;
        let fee = 0.03;
        
        let dy = cpmm_out_for_in(r_in, r_out, dx, fee);
        assert!(dy > 0.0);
        assert!(dy < 100.0); // Should get less than 100 due to slippage + fees
        
        // Test reverse calculation
        let dx_calc = cpmm_in_for_out(r_in, r_out, dy, fee);
        assert!(approx_eq(dx, dx_calc, 0.001));
    }

    #[test]
    fn test_pnl_calculation() {
        // Long position: bought at 100, now at 110
        let pnl_long = calculate_pnl(100.0, 110.0, 10.0, true);
        assert_eq!(pnl_long, 100.0); // 10 * (110 - 100)
        
        // Short position: sold at 100, now at 90
        let pnl_short = calculate_pnl(100.0, 90.0, 10.0, false);
        assert_eq!(pnl_short, 100.0); // 10 * (100 - 90)
    }

    #[test]
    fn test_spread_calculation() {
        let spread = spread_bps(99.0, 101.0);
        assert_eq!(spread, 200); // 2% spread = 200 bps
    }

    #[test]
    fn test_kelly_criterion() {
        // 60% win rate, 2:1 win/loss ratio
        let kelly = kelly_criterion(0.6, 2.0);
        assert!(kelly > 0.0);
        assert!(kelly <= 1.0);
        
        // Bad odds should return 0
        let bad_kelly = kelly_criterion(0.4, 1.0);
        assert_eq!(bad_kelly, 0.0);
    }
}