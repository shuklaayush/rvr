//! Single instruction IR.

use crate::xlen::Xlen;
use crate::stmt::Stmt;
use crate::terminator::Terminator;

/// IR for a single instruction.
#[derive(Clone, Debug)]
pub struct InstrIR<X: Xlen> {
    /// Program counter of this instruction.
    pub pc: X::Reg,
    /// Instruction size in bytes (2 or 4).
    pub size: u8,
    /// Statements (writes, side effects).
    pub statements: Vec<Stmt<X>>,
    /// Control flow terminator.
    pub terminator: Terminator<X>,
}

impl<X: Xlen> InstrIR<X> {
    /// Create a new instruction IR.
    pub fn new(
        pc: X::Reg,
        size: u8,
        statements: Vec<Stmt<X>>,
        terminator: Terminator<X>,
    ) -> Self {
        Self {
            pc,
            size,
            statements,
            terminator,
        }
    }

    /// Get the PC of the next instruction (pc + size).
    pub fn next_pc(&self) -> X::Reg {
        X::from_u64(X::to_u64(self.pc) + self.size as u64)
    }
}
