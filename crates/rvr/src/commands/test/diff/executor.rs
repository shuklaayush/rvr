//! Executor trait for differential testing.
//!
//! Defines the interface that all executors (Spike, C, ARM64) must implement.

use super::state::DiffState;

/// An executor that can step through instructions and capture state.
pub trait Executor {
    /// Execute exactly one instruction and return the resulting state.
    fn step(&mut self) -> Option<DiffState>;

    /// Execute N instructions and return the state after the last one.
    ///
    /// Default implementation calls step() N times.
    fn step_n(&mut self, n: u64) -> Option<DiffState> {
        let mut last = None;
        for _ in 0..n {
            match self.step() {
                Some(state) => {
                    if state.is_exit() {
                        return Some(state);
                    }
                    last = Some(state);
                }
                None => return None,
            }
        }
        last
    }

    /// Get the current PC without advancing.
    fn current_pc(&self) -> u64;

    /// Get the current instruction count.
    fn instret(&self) -> u64;

    /// Check if the program has exited.
    fn has_exited(&self) -> bool;

    /// Get the exit code if exited.
    fn exit_code(&self) -> Option<u8>;

    /// Reset execution to the given PC (if supported).
    ///
    /// Returns false if reset is not supported.
    fn reset_to(&mut self, _pc: u64) -> bool {
        false
    }
}
