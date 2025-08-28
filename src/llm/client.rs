use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LLMParams { pub temperature: f32, pub top_p: f32, pub max_tokens: usize }

#[async_trait::async_trait]
pub trait LLMClient: Send + Sync {
    async fn generate(&self, system: &str, prompt: &str, params: &LLMParams) -> Result<String>;
}
