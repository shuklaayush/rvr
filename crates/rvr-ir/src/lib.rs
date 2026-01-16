//! Intermediate representation for RISC-V recompiler.
//!
//! This crate provides pure IR types with no RISC-V-specific knowledge.
//! The RISC-V instruction lifting is implemented in `rvr-isa`.

mod xlen;
mod expr;
mod stmt;
mod terminator;
mod instr;
mod block;
mod builder;

pub use xlen::*;
pub use expr::*;
pub use stmt::*;
pub use terminator::*;
pub use instr::*;
pub use block::*;
pub use builder::*;
