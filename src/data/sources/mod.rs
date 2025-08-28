//! Source adapters registry.
//!
//! This module exposes concrete data source implementations used by
//! the data layer. Each submodule encapsulates a particular
//! integration, such as a WebSocket feed or a decentralized exchange
//! (DEX) API. By centralizing the module declarations here you can
//! easily add new sources or re‑export commonly used types.

pub mod ankr_ws;
pub mod cetus;
pub mod dex;
pub mod rpc;
pub mod ws;

// Re‑export a few core types for ergonomic access from downstream
// crates. Consumers can `use crate::data::sources::{AnkrWsSource, CetusSource}`
// instead of reaching into the submodules directly.
pub use ankr_ws::AnkrWsSource;
pub use cetus::CetusSource;
pub use dex::{LiquiditySource, PoolIntrospection, QuoteRequest, QuoteResponse, QuoteSource};