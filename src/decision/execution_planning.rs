//! Execution planning utilities.
//!
//! After a high‑level trading decision has been made, the order
//! quantity may need to be adjusted based on the available depth in
//! the order book. This module defines a simple planner that
//! reduces the execution size when liquidity is thin, thus helping
//! mitigate slippage.

use anyhow::Result;
use super::types::Plan;

/// A configurable execution planner.
///
/// When provided with an execution plan and an optional order book
/// depth (in basis points), the planner will scale the quantity if
/// the depth is below the configured threshold. The scaling factor
/// and depth threshold can be customised at construction time.
pub struct ExecutionPlanner {
    /// Depth threshold below which the quantity will be reduced.
    pub depth_threshold: f64,
    /// Factor by which to multiply the quantity when depth is below
    /// the threshold. A factor of 0.5 halves the size.
    pub reduction_factor: f64,
}

impl ExecutionPlanner {
    /// Construct a new planner with the specified threshold and
    /// reduction factor. Typical values might be `(10.0, 0.5)` to
    /// halve the order when the depth is less than ten basis points.
    pub fn new(depth_threshold: f64, reduction_factor: f64) -> Self {
        Self { depth_threshold, reduction_factor }
    }

    /// Adjust the plan's quantity based on the current order book
    /// depth. If `book_depth_bps` is provided and less than
    /// `depth_threshold`, the quantity is scaled by `reduction_factor`.
    pub fn shape_for_execution(&self, mut plan: Plan, book_depth_bps: Option<f64>) -> Result<Plan> {
        if let Some(depth) = book_depth_bps {
            if depth < self.depth_threshold {
                plan.qty *= self.reduction_factor;
            }
        }
        Ok(plan)
    }
}