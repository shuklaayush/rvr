//! Control flow graph analysis for RISC-V recompiler.

mod analyzer;
mod decoder;
mod instruction_table;
mod value;

use std::collections::{HashMap, HashSet};

use rvr_isa::Xlen;
use rvr_ir::BlockIR;

pub use analyzer::*;
pub use decoder::*;
pub use instruction_table::*;
pub use value::*;

/// Basic block information.
#[derive(Clone, Debug)]
pub struct BasicBlock<X: Xlen> {
    /// Starting PC.
    pub start_pc: X::Reg,
    /// Ending PC (exclusive).
    pub end_pc: X::Reg,
    /// Successor PCs.
    pub successors: Vec<X::Reg>,
    /// Predecessor PCs.
    pub predecessors: Vec<X::Reg>,
}

/// Block table - manages basic blocks.
#[derive(Clone, Debug)]
pub struct BlockTable<X: Xlen> {
    /// Map from start PC to block.
    pub blocks: HashMap<u64, BasicBlock<X>>,
    /// Lifted IR for each block.
    pub ir_blocks: HashMap<u64, BlockIR<X>>,
}

impl<X: Xlen> BlockTable<X> {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            ir_blocks: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

impl<X: Xlen> Default for BlockTable<X> {
    fn default() -> Self {
        Self::new()
    }
}

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
