use super::traits::{Engine, EngineContext, EngineKind};
use anyhow::Result;
use async_trait::async_trait;
use crate::execution::types::ExecutionSignal;

pub struct QuantizeEngine { id: String }
impl QuantizeEngine { pub fn new(id: impl Into<String>) -> Self { Self { id: id.into() } } }
#[async_trait]
impl Engine for QuantizeEngine {
    fn kind(&self) -> EngineKind { EngineKind::Quantize }
    fn id(&self) -> &str { &self.id }
    async fn tick(&self, _ctx: &EngineContext, signal: ExecutionSignal) -> Result<ExecutionSignal> { Ok(signal) }
}
