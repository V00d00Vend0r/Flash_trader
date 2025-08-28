//! AES-encrypted environment variable loader for Forge29.
//! (Full feature version, no duplicates)

use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use anyhow::{anyhow, Context, Result};
use std::env;
use tokio::fs as async_fs;
use tracing::{info, warn, debug};

// defaults
const DEFAULT_ENV_FILE: &str = ".env.enc";
const AES_KEY_VAR: &str = "FORGE29_AES_KEY";
const AES_NONCE_VAR: &str = "FORGE29_AES_NONCE";

#[derive(Debug, Clone)]
pub struct SecretsConfig {
    pub env_file_path: String,
    pub require_encrypted_file: bool,
    pub allow_plain_fallback: bool,
}
impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            env_file_path: DEFAULT_ENV_FILE.to_string(),
            require_encrypted_file: true,
            allow_plain_fallback: false,
        }
    }
}

// Load env
pub async fn load_encrypted_env() -> Result<()> {
    load_encrypted_env_with_config(SecretsConfig::default()).await
}

pub async fn load_encrypted_env_with_config(config: SecretsConfig) -> Result<()> {
    info!("Loading secrets from: {}", config.env_file_path);
    match load_from_encrypted_file(&config).await {
        Ok(n) => {
            info!("Loaded {} encrypted env vars", n);
            return Ok(())
        }
        Err(e) if !config.require_encrypted_file && config.allow_plain_fallback => {
            warn!("Encrypted load failed ({:?}), trying .env fallback", e);
            return load_from_plain_file().await.map(|_| ());
        }
        Err(e) => return Err(e)
    }
}

async fn load_from_encrypted_file(config: &SecretsConfig) -> Result<usize> {
    let key = hex::decode(env::var(AES_KEY_VAR)?)?;
    let nonce = hex::decode(env::var(AES_NONCE_VAR)?)?;
    if key.len() != 32 { return Err(anyhow!("AES key must be 32 bytes")) }
    if nonce.len() != 12 { return Err(anyhow!("AES nonce must be 12 bytes")) }

    let ciphertext = async_fs::read(&config.env_file_path).await?;
    let cipher = Aes256Gcm::new(Key::from_slice(&key));
    let plaintext = cipher.decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())?;

    parse_and_inject_env(&String::from_utf8(plaintext)?) 
}

async fn load_from_plain_file() -> Result<usize> {
    parse_and_inject_env(&async_fs::read_to_string(".env").await?)
}

fn parse_and_inject_env(content: &str) -> Result<usize> {
    let mut c = 0;
    for line in content.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') { continue; }
        if let Some((k,v)) = l.split_once('=') {
            env::set_var(k.trim(), v.trim().trim_matches('"'));
            debug!("Loaded {}", k);
            c += 1;
        }
    }
    Ok(c)
}

/// Generate AES key + nonce for initial setup
pub fn generate_key_material() -> (String,String) {
    use rand::RngCore;
    let mut key = [0u8;32];
    let mut nonce=[0u8;12];
    rand::thread_rng().fill_bytes(&mut key);
    rand::thread_rng().fill_bytes(&mut nonce);
    (hex::encode(key), hex::encode(nonce))
}

/// Macros for vars
#[macro_export]
macro_rules! env_required {
    ($var:expr) => { std::env::var($var)
        .unwrap_or_else(|_| panic!("Required env {} not set",$var)) };
}
#[macro_export]
macro_rules! env_optional {
    ($var:expr, $default:expr) => {
        std::env::var($var).unwrap_or_else(|_| $default.to_string())
    };
}