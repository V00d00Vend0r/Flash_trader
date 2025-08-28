//! Sui wallet implementation for Ravenslinger trading bot.
//!
//! This module provides a wallet that manages a local Sui keypair and tracks
//! on-chain balances. It's designed for high-performance automated trading
//! with direct RPC interaction for minimal latency.

use crate::error::RavenslingerError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use sui_keys::keystore::{AccountKeystore, InMemKeystore};
use sui_sdk::types::base_types::{ObjectID, SuiAddress};
use sui_sdk::SuiClient;
use tokio::sync::RwLock;

/// Configuration for wallet initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    pub rpc_url: String,
    pub mnemonic_env_var: String, // Environment variable containing the mnemonic
    pub refresh_interval_ms: u64, // How often to refresh balances from chain
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://fullnode.devnet.sui.io:443".to_string(),
            mnemonic_env_var: "SUI_MNEMONIC".to_string(),
            refresh_interval_ms: 5000, // 5 seconds
        }
    }
}

/// Represents a Sui wallet with local keypair management and on-chain balance tracking
#[derive(Debug)]
pub struct Wallet {
    /// The Sui address this wallet manages
    pub address: SuiAddress,
    /// Local keystore for signing transactions
    keystore: InMemKeystore,
    /// Cached balances from on-chain state
    balances: RwLock<HashMap<String, f64>>,
    /// Sui client for RPC interactions
    client: SuiClient,
    /// Configuration
    config: WalletConfig,
    /// Last balance refresh timestamp
    last_refresh: RwLock<u64>,
}

impl Wallet {
    /// Create a new wallet from environment mnemonic
    pub async fn new() -> Result<Self, RavenslingerError> {
        Self::with_config(WalletConfig::default()).await
    }

    /// Create a new wallet with custom configuration
    pub async fn with_config(config: WalletConfig) -> Result<Self, RavenslingerError> {
        // Load mnemonic from environment
        let mnemonic = std::env::var(&config.mnemonic_env_var)
            .map_err(|_| RavenslingerError::WalletError(
                format!("Environment variable {} not found", config.mnemonic_env_var)
            ))?;

        // Initialize keystore and import mnemonic
        let mut keystore = InMemKeystore::new();
        let address = keystore
            .import_from_mnemonic(&mnemonic, None)
            .map_err(|e| RavenslingerError::WalletError(
                format!("Failed to import mnemonic: {}", e)
            ))?;

        // Initialize Sui client
        let client = SuiClient::new(&config.rpc_url)
            .await
            .map_err(|e| RavenslingerError::WalletError(
                format!("Failed to connect to Sui RPC: {}", e)
            ))?;

        let wallet = Self {
            address,
            keystore,
            balances: RwLock::new(HashMap::new()),
            client,
            config,
            last_refresh: RwLock::new(0),
        };

        // Initial balance refresh
        wallet.refresh_balances().await?;

        Ok(wallet)
    }

    /// Get the wallet's Sui address
    pub fn address(&self) -> SuiAddress {
        self.address
    }

    /// Get current balance for an asset (cached)
    pub async fn balance(&self, asset: &str) -> f64 {
        let balances = self.balances.read().await;
        balances.get(asset).copied().unwrap_or(0.0)
    }

    /// List all cached balances
    pub async fn list_balances(&self) -> Vec<(String, f64)> {
        let balances = self.balances.read().await;
        let mut items: Vec<_> = balances.iter().map(|(k, v)| (k.clone(), *v)).collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
    }

    /// Refresh balances from on-chain state
    pub async fn refresh_balances(&self) -> Result<(), RavenslingerError> {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let mut last_refresh = self.last_refresh.write().await;
        
        // Rate limiting: don't refresh too frequently
        if now - *last_refresh < self.config.refresh_interval_ms {
            return Ok(());
        }

        // Fetch all coin objects for this address
        let coins = self.client
            .coin_read_api()
            .get_all_coins(self.address, None, None)
            .await
            .map_err(|e| RavenslingerError::WalletError(
                format!("Failed to fetch coins: {}", e)
            ))?;

        // Aggregate balances by coin type
        let mut new_balances = HashMap::new();
        for coin in coins.data {
            let coin_type = coin.coin_type.clone();
            let balance = coin.balance as f64 / 1_000_000_000.0; // Convert from MIST to SUI units
            
            // Extract readable asset name from coin type
            let asset_name = extract_asset_name(&coin_type);
            
            *new_balances.entry(asset_name).or_insert(0.0) += balance;
        }

        // Update cached balances
        {
            let mut balances = self.balances.write().await;
            *balances = new_balances;
        }

        *last_refresh = now;
        Ok(())
    }

