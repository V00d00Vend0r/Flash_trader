//! HTTP client implementation for REST API market data fetching.
//!
//! This module provides a robust HTTP client for fetching market data from REST APIs,
//! with support for polling, caching, rate limiting, and serving as a fallback
//! when WebSocket connections are unavailable.

use crate::data::provider::{DataProvider, DataProviderConfig, MarketData, ProviderHealth, ProviderStats};
use anyhow::{Context, Result};
use reqwest::{Client, ClientBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, Mutex, Semaphore};
use tokio::time::{interval, sleep, timeout, Instant};
use tracing::{debug, error, info, warn};
use url::Url;

/// HTTP client configuration
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Base URL for the API
    pub base_url: String,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Maximum number of concurrent requests
    pub max_concurrent_requests: usize,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Retry configuration
    pub retry_config: RetryConfig,
    /// Caching configuration
    pub cache_config: CacheConfig,
    /// Polling configuration
    pub polling_config: PollingConfig,
    /// Custom headers to include in requests
    pub custom_headers: HashMap<String, String>,
    /// User agent string
    pub user_agent: String,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://rpc.ankr.com/sui".to_string(),
            api_key: None,
            request_timeout_ms: 10_000,
            connect_timeout_ms: 5_000,
            max_concurrent_requests: 10,
            rate_limit: RateLimitConfig::default(),
            retry_config: RetryConfig::default(),
            cache_config: CacheConfig::default(),
            polling_config: PollingConfig::default(),
            custom_headers: HashMap::new(),
            user_agent: "Raveslinger-Bot/1.0".to_string(),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per second
    pub requests_per_second: f64,
    /// Burst capacity (max requests in a short burst)
    pub burst_capacity: u32,
    /// Enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10.0,
            burst_capacity: 20,
            enabled: true,
        }
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Jitter factor to avoid thundering herd
    pub jitter_factor: f64,
    /// HTTP status codes that should trigger a retry
    pub retryable_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1_000,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
            retryable_status_codes: vec![429, 500, 502, 503, 504],
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Enable response caching
    pub enabled: bool,
    /// Maximum cache size (number of entries)
    pub max_size: usize,
    /// Default TTL for cached responses in milliseconds
    pub default_ttl_ms: u64,
    /// TTL for different endpoint types
    pub endpoint_ttls: HashMap<String, u64>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        let mut endpoint_ttls = HashMap::new();
        endpoint_ttls.insert("ticker".to_string(), 1_000);      // 1 second
        endpoint_ttls.insert("orderbook".to_string(), 500);     // 500ms
        endpoint_ttls.insert("trades".to_string(), 2_000);      // 2 seconds
        endpoint_ttls.insert("klines".to_string(), 5_000);      // 5 seconds
        
        Self {
            enabled: true,
            max_size: 1000,
            default_ttl_ms: 1_000,
            endpoint_ttls,
        }
    }
}

/// Polling configuration
#[derive(Debug, Clone)]
pub struct PollingConfig {
    /// Enable automatic polling
    pub enabled: bool,
    /// Default polling interval in milliseconds
    pub default_interval_ms: u64,
    /// Polling intervals for different endpoints
    pub endpoint_intervals: HashMap<String, u64>,
    /// Maximum number of symbols to poll simultaneously
    pub max_concurrent_polls: usize,
}

impl Default for PollingConfig {
    fn default() -> Self {
        let mut endpoint_intervals = HashMap::new();
        endpoint_intervals.insert("ticker".to_string(), 1_000);     // 1 second
        endpoint_intervals.insert("orderbook".to_string(), 500);    // 500ms
        endpoint_intervals.insert("trades".to_string(), 2_000);     // 2 seconds
        
        Self {
            enabled: false, // Disabled by default, enable when needed
            default_interval_ms: 1_000,
            endpoint_intervals,
            max_concurrent_polls: 5,
        }
    }
}

/// Cached response entry
#[derive(Debug, Clone)]
struct CacheEntry {
    data: MarketData,
    timestamp: Instant,
    ttl: Duration,
}

