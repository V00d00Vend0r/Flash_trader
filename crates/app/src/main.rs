use anyhow::Result;
use std::sync::Arc;
use tokio::{signal, sync::Mutex, time::{sleep, Duration}};

// declare your local modules
mod ingest;   // must contain: pub async fn start_market_data_ingest(Arc<Mutex<PriceDataBuffer>>) -> Result<()>

mod arb;      // must contain: pub async fn run_arb_probe() -> Result<()> 
use arb::run_arb_probe;
use ingest::start_market_data_ingest;

use ravenslinger_data::PriceDataBuffer;

#[tokio::main]
async fn main() -> Result<()> {
    // optional env
    let _ = dotenvy::dotenv();

    // PriceDataBuffer requires a capacity; fall back to a sane default
    let cap: usize = std::env::var("PDB_CAPACITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60_000);

    let buffer = Arc::new(Mutex::new(PriceDataBuffer::new(cap)));

    // spawn the resilient market-data ingest task
    {
        let buffer = Arc::clone(&buffer);
        tokio::spawn(async move {
            if let Err(e) = start_market_data_ingest(buffer).await {
                eprintln!("[ingest] task exited with error: {e:?}");
            }
        });
    }

    // spawn the arbitrage probe loop (every 5s)
    tokio::spawn(async move {
        loop {
            if let Err(e) = run_arb_probe().await {
                eprintln!("[arb] probe error: {e:?}");
            }
            sleep(Duration::from_secs(5)).await;
        }
    });


    // graceful shutdown
    signal::ctrl_c().await?;
    Ok(())
}
