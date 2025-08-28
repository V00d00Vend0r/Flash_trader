//! Data provider traits and implementations for market data ingestion.
//!
//! This module defines the core abstractions for fetching market data from various sources
//! including exchanges, DEXs, and blockchain RPC endpoints. The design emphasizes modularity,
//! error resilience, and extensibility.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{interval, sleep, Instant};
use tracing::{debug, error, info, warn};

/// Core market data structure representing a snapshot of market conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    /// Timestamp in milliseconds since Unix epoch
    pub timestamp_ms: u64,
    /// Trading symbol (e.g., "SUI-USDC", "BTC-USD")
    pub symbol: String,
    /// Best bid price
    pub bid: f64,
    /// Best ask price
    pub ask: f64,
    /// Mid price (bid + ask) / 2
    pub mid: f64,
    /// Last traded price
    pub last: Option<f64>,
    /// 24h volume
    pub volume_24h: Option<f64>,
    /// Price change percentage over 24h
    pub change_24h_pct: Option<f64>,
    /// Additional market features (volatility, spreads, etc.)
    pub features: HashMap<String, f64>,
    /// Metadata (source, latency, etc.)
    pub metadata: HashMap<String, String>,
}

impl MarketData {
    /// Create a new MarketData instance with current timestamp
    pub fn new(symbol: String, bid: f64, ask: f64) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            timestamp_ms,
            symbol,
            bid,
            ask,
            mid: (bid + ask) / 2.0,
            last: None,
            volume_24h: None,
            change_24h_pct: None,
            features: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Calculate spread in basis points
    pub fn spread_bps(&self) -> f64 {
        if self.mid > 0.0 {
            ((self.ask - self.bid) / self.mid) * 10_000.0
        } else {
            0.0
        }
    }

    /// Check if the data is stale based on age threshold
    pub fn is_stale(&self, max_age_ms: u64) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        now_ms.saturating_sub(self.timestamp_ms) > max_age_ms
    }

    /// Add a feature to the market data
    pub fn with_feature(mut self, key: &str, value: f64) -> Self {
        self.features.insert(key.to_string(), value);
        self
    }

    /// Add metadata to the market data
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Configuration for data provider behavior
#[derive(Debug, Clone)]
pub struct DataProviderConfig {
    /// Maximum age of data before considered stale (milliseconds)
    pub max_data_age_ms: u64,
    /// Retry attempts for failed requests
    pub max_retries: u32,
    /// Base delay between retries (exponential backoff)
    pub retry_delay_ms: u64,
    /// Request timeout duration
    pub request_timeout_ms: u64,
    /// Rate limiting: max requests per second
    pub rate_limit_rps: f64,
    /// Enable/disable automatic reconnection
    pub auto_reconnect: bool,
    /// Heartbeat interval for connection health checks
    pub heartbeat_interval_ms: u64,
}

impl Default for DataProviderConfig {
    fn default() -> Self {
        Self {
            max_data_age_ms: 5_000,      // 5 seconds
            max_retries: 3,
            retry_delay_ms: 1_000,       // 1 second
            request_timeout_ms: 10_000,  // 10 seconds
            rate_limit_rps: 10.0,        // 10 requests per second
            auto_reconnect: true,
            heartbeat_interval_ms: 30_000, // 30 seconds
        }
    }
}

/// Health status of a data provider
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderHealth {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
    Disconnected,
}

impl fmt::Display for ProviderHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderHealth::Healthy => write!(f, "Healthy"),
            ProviderHealth::Degraded { reason } => write!(f, "Degraded: {}", reason),
            ProviderHealth::Unhealthy { reason } => write!(f, "Unhealthy: {}", reason),
            ProviderHealth::Disconnected => write!(f, "Disconnected"),
        }
    }
}

/// Statistics for monitoring data provider performance
#[derive(Debug, Clone, Default)]
pub struct ProviderStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: f64,
    pub last_success_timestamp: Option<u64>,
    pub last_error: Option<String>,
    pub reconnection_count: u32,
}

impl ProviderStats {
    /// Calculate success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.successful_requests as f64 / self.total_requests as f64) * 100.0
        }
    }

    /// Update stats for a successful request
    pub fn record_success(&mut self, latency_ms: f64) {
        self.total_requests += 1;
        self.successful_requests += 1;
        
        // Update rolling average latency
        let total_successful = self.successful_requests as f64;
        self.avg_latency_ms = ((self.avg_latency_ms * (total_successful - 1.0)) + latency_ms) / total_successful;
        
        self.last_success_timestamp = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        );
    }

    /// Update stats for a failed request
    pub fn record_failure(&mut self, error: &str) {
        self.total_requests += 1;
        self.failed_requests += 1;
        self.last_error = Some(error.to_string());
    }

    /// Record a reconnection event
    pub fn record_reconnection(&mut self) {
        self.reconnection_count += 1;
    }
}

/// Core trait for all data providers
#[async_trait::async_trait]
pub trait DataProvider: Send + Sync {
    /// Unique identifier for this provider
    fn id(&self) -> &str;

