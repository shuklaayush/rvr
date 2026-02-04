//! HTIF (Host-Target Interface) constants.
//!
//! Shared constants for HTIF protocol used by riscv-tests.
//! These addresses match the expectations of the riscv-tests suite.

/// HTIF tohost address - writes here signal exit or syscall.
pub const TOHOST_ADDR: u64 = 0x8000_1000;

/// HTIF fromhost address - used for syscall acknowledgment.
pub const FROMHOST_ADDR: u64 = 0x8000_1008;

/// HTIF syscall number for write.
pub const SYS_WRITE: u64 = 64;

/// HTIF file descriptor for stdout.
pub const STDOUT_FD: u64 = 1;
