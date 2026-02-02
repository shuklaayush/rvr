//! Differential execution for rvr.
//!
//! Compares execution between two backends instruction-by-instruction:
//! - `spike-c`: Spike (reference) vs C backend
//! - `spike-arm64`: Spike (reference) vs ARM64 backend
//! - `c-arm64`: C backend vs ARM64 backend
//!
//! Unlike trace comparison which writes traces to disk, differential execution
//! runs in lockstep and compares state in memory.

pub mod compare;
pub mod executor;
pub mod inprocess;
pub mod spike;
pub mod state;

pub use compare::compare_lockstep;
pub use inprocess::InProcessExecutor;
pub use spike::{find_spike, SpikeExecutor};
pub use state::{CompareConfig, DiffGranularity};
