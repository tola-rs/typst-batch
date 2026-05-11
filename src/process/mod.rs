//! Document processing pipeline.
//!
//! - [`Compiler`] - Builder-based compilation API
//! - [`Batcher`] - Batch compilation API for parallel processing
//! - [`Scanner`] - Builder-based scanning API (Eval only, skips Layout)

#[cfg(feature = "batch")]
pub mod batch;
mod common;
pub mod compile;
mod inputs;
#[cfg(feature = "scan")]
pub mod scan;
mod session;

pub use inputs::WithInputs;
pub use session::{AccessedDeps, CompileSession};

#[cfg(feature = "batch")]
pub use batch::{BatchScanner, Batcher};
