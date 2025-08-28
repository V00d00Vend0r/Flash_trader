//! Large language model powered decision stage.
//!
//! This stage leverages a generic [`LLMClient`] to obtain trading
//! recommendations from a language model. A prompt is constructed
//! from the incoming [`Signal`] and the model is expected to return
//! JSON describing an action (`"Buy"|"Sell"|"Hold"`), a confidence
//! value (0.0–1.0) and an optional reason. The output is parsed
//! into a [`Decision`]. If parsing fails, a sensible default is
//! returned. The temperature, top‑p and token limit of the model can
//! be tuned via the embedded [`LLMParams`].

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::llm::client::{LLMClient, LLMParams};
use super::types::{Signal, Decision, Action};
use super::stage::DecisionStage;

/// A concrete decision stage backed by a language model.
pub struct Phi3DecisionStage<C: LLMClient> {
    id: String,
    client: C,
    params: LLMParams,
}

impl<C: LLMClient> Phi3DecisionStage<C> {
    /// Create a new stage with the provided identifier and LLM client.
    /// Default language model parameters are used; see [`LLMParams`] for
    /// details.
    pub fn new(id: impl Into<String>, client: C) -> Self {
        Self {
            id: id.into(),
            client,
            params: LLMParams {
                temperature: 0.1,
                top_p: 0.9,
                max_tokens: 64,
            },
        }
    }

    /// Modify the parameters used when querying the language model.
    pub fn with_params(mut self, params: LLMParams) -> Self {
        self.params = params;
        self
    }

    /// Build the system and user prompts for the LLM from a signal.
    fn build_prompt(&self, s: &Signal) -> (String, String) {
        let system = "You are a trading decider. Reply ONLY valid JSON.";
        let prompt = format!(
            r#"
You receive a market snapshot. Output JSON: {{"action":"Buy|Sell|Hold","confidence":0..1,"reason":"short"}}
Snapshot:
symbol={sym}, mid={mid:.6}, bid={bid:.6}, ask={ask:.6}
features={features}
"#,
            sym = s.symbol,
            mid = s.mid,
            bid = s.bid,
            ask = s.ask,
            features = Value::Object(s.features.clone()),
        );
        (system.to_string(), prompt)
    }
}

/// A helper struct representing the expected JSON response from the LLM.
#[derive(Debug, Deserialize, Serialize)]
struct LlmOutput {
    action: Option<String>,
    confidence: Option<f64>,
    reason: Option<String>,
}

#[async_trait]
impl<C: LLMClient + Send + Sync> DecisionStage for Phi3DecisionStage<C> {
    fn id(&self) -> &str {
        &self.id
    }

    async fn evaluate(&self, s: &Signal) -> Result<Decision> {
        let (system, prompt) = self.build_prompt(s);
        // Query the language model.
        let out = self.client.generate(&system, &prompt, &self.params).await?;

        // Try to parse the model output into our helper struct. We
        // attempt parsing both trimmed and raw forms for robustness.
        let parsed: LlmOutput = serde_json::from_str(out.trim())
            .or_else(|_| serde_json::from_str(&out))
            .unwrap_or_else(|_| LlmOutput {
                action: Some("Hold".to_string()),
                confidence: Some(0.3),
                reason: Some("parse_error".to_string()),
            });

        // Map the textual action into our enum.
        let action = match parsed.action.as_deref() {
            Some("Buy") => Action::Buy,
            Some("Sell") => Action::Sell,
            _ => Action::Hold,
        };
        // Use the provided confidence or default.
        let confidence = parsed.confidence.unwrap_or(0.3).clamp(0.0, 1.0) as f32;
        // Simple scoring scheme: map action to a fixed score.
        let score = match action {
            Action::Buy => 0.5,
            Action::Sell => -0.5,
            Action::Hold => 0.0,
        };
        // Use the entire parsed JSON as notes to preserve the reason.
        let notes = json!({
            "action": parsed.action.unwrap_or_else(|| "Hold".to_string()),
            "confidence": parsed.confidence.unwrap_or(0.3),
            "reason": parsed.reason.unwrap_or_else(|| "unknown".to_string()),
        });
        Ok(Decision {
            action,
            score,
            confidence,
            notes,
        })
    }
}