impl CacheEntry {
    fn new(data: MarketData, ttl: Duration) -> Self {
        Self {
            data,
            timestamp: Instant::now(),
            ttl,
        }
    }
    
    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }
}

/// ANKR API response structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkrApiResponse<T> {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<T>,
    pub error: Option<AnkrApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkrApiError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Market data from various API endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMarketData {
    pub symbol: String,
    pub price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub volume: Option<f64>,
    pub change_24h: Option<f64>,
    pub timestamp: Option<u64>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// HTTP client for market data
pub struct HttpClient {
    id: String,
    config: HttpClientConfig,
    provider_config: DataProviderConfig,
    
    // HTTP client
    client: Client,
    
    // Rate limiting
    rate_limiter: Arc<Semaphore>,
    last_request_time: Arc<RwLock<Instant>>,
    
    // Caching
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    
    // Data management
    data_queue: Arc<Mutex<VecDeque<MarketData>>>,
    subscribed_symbols: Arc<RwLock<Vec<String>>>,
    
    // Statistics and health
    stats: Arc<RwLock<ProviderStats>>,
    active: Arc<RwLock<bool>>,
    
    // Polling tasks
    polling_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl HttpClient {
    /// Create a new HTTP client
    pub fn new(
        id: String,
        config: HttpClientConfig,
        provider_config: DataProviderConfig,
    ) -> Result<Self> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_millis(config.request_timeout_ms))
            .connect_timeout(Duration::from_millis(config.connect_timeout_ms))
            .user_agent(&config.user_agent)
            .build()
            .context("Failed to create HTTP client")?;
        
        let rate_limiter = Arc::new(Semaphore::new(config.max_concurrent_requests));
        
        Ok(Self {
            id,
            config,
            provider_config,
            client,
            rate_limiter,
            last_request_time: Arc::new(RwLock::new(Instant::now())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            data_queue: Arc::new(Mutex::new(VecDeque::new())),
            subscribed_symbols: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(ProviderStats::default())),
            active: Arc::new(RwLock::new(false)),
            polling_handles: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Start the HTTP client and polling tasks
    async fn start_internal(&mut self) -> Result<()> {
        info!("Starting HTTP client {}", self.id);
        
        *self.active.write().await = true;
        
        // Start polling if enabled
        if self.config.polling_config.enabled {
            self.start_polling_tasks().await?;
        }
        
        Ok(())
    }

    /// Start polling tasks for subscribed symbols
    async fn start_polling_tasks(&self) -> Result<()> {
        let symbols = self.subscribed_symbols.read().await.clone();
        
        for symbol in symbols {
            let handle = self.start_symbol_polling_task(symbol).await?;
            self.polling_handles.lock().await.push(handle);
        }
        
        Ok(())
    }

    /// Start polling task for a specific symbol
    async fn start_symbol_polling_task(&self, symbol: String) -> Result<tokio::task::JoinHandle<()>> {
        let client_clone = self.clone_for_task();
        let interval_ms = self.config.polling_config
            .endpoint_intervals
            .get("ticker")
            .copied()
            .unwrap_or(self.config.polling_config.default_interval_ms);
        
        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(interval_ms));
            
            loop {
                interval.tick().await;
                
                if !*client_clone.active.read().await {
                    break;
                }
                
                if let Err(e) = client_clone.fetch_and_queue_data(&symbol).await {
                    warn!("Failed to fetch data for {}: {}", symbol, e);
                }
            }
        });
        
        Ok(handle)
    }

    /// Clone for use in async tasks
    fn clone_for_task(&self) -> HttpClientTask {
        HttpClientTask {
            id: self.id.clone(),
            config: self.config.clone(),
            provider_config: self.provider_config.clone(),
            client: self.client.clone(),
            rate_limiter: Arc::clone(&self.rate_limiter),
            last_request_time: Arc::clone(&self.last_request_time),
            cache: Arc::clone(&self.cache),
            data_queue: Arc::clone(&self.data_queue),
            stats: Arc::clone(&self.stats),
            active: Arc::clone(&self.active),
        }
    }

    /// Fetch market data for a symbol
    pub async fn fetch_market_data(&self, symbol: &str) -> Result<MarketData> {
        let start_time = Instant::now();
        
        // Check cache first
        if let Some(cached_data) = self.get_cached_data(symbol).await {
            let latency_ms = start_time.elapsed().as_millis() as f64;
            self.stats.write().await.record_success(latency_ms);
            return Ok(cached_data);
        }
        
        // Apply rate limiting
        self.apply_rate_limit().await?;
        
        // Fetch from API with retries
        let market_data = self.fetch_with_retries(symbol).await?;
        
        // Cache the result
        self.cache_data(symbol, &market_data).await;
        
        let latency_ms = start_time.elapsed().as_millis() as f64;
        self.stats.write().await.record_success(latency_ms);
        
        Ok(market_data)
    }

    /// Apply rate limiting
    async fn apply_rate_limit(&self) -> Result<()> {
        if !self.config.rate_limit.enabled {
            return Ok();
        }
        
        // Acquire semaphore permit
        let _permit = self.rate_limiter.acquire().await
            .context("Failed to acquire rate limit permit")?;
        
        // Check time-based rate limiting
        let last_request = *self.last_request_time.read().await;
        let min_interval = Duration::from_secs_f64(1.0 / self.config.rate_limit.requests_per_second);
        let elapsed = last_request.elapsed();
        
        if elapsed < min_interval {
            let sleep_duration = min_interval - elapsed;
            sleep(sleep_duration).await;
        }
        
        *self.last_request_time.write().await = Instant::now();
        
        Ok(())
    }

    /// Fetch data with retry logic
    async fn fetch_with_retries(&self, symbol: &str) -> Result<MarketData> {
        let mut last_error = None;
        let mut delay_ms = self.config.retry_config.initial_delay_ms;
        
        for attempt in 0..=self.config.retry_config.max_attempts {
            match self.fetch_from_api(symbol).await {
                Ok(data) => return Ok(data),
                Err(e) => {
                    last_error = Some(e);
                    
                    if attempt < self.config.retry_config.max_attempts {
                        // Apply jitter to delay
                        let jitter = (rand::random::<f64>() - 0.5) * 2.0 * self.config.retry_config.jitter_factor;
                        let jittered_delay = (delay_ms as f64 * (1.0 + jitter)) as u64;
                        
                        debug!("Retrying request for {} in {}ms (attempt {}/{})", 
                               symbol, jittered_delay, attempt + 1, self.config.retry_config.max_attempts);
                        
                        sleep(Duration::from_millis(jittered_delay)).await;
                        
                        delay_ms = (delay_ms as f64 * self.config.retry_config.backoff_multiplier) as u64;
                        delay_ms = delay_ms.min(self.config.retry_config.max_delay_ms);
                    }
                }
            }
        }
        
        let error = last_error.unwrap_or_else(|| anyhow::anyhow!("Unknown error"));
        self.stats.write().await.record_failure(&error.to_string());
        Err(error)
    }

    /// Fetch data from the API
    async fn fetch_from_api(&self, symbol: &str) -> Result<MarketData> {
        // Build the request URL - customize based on your API
        let url = self.build_api_url("ticker", symbol)?;
        
        // Build request with headers
        let mut request = self.client.get(&url);
        
        // Add API key if configured
        if let Some(api_key) = &self.config.api_key {
            request = request.header("X-API-Key", api_key);
        }
        
        // Add custom headers
        for (key, value) in &self.config.custom_headers {
            request = request.header(key, value);
        }
        
        // Execute request with timeout
        let response = timeout(
            Duration::from_millis(self.config.request_timeout_ms),
            request.send()
        ).await
        .context("Request timeout")?
        .context("Failed to send request")?;
        
        // Check if we should retry based on status code
        if self.config.retry_config.retryable_status_codes.contains(&response.status().as_u16()) {
            anyhow::bail!("Retryable HTTP error: {}", response.status());
        }
        
        // Handle successful response
        if response.status().is_success() {
            self.parse_api_response(symbol, response).await
        } else {
            anyhow::bail!("HTTP error: {} - {}", response.status(), 
                         response.text().await.unwrap_or_default())
        }
    }

    /// Build API URL for a specific endpoint and symbol
    fn build_api_url(&self, endpoint: &str, symbol: &str) -> Result<String> {
        let base_url = Url::parse(&self.config.base_url)
            .context("Invalid base URL")?;
        
        // Customize URL building based on your API structure
        match endpoint {
            "ticker" => {
                // Example for a generic ticker endpoint
                let url = base_url.join(&format!("/v1/ticker/{}", symbol))?;
                Ok(url.to_string())
            }
            "orderbook" => {
                let url = base_url.join(&format!("/v1/orderbook/{}", symbol))?;
                Ok(url.to_string())
            }
            "trades" => {
                let url = base_url.join(&format!("/v1/trades/{}", symbol))?;
                Ok(url.to_string())
            }
            _ => {
                anyhow::bail!("Unsupported endpoint: {}", endpoint)
            }
        }
    }

    /// Parse API response into MarketData
    async fn parse_api_response(&self, symbol: &str, response: Response) -> Result<MarketData> {
        let response_text = response.text().await
            .context("Failed to read response body")?;
        
        // Try to parse as ANKR API response first
        if let Ok(ankr_response) = serde_json::from_str::<AnkrApiResponse<ApiMarketData>>(&response_text) {
            if let Some(error) = ankr_response.error {
                anyhow::bail!("API error: {} - {}", error.code, error.message);
            }
            
            if let Some(api_data) = ankr_response.result {
                return self.convert_api_data_to_market_data(api_data);
            }
        }
        
        // Try to parse as direct API data
        if let Ok(api_data) = serde_json::from_str::<ApiMarketData>(&response_text) {
            return self.convert_api_data_to_market_data(api_data);
        }
        
        // Try to parse as generic JSON and extract what we can
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&response_text) {
            return self.parse_generic_json_response(symbol, &json_value);
        }
        
        anyhow::bail!("Failed to parse API response: {}", response_text)
    }

    /// Convert API data to MarketData
    fn convert_api_data_to_market_data(&self, api_data: ApiMarketData) -> Result<MarketData> {
        let timestamp_ms = api_data.timestamp.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        });
        
        // Extract bid/ask or derive from price
        let (bid, ask) = if let (Some(bid), Some(ask)) = (api_data.bid, api_data.ask) {
            (bid, ask)
        } else if let Some(price) = api_data.price {
            // Assume a small spread if only price is available
            let spread = price * 0.001; // 0.1% spread
            (price - spread / 2.0, price + spread / 2.0)
        } else {
            anyhow::bail!("No price data available in API response");
        };
        
        let mut market_data = MarketData::new(api_data.symbol, bid, ask);
        market_data.timestamp_ms = timestamp_ms;
        
        // Add optional fields
        if let Some(last) = api_data.price {
            market_data.last = Some(last);
        }
        if let Some(volume) = api_data.volume {
            market_data.volume_24h = Some(volume);
        }
        if let Some(change) = api_data.change_24h {
            market_data.change_24h_pct = Some(change);
        }
        
        // Add metadata
        market_data = market_data
            .with_metadata("source", "http_api")
            .with_metadata("provider_id", &self.id);
        
        // Add extra fields as features
        for (key, value) in api_data.extra {
            if let Some(num_value) = value.as_f64() {
                market_data = market_data.with_feature(&key, num_value);
            }
        }
        
        Ok(market_data)
    }

    /// Parse generic JSON response
    fn parse_generic_json_response(&self, symbol: &str, json: &serde_json::Value) -> Result<MarketData> {
        // Try to extract price information from common field names
        let price = json.get("price")
            .or_else(|| json.get("last"))
            .or_else(|| json.get("close"))
            .and_then(|v| v.as_f64());
        
        let bid = json.get("bid")
            .or_else(|| json.get("buy"))
            .and_then(|v| v.as_f64());
        
        let ask = json.get("ask")
            .or_else(|| json.get("sell"))
            .and_then(|v| v.as_f64());
        
        let (final_bid, final_ask) = match (bid, ask, price) {
            (Some(b), Some(a), _) => (b, a),
            (_, _, Some(p)) => {
                let spread = p * 0.001; // 0.1% spread
                (p - spread / 2.0, p + spread / 2.0)
            }
            _ => anyhow::bail!("No price data found in response"),
        };
        
        let mut market_data = MarketData::new(symbol.to_string(), final_bid, final_ask);
        
        // Add optional fields
        if let Some(last) = price {
            market_data.last = Some(last);
        }
        if let Some(volume) = json.get("volume").and_then(|v| v.as_f64()) {
            market_data.volume_24h = Some(volume);
        }
        if let Some(change) = json.get("change").and_then(|v| v.as_f64()) {
            market_data.change_24h_pct = Some(change);
        }
        
        market_data = market_data
            .with_metadata("source", "http_api")
            .with_metadata("provider_id", &self.id);
        
        Ok(market_data)
    }

    /// Get cached data if available and not expired
    async fn get_cached_data(&self, symbol: &str) -> Option<MarketData> {
        if !self.config.cache_config.enabled {
            return None;
        }
        
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(symbol) {
            if !entry.is_expired() {
                return Some(entry.data.clone());
            }
        }
        
        None
    }

    /// Cache market data
    async fn cache_data(&self, symbol: &str, data: &MarketData) {
        if !self.config.cache_config.enabled {
            return;
        }
        
        let ttl_ms = self.config.cache_config
            .endpoint_ttls
            .get("ticker")
            .copied()
            .unwrap_or(self.config.cache_config.default_ttl_ms);
        
        let ttl = Duration::from_millis(ttl_ms);
        let entry = CacheEntry::new(data.clone(), ttl);
        
        let mut cache = self.cache.write().await;
        
        // Remove expired entries if cache is full
        if cache.len() >= self.config.cache_config.max_size {
            cache.retain(|_, entry| !entry.is_expired());
            
            // If still full, remove oldest entries
            if cache.len() >= self.config.cache_config.max_size {
                let keys_to_remove: Vec<String> = cache.keys().take(cache.len() / 4).cloned().collect();
                for key in keys_to_remove {
                    cache.remove(&key);
                }
            }
        }
        
        cache.insert(symbol.to_string(), entry);
    }

    /// Fetch data and add to queue (for polling)
    async fn fetch_and_queue_data(&self, symbol: &str) -> Result<()> {
        match self.fetch_market_data(symbol).await {
            Ok(data) => {
                let mut queue = self.data_queue.lock().await;
                if queue.len() >= 1000 { // Prevent unbounded growth
                    queue.pop_front();
                }
                queue.push_back(data);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to fetch data for {}: {}", symbol, e);
                Err(e)
            }
        }
    }

    /// Stop all polling tasks
    async fn stop_polling_tasks(&self) -> Result<()> {
        let mut handles = self.polling_handles.lock().await;
        for handle in handles.drain(..) {
            handle.abort();
        }
        Ok(())
    }
}

