//! Core data types for the decision engine.
//!
//! These structures describe the inputs, intermediate results and
//! outputs of the decision pipeline. They are serialisable via
//! [`serde`] for interoperability with external components.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A snapshot of market data and custom features used as input to
/// decision stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Epoch timestamp in milliseconds when the signal was generated.
    pub ts_ms: u64,
    /// Trading symbol (e.g. "BTC/USD").
    pub symbol: String,
    /// Mid‑price of the symbol at the snapshot time.
    pub mid: f64,
    /// Best bid price.
    pub bid: f64,
    /// Best ask price.
    pub ask: f64,
    /// Arbitrary feature map; keys and values are model specific.
    pub features: Map<String, Value>,
    /// Additional metadata (e.g. context, source, etc.).
    pub meta: Map<String, Value>,
}

/// Actions that a stage can recommend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Action {
    /// Enter a long position.
    Buy,
    /// Enter a short position.
    Sell,
    /// Maintain the current position.
    Hold,
}

/// Decision returned by a stage containing the recommended action,
/// a score and a confidence level along with optional notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Recommended action.
    pub action: Action,
    /// A continuous score in [-1.0, 1.0] representing the strength of the signal.
    pub score: f32,
    /// Confidence in the decision (0.0–1.0). Multiple stages can
    /// combine confidences via minimum or other strategies.
    pub confidence: f32,
    /// Arbitrary JSON payload with additional details about how the
    /// decision was derived.
    pub notes: serde_json::Value,
}

/// A plan describing how to execute a decision on the market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Final action to execute.
    pub action: Action,
    /// Quantity to buy or sell.
    pub qty: f64,
    /// Optional limit price. If `None`, a market order may be used.
    pub limit_px: Option<f64>,
    /// Maximum slippage tolerated in basis points.
    pub max_slippage_bps: u32,
    /// Time to live in milliseconds for the order (e.g. IOC or FOK durations).
    pub ttl_ms: u64,
    /// Aggregated confidence across all decision stages.
    pub confidence: f32,
}

impl Plan {
    /// Create a new plan with a mandatory action and quantity. Other fields
    /// default to sensible values: `limit_px` is `None`, `max_slippage_bps`
    /// is 30, `ttl_ms` is 1,500 and `confidence` is 1.0.
    pub fn new(action: Action, qty: f64) -> Self {
        Self {
            action,
            qty,
            limit_px: None,
            max_slippage_bps: 30,
            ttl_ms: 1_500,
            confidence: 1.0,
        }
    }
}