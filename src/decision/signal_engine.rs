//! Signal engine.
//!
//! The signal engine orchestrates a collection of [`DecisionStage`]
//! instances, blending their outputs into a single [`Plan`]. Each
//! stage contributes a weighted score and confidence; these are
//! aggregated to determine the final action, quantity and other
//! execution parameters. Thresholds for buy and sell decisions as
//! well as sizing logic can be tuned via the public fields of
//! [`SignalEngine`].

use anyhow::Result;
use std::sync::Arc;
use super::types::{Signal, Decision, Action, Plan};
use super::stage::DecisionStage;

/// Default score threshold above which a buy or below which a sell
/// action is selected.
const ACTION_THRESHOLD: f32 = 0.02;

/// Combines multiple decision stages into a single plan.
pub struct SignalEngine {
    stages: Vec<(Arc<dyn DecisionStage>, f32)>,
    /// Maximum position size to allocate when a strong signal is present.
    pub max_size: f64,
    /// Base time to live (milliseconds) for an execution plan.
    pub base_ttl_ms: u64,
    /// Maximum allowed slippage in basis points.
    pub max_slippage_bps: u32,
    /// Threshold controlling when to trigger buy/sell decisions. The
    /// default of 0.02 corresponds to a blended score of 2%.
    pub action_threshold: f32,
}

impl SignalEngine {
    /// Create a new engine with a list of stages and their weights. A
    /// weight of 1.0 means the stage’s score contributes fully to
    /// the blended score, whereas lower weights attenuate its
    /// influence.
    pub fn new(stages: Vec<(Arc<dyn DecisionStage>, f32)>) -> Self {
        Self {
            stages,
            max_size: 10.0,
            base_ttl_ms: 1_500,
            max_slippage_bps: 30,
            action_threshold: ACTION_THRESHOLD,
        }
    }

    /// Construct an engine with explicit parameter values.
    pub fn new_with_settings(
        stages: Vec<(Arc<dyn DecisionStage>, f32)>,
        max_size: f64,
        base_ttl_ms: u64,
        max_slippage_bps: u32,
        action_threshold: f32,
    ) -> Self {
        Self {
            stages,
            max_size,
            base_ttl_ms,
            max_slippage_bps,
            action_threshold,
        }
    }

    /// Append a stage to the engine with the specified weight.
    pub fn push_stage(&mut self, stage: Arc<dyn DecisionStage>, weight: f32) {
        self.stages.push((stage, weight));
    }

    /// Update the maximum position size.
    pub fn set_max_size(&mut self, size: f64) {
        self.max_size = size;
    }

    /// Perform a blended decision across all enabled stages and
    /// produce a plan. Each stage’s score is weighted and summed;
    /// confidences are aggregated by taking the minimum. If no stages
    /// produce a result, the blended score defaults to zero and the
    /// action is `Hold`.
    pub async fn decide(&self, signal: &Signal) -> Result<Plan> {
        let mut sum_w: f32 = 0.0;
        let mut sum_score: f32 = 0.0;
        let mut min_conf: f32 = 1.0;
        for (stage, weight) in &self.stages {
            if !stage.enabled(signal) {
                continue;
            }
            let decision = stage.evaluate(signal).await?;
            sum_w += *weight;
            sum_score += decision.score * *weight;
            min_conf = min_conf.min(decision.confidence);
        }
        let blended = if sum_w > 0.0 { sum_score / sum_w } else { 0.0 };
        // Determine action based on the blended score and threshold.
        let action = if blended > self.action_threshold {
            Action::Buy
        } else if blended < -self.action_threshold {
            Action::Sell
        } else {
            Action::Hold
        };
        // Size proportionally to the magnitude of the blended score.
        let qty = (blended.abs() as f64 * self.max_size).clamp(0.0, self.max_size);
        Ok(Plan {
            action,
            qty,
            limit_px: None,
            max_slippage_bps: self.max_slippage_bps,
            ttl_ms: self.base_ttl_ms,
            confidence: min_conf,
        })
    }
}