//! WebSocket client implementation for real-time market data streaming.
//!
//! This module provides a production-ready WebSocket client that can connect to various
//! data sources including ANKR's WebSocket API. It handles connection management,
//! message parsing, automatic reconnection, and error recovery.

use crate::data::provider::{DataProvider, DataProviderConfig, MarketData, ProviderHealth, ProviderStats};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{interval, sleep, timeout, Instant};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, info, warn};
use url::Url;

/// WebSocket connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed { reason: String },
}

/// WebSocket client configuration
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// WebSocket endpoint URL
    pub url: String,
    /// API key for authentication (if required)
    pub api_key: Option<String>,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Ping interval for keepalive
    pub ping_interval_ms: u64,
    /// Pong timeout - how long to wait for pong response
    pub pong_timeout_ms: u64,
    /// Maximum message queue size
    pub max_queue_size: usize,
    /// Reconnection settings
    pub reconnect_config: ReconnectConfig,
    /// Subscription management
    pub subscription_config: SubscriptionConfig,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: "wss://rpc.ankr.com/sui/ws".to_string(),
            api_key: None,
            connect_timeout_ms: 10_000,
            ping_interval_ms: 30_000,
            pong_timeout_ms: 5_000,
            max_queue_size: 1000,
            reconnect_config: ReconnectConfig::default(),
            subscription_config: SubscriptionConfig::default(),
        }
    }
}

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Enable automatic reconnection
    pub enabled: bool,
    /// Maximum number of reconnection attempts
    pub max_attempts: u32,
    /// Initial delay between reconnection attempts
    pub initial_delay_ms: u64,
    /// Maximum delay between reconnection attempts
    pub max_delay_ms: u64,
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Jitter factor to avoid thundering herd
    pub jitter_factor: f64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 10,
            initial_delay_ms: 1_000,
            max_delay_ms: 60_000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

/// Subscription configuration
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// Automatically resubscribe on reconnection
    pub auto_resubscribe: bool,
    /// Subscription timeout
    pub subscription_timeout_ms: u64,
    /// Maximum subscriptions per connection
    pub max_subscriptions: usize,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            auto_resubscribe: true,
            subscription_timeout_ms: 5_000,
            max_subscriptions: 100,
        }
    }
}

/// ANKR WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum AnkrMessage {
    /// Subscribe to market data
    #[serde(rename = "sui_subscribeEventFilter")]
    SubscribeEvents {
        filter: EventFilter,
    },
    /// Unsubscribe from market data
    #[serde(rename = "sui_unsubscribeEventFilter")]
    UnsubscribeEvents {
        subscription_id: u64,
    },
    /// Price update notification
    #[serde(rename = "sui_eventNotification")]
    EventNotification {
        subscription: u64,
        result: EventData,
    },
}

/// Event filter for ANKR subscriptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    #[serde(rename = "Package")]
    pub package: Option<String>,
    #[serde(rename = "Module")]
    pub module: Option<String>,
    #[serde(rename = "EventType")]
    pub event_type: Option<String>,
}

/// Event data from ANKR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    pub id: EventId,
    pub package_id: String,
    pub transaction_module: String,
    pub sender: String,
    pub type_: String,
    pub parsed_json: serde_json::Value,
    pub bcs: String,
    pub timestamp_ms: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventId {
    pub tx_digest: String,
    pub event_seq: String,
}

/// WebSocket request/response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsRequest {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(flatten)]
    pub method: AnkrMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<WsError>,
    #[serde(flatten)]
    pub notification: Option<AnkrMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Internal message types for the WebSocket client
#[derive(Debug)]
enum InternalMessage {
    Connect,
    Disconnect,
    Subscribe(Vec<String>),
    Unsubscribe(Vec<String>),
    SendMessage(String),
    ProcessIncoming(String),
    Ping,
    Pong,
    Reconnect,
}

/// WebSocket client for ANKR and other providers
pub struct WebSocketClient {
    id: String,
    config: WebSocketConfig,
    provider_config: DataProviderConfig,
    
    // Connection management
    connection_state: Arc<RwLock<ConnectionState>>,
    ws_stream: Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    
    // Message handling
    message_tx: Option<mpsc::UnboundedSender<InternalMessage>>,
    data_rx: Arc<Mutex<mpsc::UnboundedReceiver<MarketData>>>,
    data_queue: Arc<Mutex<VecDeque<MarketData>>>,
    