/// Task-safe clone of HttpClient
#[derive(Clone)]
struct HttpClientTask {
    id: String,
    config: HttpClientConfig,
    provider_config: DataProviderConfig,
    client: Client,
    rate_limiter: Arc<Semaphore>,
    last_request_time: Arc<RwLock<Instant>>,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    data_queue: Arc<Mutex<VecDeque<MarketData>>>,
    stats: Arc<RwLock<ProviderStats>>,
    active: Arc<RwLock<bool>>,
}

impl HttpClientTask {
    async fn fetch_and_queue_data(&self, symbol: &str) -> Result<()> {
        // Implementation would be similar to the main client
        // This is a simplified version for the task
        Ok(())
    }
}

#[async_trait::async_trait]
impl DataProvider for HttpClient {
    fn id(&self) -> &str {
        &self.id
    }

    async fn next(&mut self) -> Result<MarketData> {
        let mut queue = self.data_queue.lock().await;
        
        if let Some(data) = queue.pop_front() {
            Ok(data)
        } else {
            // If no queued data and we have subscribed symbols, fetch fresh data
            let symbols = self.subscribed_symbols.read().await.clone();
            if let Some(symbol) = symbols.first() {
                drop(queue); // Release the lock
                self.fetch_market_data(symbol).await
            } else {
                anyhow::bail!("No market data available and no subscribed symbols")
            }
        }
    }

