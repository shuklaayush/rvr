//! ARM64 register mapping for RISC-V emulation.
//!
//! Maps RISC-V registers to ARM64 registers based on EmitConfig::hot_regs.
//! Hot registers are kept in ARM64 GPRs, cold registers are accessed via memory.

/// ARM64 assembly backend: 23 GPRs available for hot registers.
///
/// Reserved:
/// - x19: state pointer (callee-saved)
/// - x20: memory pointer (callee-saved)
/// - x0-x2: temporaries for complex operations
/// - x18: instret cache (reserved)
/// - x29 (fp), x30 (lr), sp: frame/link/stack
///
/// Available: x3-x17 (15) + x21-x28 (8) = 23 registers
pub const HOT_REG_SLOTS: usize = 23;

/// Reserved ARM64 registers (not available for RISC-V register mapping).
pub mod reserved {
    /// RvState pointer (callee-saved)
    pub const STATE_PTR: &str = "x19";
    /// Memory base pointer (callee-saved)
    pub const MEMORY_PTR: &str = "x20";
    /// Instret cache register (reserved)
    pub const INSTRET: &str = "x18";
}

/// Available ARM64 registers for hot RISC-V register mapping.
/// Callee-saved first (x21-x28), then caller-saved (x3-x17).
/// Since we're the entry point, we save all in prologue anyway.
pub const AVAILABLE_REGS: [&str; 23] = [
    // Callee-saved (fewer instructions if we call out)
    "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28",
    // Caller-saved (we save in prologue, free to use)
    "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16",
    "x17",
];

/// 32-bit versions of available registers (w-registers for RV32).
pub const AVAILABLE_REGS_32: [&str; 23] = [
    "w21", "w22", "w23", "w24", "w25", "w26", "w27", "w28", "w3", "w4", "w5", "w6", "w7", "w8",
    "w9", "w10", "w11", "w12", "w13", "w14", "w15", "w16", "w17",
];

/// Register mapping from RISC-V to ARM64.
#[derive(Clone, Debug)]
pub struct RegMap {
    /// RISC-V register -> ARM64 register name (or None if in memory).
    /// Index is RISC-V register number (0-31).
    mapping: [Option<&'static str>; 32],
    /// 32-bit versions for RV32.
    mapping_32: [Option<&'static str>; 32],
    /// Whether we're in RV32 mode.
    is_rv32: bool,
}

impl RegMap {
    /// Create register mapping from hot_regs list.
    pub fn new(hot_regs: &[u8], is_rv32: bool) -> Self {
        let mut mapping = [None; 32];
        let mut mapping_32 = [None; 32];

        for (i, &rv_reg) in hot_regs.iter().enumerate() {
            if i >= AVAILABLE_REGS.len() {
                break;
            }
            if rv_reg < 32 && rv_reg != 0 {
                mapping[rv_reg as usize] = Some(AVAILABLE_REGS[i]);
                mapping_32[rv_reg as usize] = Some(AVAILABLE_REGS_32[i]);
            }
        }

        Self {
            mapping,
            mapping_32,
            is_rv32,
        }
    }

    /// Get ARM64 register for RISC-V register (or None if in memory).
    pub fn get(&self, rv_reg: u8) -> Option<&'static str> {
        if rv_reg == 0 {
            return None; // x0 is always zero
        }
        if self.is_rv32 {
            self.mapping_32.get(rv_reg as usize).copied().flatten()
        } else {
            self.mapping.get(rv_reg as usize).copied().flatten()
        }
    }

    /// Get 64-bit version of ARM64 register for RISC-V register.
    pub fn get_64(&self, rv_reg: u8) -> Option<&'static str> {
        if rv_reg == 0 {
            return None;
        }
        self.mapping.get(rv_reg as usize).copied().flatten()
    }

    /// Check if RISC-V register is mapped to an ARM64 register.
    pub fn is_hot(&self, rv_reg: u8) -> bool {
        rv_reg != 0
            && self
                .mapping
                .get(rv_reg as usize)
                .copied()
                .flatten()
                .is_some()
    }

    /// Iterator over (rv_reg, arm64_reg) pairs for hot registers.
    /// Returns 32-bit register names for RV32 mode, 64-bit for RV64.
    pub fn hot_regs(&self) -> impl Iterator<Item = (u8, &'static str)> + '_ {
        let mapping = if self.is_rv32 {
            &self.mapping_32
        } else {
            &self.mapping
        };
        mapping
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| opt.map(|r| (i as u8, r)))
    }

    /// Iterator over (rv_reg, arm64_reg_64) pairs for hot registers.
    /// Always returns 64-bit register names (for address calculations).
    pub fn hot_regs_64(&self) -> impl Iterator<Item = (u8, &'static str)> + '_ {
        self.mapping
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| opt.map(|r| (i as u8, r)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reg_map_rv64() {
        let hot = vec![1, 2, 10, 11]; // ra, sp, a0, a1
        let map = RegMap::new(&hot, false);

        assert!(map.get(0).is_none()); // x0 always None
        assert_eq!(map.get(1), Some("x21")); // ra -> x21
        assert_eq!(map.get(2), Some("x22")); // sp -> x22
        assert_eq!(map.get(10), Some("x23")); // a0 -> x23
        assert_eq!(map.get(11), Some("x24")); // a1 -> x24
        assert!(map.get(12).is_none()); // a2 not hot
    }

    #[test]
    fn test_reg_map_rv32() {
        let hot = vec![1, 2, 10];
        let map = RegMap::new(&hot, true);

        assert_eq!(map.get(1), Some("w21"));
        assert_eq!(map.get(2), Some("w22"));
        assert_eq!(map.get(10), Some("w23"));
    }

    #[test]
    fn test_max_hot_regs() {
        // All 23 available registers
        let hot: Vec<u8> = (1..=23).collect();
        let map = RegMap::new(&hot, false);

        // First 8 go to callee-saved
        assert_eq!(map.get(1), Some("x21"));
        assert_eq!(map.get(8), Some("x28"));
        // Next go to caller-saved
        assert_eq!(map.get(9), Some("x3"));
        assert_eq!(map.get(23), Some("x17"));

        // 24th register should not be mapped
        let hot24: Vec<u8> = (1..=24).collect();
        let map24 = RegMap::new(&hot24, false);
        assert!(map24.get(24).is_none());
    }

    #[test]
    fn test_all_31_rv_regs() {
        // Map all 31 RISC-V registers (excluding x0)
        let hot: Vec<u8> = (1..=31).collect();
        let map = RegMap::new(&hot, false);

        // First 24 should be mapped
        assert!(map.is_hot(1));
        assert!(map.is_hot(24));
        // Registers 25-31 should be in memory (only 24 ARM64 regs available)
        assert!(!map.is_hot(25));
        assert!(!map.is_hot(31));
    }
}
