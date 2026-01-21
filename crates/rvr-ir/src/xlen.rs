//! Register width types (XLEN).
//!
//! These are generic "32 vs 64 bit" types, not RISC-V specific.

use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::{Add, BitAnd, BitOr, BitXor, Not, Shl, Shr, Sub};

/// Marker type for 32-bit register width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rv32;

/// Marker type for 64-bit register width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rv64;

/// Trait for register-width-dependent operations.
///
/// Uses marker types (Rv32/Rv64) with associated types instead of const generics
/// because Rust doesn't support type-level computation with const generics.
pub trait Xlen: Copy + Clone + Send + Sync + Default + Debug + 'static {
    /// Register type (u32 for Rv32, u64 for Rv64).
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

    /// Shift amount mask (0x1F for 32-bit, 0x3F for 64-bit).
    const SHIFT_MASK: u8;

    /// Bytes per register (4 for 32-bit, 8 for 64-bit).
    const REG_BYTES: usize;

    /// Sign-extend a 32-bit value to register width.
    fn sign_extend_32(val: u32) -> Self::Reg;

    /// Truncate register to 32 bits.
    fn truncate_to_32(val: Self::Reg) -> u32;

    /// Convert a u64 to register width.
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
}
