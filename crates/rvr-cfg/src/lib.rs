//! Control flow graph analysis for RISC-V recompiler.

mod analyzer;
mod block_table;
mod decoder;
mod instruction_table;
mod value;

use std::collections::{HashMap, HashSet};

pub use analyzer::*;
pub use block_table::*;
pub use decoder::*;
pub use instruction_table::*;
pub use value::*;

/// Control flow analysis result (not generic - all addresses are u64).
#[derive(Clone, Debug, Default)]
pub struct CfgResult {
    /// Successors map: PC -> set of successor PCs.
    pub successors: HashMap<u64, HashSet<u64>>,
    /// Predecessors map: PC -> set of predecessor PCs.
    pub predecessors: HashMap<u64, HashSet<u64>>,
    /// Unresolved dynamic jumps (indirect jumps we couldn't resolve).
    pub unresolved_jumps: HashSet<u64>,
    /// Basic block leaders (start of each block).
    pub leaders: HashSet<u64>,
    /// Call return map: callee -> set of return addresses.
    pub call_return_map: HashMap<u64, HashSet<u64>>,
    /// Block to function mapping: block PC -> function start PC.
    pub block_to_function: HashMap<u64, u64>,
    /// Function entry points.
    pub function_entries: HashSet<u64>,
    /// Internal branch targets (within functions).
    pub internal_targets: HashSet<u64>,
}
