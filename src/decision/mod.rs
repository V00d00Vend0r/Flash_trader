//! Decision layer module index.
//!
//! This module assembles the building blocks of the decision
//! subsystem. The submodules include:
//!
//! - [`types`]: Basic data structures such as [`Signal`], [`Decision`]
//!   and [`Plan`].
//! - [`stage`]: The [`DecisionStage`] trait describing asynchronous
//!   stages.
//! - [`signal_engine`]: A coordinator that blends multiple stages
//!   together to produce an execution plan.
//! - [`arbitrage_detection`], [`execution_planning`], [`phi3_stage`]:
//!   concrete stage implementations.
//!
//! Re‑exporting these modules here allows consumers to `use
//! decision::*` for convenience.

pub mod types;
pub mod stage;
pub mod signal_engine;
pub mod arbitrage_detection;
pub mod execution_planning;
pub mod phi3_stage;