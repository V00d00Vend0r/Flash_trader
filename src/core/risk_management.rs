//! Risk management system for Ravenslinger trading bot.
//!
//! Provides simple but effective safety checks before trades are executed.
//! Future versions can extend this with adaptive logic (position sizing
//! based on volatility, dynamic drawdown limits, etc.).

use crate::core::wallet::Wallet;
use crate::decision::types::{Plan, Signal};
use serde::{Deserialize, Serialize};

/// Configuration for risk management parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_position_pct: f64,       // % of wallet per trade (0.0 - 1.0)
    pub max_total_exposure_pct: f64, // % of wallet across all trades (0.0 - 1.0)
    pub max_slippage_bps: u32,       // basis points (100 = 1%)
    pub min_confidence: f32,         // minimum required confidence
    pub emergency_stop: bool,        // hard off-switch
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_pct: 0.05,
            max_total_exposure_pct: 0.25,
            max_slippage_bps: 100,
            min_confidence: 0.6,
            emergency_stop: false,
        }
    }
}

/// Risk assessment result
#[derive(Debug, Clone)]
pub struct RiskAssessment {
    pub approved: bool,
    pub reason: String,
    pub adjusted_qty: f64,
}

/// Risk manager checks trade plans against safety rules
#[derive(Debug)]
pub struct RiskManager {
    config: RiskConfig,
}

impl RiskManager {
    pub fn new() -> Self {
        Self {
            config: RiskConfig::default(),
        }
    }

    pub fn with_config(config: RiskConfig) -> Self {
        Self { config }
    }

    /// Main risk gate for a proposed plan
    pub fn assess(&self, plan: &Plan, signal: &Signal, wallet: &Wallet) -> RiskAssessment {
        // Emergency stop
        if self.config.emergency_stop {
            return RiskAssessment {
                approved: false,
                reason: "Emergency stop active".into(),
                adjusted_qty: 0.0,
            };
        }

        // Confidence check
        if plan.confidence < self.config.min_confidence {
            return RiskAssessment {
                approved: false,
                reason: format!(
                    "Confidence {} below minimum {}",
                    plan.confidence, self.config.min_confidence
                ),
                adjusted_qty: 0.0,
            };
        }

        // Max slippage
        if plan.max_slippage_bps > self.config.max_slippage_bps {
            return RiskAssessment {
                approved: false,
                reason: format!(
                    "Slippage {}bps exceeds max {}bps",
                    plan.max_slippage_bps, self.config.max_slippage_bps
                ),
                adjusted_qty: 0.0,
            };
        }

        // Wallet balance sanity
        let total_balance: f64 = wallet.list_balances().iter().map(|(_, b)| b).sum();
        if total_balance <= 0.0 {
            return RiskAssessment {
                approved: false,
                reason: "Wallet empty".into(),
                adjusted_qty: 0.0,
            };
        }

        // Position sizing
        let est_value = plan.qty * plan.limit_px.unwrap_or(signal.mid.max(1.0));
        let max_value = total_balance * self.config.max_position_pct;
        let mut final_qty = plan.qty;

        if est_value > max_value {
            final_qty = max_value / plan.limit_px.unwrap_or(signal.mid.max(1.0));
        }

        // Total exposure check (simplified: single trade cap)
        let new_exposure_pct = est_value / total_balance;
        if new_exposure_pct > self.config.max_total_exposure_pct {
            return RiskAssessment {
                approved: false,
                reason: "Exposure limit exceeded".into(),
                adjusted_qty: 0.0,
            };
        }

        RiskAssessment {
            approved: true,
            reason: "All checks passed".into(),
            adjusted_qty: final_qty,
        }
    }
}

/// Legacy simple gate for compatibility
pub fn risk_gate(value: f64) -> bool {
    value.abs() <= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::types::{Action, Plan, Signal};

    #[test]
    fn approves_safe_trade() {
        let rm = RiskManager::new();
        let mut wallet = Wallet::new();
        wallet.deposit("USDC", 1000.0);

        let plan = Plan {
            action: Action::Buy,
            qty: 10.0,
            limit_px: Some(1.0),
            max_slippage_bps: 50,
            ttl_ms: 1000,
            confidence: 0.9,
        };

        let signal = Signal {
            ts_ms: 0,
            symbol: "SUI-USDC".into(),
            mid: 1.0,
            bid: 0.99,
            ask: 1.01,
            features: serde_json::Map::new(),
            meta: serde_json::Map::new(),
        };

        let result = rm.assess(&plan, &signal, &wallet);
        assert!(result.approved);
        assert!(result.adjusted_qty > 0.0);
    }
}