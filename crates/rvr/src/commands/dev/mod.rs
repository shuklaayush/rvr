//! Developer commands.
//!
//! - `trace`: Trace comparison between rvr and Spike for differential testing
//! - `diff`: Lockstep differential execution between backends

mod diff;
mod trace;

pub use diff::{diff_compare, DiffCompareArgs};
pub use trace::trace_compare;
