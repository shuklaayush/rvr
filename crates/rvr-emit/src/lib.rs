//! Code emission for RISC-V recompiler.
//!
//! Supports multiple backends:
//! - `c` - C code emission (default)
//! - `x86` - x86-64 assembly emission (experimental)
//! - `arm64` - ARM64 assembly emission (experimental)

mod config;
pub mod htif;
mod inputs;
mod layout;

pub mod arm64;
pub mod c;
pub mod x86;

pub use config::*;
pub use inputs::*;
pub use layout::RvStateLayout;
