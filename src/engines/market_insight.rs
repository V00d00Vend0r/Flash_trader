use super::traits::{Engine, EngineContext, EngineKind};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};
use std::collections::HashMap;

use crate::execution::types::ExecutionSignal;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInsightConfig {
    pub enabled: bool,
    /// Processing window in milliseconds
    pub processing_window_ms: u64,
    /// Anomaly detection threshold
    pub anomaly_threshold: f64,
    /// Model ensemble weights (xgb, lstm, arima)
    pub model_weights: (f64, f64, f64),
    /// Minimum confidence threshold for insights
    pub min_confidence: f64,
}

impl Default for MarketInsightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            processing_window_ms: 1000,
            anomaly_threshold: 0.8,
            model_weights: (0.4, 0.4, 0.2),
            min_confidence: 0.6,
        }
    }
}

pub struct MarketInsightEngine {
    id: String,
    cfg: MarketInsightConfig,
    /// Feature cache for pattern detection
    feature_cache: HashMap<String, f64>,
}

impl MarketInsightEngine {
    pub fn new(id: impl Into<String>, cfg: Option<MarketInsightConfig>) -> Self {
        Self {
            id: id.into(),
            cfg: cfg.unwrap_or_default(),
            feature_cache: HashMap::new(),
        }
    }

    /// Extract market features from signal metadata
    fn extract_features(&self, signal: &ExecutionSignal) -> HashMap<String, f64> {
        let mut features = HashMap::new();

        // Price-based features
        if let Some(price) = signal.meta.get_f64("price") {
            features.insert("price".to_string(), price);
        }

        // Volume features
        if let Some(volume) = signal.meta.get_f64("volume") {
            features.insert("volume".to_string(), volume);
        }

        // Volatility indicators
        if let Some(volatility) = signal.meta.get_f64("volatility") {
            features.insert("volatility".to_string(), volatility);
        }

        // Liquidity metrics
        if let Some(spread) = signal.meta.get_f64("bid_ask_spread") {
            features.insert("spread".to_string(), spread);
        }

        features
    }

    /// Detect market anomalies based on extracted features
    fn detect_anomalies(&self, features: &HashMap<String, f64>) -> f64 {
        // Simple anomaly detection based on feature deviations
        let mut anomaly_score = 0.0;

        // Check for unusual volume patterns
        if let Some(volume) = features.get("volume") {
            if *volume > 1000000.0 {  // Large volume threshold
                anomaly_score += 0.3;
            }
        }

        // Check for high volatility
        if let Some(volatility) = features.get("volatility") {
            if *volatility > 0.05 {  // 5% volatility threshold
                anomaly_score += 0.4;
            }
        }

        // Check for wide spreads (liquidity issues)
        if let Some(spread) = features.get("spread") {
            if *spread > 0.01 {  // 1% spread threshold
                anomaly_score += 0.3;
            }
        }

        anomaly_score.min(1.0)
    }

    /// Generate market insights based on signal analysis
    fn generate_insights(&self, signal: &ExecutionSignal, features: &HashMap<String, f64>) -> Vec<String> {
        let mut insights = Vec::new();

        // Anomaly detection
        let anomaly_score = self.detect_anomalies(features);
        if anomaly_score > self.cfg.anomaly_threshold {
            insights.push(format!("Market anomaly detected (score: {:.2})", anomaly_score));
        }

        // Volume analysis
        if let Some(volume) = features.get("volume") {
            if *volume > 500000.0 {
                insights.push("High volume activity detected".to_string());
            }
        }

        // Volatility warnings
        if let Some(volatility) = features.get("volatility") {
            if *volatility > 0.03 {
                insights.push(format!("Elevated volatility: {:.2}%", volatility * 100.0));
            }
        }

        // Liquidity concerns
        if let Some(spread) = features.get("spread") {
            if *spread > 0.005 {
                insights.push("Liquidity concerns - wide bid-ask spread".to_string());
            }
        }

        insights
    }
}

#[async_trait]
impl Engine for MarketInsightEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::MarketInsight
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    /// Process market signals and generate insights
    #[instrument(skip_all, fields(engine = %self.id))]
    async fn tick(&self, _ctx: &EngineContext, mut signal: ExecutionSignal) -> Result<ExecutionSignal> {
        // Extract market features from signal
        let features = self.extract_features(&signal);
        
        // Generate insights based on features
        let insights = self.generate_insights(&signal, &features);
        
        // Apply confidence adjustments based on market conditions
        let anomaly_score = self.detect_anomalies(&features);
        if anomaly_score > self.cfg.anomaly_threshold {
            let old_confidence = signal.confidence;
            // Reduce confidence during anomalous market conditions
            signal.confidence = (signal.confidence * (1.0 - anomaly_score * 0.2)).max(0.0);
            debug!(
                old_conf = old_confidence,
                new_conf = signal.confidence,
                anomaly_score,
                "Market anomaly confidence adjustment"
            );
        }

        // Store insights in signal metadata
        if !insights.is_empty() {
            signal.meta.set_string("market_insights", insights.join("; "));
            debug!(insights = ?insights, "Market insights generated");
        }

        // Mark signal with market analysis flags
        signal.meta.set_f64("anomaly_score", anomaly_score);
        signal.meta.set_bool("market_analyzed", true);

        Ok(signal)
    }
}