use anyhow::Result;
use async_trait::async_trait;
use crate::execution::types::{OrderRequest, QuotePack, GasFee, BuiltTx, SubmitResult};

/// Abstraction for obtaining market quotes for a given order request.
///
/// A [`QuoteProvider`] implementation should examine the provided [`OrderRequest`]
/// and return a [`QuotePack`] containing bid, ask and mid prices along with
/// an estimated slippage in basis points.  Implementations must be safe to
/// share across threads (`Send + Sync`) and support asynchronous operation.
#[async_trait]
pub trait QuoteProvider: Send + Sync {
    /// Retrieve the best available quote for the given order request.
    async fn best_quote(&self, req: &OrderRequest) -> Result<QuotePack>;
}

/// Abstraction for estimating network gas fees required to settle an order.
///
/// A [`GasEstimator`] uses both the order request and the selected quote to
/// compute a [`GasFee`] that includes an estimated fee and optional priority
/// (in basis points).  Implementations should ensure that fee estimates are
/// accurate and incorporate any network congestion or user-provided tolerance.
#[async_trait]
pub trait GasEstimator: Send + Sync {
    /// Estimate the gas fee for the given order and quote.
    async fn estimate(&self, req: &OrderRequest, quotes: &QuotePack) -> Result<GasFee>;
}

/// Abstraction for constructing protocol-specific transactions from an order.
///
/// A [`PtbBuilder`] receives the order details, quote and gas fee and returns a
/// built transaction representation [`BuiltTx`].  The resulting payload is
/// expected to be ready for submission to the target network.
#[async_trait]
pub trait PtbBuilder: Send + Sync {
    /// Build a transaction from the order, quote and gas fee.
    async fn build(&self, req: &OrderRequest, quotes: &QuotePack, gas: &GasFee) -> Result<BuiltTx>;
}

/// Abstraction for submitting a built transaction to a network.
///
/// A [`Submitter`] takes a [`BuiltTx`] and broadcasts it to the network,
/// returning a [`SubmitResult`] that includes the transaction digest and
/// acceptance and fill status.
#[async_trait]
pub trait Submitter: Send + Sync {
    /// Submit the transaction to the network and return the result.
    async fn submit(&self, tx: &BuiltTx) -> Result<SubmitResult>;
}
