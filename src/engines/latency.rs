use super::traits::{Engine, EngineContext, EngineKind};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::execution::types::ExecutionSignal;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyEngineConfig {
    pub enabled: bool,
    /// Soft limit (ms) above which we start shaping the signal
    pub soft_ms: u64,
    /// Hard limit (ms) above which we clamp confidence aggressively
    pub hard_ms: u64,
    /// Fraction to reduce confidence when between soft..hard
    pub soft_penalty: f32,
    /// Fraction to reduce confidence when > hard
    pub hard_penalty: f32,
}

impl Default for LatencyEngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            soft_ms: 40,
            hard_ms: 120,
            soft_penalty: 0.05,
            hard_penalty: 0.20,
        }
    }
}

pub struct LatencyEngine {
    id: String,
    cfg: LatencyEngineConfig,
}

impl LatencyEngine {
    pub fn new(id: impl Into<String>, cfg: Option<LatencyEngineConfig>) -> Self {
        Self { id: id.into(), cfg: cfg.unwrap_or_default() }
    }
}

#[async_trait]
impl Engine for LatencyEngine {
    fn kind(&self) -> EngineKind { EngineKind::Latency }
    fn id(&self) -> &str { &self.id }
    fn enabled(&self) -> bool { self.cfg.enabled }

    /// Reads latency from signal meta, applies penalties, and annotates the signal.
    #[instrument(skip_all, fields(engine = %self.id))]
    async fn tick(&self, _ctx: &EngineContext, mut signal: ExecutionSignal) -> Result<ExecutionSignal> {
        let lat_ms = signal.meta.get_f64("lat_ms")
            .or_else(|| signal.meta.get_f64("network_latency"))
            .unwrap_or(0.0) as u64;

        if lat_ms > self.cfg.hard_ms {
            let old = signal.confidence;
            signal.confidence = (signal.confidence * (1.0 - self.cfg.hard_penalty)).max(0.0);
            debug!(old_conf = old, new_conf = signal.confidence, lat_ms, "hard latency penalty applied");
        } else if lat_ms > self.cfg.soft_ms {
            let old = signal.confidence;
            signal.confidence = (signal.confidence * (1.0 - self.cfg.soft_penalty)).max(0.0);
            debug!(old_conf = old, new_conf = signal.confidence, lat_ms, "soft latency penalty applied");
        }

        // annotate for downstream inspection
        signal.meta.set_bool("latency_warn", lat_ms > self.cfg.soft_ms);
        Ok(signal)
    }
}