    /// Get the next market data point
    async fn next(&mut self) -> Result<MarketData>;

    /// Get current health status
    async fn health(&self) -> ProviderHealth;

    /// Get performance statistics
    async fn stats(&self) -> ProviderStats;

    /// Start the provider (connect, initialize, etc.)
    async fn start(&mut self) -> Result<()>;

    /// Stop the provider gracefully
    async fn stop(&mut self) -> Result<()>;

    /// Check if the provider is currently active
    fn is_active(&self) -> bool;

    /// Get supported symbols for this provider
    async fn supported_symbols(&self) -> Result<Vec<String>>;

    /// Subscribe to specific symbols (for streaming providers)
    async fn subscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        // Default implementation - providers can override
        debug!("Provider {} doesn't support symbol subscription", self.id());
        Ok(())
    }

    /// Unsubscribe from symbols
    async fn unsubscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        // Default implementation - providers can override
        debug!("Provider {} doesn't support symbol unsubscription", self.id());
        Ok(())
    }
}

/// Aggregated data provider that combines multiple sources
pub struct AggregatedDataProvider {
    id: String,
    providers: Vec<Box<dyn DataProvider>>,
    config: DataProviderConfig,
    stats: Arc<RwLock<ProviderStats>>,
    active: bool,
}

impl AggregatedDataProvider {
    /// Create a new aggregated provider
    pub fn new(id: String, config: DataProviderConfig) -> Self {
        Self {
            id,
            providers: Vec::new(),
            config,
            stats: Arc::new(RwLock::new(ProviderStats::default())),
            active: false,
        }
    }

    /// Add a data provider to the aggregation
    pub fn add_provider(&mut self, provider: Box<dyn DataProvider>) {
        info!("Adding provider {} to aggregated provider {}", provider.id(), self.id);
        self.providers.push(provider);
    }

    /// Get data from the first healthy provider
    async fn get_from_healthy_provider(&mut self) -> Result<MarketData> {
        let start_time = Instant::now();
        
        for provider in &mut self.providers {
            match provider.health().await {
                ProviderHealth::Healthy => {
                    match provider.next().await {
                        Ok(data) => {
                            let latency_ms = start_time.elapsed().as_millis() as f64;
                            self.stats.write().await.record_success(latency_ms);
                            return Ok(data);
                        }
                        Err(e) => {
                            warn!("Provider {} failed: {}", provider.id(), e);
                            self.stats.write().await.record_failure(&e.to_string());
                            continue;
                        }
                    }
                }
                health => {
                    debug!("Provider {} not healthy: {}", provider.id(), health);
                    continue;
                }
            }
        }

        let error_msg = "No healthy providers available";
        self.stats.write().await.record_failure(error_msg);
        anyhow::bail!(error_msg)
    }
}

#[async_trait::async_trait]
impl DataProvider for AggregatedDataProvider {
    fn id(&self) -> &str {
        &self.id
    }

    async fn next(&mut self) -> Result<MarketData> {
        if !self.active {
            anyhow::bail!("Provider {} is not active", self.id);
        }

        self.get_from_healthy_provider().await
            .context("Failed to get data from aggregated providers")
    }

    async fn health(&self) -> ProviderHealth {
        if !self.active {
            return ProviderHealth::Disconnected;
        }

        let mut healthy_count = 0;
        let mut degraded_count = 0;
        let mut total_count = 0;

        for provider in &self.providers {
            total_count += 1;
            match provider.health().await {
                ProviderHealth::Healthy => healthy_count += 1,
                ProviderHealth::Degraded { .. } => degraded_count += 1,
                _ => {}
            }
        }

        if healthy_count == 0 {
            ProviderHealth::Unhealthy {
                reason: "No healthy providers available".to_string(),
            }
        } else if healthy_count == total_count {
            ProviderHealth::Healthy
        } else {
            ProviderHealth::Degraded {
                reason: format!("{}/{} providers healthy", healthy_count, total_count),
            }
        }
    }

    async fn stats(&self) -> ProviderStats {
        self.stats.read().await.clone()
    }

    async fn start(&mut self) -> Result<()> {
        info!("Starting aggregated provider {}", self.id);
        
        for provider in &mut self.providers {
            if let Err(e) = provider.start().await {
                warn!("Failed to start provider {}: {}", provider.id(), e);
            }
        }

        self.active = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Stopping aggregated provider {}", self.id);
        
        for provider in &mut self.providers {
            if let Err(e) = provider.stop().await {
                warn!("Failed to stop provider {}: {}", provider.id(), e);
            }
        }

        self.active = false;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn supported_symbols(&self) -> Result<Vec<String>> {
        let mut all_symbols = std::collections::HashSet::new();
        
        for provider in &self.providers {
            if let Ok(symbols) = provider.supported_symbols().await {
                all_symbols.extend(symbols);
            }
        }

        Ok(all_symbols.into_iter().collect())
    }

    async fn subscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        for provider in &mut self.providers {
            if let Err(e) = provider.subscribe(symbols.clone()).await {
                warn!("Provider {} failed to subscribe: {}", provider.id(), e);
            }
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        for provider in &mut self.providers {
            if let Err(e) = provider.unsubscribe(symbols.clone()).await {
                warn!("Provider {} failed to unsubscribe: {}", provider.id(), e);
            }
        }
        Ok(())
    }
}

/// Mock data provider for testing and development
pub struct MockDataProvider {
    id: String,
    symbol: String,
    base_price: f64,
    spread_bps: f64,
    active: bool,
    stats: Arc<RwLock<ProviderStats>>,
    config: DataProviderConfig,
}

impl MockDataProvider {
    /// Create a new mock provider
    pub fn new(id: String, symbol: String, base_price: f64, spread_bps: f64) -> Self {
        Self {
            id,
            symbol,
            base_price,
            spread_bps,
            active: false,
            stats: Arc::new(RwLock::new(ProviderStats::default())),
            config: DataProviderConfig::default(),
        }
    }

