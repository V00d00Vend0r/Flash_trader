//! Decision stage trait.
//!
//! Stages encapsulate logic for producing a [`Decision`] from a
//! [`Signal`]. They are designed to be composed by the
//! [`SignalEngine`] and may run asynchronously (for example, when
//! querying external services). A stage can also decide whether it
//! is enabled for a given signal via the default implementation of
//! [`enabled`].

use anyhow::Result;
use async_trait::async_trait;
use super::types::{Signal, Decision};

/// A unit of decision logic.
///
/// Each stage must have a unique identifier and implement
/// [`evaluate`], which returns a [`Decision`] for the provided
/// [`Signal`]. The default implementation of [`enabled`] always
/// returns `true`, but stages can override this to skip evaluation
/// based on contextual criteria (e.g. symbol filtering).
#[async_trait]
pub trait DecisionStage: Send + Sync {
    /// Return a stable identifier for this stage.
    fn id(&self) -> &str;
    /// Execute the stage logic asynchronously on a signal.
    async fn evaluate(&self, signal: &Signal) -> Result<Decision>;
    /// Indicate whether this stage should run for the given signal. Stages may
    /// override this to perform light‑weight checks before committing to
    /// a potentially expensive asynchronous evaluation.
    fn enabled(&self, _signal: &Signal) -> bool {
        true
    }
}