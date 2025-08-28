//! ROI (Return on Investment) calculation strategies.
//!
//! This module provides pluggable ROI calculation strategies to evaluate
//! trading performance.

use std::time::Duration;

/// Trait for ROI calculators
pub trait RoiCalculator {
    fn calculate_roi(&self, record: &TradeRecord) -> Result<RoiResult, RoiError>;
    fn annualize(&self, roi: &RoiResult) -> f64;
}

/// Trade record for ROI
#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub capital_invested: f64,
    pub capital_returned: f64,
    pub duration: Duration,
    pub fees_paid: f64,
    pub gas_cost: f64,
}

/// ROI result
#[derive(Debug, Clone)]
pub struct RoiResult {
    pub absolute_return: f64,   // Net profit/loss
    pub percent_return: f64,    // % ROI
    pub annualized_return: f64, // annualized %
    pub duration: Duration,
}

/// ROI errors
#[derive(Debug, thiserror::Error)]
pub enum RoiError {
    #[error("Invalid ROI calculation: {0}")]
    Invalid(String),
    #[error("Math error: {0}")]
    MathError(String),
}

/// Simple ROI calculator using basic formula
pub struct SimpleRoiCalculator;

impl RoiCalculator for SimpleRoiCalculator {
    fn calculate_roi(&self, record: &TradeRecord) -> Result<RoiResult, RoiError> {
        if record.capital_invested <= 0.0 {
            return Err(RoiError::Invalid("Capital invested must be > 0".into()));
        }
        
        let net_return = record.capital_returned - record.capital_invested - record.fees_paid - record.gas_cost;
        let percent_return = net_return / record.capital_invested;
        let duration = record.duration;
        
        let annualized_return = self.annualize(&RoiResult {
            absolute_return: net_return,
            percent_return,
            annualized_return: 0.0,
            duration,
        });
        
        Ok(RoiResult {
            absolute_return: net_return,
            percent_return,
            annualized_return,
            duration,
        })
    }
    
    fn annualize(&self, roi: &RoiResult) -> f64 {
        let seconds = roi.duration.as_secs_f64();
        if seconds <= 0.0 {
            return 0.0;
        }
        let years = seconds / (365.25 * 24.0 * 60.0 * 60.0);
        (1.0 + roi.percent_return).powf(1.0 / years) - 1.0
    }
}

/// Risk-adjusted ROI calculator using Sharpe-like ratio
pub struct RiskAdjustedRoiCalculator {
    pub risk_free_rate: f64,
    pub volatility: f64,
}

impl RoiCalculator for RiskAdjustedRoiCalculator {
    fn calculate_roi(&self, record: &TradeRecord) -> Result<RoiResult, RoiError> {
        let base = SimpleRoiCalculator.calculate_roi(record)?;
        
        // Adjust ROI by subtracting risk-free return and dividing by volatility
        if self.volatility <= 0.0 {
            return Ok(base);
        }
        
        let excess_return = base.percent_return - self.risk_free_rate;
        let adjusted = excess_return / self.volatility;
        
        Ok(RoiResult {
            absolute_return: base.absolute_return,
            percent_return: adjusted,
            annualized_return: adjusted, // For simplicity
            duration: record.duration,
        })
    }
    
    fn annualize(&self, roi: &RoiResult) -> f64 {
        roi.annualized_return
    }
}