//! Shared abstractions and helpers for decentralised exchanges (DEXes).
//!
//! To support multiple venues (such as Cetus and future integrations)
//! without tightly coupling the decision layer to any one exchange,
//! this module defines a set of traits and common data structures
//! representing liquidity pools, quotes and requests. The goal is to
//! enable a generic path through your trading strategy: given a
//! `QuoteRequest` you obtain a `QuoteResponse` regardless of the
//! underlying venue implementation.

use std::fmt;

/// Parameters for a quote request.
///
/// A `QuoteRequest` encodes everything needed to price a trade on a
/// DEX. At minimum you specify the input amount and token pair. More
/// sophisticated venues may honour additional fields such as slippage
/// tolerances or fee tiers.
#[derive(Debug, Clone)]
pub struct QuoteRequest {
    /// The identifier of the input asset (e.g. a token address).
    pub input_asset: String,
    /// The identifier of the output asset (e.g. a token address).
    pub output_asset: String,
    /// The quantity of input asset to swap, in natural units.
    pub amount_in: u128,
    /// Optional maximum slippage expressed as a fraction (e.g. 0.005 for 0.5%).
    pub max_slippage: Option<f64>,
}

/// The result of a quote request.
///
/// Contains the expected output amount along with any fees charged
/// by the pool. Exchanges may populate additional fields as
/// appropriate (e.g. route path or tick range). For now this is a
/// minimal container to illustrate the abstraction.
#[derive(Debug, Clone)]
pub struct QuoteResponse {
    /// The quantity of output asset expected, in natural units.
    pub amount_out: u128,
    /// The total fees charged for the trade, in input asset units.
    pub fee_amount: u128,
}

impl fmt::Display for QuoteResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QuoteResponse {{ amount_out: {}, fee_amount: {} }}",
            self.amount_out, self.fee_amount
        )
    }
}

/// A trait for sources capable of providing swap quotes.
///
/// Implementors should return a [`QuoteResponse`] for a given
/// [`QuoteRequest`]. This trait is synchronous for simplicity but
/// could easily be made asynchronous by returning a future if needed.
pub trait QuoteSource: Send + Sync {
    /// Produce a [`QuoteResponse`] for a given request.
    fn quote(&self, req: QuoteRequest) -> QuoteResponse;
}

/// A trait for sources that provide on‑chain liquidity information.
///
/// Liquidity sources expose their reserves or pool snapshots so that
/// other components (such as the optimisation engine) can compute
/// slippage curves, tick spacing and other metrics. Implementors
/// should return a vector of pool snapshots; the concrete type of
/// snapshot is intentionally generic to support different DEX models.
pub trait LiquiditySource<P>: Send + Sync {
    /// Return a collection of pool snapshots of type `P`.
    fn pools(&self) -> Vec<P>;
}

/// A trait for sources that can introspect pool metadata.
///
/// Certain venues expose additional metadata about pools (e.g. fee
/// tiers, tick ranges, underlying token decimals). Implementors can
/// provide synchronous accessors via this trait.
pub trait PoolIntrospection: Send + Sync {
    /// Returns the fee percentage charged by the pool as a fraction
    /// (e.g. 0.003 for 0.3%).
    fn fee_fraction(&self) -> f64;
    /// Returns the number of decimal places used for the input asset.
    fn input_decimals(&self) -> u8;
    /// Returns the number of decimal places used for the output asset.
    fn output_decimals(&self) -> u8;
}

/// Basic maths helpers used across DEX modules.
///
/// These functions provide common computations such as slippage
/// adjustment and fee calculation. They are kept outside of any
/// particular implementation so they can be reused by multiple
/// venues.
pub mod math {
    /// Compute the expected output after applying slippage.
    ///
    /// Given a nominal amount and a slippage fraction (e.g. 0.005 for
    /// 0.5%), this returns the minimum acceptable output.
    pub fn apply_slippage(amount_out: u128, slippage: f64) -> u128 {
        let slip = (amount_out as f64) * (1.0 - slippage);
        slip.floor() as u128
    }

    /// Compute the fee charged on an input amount given a fee fraction.
    ///
    /// The fee is returned in the same units as the input amount.
    pub fn calculate_fee(amount_in: u128, fee_fraction: f64) -> u128 {
        let fee = (amount_in as f64) * fee_fraction;
        fee.ceil() as u128
    }
}