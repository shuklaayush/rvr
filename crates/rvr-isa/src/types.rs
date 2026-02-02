//! Core types for RISC-V ISA.

use std::fmt::Display;

// Re-export Xlen types from rvr-ir
pub use rvr_ir::{Rv32, Rv64, Xlen};

/// Decoded instruction with all fields extracted.
#[derive(Clone, Debug)]
pub struct DecodedInstr<X: Xlen> {
    /// Instruction identifier.
    pub opid: OpId,
    /// Program counter.
    pub pc: X::Reg,
    /// Instruction size in bytes (2 for compressed, 4 for normal).
    pub size: u8,
    /// Raw instruction bytes (16-bit for compressed, 32-bit for normal).
    pub raw: u32,
    /// Instruction arguments.
    pub args: InstrArgs,
}

impl<X: Xlen> DecodedInstr<X> {
    pub fn new(opid: OpId, pc: X::Reg, size: u8, raw: u32, args: InstrArgs) -> Self {
        Self {
            opid,
            pc,
            size,
            raw,
            args,
        }
    }
}

/// Instruction argument patterns (covers all RISC-V formats + custom).
#[derive(Clone, Debug, PartialEq)]
pub enum InstrArgs {
    /// R-type: rd, rs1, rs2
    R { rd: u8, rs1: u8, rs2: u8 },
    /// R4-type: rd, rs1, rs2, rs3 (for fused ops)
    R4 { rd: u8, rs1: u8, rs2: u8, rs3: u8 },
    /// I-type: rd, rs1, imm
    I { rd: u8, rs1: u8, imm: i32 },
    /// S-type: rs1, rs2, imm
    S { rs1: u8, rs2: u8, imm: i32 },
    /// B-type: rs1, rs2, imm
    B { rs1: u8, rs2: u8, imm: i32 },
    /// U-type: rd, imm
    U { rd: u8, imm: i32 },
    /// J-type: rd, imm
    J { rd: u8, imm: i32 },
    /// CSR: rd, rs1, csr
    Csr { rd: u8, rs1: u8, csr: u16 },
    /// CSRI: rd, imm, csr
    CsrI { rd: u8, imm: u8, csr: u16 },
    /// AMO: rd, rs1, rs2, aq, rl
    Amo {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// No arguments (ECALL, EBREAK, etc.)
    None,
    /// Custom instruction arguments
    Custom(Box<[u32]>),
}

/// Compact instruction identifier (2 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct OpId {
    /// Extension (EXT_I, EXT_M, etc.)
    pub ext: u8,
    /// Index within extension
    pub idx: u8,
}

impl OpId {
    pub const fn new(ext: u8, idx: u8) -> Self {
        Self { ext, idx }
    }

    /// Pack OpId into uint16_t: (ext << 8) | idx.
    pub const fn pack(self) -> u16 {
        ((self.ext as u16) << 8) | (self.idx as u16)
    }
}

impl Display for OpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpId({}, {})", self.ext, self.idx)
    }
}

/// Instruction classification for control flow and optimization.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpClass {
    /// Arithmetic/logical operation (ADD, SUB, AND, etc.)
    Alu,
    /// Load from memory
    Load,
    /// Store to memory
    Store,
    /// Conditional branch (BEQ, BNE, etc.)
    Branch,
    /// Unconditional jump (JAL)
    Jump,
    /// Indirect jump (JALR)
    JumpIndirect,
    /// CSR read/write
    Csr,
    /// Atomic memory operation
    Atomic,
    /// Fence/barrier
    Fence,
    /// System call (ECALL, EBREAK)
    System,
    /// Multiply operation
    Mul,
    /// Division operation
    Div,
    /// No operation
    Nop,
    /// Unknown/other
    Other,
}

