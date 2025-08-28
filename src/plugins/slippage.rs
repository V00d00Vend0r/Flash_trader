//! Slippage modeling strategies for trade execution.
//!
//! This module provides pluggable slippage calculation strategies to predict
//! and manage price impact during trades.

use std::collections::HashMap;

/// Trait for slippage calculation strategies
pub trait SlippageCalculator {
    /// Calculate expected slippage for a trade
    fn calculate_slippage(&self, trade_params: &TradeParams) -> Result<SlippageEstimate, SlippageError>;
    
    /// Get minimum acceptable output considering slippage
    fn get_minimum_output(&self, trade_params: &TradeParams, tolerance: f64) -> Result<u64, SlippageError>;
    
    /// Check if a trade is within acceptable slippage bounds
    fn is_acceptable_slippage(&self, expected: u64, actual: u64, tolerance: f64) -> bool {
        let slippage = ((expected as f64 - actual as f64) / expected as f64).abs();
        slippage <= tolerance
    }
}

/// Parameters for slippage calculation
#[derive(Debug, Clone)]
pub struct TradeParams {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: u64,
    pub pool_liquidity: u64,
    pub pool_fee: f64,
    pub trade_size_ratio: f64, // trade size / pool size
}

/// Slippage estimation result
#[derive(Debug, Clone)]
pub struct SlippageEstimate {
    pub expected_slippage: f64, // As percentage (0.01 = 1%)
    pub price_impact: f64,
    pub confidence: f64,
    pub recommended_tolerance: f64,
}

/// Slippage calculation errors
#[derive(Debug, thiserror::Error)]
pub enum SlippageError {
    #[error("Invalid trade parameters: {0}")]
    InvalidParams(String),
    #[error("Insufficient liquidity")]
    InsufficientLiquidity,
    #[error("Calculation error: {0}")]
    CalculationError(String),
}

/// Conservative slippage calculator with safety margins
pub struct ConservativeSlippageCalculator {
    base_slippage_rates: HashMap<String, f64>,
    safety_multiplier: f64,
}

impl ConservativeSlippageCalculator {
    pub fn new() -> Self {
        let mut base_slippage_rates = HashMap::new();
        base_slippage_rates.insert("STABLE".to_string(), 0.001); // 0.1% for stablecoins
        base_slippage_rates.insert("MAJOR".to_string(), 0.005);  // 0.5% for major tokens
        base_slippage_rates.insert("MINOR".to_string(), 0.02);   // 2% for minor tokens
        
        Self {
            base_slippage_rates,
            safety_multiplier: 2.0, // Double the estimated slippage for safety
        }
    }
    
    fn classify_token_pair(&self, token_in: &str, token_out: &str) -> String {
        // Simplified classification - in reality, this would check against known token lists
        let major_tokens = ["USDC", "USDT", "ETH", "BTC", "SUI"];
        
        if major_tokens.contains(&token_in) && major_tokens.contains(&token_out) {
            if token_in.starts_with("USD") || token_out.starts_with("USD") {
                "STABLE".to_string()
            } else {
                "MAJOR".to_string()
            }
        } else {
            "MINOR".to_string()
        }
    }
}

impl SlippageCalculator for ConservativeSlippageCalculator {
    fn calculate_slippage(&self, trade_params: &TradeParams) -> Result<SlippageEstimate, SlippageError> {
        if trade_params.amount_in == 0 || trade_params.pool_liquidity == 0 {
            return Err(SlippageError::InvalidParams("Zero amount or liquidity".to_string()));
        }
        
        let token_class = self.classify_token_pair(&trade_params.token_in, &trade_params.token_out);
        let base_slippage = self.base_slippage_rates
            .get(&token_class)
            .copied()
            .unwrap_or(0.03); // 3% default for unknown pairs
        
        // Calculate price impact based on trade size
        let size_impact = trade_params.trade_size_ratio.powi(2) * 0.1; // Quadratic impact
        let fee_impact = trade_params.pool_fee;
        
        let total_slippage = (base_slippage + size_impact + fee_impact) * self.safety_multiplier;
        
        Ok(SlippageEstimate {
            expected_slippage: total_slippage,
            price_impact: size_impact,
            confidence: 0.8,
            recommended_tolerance: total_slippage * 1.2, // 20% buffer on top
        })
    }
    
    fn get_minimum_output(&self, trade_params: &TradeParams, tolerance: f64) -> Result<u64, SlippageError> {
        let slippage_estimate = self.calculate_slippage(trade_params)?;
        let effective_slippage = slippage_estimate.expected_slippage.max(tolerance);
        
        // Simplified calculation - in reality, this would use AMM formulas
        let expected_output = trade_params.amount_in; // Placeholder
        let minimum_output = (expected_output as f64 * (1.0 - effective_slippage)) as u64;
        
        Ok(minimum_output)
    }
}

/// Aggressive slippage calculator for faster execution
pub struct AggressiveSlippageCalculator {
    risk_tolerance: f64,
}

impl AggressiveSlippageCalculator {
    pub fn new() -> Self {
        Self {
            risk_tolerance: 0.05, // Accept up to 5% slippage for speed
        }
    }
}

impl SlippageCalculator for AggressiveSlippageCalculator {
    fn calculate_slippage(&self, trade_params: &TradeParams) -> Result<SlippageEstimate, SlippageError> {
        // Minimal slippage calculation for speed
        let base_slippage = 0.002; // 0.2% base
        let size_impact = trade_params.trade_size_ratio * 0.01;
        
        let total_slippage = base_slippage + size_impact;
        
        Ok(SlippageEstimate {
            expected_slippage: total_slippage,
            price_impact: size_impact,
            confidence: 0.6,
            recommended_tolerance: self.risk_tolerance,
        })
    }
    
    fn get_minimum_output(&self, trade_params: &TradeParams, tolerance: f64) -> Result<u64, SlippageError> {
        let effective_tolerance = tolerance.min(self.risk_tolerance);
        let expected_output = trade_params.amount_in; // Placeholder
        let minimum_output = (expected_output as f64 * (1.0 - effective_tolerance)) as u64;
        
        Ok(minimum_output)
    }
}