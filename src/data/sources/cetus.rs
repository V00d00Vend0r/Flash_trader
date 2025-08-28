//! Cetus DEX adapter.
//!
//! This module provides a concrete implementation of both
//! [`QuoteSource`] and optionally [`WsSource`] for the Cetus
//! decentralised exchange. Cetus may expose WebSocket feeds for
//! pool state updates and a REST/JSON‑RPC API for on‑demand quotes.
//!
//! The current implementation demonstrates how to compose the
//! abstractions defined in `dex.rs` and `ws.rs` while leaving room
//! to flesh out the actual interaction with the Cetus protocols.

use tokio::sync::broadcast;

use crate::data::sources::dex::{QuoteRequest, QuoteResponse, QuoteSource};
use crate::data::sources::rpc::RpcClient;
use crate::data::sources::ws::{NormalisedEvent, WsAdapter, WsSource};
use crate::data::websocket_client::{WsClient, WsConfig, WsEvent as RawWsEvent};

/// A DEX adapter for Cetus.
///
/// The `CetusSource` can provide price quotes via the Cetus JSON API
/// and can optionally subscribe to a WebSocket feed for real‑time
/// pool updates. The fields are kept optional so that either mode
/// can be used independently.
pub struct CetusSource {
    rpc: Option<RpcClient>,
    ws_adapter: Option<WsAdapter>,
}

impl CetusSource {
    /// Construct a new Cetus source with an HTTP RPC endpoint.
    pub fn new_rpc(endpoint: impl Into<String>) -> Self {
        let rpc = RpcClient::new(endpoint);
        Self {
            rpc: Some(rpc),
            ws_adapter: None,
        }
    }

    /// Construct a new Cetus source with a WebSocket endpoint and
    /// optional subscription payload. If `subscribe_payload` is
    /// provided it will be sent immediately upon connection.
    pub fn new_ws(url: impl Into<String>, subscribe_payload: Option<String>) -> Self {
        let cfg = WsConfig {
            url: url.into(),
            subscribe_payload,
            ..Default::default()
        };
        let (client, events) = WsClient::new(cfg);
        let adapter = WsAdapter::new(client, events);
        Self {
            rpc: None,
            ws_adapter: Some(adapter),
        }
    }
}

impl QuoteSource for CetusSource {
    fn quote(&self, req: QuoteRequest) -> QuoteResponse {
        // A realistic implementation would call into the Cetus HTTP API
        // or perform on‑chain computations to determine the expected
        // output. Here we simply echo the input as output and take
        // 0.3% fee as an example. Note: decimal arithmetic is
        // truncated for simplicity.
        let fee_fraction = 0.003;
        let fee = (req.amount_in as f64 * fee_fraction).ceil() as u128;
        let amount_out = req.amount_in - fee;
        QuoteResponse {
            amount_out,
            fee_amount: fee,
        }
    }
}

impl WsSource for CetusSource {
    type Event = NormalisedEvent;

    fn spawn(&self) -> tokio::task::JoinHandle<()> {
        if let Some(adapter) = &self.ws_adapter {
            adapter.spawn()
        } else {
            // If no WebSocket configured, spawn a no‑op task.
            tokio::spawn(async move {})
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<Self::Event> {
        if let Some(adapter) = &self.ws_adapter {
            adapter.subscribe()
        } else {
            let (tx, rx) = broadcast::channel(1);
            // Immediately drop the sender; there will never be any events.
            drop(tx);
            rx
        }
    }

    fn send_text(&self, msg: impl Into<String> + Send) -> tokio::task::JoinHandle<Result<(), String>> {
        if let Some(adapter) = &self.ws_adapter {
            adapter.send_text(msg)
        } else {
            // Without a WebSocket connection, sending text is meaningless. Return an error.
            tokio::spawn(async move { Err("no websocket configured".into()) })
        }
    }

    fn stop(&self) -> tokio::task::JoinHandle<()> {
        if let Some(adapter) = &self.ws_adapter {
            adapter.stop()
        } else {
            tokio::spawn(async move {})
        }
    }
}