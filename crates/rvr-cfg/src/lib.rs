//! Control flow graph analysis for RISC-V recompiler.

use std::collections::HashMap;

use rvr_isa::Xlen;
use rvr_ir::BlockIR;

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
