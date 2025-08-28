use anyhow::Result;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub llm: Option<LlmConfig>,
    pub decision: Option<DecisionConfig>,
    pub engines: Option<toml::value::Table>,
    pub execution: Option<toml::value::Table>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    pub backend: Option<String>,
    pub onnx: Option<OnnxConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OnnxConfig {
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DecisionConfig {
    pub stages: Option<toml::value::Table>,
}

pub fn load_base() -> Result<AppConfig> {
    let s = fs::read_to_string("config/base.toml")?;
    let cfg: AppConfig = toml::from_str(&s)?;
    Ok(cfg)
}
