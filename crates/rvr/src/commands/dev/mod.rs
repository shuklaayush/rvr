//! Developer commands.
//!
//! - `trace`: Trace comparison between rvr and Spike for differential testing
//! - `diff`: Lockstep differential execution between backends

mod diff;
mod trace;

pub use diff::{DiffCompareArgs, diff_compare};
pub use trace::trace_compare;