/// Instruction metadata for analysis and optimization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpInfo {
    /// Instruction identifier
    pub opid: OpId,
    /// Mnemonic name (e.g., "add", "beq")
    pub name: &'static str,
    /// Instruction class for control flow analysis
    pub class: OpClass,
    /// Typical size in bytes (2 for compressed, 4 for normal)
    pub size_hint: u8,
}

// Extension constants
pub const EXT_I: u8 = 0;
pub const EXT_M: u8 = 1;
pub const EXT_A: u8 = 2;
pub const EXT_C: u8 = 3;
pub const EXT_ZICSR: u8 = 4;
pub const EXT_ZIFENCEI: u8 = 5;
pub const EXT_ZBA: u8 = 6;
pub const EXT_ZBB: u8 = 7;
pub const EXT_ZBS: u8 = 8;
pub const EXT_ZBKB: u8 = 9;
pub const EXT_ZICOND: u8 = 10;

// Number of registers
pub const NUM_REGS_I: usize = 32;
pub const NUM_REGS_E: usize = 16;
pub const NUM_CSRS: usize = 4096;

// Register ABI names
pub const REG_ZERO: u8 = 0;
pub const REG_RA: u8 = 1;
pub const REG_SP: u8 = 2;
pub const REG_GP: u8 = 3;
pub const REG_TP: u8 = 4;
pub const REG_T0: u8 = 5;
pub const REG_T1: u8 = 6;
pub const REG_T2: u8 = 7;
pub const REG_S0: u8 = 8;
pub const REG_FP: u8 = 8; // Frame pointer alias for s0
pub const REG_S1: u8 = 9;
pub const REG_A0: u8 = 10;
pub const REG_A1: u8 = 11;
pub const REG_A2: u8 = 12;
pub const REG_A3: u8 = 13;
pub const REG_A4: u8 = 14;
pub const REG_A5: u8 = 15;
pub const REG_A6: u8 = 16;
pub const REG_A7: u8 = 17;
pub const REG_S2: u8 = 18;
pub const REG_S3: u8 = 19;
pub const REG_S4: u8 = 20;
pub const REG_S5: u8 = 21;
pub const REG_S6: u8 = 22;
pub const REG_S7: u8 = 23;
pub const REG_S8: u8 = 24;
pub const REG_S9: u8 = 25;
pub const REG_S10: u8 = 26;
pub const REG_S11: u8 = 27;
pub const REG_T3: u8 = 28;
pub const REG_T4: u8 = 29;
pub const REG_T5: u8 = 30;
pub const REG_T6: u8 = 31;

/// Canonical register ABI names array.
pub const REG_ABI_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

/// Get register ABI name.
#[inline]
pub fn reg_name(reg: u8) -> &'static str {
    REG_ABI_NAMES.get(reg as usize).copied().unwrap_or("??")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xlen_rv32() {
        assert_eq!(Rv32::VALUE, 32);
        assert_eq!(Rv32::SHIFT_MASK, 0x1F);
        assert_eq!(Rv32::REG_BYTES, 4);
        assert_eq!(Rv32::sign_extend_32(0xFFFFFFFF), 0xFFFFFFFF);
    }

    #[test]
    fn test_xlen_rv64() {
        assert_eq!(Rv64::VALUE, 64);
        assert_eq!(Rv64::SHIFT_MASK, 0x3F);
        assert_eq!(Rv64::REG_BYTES, 8);
        // Sign extension: 0xFFFFFFFF (-1 as i32) becomes 0xFFFFFFFFFFFFFFFF
        assert_eq!(Rv64::sign_extend_32(0xFFFFFFFF), 0xFFFFFFFFFFFFFFFF);
        // Positive value stays the same
        assert_eq!(Rv64::sign_extend_32(0x7FFFFFFF), 0x7FFFFFFF);
    }

    #[test]
    fn test_opid() {
        let op = OpId::new(EXT_I, 5);
        assert_eq!(op.ext, EXT_I);
        assert_eq!(op.idx, 5);
    }
}
