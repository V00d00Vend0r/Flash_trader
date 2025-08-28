use anyhow::Result;
use std::sync::Arc;

use crate::execution::traits::{QuoteProvider, GasEstimator, PtbBuilder, Submitter};
use crate::execution::types::{OrderRequest, QuotePack, GasFee, BuiltTx, SubmitResult};
use anyhow::Result;
use std::sync::Arc;

pub struct Executor {
    quotes: Arc<dyn QuoteProvider>,
    gas: Arc<dyn GasEstimator>,
    builder: Arc<dyn PtbBuilder>,
    submitter: Arc<dyn Submitter>,
}

impl Executor {
    pub fn new(
        quotes: Arc<dyn QuoteProvider>,
        gas: Arc<dyn GasEstimator>,
        builder: Arc<dyn PtbBuilder>,
        submitter: Arc<dyn Submitter>,
    ) -> Self {
        Self { quotes, gas, builder, submitter }
    }
    pub async fn execute(&self, req: &OrderRequest) -> Result<SubmitResult> {
        let q: QuotePack = self.quotes.best_quote(req).await?;
        let g: GasFee   = self.gas.estimate(req, &q).await?;
let tx: BuiltTx = self.builder.build(req, &q, &g).await?;
let res: SubmitResult = self.submitter.submit(&tx).await?;
Ok(res)
    }
}
