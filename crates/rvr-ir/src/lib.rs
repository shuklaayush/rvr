//! Intermediate representation for RISC-V recompiler.
//!
//! This crate provides pure IR types with no RISC-V-specific knowledge.
//! The RISC-V instruction lifting is implemented in `rvr-isa`.

mod block;
mod builder;
mod expr;
mod instr;
mod stmt;
mod terminator;
mod xlen;

pub use block::*;
pub use builder::*;
pub use expr::*;
pub use instr::*;
pub use stmt::*;
pub use terminator::*;
pub use xlen::*;
