//! Differential execution state and comparison.
//!
//! Defines the state captured after each instruction and comparison algorithms.

/// Effects observed for one instruction execution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffState {
    /// Program counter.
    pub pc: u64,
    /// Raw instruction opcode.
    pub opcode: u32,
    /// Instructions retired so far.
    pub instret: u64,
    /// Destination register written (None if no write, or x0).
    pub rd: Option<u8>,
    /// Value written to rd.
    pub rd_value: Option<u64>,
    /// Memory address accessed (if any).
    pub mem_addr: Option<u64>,
    /// Value read/written (if available).
    pub mem_value: Option<u64>,
    /// Memory access width in bytes (1/2/4/8).
    pub mem_width: Option<u8>,
    /// True if this was a store, false if load.
    pub is_write: bool,
    /// True if this instruction caused program exit.
    pub is_exit: bool,
}

impl DiffState {
    /// Check if this state represents program exit.
    pub fn is_exit(&self) -> bool {
        self.is_exit
    }
}

/// Granularity of comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffGranularity {
    /// Compare after every instruction.
    Instruction,
    /// Compare at CFG block boundaries.
    Block,
    /// Compare by block, drill down to instruction on divergence.
    #[default]
    Hybrid,
    /// Fast checkpoint-based comparison: compare PC+registers at intervals.
    /// Falls back to instruction-level on divergence.
    Checkpoint,
    /// Pure C comparison: generates a standalone C program that dlopen's both
    /// backends and compares execution without any Rust FFI overhead.
    PureC,
}

impl std::str::FromStr for DiffGranularity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "instruction" | "instr" | "i" => Ok(Self::Instruction),
            "block" | "b" => Ok(Self::Block),
            "hybrid" | "h" => Ok(Self::Hybrid),
            "checkpoint" | "ckpt" | "c" | "fast" => Ok(Self::Checkpoint),
            "purec" | "pure-c" | "native" | "n" => Ok(Self::PureC),
            _ => Err(format!("unknown granularity: {}", s)),
        }
    }
}

/// Kind of divergence between reference and test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceKind {
    /// PC mismatch.
    Pc,
    /// Opcode mismatch (same PC but different instruction).
    Opcode,
    /// Different destination register.
    RegDest,
    /// Same register but different value.
    RegValue,
    /// Memory address mismatch.
    MemAddr,
    /// Memory value mismatch.
    MemValue,
    /// Reference wrote a register but test didn't.
    MissingRegWrite,
    /// Test wrote a register but reference didn't.
    ExtraRegWrite,
    /// Reference accessed memory but test didn't.
    MissingMemAccess,
    /// Test accessed memory but reference didn't.
    ExtraMemAccess,
    /// Reference has more instructions.
    ExpectedTail,
    /// Test has more instructions.
    ActualTail,
}

impl std::fmt::Display for DivergenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pc => write!(f, "PC mismatch"),
            Self::Opcode => write!(f, "opcode mismatch"),
            Self::RegDest => write!(f, "register destination mismatch"),
            Self::RegValue => write!(f, "register value mismatch"),
            Self::MemAddr => write!(f, "memory address mismatch"),
            Self::MemValue => write!(f, "memory value mismatch"),
            Self::MissingRegWrite => write!(f, "missing register write"),
            Self::ExtraRegWrite => write!(f, "extra register write"),
            Self::MissingMemAccess => write!(f, "missing memory access"),
            Self::ExtraMemAccess => write!(f, "extra memory access"),
            Self::ExpectedTail => write!(f, "reference has more instructions"),
            Self::ActualTail => write!(f, "test has more instructions"),
        }
    }
}

/// A divergence between reference and test execution.
#[derive(Debug, Clone)]
pub struct Divergence {
    /// Instruction index where divergence occurred.
    pub index: usize,
    /// Expected state (from reference).
    pub expected: DiffState,
    /// Actual state (from test).
    pub actual: DiffState,
    /// Kind of divergence.
    pub kind: DivergenceKind,
}

/// Configuration for comparison.
#[derive(Debug, Clone)]
pub struct CompareConfig {
    /// Require exact register write matching.
    pub strict_reg_writes: bool,
    /// Require exact memory access matching.
    pub strict_mem_access: bool,
}

impl Default for CompareConfig {
    fn default() -> Self {
        Self {
            strict_reg_writes: true,
            strict_mem_access: false, // Spike doesn't always log mem for loads
        }
    }
}

