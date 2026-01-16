//! Core types for RISC-V ISA.

use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::{Add, BitAnd, BitOr, BitXor, Not, Shl, Shr, Sub};

/// Marker type for RV32.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rv32;

/// Marker type for RV64.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rv64;

/// Trait for XLEN-dependent operations.
///
/// Uses marker types (Rv32/Rv64) with associated types instead of const generics
/// because Rust doesn't support type-level computation like Mojo's comptime.
pub trait Xlen: Copy + Clone + Send + Sync + Default + Debug + 'static {
    /// Register type (u32 for RV32, u64 for RV64).
    type Reg: Copy
        + Clone
        + Default
        + Eq
        + Ord
        + Hash
        + Debug
        + Display
        + Send
        + Sync
        + From<u32>
        + Into<u64>
        + Add<Output = Self::Reg>
        + Sub<Output = Self::Reg>
        + BitAnd<Output = Self::Reg>
        + BitOr<Output = Self::Reg>
        + BitXor<Output = Self::Reg>
        + Not<Output = Self::Reg>
        + Shl<u32, Output = Self::Reg>
        + Shr<u32, Output = Self::Reg>;

    /// Signed register type.
    type SignedReg: Copy + Clone + Debug;

    /// XLEN value (32 or 64).
    const VALUE: u8;

    /// Shift amount mask (0x1F for RV32, 0x3F for RV64).
    const SHIFT_MASK: u8;

    /// Bytes per register (4 for RV32, 8 for RV64).
    const REG_BYTES: usize;

    /// Sign-extend a 32-bit value to register width.
    fn sign_extend_32(val: u32) -> Self::Reg;

    /// Truncate register to 32 bits.
    fn truncate_to_32(val: Self::Reg) -> u32;

    /// Zero-extend a u64 to register width.
    fn from_u64(val: u64) -> Self::Reg;

    /// Convert register to u64.
    fn to_u64(val: Self::Reg) -> u64;
}

impl Xlen for Rv32 {
    type Reg = u32;
    type SignedReg = i32;

    const VALUE: u8 = 32;
    const SHIFT_MASK: u8 = 0x1F;
    const REG_BYTES: usize = 4;

    #[inline]
    fn sign_extend_32(val: u32) -> u32 {
        val
    }

    #[inline]
    fn truncate_to_32(val: u32) -> u32 {
        val
    }

    #[inline]
    fn from_u64(val: u64) -> u32 {
        val as u32
    }

    #[inline]
    fn to_u64(val: u32) -> u64 {
        val as u64
    }
}

impl Xlen for Rv64 {
    type Reg = u64;
    type SignedReg = i64;

    const VALUE: u8 = 64;
    const SHIFT_MASK: u8 = 0x3F;
    const REG_BYTES: usize = 8;

    #[inline]
    fn sign_extend_32(val: u32) -> u64 {
        val as i32 as i64 as u64
    }

    #[inline]
    fn truncate_to_32(val: u64) -> u32 {
        val as u32
    }

    #[inline]
    fn from_u64(val: u64) -> u64 {
        val
    }

    #[inline]
    fn to_u64(val: u64) -> u64 {
        val
    }
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
}

impl Display for OpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpId({}, {})", self.ext, self.idx)
    }
}

// Extension constants
pub const EXT_I: u8 = 0;
pub const EXT_M: u8 = 1;
pub const EXT_A: u8 = 2;
pub const EXT_C: u8 = 3;
pub const EXT_ZICSR: u8 = 4;
pub const EXT_ZIFENCEI: u8 = 5;
pub const EXT_CUSTOM: u8 = 128;

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

/// Get register ABI name.
pub fn reg_name(reg: u8) -> &'static str {
    match reg {
        0 => "zero",
        1 => "ra",
        2 => "sp",
        3 => "gp",
        4 => "tp",
        5 => "t0",
        6 => "t1",
        7 => "t2",
        8 => "s0",
        9 => "s1",
        10 => "a0",
        11 => "a1",
        12 => "a2",
        13 => "a3",
        14 => "a4",
        15 => "a5",
        16 => "a6",
        17 => "a7",
        18 => "s2",
        19 => "s3",
        20 => "s4",
        21 => "s5",
        22 => "s6",
        23 => "s7",
        24 => "s8",
        25 => "s9",
        26 => "s10",
        27 => "s11",
        28 => "t3",
        29 => "t4",
        30 => "t5",
        31 => "t6",
        _ => "??",
    }
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
