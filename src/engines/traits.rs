use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::execution::types::ExecutionSignal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineKind { Latency, Performance, DataHandling, MarketInsight, Quantize, Optimization, Resource }

#[derive(Clone)]
pub struct EngineContext { pub id: String }

#[async_trait]
pub trait Engine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn id(&self) -> &str;
    fn enabled(&self) -> bool { true }
    async fn init(&mut self, _ctx: &EngineContext) -> Result<()> { Ok(()) }
    async fn tick(&self, _ctx: &EngineContext, signal: ExecutionSignal) -> Result<ExecutionSignal>;
    async fn shutdown(&mut self) -> Result<()> { Ok(()) }
}