/// Compare two states and return the kind of divergence if any.
///
/// Comparison rules:
/// - Always ignore x0 writes (both sides).
/// - PC and opcode must match exactly.
/// - Register writes: if either has a write (non-x0), compare.
/// - Memory: only compare if strict_mem_access is true.
pub fn compare_states(
    expected: &DiffState,
    actual: &DiffState,
    config: &CompareConfig,
) -> Option<DivergenceKind> {
    // PC must match
    if expected.pc != actual.pc {
        return Some(DivergenceKind::Pc);
    }

    // Opcode must match (allows detecting decode issues)
    if expected.opcode != 0 && actual.opcode != 0 && expected.opcode != actual.opcode {
        return Some(DivergenceKind::Opcode);
    }

    // Register write comparison
    if config.strict_reg_writes {
        match (expected.rd, actual.rd) {
            (Some(e_rd), Some(a_rd)) => {
                if e_rd != a_rd {
                    return Some(DivergenceKind::RegDest);
                }
                // Same register, compare values
                if let (Some(e_val), Some(a_val)) = (expected.rd_value, actual.rd_value)
                    && e_val != a_val
                {
                    return Some(DivergenceKind::RegValue);
                }
            }
            (Some(_), None) => return Some(DivergenceKind::MissingRegWrite),
            (None, Some(_)) => return Some(DivergenceKind::ExtraRegWrite),
            (None, None) => {}
        }
    }

    // Memory access comparison
    if config.strict_mem_access {
        match (expected.mem_addr, actual.mem_addr) {
            (Some(e_addr), Some(a_addr)) => {
                if e_addr != a_addr {
                    return Some(DivergenceKind::MemAddr);
                }
                // Same address, compare values if available
                if let (Some(e_val), Some(a_val)) = (expected.mem_value, actual.mem_value)
                    && e_val != a_val
                {
                    return Some(DivergenceKind::MemValue);
                }
            }
            (Some(_), None) => return Some(DivergenceKind::MissingMemAccess),
            (None, Some(_)) => return Some(DivergenceKind::ExtraMemAccess),
            (None, None) => {}
        }
    }

    None
}

/// Result of a differential comparison run.
#[derive(Debug)]
pub struct CompareResult {
    /// Number of instructions that matched.
    pub matched: usize,
    /// First divergence (if any).
    pub divergence: Option<Divergence>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_matching_states() {
        let config = CompareConfig::default();
        let s1 = DiffState {
            pc: 0x1000,
            opcode: 0x00000013, // NOP
            rd: Some(1),
            rd_value: Some(42),
            ..Default::default()
        };
        let s2 = s1.clone();
        assert!(compare_states(&s1, &s2, &config).is_none());
    }

    #[test]
    fn test_compare_pc_mismatch() {
        let config = CompareConfig::default();
        let s1 = DiffState {
            pc: 0x1000,
            ..Default::default()
        };
        let s2 = DiffState {
            pc: 0x1004,
            ..Default::default()
        };
        assert_eq!(compare_states(&s1, &s2, &config), Some(DivergenceKind::Pc));
    }

    #[test]
    fn test_compare_reg_value_mismatch() {
        let config = CompareConfig::default();
        let s1 = DiffState {
            pc: 0x1000,
            rd: Some(5),
            rd_value: Some(100),
            ..Default::default()
        };
        let s2 = DiffState {
            pc: 0x1000,
            rd: Some(5),
            rd_value: Some(200),
            ..Default::default()
        };
        assert_eq!(
            compare_states(&s1, &s2, &config),
            Some(DivergenceKind::RegValue)
        );
    }

    #[test]
    fn test_compare_missing_reg_write() {
        let config = CompareConfig::default();
        let s1 = DiffState {
            pc: 0x1000,
            rd: Some(5),
            rd_value: Some(100),
            ..Default::default()
        };
        let s2 = DiffState {
            pc: 0x1000,
            ..Default::default()
        };
        assert_eq!(
            compare_states(&s1, &s2, &config),
            Some(DivergenceKind::MissingRegWrite)
        );
    }

    #[test]
    fn test_compare_ignores_x0() {
        let config = CompareConfig::default();
        // x0 writes should be filtered before reaching compare
        let s1 = DiffState {
            pc: 0x1000,
            rd: None, // x0 filtered to None
            ..Default::default()
        };
        let s2 = DiffState {
            pc: 0x1000,
            rd: None,
            ..Default::default()
        };
        assert!(compare_states(&s1, &s2, &config).is_none());
    }

    #[test]
    fn test_compare_mem_lenient() {
        let config = CompareConfig {
            strict_mem_access: false,
            ..Default::default()
        };
        let s1 = DiffState {
            pc: 0x1000,
            mem_addr: Some(0x2000),
            ..Default::default()
        };
        let s2 = DiffState {
            pc: 0x1000,
            mem_addr: None, // Spike didn't log it
            ..Default::default()
        };
        // Lenient mode should not fail
        assert!(compare_states(&s1, &s2, &config).is_none());
    }

    #[test]
    fn test_compare_mem_strict() {
        let config = CompareConfig {
            strict_mem_access: true,
            ..Default::default()
        };
        let s1 = DiffState {
            pc: 0x1000,
            mem_addr: Some(0x2000),
            ..Default::default()
        };
        let s2 = DiffState {
            pc: 0x1000,
            mem_addr: None,
            ..Default::default()
        };
        assert_eq!(
            compare_states(&s1, &s2, &config),
            Some(DivergenceKind::MissingMemAccess)
        );
    }
}
