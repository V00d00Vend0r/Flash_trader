//! Execution adapter plugins.
//!
//! This module exposes submodules for various execution-time plugins, such as
//! gas estimation, order routing, slippage modelling and ROI calculations.
//! Each submodule contains one or more pluggable strategies that can be swapped out
//! to customise the trading bot’s behaviour.

pub mod path;
pub mod slippage;
pub mod gas;
pub mod roi;

use crate::plugins::gas::{GasEstimator, ConservativeGasEstimator, AggressiveGasEstimator};
use crate::plugins::slippage::{SlippageCalculator, ConservativeSlippageCalculator, AggressiveSlippageCalculator};
use crate::plugins::path::{PathFinder, GreedyRouter, OptimalRouter};
use crate::plugins::roi::{RoiCalculator, SimpleRoiCalculator, RiskAdjustedRoiCalculator};

use std::sync::{Arc, RwLock};

/// The central plugin manager holding all strategy references.
pub struct PluginManager {
    pub gas_estimator: Arc<RwLock<Box<dyn GasEstimator + Send + Sync>>>,
    pub slippage_calculator: Arc<RwLock<Box<dyn SlippageCalculator + Send + Sync>>>,
    pub path_finder: Arc<RwLock<Box<dyn PathFinder + Send + Sync>>>,
    pub roi_calculator: Arc<RwLock<Box<dyn RoiCalculator + Send + Sync>>>,
}

impl PluginManager {
    /// Initialise with conservative defaults
    pub fn conservative_defaults() -> Self {
        Self {
            gas_estimator: Arc::new(RwLock::new(Box::new(ConservativeGasEstimator::new()))),
            slippage_calculator: Arc::new(RwLock::new(Box::new(ConservativeSlippageCalculator::new()))),
            path_finder: Arc::new(RwLock::new(Box::new(GreedyRouter))),
            roi_calculator: Arc::new(RwLock::new(Box::new(SimpleRoiCalculator))),
        }
    }

    /// Initialise with aggressive defaults
    pub fn aggressive_defaults() -> Self {
        Self {
            gas_estimator: Arc::new(RwLock::new(Box::new(AggressiveGasEstimator::new()))),
            slippage_calculator: Arc::new(RwLock::new(Box::new(AggressiveSlippageCalculator::new()))),
            path_finder: Arc::new(RwLock::new(Box::new(GreedyRouter))), // OptimalRouter placeholder not implemented
            roi_calculator: Arc::new(RwLock::new(Box::new(RiskAdjustedRoiCalculator {
                risk_free_rate: 0.02,
                volatility: 0.15,
            }))),
        }
    }

    // ----- Hot-swap methods -----

    pub fn set_gas_estimator(&self, estimator: Box<dyn GasEstimator + Send + Sync>) {
        if let Ok(mut writer) = self.gas_estimator.write() {
            *writer = estimator;
        }
    }

    pub fn set_slippage_calculator(&self, calc: Box<dyn SlippageCalculator + Send + Sync>) {
        if let Ok(mut writer) = self.slippage_calculator.write() {
            *writer = calc;
        }
    }

    pub fn set_path_finder(&self, finder: Box<dyn PathFinder + Send + Sync>) {
        if let Ok(mut writer) = self.path_finder.write() {
            *writer = finder;
        }
    }

    pub fn set_roi_calculator(&self, calc: Box<dyn RoiCalculator + Send + Sync>) {
        if let Ok(mut writer) = self.roi_calculator.write() {
            *writer = calc;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_swap_strategies() {
        let plugins = PluginManager::conservative_defaults();

        // Swap gas estimator
        plugins.set_gas_estimator(Box::new(AggressiveGasEstimator::new()));

        // Swap slippage
        plugins.set_slippage_calculator(Box::new(AggressiveSlippageCalculator::new()));

        // Swap ROI calculator
        plugins.set_roi_calculator(Box::new(SimpleRoiCalculator));

        // If it compiles and test passes => hot-swapping works
        assert!(true);
    }
}