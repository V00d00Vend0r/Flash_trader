use adapters_cetus::CetusSidecar;
use anyhow::{Result, anyhow};

fn deatomize(amount_atomic: &str, decimals: u32) -> Result<f64> {
    let raw: f64 = amount_atomic.parse()
        .map_err(|_| anyhow!("invalid atomic amount: {}", amount_atomic))?;
    Ok(raw / 10f64.powi(decimals as i32))
}

#[derive(Debug, Clone)]
pub struct ArbResult {
    pub base_in: f64,
    pub quote_out: f64,
    pub base_back: f64,
    pub gross_edge: f64,
    pub net_edge: f64,
    pub roi_pct: f64,
}

pub async fn check_roundtrip_base_quote(
    sidecar: &CetusSidecar,
    base_tag: &str,
    quote_tag: &str,
    base_dec: u32,
    quote_dec: u32,
    base_in_atomic: &str,
    gas_base: f64,
    extra_fees_base: f64,
    slippage_buffer_pct: f64,
) -> Result<ArbResult> {
    // BUY: BASE -> QUOTE
    let buy = sidecar.quote(base_tag, quote_tag, base_in_atomic).await?;
    let quote_out_atomic = buy.amountOut.as_deref().ok_or_else(|| anyhow!("buy: no amountOut"))?;

    // SELL: QUOTE -> BASE
    let sell = sidecar.quote(quote_tag, base_tag, quote_out_atomic).await?;
    let base_back_atomic = sell.amountOut.as_deref().ok_or_else(|| anyhow!("sell: no amountOut"))?;

    // Convert to units
    let base_in = deatomize(base_in_atomic, base_dec)?;
    let quote_out = deatomize(quote_out_atomic, quote_dec)?;
    let mut base_back = deatomize(base_back_atomic, base_dec)?;

    // Conservative slippage buffer on return leg
    base_back *= 1.0 - slippage_buffer_pct;

    let gross_edge = base_back - base_in;
    let net_edge = gross_edge - gas_base - extra_fees_base;
    let roi_pct = (net_edge / base_in) * 100.0;

    Ok(ArbResult { base_in, quote_out, base_back, gross_edge, net_edge, roi_pct })
}

fn parse_atom_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// One-shot probe with env-driven params (defaults to SUI<->CETUS, 1 SUI in)
pub async fn run_arb_probe() -> Result<()> {
    let sidecar = CetusSidecar::new("http://127.0.0.1:12789")?;

    // Defaults (Sui mainnet)
    let default_base = "0x2::sui::SUI";
    let default_quote = "0x06864a6f921804860930db6ddbe2e16acdf8504495ea7481637a1c8b9a8fe54b::cetus::CETUS";

    // Base config
    let base_tag  = std::env::var("FROM_TAG").unwrap_or_else(|_| default_base.into());
    let quote_tag = std::env::var("TO_TAG").unwrap_or_else(|_| default_quote.into());
    let base_dec: u32  = std::env::var("FROM_DEC").ok().and_then(|v| v.parse().ok()).unwrap_or(9);  // SUI=9
    let quote_dec: u32 = std::env::var("TO_DEC").ok().and_then(|v| v.parse().ok()).unwrap_or(9);   // CETUS=9
    let gas_base: f64        = std::env::var("GAS_BASE").ok().and_then(|v| v.parse().ok()).unwrap_or(0.002);
    let extra_fees_base: f64 = std::env::var("EXTRA_FEES_BASE").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let slip_bps: f64        = std::env::var("SLIPPAGE_BPS").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0); // 0.30%
    let min_roi_pct: f64     = std::env::var("MIN_ROI_PCT").ok().and_then(|v| v.parse().ok()).unwrap_or(0.25);
    let slip_pct = slip_bps / 10_000.0;

    // Optional size sweep: comma-separated list of atomic amounts
    if let Ok(list) = std::env::var("SWEEP_ATOMICS") {
        let mut best: Option<(String, ArbResult)> = None;
        for amt in parse_atom_list(&list) {
            let r = check_roundtrip_base_quote(
                &sidecar, &base_tag, &quote_tag, base_dec, quote_dec, &amt,
                gas_base, extra_fees_base, slip_pct
            ).await?;

            println!(
                "[ARB] amt={} -> roi={:.4}% net_edge={:.9} base_in={:.9} base_back={:.9} quote_out={:.9}",
                amt, r.roi_pct, r.net_edge, r.base_in, r.base_back, r.quote_out
            );

            if best.as_ref().map(|(_, b)| r.roi_pct > b.roi_pct).unwrap_or(true) {
                best = Some((amt.clone(), r));
            }
        }
        if let Some((amt, r)) = best {
            println!(
                "[ARB] BEST amt={} roi={:.4}% net_edge={:.9} (min_roi={:.3}%)",
                amt, r.roi_pct, r.net_edge, min_roi_pct
            );
            if r.roi_pct >= min_roi_pct && r.net_edge > 0.0 {
                println!("[ARB] PROFITABLE: would execute this size.");
            } else {
                println!("[ARB] Not profitable at these settings.");
            }
        }
        return Ok(());
    }

    // Single-size mode (fallback)
    let base_in_atomic = std::env::var("AMOUNT_ATOMIC").unwrap_or_else(|_| "1000000000".into()); // 1 SUI
    let r = check_roundtrip_base_quote(
        &sidecar, &base_tag, &quote_tag, base_dec, quote_dec, &base_in_atomic,
        gas_base, extra_fees_base, slip_pct
    ).await?;

    println!(
        "[ARB] {} -> {} -> {}\n  base_in: {:.9}\n  quote_out: {:.9}\n  base_back(after slip): {:.9}\n  gross_edge: {:.9}\n  net_edge: {:.9}\n  roi_pct: {:.4}%",
        base_tag, quote_tag, base_tag,
        r.base_in, r.quote_out, r.base_back, r.gross_edge, r.net_edge, r.roi_pct
    );

    if r.roi_pct >= min_roi_pct && r.net_edge > 0.0 {
        println!("[ARB] PROFITABLE (>= {:.3}%): OK to execute", min_roi_pct);
    } else {
        println!("[ARB] Not profitable (min {:.3}% or net<=0); skipping.", min_roi_pct);
    }
    Ok(())
}
