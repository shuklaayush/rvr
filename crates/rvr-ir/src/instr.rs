//! Single instruction IR.

use crate::xlen::Xlen;
use crate::stmt::Stmt;
use crate::terminator::Terminator;

/// Source location for debug info (#line directives).
#[derive(Clone, Debug, Default)]
pub struct SourceLoc {
    /// Source file name.
    pub file: String,
    /// Line number.
    pub line: u32,
    /// Optional function name for comments.
    pub function: String,
}

impl SourceLoc {
    /// Create a new source location.
    pub fn new(file: &str, line: u32, function: &str) -> Self {
        Self {
            file: file.to_string(),
            line,
            function: function.to_string(),
        }
    }

    /// Check if this is a valid source location.
    pub fn is_valid(&self) -> bool {
        !self.file.is_empty() && self.file != "??" && self.line > 0
    }
}

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
    /// Optional source location for debug info.
    pub source_loc: Option<SourceLoc>,
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
            source_loc: None,
        }
    }

    /// Create a new instruction IR with source location.
    pub fn with_source_loc(
        pc: X::Reg,
        size: u8,
        statements: Vec<Stmt<X>>,
        terminator: Terminator<X>,
        source_loc: SourceLoc,
    ) -> Self {
        Self {
            pc,
            size,
            statements,
            terminator,
            source_loc: Some(source_loc),
        }
    }

    /// Set the source location.
    pub fn set_source_loc(&mut self, loc: SourceLoc) {
        self.source_loc = Some(loc);
    }

    /// Get the PC of the next instruction (pc + size).
    pub fn next_pc(&self) -> X::Reg {
        X::from_u64(X::to_u64(self.pc) + self.size as u64)
    }

    /// Check if this is a compressed (16-bit) instruction.
    pub fn is_compressed(&self) -> bool {
        self.size == 2
    }

    /// Check if this instruction is a branch.
    pub fn is_branch(&self) -> bool {
        self.terminator.is_branch()
    }

    /// Check if this instruction is any kind of jump.
    pub fn is_jump(&self) -> bool {
        self.terminator.is_jump()
    }

    /// Check if this instruction ends a basic block.
    pub fn is_block_end(&self) -> bool {
        self.terminator.is_control_flow()
    }
}
