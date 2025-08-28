use super::traits::{Engine, EngineContext, EngineKind};
use anyhow::Result;
use async_trait::async_trait;
use crate::execution::types::ExecutionSignal;

pub struct ResourceEngine { id: String }
impl ResourceEngine { pub fn new(id: impl Into<String>) -> Self { Self { id: id.into() } } }
#[async_trait]
impl Engine for ResourceEngine {
    fn kind(&self) -> EngineKind { EngineKind::Resource }
    fn id(&self) -> &str { &self.id }
    async fn tick(&self, _ctx: &EngineContext, signal: ExecutionSignal) -> Result<ExecutionSignal> { Ok(signal) }
}
