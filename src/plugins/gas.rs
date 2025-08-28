//! Gas estimation strategies for transaction cost prediction.
//!
//! This module provides pluggable gas estimation strategies that can be used
//! to predict transaction costs before execution.

use std::collections::HashMap;

/// Trait for gas estimation strategies
pub trait GasEstimator {
    /// Estimate gas cost for a transaction
    fn estimate_gas(&self, transaction_data: &TransactionData) -> Result<GasEstimate, GasError>;
    
    /// Get current gas price from network
    fn get_gas_price(&self) -> Result<u64, GasError>;
    
    /// Calculate total transaction cost (gas * price)
    fn calculate_cost(&self, gas_estimate: &GasEstimate) -> Result<u64, GasError> {
        let gas_price = self.get_gas_price()?;
        Ok(gas_estimate.gas_limit * gas_price)
    }
}

/// Transaction data for gas estimation
#[derive(Debug, Clone)]
pub struct TransactionData {
    pub to: String,
    pub value: u64,
    pub data: Vec<u8>,
    pub transaction_type: TransactionType,
}

/// Types of transactions that affect gas estimation
#[derive(Debug, Clone)]
pub enum TransactionType {
    Transfer,
    SwapExactIn,
    SwapExactOut,
    AddLiquidity,
    RemoveLiquidity,
    Custom(String),
}

/// Gas estimation result
#[derive(Debug, Clone)]
pub struct GasEstimate {
    pub gas_limit: u64,
    pub confidence: f64, // 0.0 to 1.0
    pub buffer_percentage: f64,
}

/// Gas estimation errors
#[derive(Debug, thiserror::Error)]
pub enum GasError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Invalid transaction data: {0}")]
    InvalidData(String),
    #[error("Estimation failed: {0}")]
    EstimationFailed(String),
}

/// Conservative gas estimator with safety buffers
pub struct ConservativeGasEstimator {
    base_gas_costs: HashMap<TransactionType, u64>,
    safety_buffer: f64,
}

impl ConservativeGasEstimator {
    pub fn new() -> Self {
        let mut base_gas_costs = HashMap::new();
        base_gas_costs.insert(TransactionType::Transfer, 21_000);
        base_gas_costs.insert(TransactionType::SwapExactIn, 150_000);
        base_gas_costs.insert(TransactionType::SwapExactOut, 160_000);
        base_gas_costs.insert(TransactionType::AddLiquidity, 200_000);
        base_gas_costs.insert(TransactionType::RemoveLiquidity, 180_000);
        
        Self {
            base_gas_costs,
            safety_buffer: 0.2, // 20% buffer
        }
    }
}

impl GasEstimator for ConservativeGasEstimator {
    fn estimate_gas(&self, transaction_data: &TransactionData) -> Result<GasEstimate, GasError> {
        let base_gas = self.base_gas_costs
            .get(&transaction_data.transaction_type)
            .copied()
            .unwrap_or(100_000); // Default fallback
        
        // Add complexity factor based on data size
        let data_complexity = (transaction_data.data.len() as u64) * 16;
        let estimated_gas = base_gas + data_complexity;
        
        // Apply safety buffer
        let gas_with_buffer = (estimated_gas as f64 * (1.0 + self.safety_buffer)) as u64;
        
        Ok(GasEstimate {
            gas_limit: gas_with_buffer,
            confidence: 0.85,
            buffer_percentage: self.safety_buffer,
        })
    }
    
    fn get_gas_price(&self) -> Result<u64, GasError> {
        // In a real implementation, this would query the network
        // For now, return a reasonable default (20 gwei in wei)
        Ok(20_000_000_000)
    }
}

/// Aggressive gas estimator for faster execution
pub struct AggressiveGasEstimator {
    price_multiplier: f64,
}

impl AggressiveGasEstimator {
    pub fn new() -> Self {
        Self {
            price_multiplier: 1.5, // 50% higher gas price for speed
        }
    }
}

impl GasEstimator for AggressiveGasEstimator {
    fn estimate_gas(&self, transaction_data: &TransactionData) -> Result<GasEstimate, GasError> {
        // Use minimal gas estimates for speed
        let base_gas = match transaction_data.transaction_type {
            TransactionType::Transfer => 21_000,
            TransactionType::SwapExactIn => 120_000,
            TransactionType::SwapExactOut => 130_000,
            TransactionType::AddLiquidity => 170_000,
            TransactionType::RemoveLiquidity => 150_000,
            TransactionType::Custom(_) => 80_000,
        };
        
        Ok(GasEstimate {
            gas_limit: base_gas,
            confidence: 0.7,
            buffer_percentage: 0.05, // Minimal 5% buffer
        })
    }
    
    fn get_gas_price(&self) -> Result<u64, GasError> {
        let base_price = 20_000_000_000; // 20 gwei
        Ok((base_price as f64 * self.price_multiplier) as u64)
    }
}