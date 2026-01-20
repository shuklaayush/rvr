//! Control flow graph construction for RISC-V recompiler.

mod analysis;
mod block_table;
mod instruction_table;

pub use block_table::*;
pub use instruction_table::*;
