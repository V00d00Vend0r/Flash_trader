use serde_json::{Map, Value};
use crate::decision::types::{Action, Plan};

/// A wrapper around a JSON map that provides typed getters and setters.
///
/// `MetaMap` allows for convenient storage of heterogeneous metadata alongside
/// execution signals.  It exposes helper methods to retrieve and update
/// numeric and boolean values without manually handling JSON conversions.
#[derive(Debug, Clone, Default)]
pub struct MetaMap(pub Map<String, Value>);
impl MetaMap {
    /// Retrieve a floating point value by key, if present.
    pub fn get_f64(&self, k: &str) -> Option<f64> {
        self.0.get(k).and_then(|v| v.as_f64())
    }
    /// Retrieve an unsigned integer value by key, if present.
    pub fn get_u64(&self, k: &str) -> Option<u64> {
        self.0.get(k).and_then(|v| v.as_u64())
    }
    /// Retrieve a boolean value by key, if present.
    pub fn get_bool(&self, k: &str) -> Option<bool> {
        self.0.get(k).and_then(|v| v.as_bool())
    }
    /// Set a floating point value for the given key.
    pub fn set_f64(&mut self, k: &str, v: f64) {
        self.0.insert(k.to_string(), Value::from(v));
    }
    /// Set an unsigned integer value for the given key.
    pub fn set_u64(&mut self, k: &str, v: u64) {
        self.0.insert(k.to_string(), Value::from(v));
    }
    /// Set a boolean value for the given key.
    pub fn set_bool(&mut self, k: &str, v: bool) {
        self.0.insert(k.to_string(), Value::from(v));
    }
}

/// Signal that flows through the execution pipeline conveying confidence and
/// arbitrary metadata.
#[derive(Debug, Clone)]
pub struct ExecutionSignal {
    /// Current confidence score for executing a trade (0.0–1.0).
    pub confidence: f32,
    /// Arbitrary metadata associated with the signal.
    pub meta: MetaMap,
}

impl Default for ExecutionSignal {
    /// Provide a default execution signal with neutral confidence and an empty
    /// metadata map.
    fn default() -> Self {
        Self {
            confidence: 0.5,
            meta: MetaMap(Map::new()),
        }
    }
}

// Bridge Plan → adapters
/// A request to place a trade order derived from a decision plan.
#[derive(Debug, Clone)]
pub struct OrderRequest {
    /// Symbol or pair to trade (e.g., "ETH/USDC").
    pub symbol: String,
    /// Desired action (buy or sell) taken from the plan.
    pub action: Action,
    /// Quantity of the base asset to transact.
    pub qty: f64,
    /// Optional limit price specified by the plan.
    pub limit_px: Option<f64>,
    /// Maximum allowable slippage in basis points.
    pub max_slippage_bps: u32,
    /// Time-to-live for the order in milliseconds.
    pub ttl_ms: u64,
    /// Confidence score reflecting the planner's conviction in this trade.
    pub confidence: f32,
}
impl From<(&str, &Plan)> for OrderRequest {
    fn from((symbol, p): (&str, &Plan)) -> Self {
        Self {
            symbol: symbol.to_string(),
            action: p.action,
            qty: p.qty,
            limit_px: p.limit_px,
            max_slippage_bps: p.max_slippage_bps,
            ttl_ms: p.ttl_ms,
            confidence: p.confidence,
        }
    }
}

/// Quote information returned by a [`QuoteProvider`].
#[derive(Debug, Clone)]
pub struct QuotePack {
    /// Bid price offered by the venue.
    pub bid: f64,
    /// Ask price offered by the venue.
    pub ask: f64,
    /// Mid price computed from bid and ask.
    pub mid: f64,
    /// Estimated slippage (in basis points) when executing at this quote.
    pub est_slippage_bps: u32,
}
/// Estimated network fee required to settle a transaction.
#[derive(Debug, Clone)]
pub struct GasFee {
    /// Estimated absolute fee in native currency (e.g., ETH).
    pub estimated_fee: f64,
    /// Priority fee in basis points, used to accelerate transaction inclusion.
    pub priority_bps: u32,
}
/// Built transaction ready for submission to the network.
#[derive(Debug, Clone)]
pub struct BuiltTx {
    /// Human-friendly description of the transaction (e.g., trade summary).
    pub description: String,
    /// JSON payload encoding the transaction in a protocol-specific format.
    pub payload_json: String,
}
/// Result returned after a transaction submission.
#[derive(Debug, Clone)]
pub struct SubmitResult {
    /// Digest or hash identifying the transaction on-chain.
    pub tx_digest: String,
    /// Whether the transaction was accepted by the network.
    pub accepted: bool,
    /// Quantity actually filled by the network, which may be less than the
    /// requested quantity due to partial fills.
    pub filled_qty: f64,
}
