//! Trace comparison for differential testing.
//!
//! Compares instruction traces between rvr and Spike (the RISC-V reference simulator)
//! to catch bugs at the instruction level rather than just end-state.

mod compare;
mod parse;
mod util;

#[cfg(test)]
mod tests;

pub use compare::{align_traces_at, compare_traces_with_config};
pub use parse::parse_trace_file;
pub use util::{
    elf_entry_point, elf_to_isa, find_spike, isa_from_test_name, run_command_with_timeout,
};

use std::fmt;

/// A single instruction trace entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    /// Program counter.
    pub pc: u64,
    /// Raw instruction opcode.
    pub opcode: u32,
    /// Destination register (if any).
    pub rd: Option<u8>,
    /// Value written to rd (if any).
    pub rd_value: Option<u64>,
    /// Memory address accessed (if any).
    pub mem_addr: Option<u64>,
}

/// Result of comparing two traces.
#[derive(Debug)]
pub struct TraceComparison {
    /// Number of instructions that matched.
    pub matched: usize,
    /// First divergence (if any).
    pub divergence: Option<TraceDivergence>,
}

/// Information about where traces diverged.
#[derive(Debug)]
pub struct TraceDivergence {
    /// Instruction index in the aligned stream where divergence occurred.
    pub index: usize,
    /// Expected entry (from Spike).
    pub expected: TraceEntry,
    /// Actual entry (from rvr).
    pub actual: TraceEntry,
    /// Type of divergence.
    pub kind: DivergenceKind,
}

/// Type of divergence between traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceKind {
    /// PC mismatch.
    Pc,
    /// Opcode mismatch.
    Opcode,
    /// Register write destination mismatch.
    RegDest,
    /// Register write value mismatch.
    RegValue,
    /// Memory address mismatch.
    MemAddr,
    /// Expected had register write, actual didn't.
    MissingRegWrite,
    /// Actual had register write, expected didn't.
    ExtraRegWrite,
    /// Expected had memory access, actual didn't.
    MissingMemAccess,
    /// Actual had memory access, expected didn't.
    ExtraMemAccess,
    /// Expected trace has remaining entries (actual ended early).
    ExpectedTail,
    /// Actual trace has remaining entries (expected ended early).
    ActualTail,
}

impl fmt::Display for DivergenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pc => write!(f, "PC mismatch"),
            Self::Opcode => write!(f, "opcode mismatch"),
            Self::RegDest => write!(f, "register destination mismatch"),
            Self::RegValue => write!(f, "register value mismatch"),
            Self::MemAddr => write!(f, "memory address mismatch"),
            Self::MissingRegWrite => write!(f, "missing register write in actual"),
            Self::ExtraRegWrite => write!(f, "extra register write in actual"),
            Self::MissingMemAccess => write!(f, "missing memory access in actual"),
            Self::ExtraMemAccess => write!(f, "extra memory access in actual"),
            Self::ExpectedTail => write!(f, "expected trace has extra tail"),
            Self::ActualTail => write!(f, "actual trace has extra tail"),
        }
    }
}

/// Configuration for trace comparison behavior.
#[derive(Debug, Clone)]
pub struct CompareConfig {
    /// Entry point address for alignment (from ELF).
    pub entry_point: u64,
    /// Whether to require matching register writes (strict mode).
    /// If false, missing writes on one side are tolerated.
    pub strict_reg_writes: bool,
    /// Whether to require matching memory accesses (strict mode).
    /// If false, missing mem accesses on one side are tolerated.
    pub strict_mem_access: bool,
    /// Whether to stop on the first divergence.
    pub stop_on_first: bool,
}

impl Default for CompareConfig {
    fn default() -> Self {
        Self {
            entry_point: 0x8000_0000,
            strict_reg_writes: true,
            strict_mem_access: false, // Spike doesn't always log mem for loads
            stop_on_first: true,
        }
    }
}
