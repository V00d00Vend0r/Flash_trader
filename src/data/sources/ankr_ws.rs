//! ANKR Sui WebSocket adapter.
//!
//! This module implements a concrete [`WsSource`] for the Sui
//! WebSocket API provided by ANKR. It builds a subscription payload
//! using the Sui JSON‑RPC `suix_subscribeEvent` method and forwards
//! inbound messages as raw JSON strings. Consumers can further
//! deserialize these into domain types as needed.

use serde_json::json;
use tokio::sync::broadcast;

use crate::data::websocket_client::{WsClient, WsConfig, WsEvent as RawWsEvent};
use crate::data::sources::ws::{NormalisedEvent, WsAdapter, WsSource};

/// A WebSocket source for ANKR's Sui event stream.
///
/// The `AnkrWsSource` encapsulates a [`WsClient`] configured with the
/// appropriate URL and subscription payload. It exposes a
/// [`NormalisedEvent`] stream via the [`WsSource`] trait.
pub struct AnkrWsSource {
    adapter: WsAdapter,
}

impl AnkrWsSource {
    /// Construct a new ANKR Sui WebSocket source.
    ///
    /// * `url` – the WebSocket endpoint (e.g. `wss://rpc.ankr.com/sui/ws`).
    /// * `filters` – a JSON object describing event filters. Pass an
    ///   empty object to subscribe to all events.
    pub fn new(url: impl Into<String>, filters: serde_json::Value) -> Self {
        let subscribe_payload = Some(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "suix_subscribeEvent",
                "params": [filters],
            })
            .to_string(),
        );
        let cfg = WsConfig {
            url: url.into(),
            subscribe_payload,
            ..Default::default()
        };
        let (client, events) = WsClient::new(cfg);
        let adapter = WsAdapter::new(client, events);
        Self { adapter }
    }
}

impl WsSource for AnkrWsSource {
    type Event = NormalisedEvent;

    fn spawn(&self) -> tokio::task::JoinHandle<()> {
        self.adapter.spawn()
    }

    fn subscribe(&self) -> broadcast::Receiver<Self::Event> {
        self.adapter.subscribe()
    }

    fn send_text(&self, msg: impl Into<String> + Send) -> tokio::task::JoinHandle<Result<(), String>> {
        self.adapter.send_text(msg)
    }

    fn stop(&self) -> tokio::task::JoinHandle<()> {
        self.adapter.stop()
    }
}