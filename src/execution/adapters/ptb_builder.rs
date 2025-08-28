use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use crate::execution::traits::PtbBuilder;
use crate::execution::types::{OrderRequest, QuotePack, GasFee, BuiltTx};

pub struct SuiCetusPtbBuilder { pub router_addr: String }

#[async_trait]
impl PtbBuilder for SuiCetusPtbBuilder {
    async fn build(&self, req: &OrderRequest, q: &QuotePack, g: &GasFee) -> Result<BuiltTx> {
        let min_out = if let Some(limit) = req.limit_px {
            (req.qty / limit) * (1.0 - (req.max_slippage_bps as f64 / 10_000.0))
        } else {
            (req.qty * q.bid) * (1.0 - (req.max_slippage_bps as f64 / 10_000.0))
        };
        let payload = json!({
            "kind": "cetus_swap_exact_in",
            "router": self.router_addr,
            "symbol": req.symbol,
            "action": format!("{:?}", req.action),
            "qty_in": req.qty,
            "min_out": min_out,
            "gas_price": g.estimated_fee,
        }).to_string();
        Ok(BuiltTx { description: "sui-ptb-cetus-swap".into(), payload_json: payload })
    }
}
