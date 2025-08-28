use anyhow::Result;
use async_trait::async_trait;
use crate::execution::traits::GasEstimator;
use crate::execution::types::{OrderRequest, QuotePack, GasFee};

pub struct SuiGasEstimator {
    pub rpc_url: String,
    pub multiplier: f64,
}
#[async_trait]
impl GasEstimator for SuiGasEstimator {
    async fn estimate(&self, _req: &OrderRequest, _quotes: &QuotePack) -> Result<GasFee> {
        // TODO: query reference gas price; placeholder below
        let ref_price = 1.0e-6_f64;
        Ok(GasFee { estimated_fee: ref_price * self.multiplier, priority_bps: ((self.multiplier - 1.0) * 10_000.0).max(0.0) as u32 })
    }
}
