//! Differential execution for rvr.
//!
//! Compares execution between two backends instruction-by-instruction:
//! - `spike-c`: Spike (reference) vs C backend
//! - `spike-arm64`: Spike (reference) vs ARM64 backend
//! - `c-arm64`: C backend vs ARM64 backend
//!
//! Unlike trace comparison which writes traces to disk, differential execution
//! runs in lockstep and compares state in memory.

pub mod c_compare;
pub mod compile;
pub mod compare;
pub mod executor;
pub mod inprocess;
pub mod spike;
pub mod state;

pub use c_compare::{CCompareConfig, compile_c_compare, generate_c_compare, run_c_compare};
pub use compile::{compile_for_checkpoint, compile_for_diff, compile_for_diff_block};
pub use compare::{compare_block_vs_linear, compare_checkpoint, compare_lockstep};
pub use inprocess::{BufferedInProcessExecutor, InProcessExecutor};
pub use spike::{SpikeExecutor, find_spike};
pub use state::{CompareConfig, CompareResult, DiffGranularity, DiffState, Divergence, DivergenceKind};

/// Check if a backend supports diff tracing.
pub fn backend_supports_diff(backend: rvr_emit::Backend) -> bool {
    matches!(
        backend,
        rvr_emit::Backend::C | rvr_emit::Backend::ARM64Asm | rvr_emit::Backend::X86Asm
    )
}

/// Check if a backend supports buffered diff tracing.
pub fn backend_supports_buffered_diff(backend: rvr_emit::Backend) -> bool {
    matches!(backend, rvr_emit::Backend::C)
}
