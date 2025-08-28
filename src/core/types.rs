//! Core data types and structures for the Ravenslinger trading bot.
//!
//! This module defines fundamental types used across the data, decision,
//! and execution layers, ensuring consistent data representation.

use serde::{Deserialize, Serialize};
use std::fmt;
use sui_sdk::types::base_types::{ObjectID, SuiAddress};

/// Represents a trading pair, e.g., SUI-USDC.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TradingPair {
    pub base: String,  // The asset being traded (e.g., "SUI")
    pub quote: String, // The asset used to price the base (e.g., "USDC")
}

impl fmt::Display for TradingPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.base, self.quote)
    }
}

/// Represents a simple bid/ask quote for a trading pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub bid: f64, // The highest price a buyer is willing to pay
    pub ask: f64, // The lowest price a seller is willing to accept
}

/// Represents a snapshot of market data for a trading pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketData {
    pub pair: TradingPair,
    pub price: f64, // Last traded price or mid-price
    pub timestamp_ms: u64, // Timestamp of the data in milliseconds
    pub bid: Option<f64>, // Optional: current best bid
    pub ask: Option<f64>, // Optional: current best ask
    pub volume_24h: Option<f64>, // Optional: 24-hour trading volume
}

/// Represents a specific asset (token) on the Sui blockchain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset {
    pub symbol: String, // e.g., "SUI", "USDC"
    pub coin_type: String, // Full Sui coin type, e.g., "0x2::sui::SUI"
    pub decimals: u8, // Number of decimals for this asset
    pub object_id: Option<ObjectID>, // Optional: object ID if it's a specific coin object
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol)
    }
}

/// Represents a generic order type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
}

/// Represents the side of an order (Buy or Sell).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Represents the status of an order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    Open,
    Filled,
    PartialFill,
    Cancelled,
    Rejected,
    Expired,
}

/// Represents a single trade execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: String,
    pub order_id: String,
    pub pair: TradingPair,
    pub side: OrderSide,
    pub quantity: f64,
    pub price: f64,
    pub fee: f64,
    pub fee_asset: String,
    pub timestamp_ms: u64,
}

/// Represents an active position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub position_id: String,
    pub pair: TradingPair,
    pub side: OrderSide, // Long or Short
    pub entry_price: f64,
    pub quantity: f64,
    pub current_price: f64, // Current market price for PnL calculation
    pub timestamp_ms: u64, // Time position was opened
    pub pnl: f64, // Realized or unrealized PnL
}

/// Represents a configuration for a specific DEX or data source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexConfig {
    pub name: String,
    pub rpc_url: String,
    pub websocket_url: Option<String>,
    pub chain_id: String, // e.g., "sui:devnet"
    pub contract_address: Option<SuiAddress>, // Main contract address for the DEX
}

/// Represents a generic error type for the bot.
/// This will be used across modules for consistent error handling.
#[derive(Debug, thiserror::Error)]
pub enum RavenslingerError {
    #[error("Data error: {0}")]
    DataError(String),
    #[error("Decision error: {0}")]
    DecisionError(String),
    #[error("Execution error: {0}")]
    ExecutionError(String),
    #[error("Wallet error: {0}")]
    WalletError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Sui SDK error: {0}")]
    SuiError(#[from] sui_sdk::error::Error),
    #[error("Other error: {0}")]
    Other(String),
}

// Implement From for std::io::Error to convert to RavenslingerError
impl From<std::io::Error> for RavenslingerError {
    fn from(err: std::io::Error) -> Self {
        RavenslingerError::Other(format!("IO error: {}", err))
    }
}

// Implement From for std::env::VarError to convert to RavenslingerError
impl From<std::env::VarError> for RavenslingerError {
    fn from(err: std::env::VarError) -> Self {
        RavenslingerError::ConfigError(format!("Environment variable error: {}", err))
    }
}

// Implement From for serde_json::Error to convert to RavenslingerError
impl From<serde_json::Error> for RavenslingerError {
    fn from(err: serde_json::Error) -> Self {
        RavenslingerError::DataError(format!("JSON parsing error: {}", err))
    }
}

// Implement From for tokio::task::JoinError to convert to RavenslingerError
impl From<tokio::task::JoinError> for RavenslingerError {
    fn from(err: tokio::task::JoinError) -> Self {
        RavenslingerError::Other(format!("Task join error: {}", err))
    }
}

// Implement From for url::ParseError to convert to RavenslingerError
impl From<url::ParseError> for RavenslingerError {
    fn from(err: url::ParseError) -> Self {
        RavenslingerError::ConfigError(format!("URL parsing error: {}", err))
    }
}

// Implement From for reqwest::Error to convert to RavenslingerError
impl From<reqwest::Error> for RavenslingerError {
    fn from(err: reqwest::Error) -> Self {
        RavenslingerError::DataError(format!("HTTP request error: {}", err))
    }
}

// Implement From for tokio_tungstenite::tungstenite::Error to convert to RavenslingerError
impl From<tokio_tungstenite::tungstenite::Error> for RavenslingerError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        RavenslingerError::DataError(format!("WebSocket error: {}", err))
    }
}

// Implement From for std::num::ParseIntError to convert to RavenslingerError
impl From<std::num::ParseIntError> for RavenslingerError {
    fn from(err: std::num::ParseIntError) -> Self {
        RavenslingerError::DataError(format!("Integer parsing error: {}", err))
    }
}

// Implement From for std::num::ParseFloatError to convert to RavenslingerError
impl From<std::num::ParseFloatError> for RavenslingerError {
    fn from(err: std::num::ParseFloatError) -> Self {
        RavenslingerError::DataError(format!("Float parsing error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trading_pair_display() {
        let pair = TradingPair {
            base: "SUI".to_string(),
            quote: "USDC".to_string(),
        };
        assert_eq!(format!("{}", pair), "SUI-USDC");
    }

    #[test]
    fn test_asset_display() {
        let asset = Asset {
            symbol: "SUI".to_string(),
            coin_type: "0x2::sui::SUI".to_string(),
            decimals: 9,
            object_id: None,
        };
        assert_eq!(format!("{}", asset), "SUI");
    }
}