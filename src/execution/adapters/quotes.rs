use anyhow::{anyhow, Result};
use async_trait::async_trait;
use crate::execution::traits::QuoteProvider;
use crate::execution::types::{OrderRequest, QuotePack};

pub struct SuiDexQuotes {
    pub fee_bps: u32,
    pub pool_reserve_in: f64,
    pub pool_reserve_out: f64,
}

#[async_trait]
impl QuoteProvider for SuiDexQuotes {
    async fn best_quote(&self, _req: &OrderRequest) -> Result<QuotePack> {
        let r_in = self.pool_reserve_in;
        let r_out = self.pool_reserve_out;
        if r_in <= 0.0 || r_out <= 0.0 { return Err(anyhow!("empty pool reserves")); }

        let mid = r_out / r_in;
        let fee = 1.0 - (self.fee_bps as f64 / 10_000.0);
        let dx = 1.0_f64;
        let dx_eff = dx * fee;
        let dy = (r_out - (r_in * r_out) / (r_in + dx_eff)).max(0.0);
        let ask = if dy > 0.0 { dx / dy } else { f64::INFINITY };
        let bid = (dy / dx).max(0.0);

        Ok(QuotePack { bid, ask, mid, est_slippage_bps: 20 })
    }
}
