use anyhow::{anyhow, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use super::client::{LLMClient, LLMParams};

#[derive(Clone)]
pub struct Phi3OnnxClient { http: Client, base_url: String }
impl Phi3OnnxClient { pub fn new(base_url: impl Into<String>) -> Self { Self { http: Client::new(), base_url: base_url.into() } } }

#[derive(Serialize)]
struct Req<'a> { system: &'a str, prompt: &'a str, #[serde(flatten)] params: &'a LLMParams }
#[derive(Deserialize)]
struct Resp { text: String }

#[async_trait::async_trait]
impl LLMClient for Phi3OnnxClient {
    async fn generate(&self, system: &str, prompt: &str, params: &LLMParams) -> Result<String> {
        let url = format!("{}/generate", self.base_url);
        let r = self.http.post(url).json(&Req { system, prompt, params }).send().await?;
        if r.status() != StatusCode::OK { return Err(anyhow!("phi3 sidecar error: {}", r.text().await?)); }
        let body: Resp = r.json().await?;
        Ok(body.text)
    }
}