    /// Force refresh balances (ignores rate limiting)
    pub async fn force_refresh_balances(&self) -> Result<(), RavenslingerError> {
        *self.last_refresh.write().await = 0;
        self.refresh_balances().await
    }

    /// Check if wallet has sufficient balance for a trade
    pub async fn has_sufficient_balance(&self, asset: &str, amount: f64) -> bool {
        // Auto-refresh if stale
        if let Err(e) = self.refresh_balances().await {
            log::warn!("Failed to refresh balances: {}", e);
        }
        
        self.balance(asset).await >= amount
    }

    /// Get the keystore for transaction signing
    pub fn keystore(&self) -> &InMemKeystore {
        &self.keystore
    }

    /// Get the Sui client for RPC operations
    pub fn client(&self) -> &SuiClient {
        &self.client
    }

    /// Estimate gas cost for a transaction (simplified)
    pub async fn estimate_gas(&self) -> Result<u64, RavenslingerError> {
        // For now, return a conservative estimate
        // In production, you'd want to simulate the transaction
        Ok(1_000_000) // 0.001 SUI in MIST
    }

    /// Get total portfolio value in SUI equivalent (simplified)
    pub async fn total_value_sui(&self) -> f64 {
        let balances = self.balances.read().await;
        
        // For now, just return SUI balance
        // In production, you'd convert all assets to SUI using current prices
        balances.get("SUI").copied().unwrap_or(0.0)
    }

    /// Legacy compatibility methods for existing code
    pub fn deposit(&mut self, _asset: &str, _amount: f64) {
        log::warn!("deposit() called on Sui wallet - this is a no-op. Use on-chain transactions instead.");
    }

    pub fn withdraw(&mut self, _asset: &str, _amount: f64) -> bool {
        log::warn!("withdraw() called on Sui wallet - this is a no-op. Use on-chain transactions instead.");
        false
    }
}

/// Extract a readable asset name from a Sui coin type
/// e.g., "0x2::sui::SUI" -> "SUI"
fn extract_asset_name(coin_type: &str) -> String {
    if coin_type == "0x2::sui::SUI" {
        return "SUI".to_string();
    }
    
    // For other tokens, try to extract the last part
    if let Some(last_part) = coin_type.split("::").last() {
        last_part.to_uppercase()
    } else {
        coin_type.to_string()
    }
}

/// Utility function to create a wallet for testing with a generated keypair
#[cfg(test)]
pub async fn create_test_wallet() -> Result<Wallet, RavenslingerError> {
    use sui_keys::keystore::AccountKeystore;
    
    // Generate a new keypair for testing
    let mut keystore = InMemKeystore::new();
    let (address, _phrase, _scheme) = keystore.generate_and_add_new_key(
        sui_keys::key_derive::SignatureScheme::ED25519,
        None,
        None,
    ).map_err(|e| RavenslingerError::WalletError(format!("Failed to generate test keypair: {}", e)))?;

    let config = WalletConfig {
        rpc_url: "https://fullnode.devnet.sui.io:443".to_string(),
        mnemonic_env_var: "TEST_MNEMONIC".to_string(),
        refresh_interval_ms: 1000,
    };

    let client = SuiClient::new(&config.rpc_url)
        .await
        .map_err(|e| RavenslingerError::WalletError(format!("Failed to connect to Sui RPC: {}", e)))?;

    Ok(Wallet {
        address,
        keystore,
        balances: RwLock::new(HashMap::new()),
        client,
        config,
        last_refresh: RwLock::new(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_asset_name() {
        assert_eq!(extract_asset_name("0x2::sui::SUI"), "SUI");
        assert_eq!(extract_asset_name("0x123::usdc::USDC"), "USDC");
        assert_eq!(extract_asset_name("invalid"), "INVALID");
    }

    #[tokio::test]
    async fn test_wallet_creation() {
        // This test requires a test environment with SUI_MNEMONIC set
        if std::env::var("SUI_MNEMONIC").is_err() {
            println!("Skipping wallet creation test - SUI_MNEMONIC not set");
            return;
        }

        let wallet = Wallet::new().await;
        assert!(wallet.is_ok());
        
        if let Ok(w) = wallet {
            println!("Test wallet address: {}", w.address());
            let balances = w.list_balances().await;
            println!("Balances: {:?}", balances);
        }
    }

    #[tokio::test]
    async fn test_balance_operations() {
        let wallet = create_test_wallet().await.expect("Failed to create test wallet");
        
        // Test balance retrieval (should be 0 for new wallet)
        let sui_balance = wallet.balance("SUI").await;
        assert_eq!(sui_balance, 0.0);
        
        // Test balance listing
        let balances = wallet.list_balances().await;
        assert!(balances.is_empty());
    }
}