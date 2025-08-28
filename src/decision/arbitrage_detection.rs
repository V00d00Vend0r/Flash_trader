//! Arbitrage detection stage.
//!
//! This stage inspects a [`Signal`] and computes the
//! potential arbitrage edge between two venues expressed in basis
//! points (bps). It accounts for transaction costs and returns a
//! [`Decision`] containing an action (buy/sell/hold), a score
//! scaled to the range [-1.0, 1.0], and a confidence level. A
//! configurable minimum edge threshold controls when to trigger
//! trades.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use super::types::{Signal, Decision, Action};
use super::stage::DecisionStage;

/// Conversion factor from price difference fraction to basis points.
const BPS_FACTOR: f64 = 10_000.0;

/// A stage that identifies profitable arbitrage opportunities.
///
/// When the calculated net basis points (edge minus transaction
/// costs) exceeds `min_edge_bps`, a `Buy` or `Sell` action is
/// selected. Otherwise the stage advises holding the current
/// position. Implementations may choose a more sophisticated edge
/// computation by modifying `evaluate`.
#[derive(Debug)]
pub struct ArbitrageDetectionStage {
    /// Minimum net edge in basis points required to enter a trade.
    pub min_edge_bps: f64,
}

impl Default for ArbitrageDetectionStage {
    fn default() -> Self {
        Self { min_edge_bps: 5.0 }
    }
}

#[async_trait]
impl DecisionStage for ArbitrageDetectionStage {
    fn id(&self) -> &str {
        "arbitrage_detection"
    }

    async fn evaluate(&self, s: &Signal) -> Result<Decision> {
        // Helper closure to extract a floating point value from
        // arbitrary feature keys with a fallback.
        let get_f64 = |key: &str, default: f64| -> f64 {
            s.features
                .get(key)
                .and_then(|v| v.as_f64())
                .unwrap_or(default)
        };

        let (va_bid, vb_ask, tx_cost) = (
            get_f64("venueA_bid", 0.0),
            get_f64("venueB_ask", f64::MAX),
            get_f64("tx_cost_bps", 0.0),
        );

        // Compute the raw and net edge in basis points. Guard
        // against division by zero by returning zero when the ask is
        // non‑positive.
        let edge_bps = if vb_ask > 0.0 {
            (va_bid - vb_ask) / vb_ask * BPS_FACTOR
        } else {
            0.0
        };
        let net_bps = edge_bps - tx_cost;

        // Normalise the net edge into a [-1, 1] score by dividing by
        // 100 (so 100 bps maps to 1.0) and clamping.
        let score = (net_bps / 100.0).clamp(-1.0, 1.0) as f32;

        // Determine an action based on the net edge relative to the
        // configured threshold.
        let action = if net_bps > self.min_edge_bps {
            Action::Buy
        } else if net_bps < -self.min_edge_bps {
            Action::Sell
        } else {
            Action::Hold
        };

        // Confidence reflects how far the net edge is from zero.
        let confidence = (net_bps.abs() / 50.0).clamp(0.0, 1.0) as f32;

        Ok(Decision {
            action,
            score,
            confidence,
            notes: json!({
                "edge_bps": edge_bps,
                "net_bps": net_bps,
            }),
        })
    }
}