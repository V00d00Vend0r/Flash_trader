use std::sync::Arc;
use tokio::{sync::Mutex, time::{sleep, Duration}};
use anyhow::Result;
use ravenslinger_data::PriceDataBuffer;
use adapters_cetus::CetusSidecar;

/// Resilient ingest loop. Currently probes the sidecar; you can add buffer pushes later.
pub async fn start_market_data_ingest(buffer: Arc<Mutex<PriceDataBuffer>>) -> Result<()> {
    let sidecar = CetusSidecar::new("http://127.0.0.1:12789")?;
    let mut interval = Duration::from_secs(5);

    loop {
        let res = async {
            // Lightweight quote probe: SUI -> CETUS for 1 SUI (1e9 atomic)
            let _quote = sidecar.quote(
                "0x2::sui::SUI",
                "0x06864a6f921804860930db6ddbe2e16acdf8504495ea7481637a1c8b9a8fe54b::cetus::CETUS",
                "1000000000",
            ).await?;

            // Lock buffer for future writes (no-op for now)
            let _buf = buffer.lock().await;

            // TODO: convert quote -> price and push into buffer here
            // e.g. _buf.push_raw(ts, price, None);

            Ok::<(), anyhow::Error>(())
        }.await;

        match res {
            Ok(()) => { interval = Duration::from_secs(5); }
            Err(e) => {
                eprintln!("[ingest] cetus probe failed: {e:?}");
                interval = std::cmp::min(interval * 2, Duration::from_secs(60));
            }
        }
        sleep(interval).await;
    }
}
