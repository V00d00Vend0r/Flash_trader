use anyhow::Result;
use std::sync::Arc;
use crate::execution::types::ExecutionSignal;
use crate::engines::{Engine, EngineContext};

/// A simple ordered pipeline that sequentially runs a series of [`Engine`] stages.
///
/// Each stage is executed only if it reports itself as enabled via [`Engine::enabled`].
/// The pipeline holds a shared [`EngineContext`] that is passed through to every
/// stage invocation. On execution, an [`ExecutionSignal`] flows through the
/// pipeline, allowing each stage to augment or act upon the signal. The final
/// signal is returned to the caller.
pub struct EnginePipeline {
    /// Shared execution context provided to each pipeline stage.  This typically
    /// contains state or configuration that all stages may need to access.
    ctx: EngineContext,
    /// Ordered collection of stages to run.  Each stage must implement the
    /// [`Engine`] trait and is stored behind an [`Arc`] to allow for cheap cloning
    /// and shared ownership across pipelines.
    stages: Vec<Arc<dyn Engine>>,
}
impl EnginePipeline {
    /// Create a new [`EnginePipeline`] from a context and list of stages.
    pub fn new(ctx: EngineContext, stages: Vec<Arc<dyn Engine>>) -> Self {
        Self { ctx, stages }
    }
    /// Execute each enabled stage in order, passing the [`ExecutionSignal`] along.
    ///
    /// If any stage returns an error the pipeline will short circuit and
    /// propagate that error to the caller.  Otherwise the updated signal from
    /// the final stage is returned.
    pub async fn run(&self, mut signal: ExecutionSignal) -> Result<ExecutionSignal> {
        for stage in &self.stages {
            if stage.enabled() {
                // Run the stage and update the signal.  Errors are propagated to the
                // caller which allows upstream code to handle failures appropriately.
                signal = stage.tick(&self.ctx, signal).await?;
            }
        }
        Ok(signal)
    }
}
