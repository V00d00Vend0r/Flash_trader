//! Thin JSON‑RPC helper.
//!
//! This module provides a minimal abstraction over JSON‑RPC
//! endpoints. It is designed to be lightweight and easy to extend for
//! additional methods without coupling the rest of the application to
//! a particular HTTP client. The implementation here uses
//! [`reqwest`](https://docs.rs/reqwest/) under the hood, but this is hidden
//! behind the `RpcClient` type.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

/// A simple JSON‑RPC client.
///
/// This client exposes a single `call` method that will perform a
/// JSON‑RPC 2.0 request to the configured `endpoint` with an
/// arbitrary method and parameters. Retries with exponential
/// backoff can be configured via `max_retries` and `backoff`.
#[derive(Clone)]
pub struct RpcClient {
    endpoint: String,
    max_retries: u8,
    backoff: Duration,
}

impl RpcClient {
    /// Create a new RPC client for a given endpoint.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            max_retries: 3,
            backoff: Duration::from_millis(250),
        }
    }

    /// Perform a JSON‑RPC call with arbitrary typed parameters and response.
    ///
    /// The method name must be a string and `params` must be
    /// serialisable into JSON. The return type must implement
    /// [`DeserializeOwned`]. If the call fails, it is retried up to
    /// `max_retries` times with exponential backoff. A failure is
    /// signalled by returning `None`.
    pub async fn call<P, R>(&self, method: &str, params: P) -> Option<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        for attempt in 0..=self.max_retries {
            // Build the JSON‑RPC request body.
            let body = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            });
            let resp = self.send_request(&body).await;
            match resp {
                Ok(val) => {
                    // Try to deserialize the `result` field.
                    if let Some(result) = val.get("result") {
                        let parsed: serde_json::Result<R> = serde_json::from_value(result.clone());
                        if let Ok(ret) = parsed {
                            return Some(ret);
                        }
                    }
                    // Otherwise treat as failure.
                    return None;
                }
                Err(_e) => {
                    // Sleep with exponential backoff.
                    let delay = self.backoff * (1u32 << attempt);
                    sleep(delay).await;
                    continue;
                }
            }
        }
        None
    }

    /// Internal helper that actually performs the HTTP request via reqwest.
    async fn send_request(&self, body: &Value) -> Result<Value, reqwest::Error> {
        let client = reqwest::Client::new();
        let res = client
            .post(&self.endpoint)
            .json(body)
            .send()
            .await?;
        let json = res.json::<Value>().await?;
        Ok(json)
    }
}