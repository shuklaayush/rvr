//! Executor trait for differential testing.
//!
//! Defines the interface that all executors (Spike, C, ARM64) must implement.

use super::state::DiffState;

/// An executor that can step through instructions and capture state.
pub trait Executor {
    /// Execute exactly one instruction and return the resulting state.
    fn step(&mut self) -> Option<DiffState>;
}