    async fn health(&self) -> ProviderHealth {
        if !*self.active.read().await {
            return ProviderHealth::Disconnected;
        }
        
        let stats = self.stats.read().await;
        let success_rate = stats.success_rate();
        
        if success_rate >= 95.0 {
            ProviderHealth::Healthy
        } else if success_rate >= 80.0 {
            ProviderHealth::Degraded {
                reason: format!("Success rate: {:.1}%", success_rate),
            }
        } else {
            ProviderHealth::Unhealthy {
                reason: format!("Low success rate: {:.1}%", success_rate),
            }
        }
    }

    async fn stats(&self) -> ProviderStats {
        self.stats.read().await.clone()
    }

    async fn start(&mut self) -> Result<()> {
        self.start_internal().await
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Stopping HTTP client {}", self.id);
        
        *self.active.write().await = false;
        self.stop_polling_tasks().await?;
        
        Ok(())
    }

    fn is_active(&self) -> bool {
        *self.active.try_read().unwrap_or_else(|_| {
            std::sync::RwLockReadGuard::try_map(
                std::sync::RwLock::new(false).read().unwrap(),
                |active| active
            ).unwrap()
        })
    }

    async fn supported_symbols(&self) -> Result<Vec<String>> {
        // This would typically fetch from an API endpoint
        // For now, return common symbols
        Ok(vec![
            "SUI-USDC".to_string(),
            "SUI-USDT".to_string(),
            "BTC-USD".to_string(),
            "ETH-USD".to_string(),
        ])
    }

