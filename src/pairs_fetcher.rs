use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tracing::{info, warn};

static TARGETS: &[&str] = &["BTC", "ETH", "BNB", "SOL", "XRP", "TRX", "ADA", "SUI", "AVAX"];

#[derive(Debug, Serialize, Deserialize)]
pub struct Pair {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub source: String,
    pub status: String,
}

#[derive(Deserialize)]
struct ExchangeInfo {
    symbols: Vec<Symbol>,
}

#[derive(Deserialize)]
struct Symbol {
    symbol: String,
    base_asset: String,
    quote_asset: String,
    status: String,
}

#[derive(Deserialize)]
struct DexResponse {
    data: Option<Vec<DexToken>>,
    tokens: Option<Vec<DexToken>>,
}

#[derive(Deserialize)]
struct DexToken {
    symbol: Option<String>,
    base: Option<String>,
    token0: Option<String>,
    quote: Option<String>,
    token1: Option<String>,
    status: Option<String>,
}

#[tokio::main]
pub async fn run_pairs_fetcher() -> Result<()> {
    info!("[DevotedSon] Starting MEXC pair collection...");

    let client = Client::builder()
        .user_agent("DevotedSonBot/1.0")
        .build()?;

    let spot_pairs = fetch_spot_pairs(&client).await?;
    info!("✅ Found {} spot pairs.", spot_pairs.len());

    let mut all_pairs = spot_pairs;

    if let Ok(dex_api) = std::env::var("DEXPLUS_API") {
        let dex_future = fetch_dexplus_pairs(&client, &dex_api);
        let (dex_pairs,) = tokio::join!(dex_future);
        match dex_pairs {
            Ok(mut dex_pairs) => {
                info!("✅ Found {} DEX+ pairs.", dex_pairs.len());
                all_pairs.append(&mut dex_pairs);
            }
            Err(e) => warn!("⚠️  DEX+ fetch failed: {}", e),
        }
    }

    save_pairs(&all_pairs)?;
    info!(
        "✨ All done — {} pairs written to data/mexc/pairs.json & pairs.csv",
        all_pairs.len()
    );
    info!("📖 Proverbs 16:3 — Commit to the Lord whatever you do, and your plans will succeed.");
    Ok(())
}

async fn fetch_spot_pairs(client: &Client) -> Result<Vec<Pair>> {
    let url = "https://api.mexc.com/api/v3/exchangeInfo";
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!("Failed to fetch exchange info: {}", resp.status()));
    }

    let exchange_info: ExchangeInfo = resp.json().await?;
    let mut out = Vec::new();
    for symbol in exchange_info.symbols {
        if TARGETS.contains(&symbol.base_asset.as_str()) || TARGETS.contains(&symbol.quote_asset.as_str()) {
            out.push(Pair {
                symbol: symbol.symbol,
                base: symbol.base_asset,
                quote: symbol.quote_asset,
                source: "spot".to_string(),
                status: symbol.status,
            });
        }
    }
    Ok(out)
}

async fn fetch_dexplus_pairs(client: &Client, base_url: &str) -> Result<Vec<Pair>> {
    let url = format!("{}/tokens", base_url.trim_end_matches('/'));
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!("DEX+ endpoint returned {}", resp.status()));
    }

    let dex_response: DexResponse = resp.json().await?;
    let tokens = dex_response.data.or(dex_response.tokens).ok_or_else(|| anyhow!("Invalid DEX+ response format"))?;
    let mut out = Vec::new();
    for token in tokens {
        let base = token.base.or(token.token0);
        let quote = token.quote.or(token.token1);
        if let (Some(b), Some(q)) = (base, quote) {
            if TARGETS.contains(&b.as_str()) || TARGETS.contains(&q.as_str()) {
                out.push(Pair {
                    symbol: token.symbol.unwrap_or_default(),
                    base: b,
                    quote: q,
                    source: "dexplus".to_string(),
                    status: token.status.unwrap_or_else(|| "ACTIVE".to_string()),
                });
            }
        }
    }
    Ok(out)
}

fn save_pairs(pairs: &[Pair]) -> Result<()> {
    let dir = PathBuf::from("data/mexc");
    fs::create_dir_all(&dir)?;

    let json_path = dir.join("pairs.json");
    let csv_path = dir.join("pairs.csv");

    fs::write(&json_path, serde_json::to_string_pretty(pairs)?)?;
    let mut wtr = csv::Writer::from_path(&csv_path)?;
    wtr.write_record(&["symbol", "base", "quote", "source", "status"])?;
    for p in pairs {
        wtr.write_record(&[&p.symbol, &p.base, &p.quote, &p.source, &p.status])?;
    }
    wtr.flush()?;
    Ok(())
}