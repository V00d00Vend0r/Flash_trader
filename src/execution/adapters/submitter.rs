use anyhow::{anyhow, Result};
use async_trait::async_trait;
use crate::execution::traits::Submitter;
use crate::execution::types::{BuiltTx, SubmitResult};

pub struct SuiSubmitter { pub rpc_url: String }

#[async_trait]
impl Submitter for SuiSubmitter {
    async fn submit(&self, tx: &BuiltTx) -> Result<SubmitResult> {
        if tx.payload_json.is_empty() { return Err(anyhow!("empty tx payload")); }
        Ok(SubmitResult { tx_digest: "0xMOCK_SUI_DIGEST".into(), accepted: true, filled_qty: 0.0 })
    }
}
