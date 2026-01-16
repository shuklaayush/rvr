//! Basic block IR.

use crate::xlen::Xlen;

use crate::instr::InstrIR;
use crate::terminator::Terminator;

/// IR for a basic block (sequence of instructions).
#[derive(Clone, Debug)]
pub struct BlockIR<X: Xlen> {
    /// Starting PC of the block.
    pub start_pc: X::Reg,
    /// Ending PC (exclusive) of the block.
    pub end_pc: X::Reg,
    /// Instructions in the block.
    pub instructions: Vec<InstrIR<X>>,
}

impl<X: Xlen> BlockIR<X> {
    /// Create a new block.
    pub fn new(start_pc: X::Reg) -> Self {
        Self {
            start_pc,
            end_pc: start_pc,
            instructions: Vec::new(),
        }
    }

    /// Add an instruction to the block.
    pub fn push(&mut self, instr: InstrIR<X>) {
        self.end_pc = instr.next_pc();
        self.instructions.push(instr);
    }

    /// Get the terminator of the last instruction.
    pub fn terminator(&self) -> Option<&Terminator<X>> {
        self.instructions.last().map(|i| &i.terminator)
    }

    /// Get block size in bytes.
    pub fn size(&self) -> u64 {
        X::to_u64(self.end_pc) - X::to_u64(self.start_pc)
    }

    /// Get number of instructions.
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Check if block is empty.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}