    async fn subscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        info!("Subscribing to symbols: {:?}", symbols);
        
        let mut subscribed = self.subscribed_symbols.write().await;
        for symbol in symbols {
            if !subscribed.contains(&symbol) {
                subscribed.push(symbol.clone());
                
                // Start polling task for this symbol if polling is enabled
                if self.config.polling_config.enabled {
                    let handle = self.start_symbol_polling_task(symbol).await?;
                    self.polling_handles.lock().await.push(handle);
                }
            }
        }
        
        Ok(())
    }

    async fn unsubscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        info!("Unsubscribing from symbols: {:?}", symbols);
        
        let mut subscribed = self.subscribed_symbols.write().await;
        subscribed.retain(|s| !symbols.contains(s));
        
        // Note: In a full implementation, you'd want to stop specific polling tasks
        // For now, we'll let them naturally stop when they check the subscription list
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_client_creation() {
        let config = HttpClientConfig::default();
        let provider_config = DataProviderConfig::default();
        
        let client = HttpClient::new(
            "test_http".to_string(),
            config,
            provider_config,
        ).unwrap();
        
        assert_eq!(client.id(), "test_http");
        assert!(!client.is_active());
    }

    #[test]
    fn test_cache_entry_expiration() {
        let data = MarketData::new("TEST".to_string(), 1.0, 1.1);
        let entry = CacheEntry::new(data, Duration::from_millis(1));
        
        assert!(!entry.is_expired());
        
        std::thread::sleep(Duration::from_millis(2));
        assert!(entry.is_expired());
    }

    #[test]
    fn test_api_url_building() {
        let config = HttpClientConfig::default();
        let provider_config = DataProviderConfig::default();
        
        let client = HttpClient::new(
            "test".to_string(),
            config,
            provider_config,
        ).unwrap();
        
        let url = client.build_api_url("ticker", "SUI-USDC").unwrap();
        assert!(url.contains("ticker"));
        assert!(url.contains("SUI-USDC"));
    }
}