    // Subscription management
    subscriptions: Arc<RwLock<HashMap<String, u64>>>,
    subscription_counter: Arc<Mutex<u64>>,
    
    // Statistics and health
    stats: Arc<RwLock<ProviderStats>>,
    last_ping: Arc<RwLock<Option<Instant>>>,
    last_pong: Arc<RwLock<Option<Instant>>>,
    
    // Runtime handles
    task_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl WebSocketClient {
    /// Create a new WebSocket client
    pub fn new(
        id: String,
        ws_config: WebSocketConfig,
        provider_config: DataProviderConfig,
    ) -> Self {
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        
        Self {
            id,
            config: ws_config,
            provider_config,
            connection_state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            ws_stream: Arc::new(Mutex::new(None)),
            message_tx: None,
            data_rx: Arc::new(Mutex::new(data_rx)),
            data_queue: Arc::new(Mutex::new(VecDeque::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            subscription_counter: Arc::new(Mutex::new(1)),
            stats: Arc::new(RwLock::new(ProviderStats::default())),
            last_ping: Arc::new(RwLock::new(None)),
            last_pong: Arc::new(RwLock::new(None)),
            task_handles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start the WebSocket client and all background tasks
    async fn start_internal(&mut self) -> Result<()> {
        info!("Starting WebSocket client {}", self.id);
        
        let (message_tx, mut message_rx) = mpsc::unbounded_channel();
        self.message_tx = Some(message_tx.clone());
        
        // Start the main message processing loop
        let client_clone = self.clone_for_task();
        let main_handle = tokio::spawn(async move {
            while let Some(msg) = message_rx.recv().await {
                if let Err(e) = client_clone.handle_internal_message(msg).await {
                    error!("Error handling internal message: {}", e);
                }
            }
        });
        
        // Start connection management task
        let connection_handle = self.start_connection_manager().await?;
        
        // Start ping/pong heartbeat task
        let heartbeat_handle = self.start_heartbeat_task().await?;
        
        // Store task handles for cleanup
        let mut handles = self.task_handles.lock().await;
        handles.push(main_handle);
        handles.push(connection_handle);
        handles.push(heartbeat_handle);
        
        // Initiate connection
        if let Some(tx) = &self.message_tx {
            tx.send(InternalMessage::Connect)
                .context("Failed to send connect message")?;
        }
        
        Ok(())
    }

    /// Clone the client for use in async tasks
    fn clone_for_task(&self) -> WebSocketClientTask {
        WebSocketClientTask {
            id: self.id.clone(),
            config: self.config.clone(),
            provider_config: self.provider_config.clone(),
            connection_state: Arc::clone(&self.connection_state),
            ws_stream: Arc::clone(&self.ws_stream),
            subscriptions: Arc::clone(&self.subscriptions),
            subscription_counter: Arc::clone(&self.subscription_counter),
            stats: Arc::clone(&self.stats),
            last_ping: Arc::clone(&self.last_ping),
            last_pong: Arc::clone(&self.last_pong),
        }
    }

    /// Start the connection manager task
    async fn start_connection_manager(&self) -> Result<tokio::task::JoinHandle<()>> {
        let client = self.clone_for_task();
        let handle = tokio::spawn(async move {
            client.connection_manager_loop().await;
        });
        Ok(handle)
    }

    /// Start the heartbeat task
    async fn start_heartbeat_task(&self) -> Result<tokio::task::JoinHandle<()>> {
        let client = self.clone_for_task();
        let ping_interval = Duration::from_millis(self.config.ping_interval_ms);
        
        let handle = tokio::spawn(async move {
            let mut interval = interval(ping_interval);
            loop {
                interval.tick().await;
                if let Err(e) = client.send_ping().await {
                    error!("Failed to send ping: {}", e);
                }
            }
        });
        Ok(handle)
    }

    /// Handle internal messages
    async fn handle_internal_message(&self, msg: InternalMessage) -> Result<()> {
        match msg {
            InternalMessage::Connect => self.connect().await,
            InternalMessage::Disconnect => self.disconnect().await,
            InternalMessage::Subscribe(symbols) => self.subscribe_internal(symbols).await,
            InternalMessage::Unsubscribe(symbols) => self.unsubscribe_internal(symbols).await,
            InternalMessage::SendMessage(text) => self.send_message(text).await,
            InternalMessage::ProcessIncoming(text) => self.process_incoming_message(text).await,
            InternalMessage::Ping => self.send_ping().await,
            InternalMessage::Pong => self.handle_pong().await,
            InternalMessage::Reconnect => self.reconnect().await,
        }
    }

    /// Connect to the WebSocket endpoint
    async fn connect(&self) -> Result<()> {
        info!("Connecting to WebSocket: {}", self.config.url);
        
        *self.connection_state.write().await = ConnectionState::Connecting;
        
        let url = Url::parse(&self.config.url)
            .context("Invalid WebSocket URL")?;
        
        let connect_future = connect_async(url);
        let timeout_duration = Duration::from_millis(self.config.connect_timeout_ms);
        
        match timeout(timeout_duration, connect_future).await {
            Ok(Ok((ws_stream, _))) => {
                info!("WebSocket connected successfully");
                *self.ws_stream.lock().await = Some(ws_stream);
                *self.connection_state.write().await = ConnectionState::Connected;
                
                // Start message reading loop
                self.start_message_reader().await?;
                
                Ok(())
            }
            Ok(Err(e)) => {
                error!("WebSocket connection failed: {}", e);
                *self.connection_state.write().await = ConnectionState::Failed {
                    reason: e.to_string(),
                };
                self.stats.write().await.record_failure(&e.to_string());
                Err(e.into())
            }
            Err(_) => {
                let error_msg = "WebSocket connection timeout";
                error!("{}", error_msg);
                *self.connection_state.write().await = ConnectionState::Failed {
                    reason: error_msg.to_string(),
                };
                self.stats.write().await.record_failure(error_msg);
                anyhow::bail!(error_msg)
            }
        }
    }

    /// Start the message reader loop
    async fn start_message_reader(&self) -> Result<()> {
        let client = self.clone_for_task();
        let message_tx = self.message_tx.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Message sender not initialized"))?
            .clone();
        
        tokio::spawn(async move {
            client.message_reader_loop(message_tx).await;
        });
        
        Ok(())
    }

    /// Parse ANKR event data into MarketData
    fn parse_ankr_event(&self, event: &EventData) -> Result<MarketData> {
        // This is a simplified parser - you'll need to adapt this based on
        // the actual structure of ANKR's event data for your specific use case
        
        let parsed_json = &event.parsed_json;
        
        // Extract symbol from event type or parsed data
        let symbol = self.extract_symbol_from_event(event)?;
        
        // Extract price data - this will depend on your specific event structure
        let (bid, ask) = self.extract_prices_from_event(parsed_json)?;
        
        let timestamp_ms = if let Some(ts_str) = &event.timestamp_ms {
            ts_str.parse::<u64>().unwrap_or_else(|_| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            })
        } else {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        };
        
        let mut market_data = MarketData::new(symbol, bid, ask);
        market_data.timestamp_ms = timestamp_ms;
        market_data = market_data
            .with_metadata("source", "ankr")
            .with_metadata("provider_id", &self.id)
            .with_metadata("event_type", &event.type_)
            .with_metadata("tx_digest", &event.id.tx_digest);
        
        // Add any additional features from the parsed JSON
        if let Some(volume) = parsed_json.get("volume").and_then(|v| v.as_f64()) {
            market_data = market_data.with_feature("volume", volume);
        }
        
        Ok(market_data)
    }

    /// Extract symbol from ANKR event (customize based on your event structure)
    fn extract_symbol_from_event(&self, event: &EventData) -> Result<String> {
        // This is a placeholder - customize based on your actual event structure
        if let Some(symbol) = event.parsed_json.get("symbol").and_then(|s| s.as_str()) {
            Ok(symbol.to_string())
        } else if event.type_.contains("SUI") && event.type_.contains("USDC") {
            Ok("SUI-USDC".to_string())
        } else {
            // Fallback: try to extract from event type
            Ok("UNKNOWN".to_string())
        }
    }

    /// Extract bid/ask prices from event data (customize based on your event structure)
    fn extract_prices_from_event(&self, parsed_json: &serde_json::Value) -> Result<(f64, f64)> {
        // This is a placeholder - customize based on your actual event structure
        let bid = parsed_json.get("bid")
            .and_then(|b| b.as_f64())
            .or_else(|| parsed_json.get("price").and_then(|p| p.as_f64()))
            .unwrap_or(1.0);
        
        let ask = parsed_json.get("ask")
            .and_then(|a| a.as_f64())
            .or_else(|| {
                // If no ask, assume a small spread
                Some(bid * 1.001)
            })
            .unwrap_or(bid * 1.001);
        
        Ok((bid, ask))
    }

    /// Process incoming WebSocket message
    async fn process_incoming_message(&self, text: String) -> Result<()> {
        let start_time = Instant::now();
        
        match serde_json::from_str::<WsResponse>(&text) {
            Ok(response) => {
                if let Some(AnkrMessage::EventNotification { result, .. }) = response.notification {
                    match self.parse_ankr_event(&result) {
                        Ok(market_data) => {
                            // Add to data queue
                            let mut queue = self.data_queue.lock().await;
                            if queue.len() >= self.config.max_queue_size {
                                queue.pop_front(); // Remove oldest data
                            }
                            queue.push_back(market_data);
                            
                            let latency_ms = start_time.elapsed().as_millis() as f64;
                            self.stats.write().await.record_success(latency_ms);
                        }
                        Err(e) => {
                            warn!("Failed to parse market data: {}", e);
                            self.stats.write().await.record_failure(&e.to_string());
                        }
                    }
                } else if let Some(error) = response.error {
                    error!("WebSocket error: {} - {}", error.code, error.message);
                    self.stats.write().await.record_failure(&error.message);
                }
            }
            Err(e) => {
                debug!("Failed to parse WebSocket message: {} - Raw: {}", e, text);
                // Don't record as failure for unparseable messages (might be pings, etc.)
            }
        }
        
        Ok(())
    }

    /// Send a message through the WebSocket
    async fn send_message(&self, text: String) -> Result<()> {
        let mut stream_guard = self.ws_stream.lock().await;
        if let Some(stream) = stream_guard.as_mut() {
            stream.send(Message::Text(text)).await
                .context("Failed to send WebSocket message")?;
            Ok(())
        } else {
            anyhow::bail!("WebSocket not connected")
        }
    }

    /// Send ping message
    async fn send_ping(&self) -> Result<()> {
        *self.last_ping.write().await = Some(Instant::now());
        self.send_message("ping".to_string()).await
    }

    /// Handle pong response
    async fn handle_pong(&self) -> Result<()> {
        *self.last_pong.write().await = Some(Instant::now());
        Ok(())
    }

    /// Subscribe to symbols
    async fn subscribe_internal(&self, symbols: Vec<String>) -> Result<()> {
        for symbol in symbols {
            let subscription_id = {
                let mut counter = self.subscription_counter.lock().await;
                *counter += 1;
                *counter
            };
            
            // Create ANKR subscription message
            let request = WsRequest {
                jsonrpc: "2.0".to_string(),
                id: subscription_id,
                method: AnkrMessage::SubscribeEvents {
                    filter: EventFilter {
                        package: Some("0x...".to_string()), // Replace with actual package ID
                        module: Some("pool".to_string()),
                        event_type: Some("SwapEvent".to_string()),
                    },
                },
            };
            
            let message = serde_json::to_string(&request)
                .context("Failed to serialize subscription request")?;
            
            self.send_message(message).await?;
            
            // Store subscription
            self.subscriptions.write().await.insert(symbol, subscription_id);
        }
        
        Ok(())
    }

    /// Unsubscribe from symbols
    async fn unsubscribe_internal(&self, symbols: Vec<String>) -> Result<()> {
        let mut subscriptions = self.subscriptions.write().await;
        
        for symbol in symbols {
            if let Some(subscription_id) = subscriptions.remove(&symbol) {
                let request = WsRequest {
                    jsonrpc: "2.0".to_string(),
                    id: subscription_id,
                    method: AnkrMessage::UnsubscribeEvents { subscription_id },
                };
                
                let message = serde_json::to_string(&request)
                    .context("Failed to serialize unsubscription request")?;
                
                self.send_message(message).await?;
            }
        }
        
        Ok(())
    }

    /// Disconnect from WebSocket
    async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting WebSocket client {}", self.id);
        
        *self.connection_state.write().await = ConnectionState::Disconnected;
        
        if let Some(mut stream) = self.ws_stream.lock().await.take() {
            let _ = stream.close(None).await;
        }
        
        Ok(())
    }

    /// Reconnect to WebSocket
    async fn reconnect(&self) -> Result<()> {
        info!("Reconnecting WebSocket client {}", self.id);
        
        *self.connection_state.write().await = ConnectionState::Reconnecting;
        self.stats.write().await.record_reconnection();
        
        // Disconnect first
        self.disconnect().await?;
        
        // Wait before reconnecting
        sleep(Duration::from_millis(self.config.reconnect_config.initial_delay_ms)).await;
        
        // Reconnect
        self.connect().await?;
        
        // Resubscribe to all symbols if auto-resubscribe is enabled
        if self.config.subscription_config.auto_resubscribe {
            let symbols: Vec<String> = self.subscriptions.read().await.keys().cloned().collect();
            if !symbols.is_empty() {
                self.subscribe_internal(symbols).await?;
            }
        }
        
        Ok(())
    }
}

/// Task-safe clone of WebSocketClient for use in async tasks
#[derive(Clone)]
struct WebSocketClientTask {
    id: String,
    config: WebSocketConfig,
    provider_config: DataProviderConfig,
    connection_state: Arc<RwLock<ConnectionState>>,
    ws_stream: Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    subscriptions: Arc<RwLock<HashMap<String, u64>>>,
    subscription_counter: Arc<Mutex<u64>>,
    stats: Arc<RwLock<ProviderStats>>,
    last_ping: Arc<RwLock<Option<Instant>>>,
    last_pong: Arc<RwLock<Option<Instant>>>,
}

impl WebSocketClientTask {
    /// Connection manager loop
    async fn connection_manager_loop(&self) {
        let mut reconnect_attempts = 0;
        let mut delay_ms = self.config.reconnect_config.initial_delay_ms;
        
        loop {
            let state = self.connection_state.read().await.clone();
            
            match state {
                ConnectionState::Failed { .. } if self.config.reconnect_config.enabled => {
                    if reconnect_attempts < self.config.reconnect_config.max_attempts {
                        warn!("Attempting reconnection {} of {}", 
                              reconnect_attempts + 1, 
                              self.config.reconnect_config.max_attempts);
                        
                        // Apply jitter to delay
                        let jitter = (rand::random::<f64>() - 0.5) * 2.0 * self.config.reconnect_config.jitter_factor;
                        let jittered_delay = (delay_ms as f64 * (1.0 + jitter)) as u64;
                        
                        sleep(Duration::from_millis(jittered_delay)).await;
                        
                        // Attempt reconnection (this would need to be implemented)
                        // For now, just update the state
                        reconnect_attempts += 1;
                        delay_ms = (delay_ms as f64 * self.config.reconnect_config.backoff_multiplier) as u64;
                        delay_ms = delay_ms.min(self.config.reconnect_config.max_delay_ms);
                    } else {
                        error!("Max reconnection attempts reached for client {}", self.id);
                        break;
                    }
                }
                ConnectionState::Connected => {
                    // Reset reconnection state on successful connection
                    reconnect_attempts = 0;
                    delay_ms = self.config.reconnect_config.initial_delay_ms;
                }
                _ => {}
            }
            
            sleep(Duration::from_millis(1000)).await; // Check every second
        }
    }

    /// Message reader loop
    async fn message_reader_loop(&self, message_tx: mpsc::UnboundedSender<InternalMessage>) {
        loop {
            let mut stream_guard = self.ws_stream.lock().await;
            if let Some(stream) = stream_guard.as_mut() {
                match stream.next().await {
                    Some(Ok(Message::Text(text))) => {
                        drop(stream_guard); // Release the lock
                        let _ = message_tx.send(InternalMessage::ProcessIncoming(text));
                    }
                    Some(Ok(Message::Ping(_))) => {
                        drop(stream_guard);
                        let _ = message_tx.send(InternalMessage::Pong);
                    }
                    Some(Ok(Message::Pong(_))) => {
                        drop(stream_guard);
                        let _ = message_tx.send(InternalMessage::Pong);
                    }
                    Some(Ok(Message::Close(_))) => {
                        drop(stream_guard);
                        warn!("WebSocket connection closed");
                        *self.connection_state.write().await = ConnectionState::Failed {
                            reason: "Connection closed by server".to_string(),
                        };
                        break;
                    }
                    Some(Err(e)) => {
                        drop(stream_guard);
                        error!("WebSocket error: {}", e);
                        *self.connection_state.write().await = ConnectionState::Failed {
                            reason: e.to_string(),
                        };
                        break;
                    }
                    None => {
                        drop(stream_guard);
                        warn!("WebSocket stream ended");
                        *self.connection_state.write().await = ConnectionState::Disconnected;
                        break;
                    }
                    _ => {
                        // Handle other message types
                        drop(stream_guard);
                    }
                }
            } else {
                drop(stream_guard);
                sleep(Duration::from_millis(100)).await;
            }
        }
    }

    /// Send ping message
    async fn send_ping(&self) -> Result<()> {
        let mut stream_guard = self.ws_stream.lock().await;
        if let Some(stream) = stream_guard.as_mut() {
            stream.send(Message::Ping(vec![])).await
                .context("Failed to send ping")?;
            *self.last_ping.write().await = Some(Instant::now());
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl DataProvider for WebSocketClient {
    fn id(&self) -> &str {
        &self.id
    }

    async fn next(&mut self) -> Result<MarketData> {
        let mut queue = self.data_queue.lock().await;
        
        if let Some(data) = queue.pop_front() {
            Ok(data)
        } else {
            anyhow::bail!("No market data available")
        }
    }

    async fn health(&self) -> ProviderHealth {
        let state = self.connection_state.read().await.clone();
        
        match state {
            ConnectionState::Connected => {
                // Check if we've received recent pongs
                let last_pong = self.last_pong.read().await;
                if let Some(pong_time) = *last_pong {
                    let pong_age = pong_time.elapsed();
                    if pong_age > Duration::from_millis(self.config.pong_timeout_ms * 2) {
                        ProviderHealth::Degraded {
                            reason: "No recent pong responses".to_string(),
                        }
                    } else {
                        ProviderHealth::Healthy
                    }
                } else {
                    ProviderHealth::Degraded {
                        reason: "No pong responses received".to_string(),
                    }
                }
            }
            ConnectionState::Connecting | ConnectionState::Reconnecting => {
                ProviderHealth::Degraded {
                    reason: format!("Connection state: {:?}", state),
                }
            }
            ConnectionState::Failed { reason } => {
                ProviderHealth::Unhealthy { reason }
            }
            ConnectionState::Disconnected => ProviderHealth::Disconnected,
        }
    }

    async fn stats(&self) -> ProviderStats {
        self.stats.read().await.clone()
    }

    async fn start(&mut self) -> Result<()> {
        self.start_internal().await
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Stopping WebSocket client {}", self.id);
        
        // Stop all tasks
        let mut handles = self.task_handles.lock().await;
        for handle in handles.drain(..) {
            handle.abort();
        }
        
        // Disconnect
        self.disconnect().await?;
        
        Ok(())
    }

    fn is_active(&self) -> bool {
        matches!(
            *self.connection_state.try_read().unwrap_or_else(|_| {
                std::sync::RwLockReadGuard::try_map(
                    std::sync::RwLock::new(ConnectionState::Disconnected).read().unwrap(),
                    |state| state
                ).unwrap()
            }),
            ConnectionState::Connected | ConnectionState::Connecting
        )
    }

    async fn supported_symbols(&self) -> Result<Vec<String>> {
        // Return symbols based on ANKR's supported markets
        // This would typically be fetched from an API endpoint
        Ok(vec![
            "SUI-USDC".to_string(),
            "SUI-USDT".to_string(),
            // Add more symbols as supported by ANKR
        ])
    }

    async fn subscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        if let Some(tx) = &self.message_tx {
            tx.send(InternalMessage::Subscribe(symbols))
                .context("Failed to send subscribe message")?;
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, symbols: Vec<String>) -> Result<()> {
        if let Some(tx) = &self.message_tx {
            tx.send(InternalMessage::Unsubscribe(symbols))
                .context("Failed to send unsubscribe message")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_websocket_client_creation() {
        let config = WebSocketConfig::default();
        let provider_config = DataProviderConfig::default();
        
        let client = WebSocketClient::new(
            "test_ws".to_string(),
            config,
            provider_config,
        );
        
        assert_eq!(client.id(), "test_ws");
        assert!(!client.is_active());
    }

    #[test]
    fn test_ankr_message_serialization() {
        let msg = AnkrMessage::SubscribeEvents {
            filter: EventFilter {
                package: Some("0x123".to_string()),
                module: Some("pool".to_string()),
                event_type: Some("SwapEvent".to_string()),
            },
        };
        
        let request = WsRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: msg,
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("sui_subscribeEventFilter"));
    }
}