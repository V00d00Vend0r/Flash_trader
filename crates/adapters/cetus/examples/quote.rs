use adapters_cetus::CetusSidecar;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sidecar = CetusSidecar::new("http://127.0.0.1:12789")?;
    // 1 SUI (atomic units = 1_000_000_000); SUI -> CETUS
    let q = sidecar
        .quote(
            "0x2::sui::SUI",
            "0x06864a6f921804860930db6ddbe2e16acdf8504495ea7481637a1c8b9a8fe54b::cetus::CETUS",
            "1000000000",
        )
        .await?;
    println!("{:#?}", q);
    Ok(())
}
