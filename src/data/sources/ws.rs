//! Generic WebSocket source plumbing.
//!
//! This module defines a simple abstraction over the [`WsClient`]
//! defined in `crate::data::websocket_client`. The idea is to provide a
//! common trait for any WebSocket based data source and a set of
//! utility types for normalising events. Concrete sources such as
//! `ankr_ws` or future exchange feeds will implement this trait to
//! expose their events in a consistent manner.

use std::fmt;

use tokio::sync::broadcast;

use crate::data::websocket_client::{WsClient, WsEvent as RawWsEvent};

/// A simplified, normalised WebSocket event used by higher layers.
///
/// Instead of exposing the raw tungstenite messages, adapters
/// translate incoming frames into this enum. Additional metadata
/// (such as parsed JSON) can be layered in by concrete sources.
#[derive(Debug, Clone)]
pub enum NormalisedEvent {
    /// A UTF‑8 text message.
    Text(String),
    /// A binary message.
    Binary(Vec<u8>),
    /// A pong heartbeat.
    Pong,
    /// The underlying socket disconnected.
    Disconnected(String),
    /// The source was explicitly stopped.
    Stopped,
}

impl From<RawWsEvent> for NormalisedEvent {
    fn from(evt: RawWsEvent) -> Self {
        match evt {
            RawWsEvent::Text(t) => NormalisedEvent::Text(t),
            RawWsEvent::Binary(b) => NormalisedEvent::Binary(b),
            RawWsEvent::Pong => NormalisedEvent::Pong,
            RawWsEvent::Disconnected(reason) => NormalisedEvent::Disconnected(reason),
            RawWsEvent::Stopped => NormalisedEvent::Stopped,
            // Other events (Connected, Subscribed) are not mapped and can be handled by
            // the adapter if desired. Here we ignore them.
            _ => NormalisedEvent::Pong,
        }
    }
}

impl fmt::Display for NormalisedEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NormalisedEvent::Text(_) => write!(f, "Text"),
            NormalisedEvent::Binary(_) => write!(f, "Binary"),
            NormalisedEvent::Pong => write!(f, "Pong"),
            NormalisedEvent::Disconnected(reason) => write!(f, "Disconnected: {reason}"),
            NormalisedEvent::Stopped => write!(f, "Stopped"),
        }
    }
}

/// A trait for any WebSocket source.
///
/// The trait defines a minimal interface for starting the underlying
/// connection, exposing a broadcast channel of normalised events and
/// performing graceful shutdown. Concrete implementors can embed
/// additional behaviour such as JSON parsing or domain specific
/// filtering on top of this basic contract.
pub trait WsSource: Send + Sync {
    /// The type of event this source emits.
    type Event: Clone + Send + 'static;

    /// Spawn the background task that drives the WebSocket
    /// connection. This is typically delegated to [`WsClient::spawn`].
    fn spawn(&self) -> tokio::task::JoinHandle<()>;

    /// Obtain a new broadcast receiver for events.
    fn subscribe(&self) -> broadcast::Receiver<Self::Event>;

    /// Send a raw text payload to the remote peer. For sources that
    /// support RPC requests (such as JSON‑RPC), this can be used to
    /// emit arbitrary messages. Returns an error if the underlying
    /// send channel has closed.
    fn send_text(&self, msg: impl Into<String> + Send) -> tokio::task::JoinHandle<Result<(), String>>;

    /// Request a graceful stop of the connection. Implementors should
    /// propagate this to the underlying [`WsClient`] stop signal.
    fn stop(&self) -> tokio::task::JoinHandle<()>;
}

/// A simple adapter that wraps a [`WsClient`] and exposes it as a
/// [`WsSource`].
///
/// Most concrete sources will embed a `WsClient` internally and
/// forward calls through to it. The `WsAdapter` is provided for
/// convenience and is used by the default implementations generated
/// by the `ankr_ws` module.
pub struct WsAdapter {
    client: WsClient,
    events: broadcast::Receiver<RawWsEvent>,
}

impl WsAdapter {
    /// Construct a new adapter given a [`WsClient`] and its event
    /// receiver. This is usually created in tandem via
    /// [`WsClient::new`].
    pub fn new(client: WsClient, events: broadcast::Receiver<RawWsEvent>) -> Self {
        Self { client, events }
    }
}

impl WsSource for WsAdapter {
    type Event = NormalisedEvent;

    fn spawn(&self) -> tokio::task::JoinHandle<()> {
        self.client.clone().spawn()
    }

    fn subscribe(&self) -> broadcast::Receiver<Self::Event> {
        // Map raw events into NormalisedEvent on the fly using a new broadcast
        // channel. Each subscriber gets its own mapping layer.
        let mut inner = self.events.resubscribe();
        let (tx, rx) = broadcast::channel(1024);
        tokio::spawn(async move {
            while let Ok(evt) = inner.recv().await {
                let norm: NormalisedEvent = evt.into();
                let _ = tx.send(norm);
            }
        });
        rx
    }

    fn send_text(&self, msg: impl Into<String> + Send) -> tokio::task::JoinHandle<Result<(), String>> {
        let client = self.client.clone();
        let m = msg.into();
        tokio::spawn(async move {
            client.send_text(m).await
        })
    }

    fn stop(&self) -> tokio::task::JoinHandle<()> {
        let client = self.client.clone();
        tokio::spawn(async move {
            client.stop().await;
        })
    }
}