    /// Generate realistic price movement
    fn generate_price(&self) -> (f64, f64) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        
        // Add some random walk to the base price
        let price_change = rng.gen_range(-0.01..0.01);
        let mid_price = self.base_price * (1.0 + price_change);
        
        // Calculate bid/ask from spread
        let half_spread = (self.spread_bps / 10_000.0) * mid_price / 2.0;
        let bid = mid_price - half_spread;
        let ask = mid_price + half_spread;
        
        (bid, ask)
    }
}

#[async_trait::async_trait]
impl DataProvider for MockDataProvider {
    fn id(&self) -> &str {
        &self.id
    }

    async fn next(&mut self) -> Result<MarketData> {
        if !self.active {
            anyhow::bail!("Mock provider {} is not active", self.id);
        }

        let start_time = Instant::now();
        
        // Simulate some processing delay
        sleep(Duration::from_millis(10)).await;
        
        let (bid, ask) = self.generate_price();
        let data = MarketData::new(self.symbol.clone(), bid, ask)
            .with_metadata("source", &self.id)
            .with_metadata("provider_type", "mock")
            .with_feature("spread_bps", (ask - bid) / ((ask + bid) / 2.0) * 10_000.0);

        let latency_ms = start_time.elapsed().as_millis() as f64;
        self.stats.write().await.record_success(latency_ms);

        Ok(data)
    }

    async fn health(&self) -> ProviderHealth {
        if self.active {
            ProviderHealth::Healthy
        } else {
            ProviderHealth::Disconnected
        }
    }

    async fn stats(&self) -> ProviderStats {
        self.stats.read().await.clone()
    }

    async fn start(&mut self) -> Result<()> {
        info!("Starting mock provider {}", self.id);
        self.active = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Stopping mock provider {}", self.id);
        self.active = false;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn supported_symbols(&self) -> Result<Vec<String>> {
        Ok(vec![self.symbol.clone()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_market_data_creation() {
        let data = MarketData::new("SUI-USDC".to_string(), 1.45, 1.47);
        assert_eq!(data.symbol, "SUI-USDC");
        assert_eq!(data.bid, 1.45);
        assert_eq!(data.ask, 1.47);
        assert_eq!(data.mid, 1.46);
        assert!(!data.is_stale(10_000)); // Should not be stale immediately
    }

    #[tokio::test]
    async fn test_mock_provider() {
        let mut provider = MockDataProvider::new(
            "test_mock".to_string(),
            "SUI-USDC".to_string(),
            1.46,
            30.0, // 30 bps spread
        );

        assert!(!provider.is_active());
        assert_eq!(provider.health().await, ProviderHealth::Disconnected);

        provider.start().await.unwrap();
        assert!(provider.is_active());
        assert_eq!(provider.health().await, ProviderHealth::Healthy);

        let data = provider.next().await.unwrap();
        assert_eq!(data.symbol, "SUI-USDC");
        assert!(data.bid > 0.0);
        assert!(data.ask > data.bid);

        let stats = provider.stats().await;
        assert_eq!(stats.successful_requests, 1);
        assert!(stats.success_rate() > 99.0);

        provider.stop().await.unwrap();
        assert!(!provider.is_active());
    }

    #[tokio::test]
    async fn test_aggregated_provider() {
        let mut aggregated = AggregatedDataProvider::new(
            "test_aggregated".to_string(),
            DataProviderConfig::default(),
        );

        let mock1 = MockDataProvider::new(
            "mock1".to_string(),
            "SUI-USDC".to_string(),
            1.46,
            30.0,
        );
        let mock2 = MockDataProvider::new(
            "mock2".to_string(),
            "SUI-USDC".to_string(),
            1.47,
            25.0,
        );

        aggregated.add_provider(Box::new(mock1));
        aggregated.add_provider(Box::new(mock2));

        aggregated.start().await.unwrap();
        assert!(aggregated.is_active());

        let data = aggregated.next().await.unwrap();
        assert_eq!(data.symbol, "SUI-USDC");

        let symbols = aggregated.supported_symbols().await.unwrap();
        assert!(symbols.contains(&"SUI-USDC".to_string()));

        aggregated.stop().await.unwrap();
    }
}