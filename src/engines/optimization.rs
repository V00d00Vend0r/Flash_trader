use super::traits::{Engine, EngineContext, EngineKind};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};
use crate::execution::types::ExecutionSignal;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationEngineConfig {
    pub enabled: bool,
    pub cpu_threshold: f64, pub gpu_threshold: f64, pub memory_threshold: f64, pub network_threshold: f64,
    pub history_window: usize, pub adjustment_step: f64, pub min_network_bandwidth: f64, pub confidence_penalty: f32,
}
impl Default for OptimizationEngineConfig {
    fn default() -> Self { Self { enabled: true, cpu_threshold: 75.0, gpu_threshold: 85.0, memory_threshold: 80.0, network_threshold: 70.0, history_window: 5, adjustment_step: 5.0, min_network_bandwidth: 20.0, confidence_penalty: 0.05 } }
}
pub struct OptimizationEngine { id: String, cfg: OptimizationEngineConfig }
impl OptimizationEngine { pub fn new(id: impl Into<String>, cfg: Option<OptimizationEngineConfig>) -> Self { Self { id: id.into(), cfg: cfg.unwrap_or_default() } } }

#[async_trait]
impl Engine for OptimizationEngine {
    fn kind(&self) -> EngineKind { EngineKind::Optimization }
    fn id(&self) -> &str { &self.id }
    fn enabled(&self) -> bool { self.cfg.enabled }
    #[instrument(skip_all, fields(engine = %self.id))]
    async fn tick(&self, _ctx: &EngineContext, mut signal: ExecutionSignal) -> Result<ExecutionSignal> {
        let cfg = &self.cfg;
        let mut cpu  = signal.meta.get_f64("cpu_usage").unwrap_or(0.0);
        let mut mem  = signal.meta.get_f64("memory_usage").unwrap_or(0.0);
        let mut gpu  = signal.meta.get_f64("gpu_usage").unwrap_or(0.0);
        let mut net  = signal.meta.get_f64("network_usage").unwrap_or(0.0);
        let lat_ms   = signal.meta.get_f64("network_latency").unwrap_or(0.0);
        let q_len    = signal.meta.get_f64("queue_length").unwrap_or(0.0);

        if mem > cfg.memory_threshold {
            let reduction = cfg.adjustment_step * 1.5; mem = (mem - reduction).max(0.0); cpu = (cpu - reduction).max(0.0);
        }
        if cpu > cfg.cpu_threshold {
            let shift = cfg.adjustment_step; cpu = (cpu - shift).max(0.0); gpu = (gpu + shift).min(100.0);
        }

        let latency_factor = lat_ms / 100.0; let queue_factor = q_len / 10.0;
        if latency_factor + queue_factor > 1.0 {
            let reduction = cfg.adjustment_step * (latency_factor + queue_factor);
            net = (net - reduction).max(cfg.min_network_bandwidth);
        }

        cpu = cpu.clamp(0.0, 100.0); mem = mem.clamp(0.0, 100.0); gpu = gpu.clamp(0.0, 100.0); net = net.clamp(cfg.min_network_bandwidth, 100.0);

        let mut stress = 0.0_f32;
        if cpu > cfg.cpu_threshold { stress += 0.25; }
        if mem > cfg.memory_threshold { stress += 0.25; }
        if gpu > cfg.gpu_threshold { stress += 0.25; }
        if net > cfg.network_threshold { stress += 0.25; }
        if stress > 0.0 && cfg.confidence_penalty > 0.0 {
            let old = signal.confidence; signal.confidence = (signal.confidence * (1.0 - cfg.confidence_penalty * stress)).max(0.0);
            debug!(old_conf = old, new_conf = signal.confidence, stress, "optimization penalty");
        }

        signal.meta.set_f64("cpu_usage", cpu);
        signal.meta.set_f64("memory_usage", mem);
        signal.meta.set_f64("gpu_usage", gpu);
        signal.meta.set_f64("network_usage", net);
        signal.meta.set_f64("network_latency", lat_ms);
        signal.meta.set_f64("queue_length", q_len);
        Ok(signal)
    }
}
