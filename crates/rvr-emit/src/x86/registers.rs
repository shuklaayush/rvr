//! x86-64 register mapping for RISC-V emulation.
//!
//! Maps RISC-V registers to x86-64 registers based on EmitConfig::hot_regs.
//! Hot registers are kept in x86 GPRs, cold registers are accessed via memory.

/// x86_64 assembly backend: 8 GPRs available for hot registers.
///
/// Reserved: rbx (state ptr), r15 (memory ptr), rsp (stack), rax/rcx/rdx (temps)
/// Additional reserved: r10 (instret cache), r11 (cold-reg cache)
/// Available: r14, r13, r12, rbp, rdi, rsi, r9, r8
pub const HOT_REG_SLOTS: usize = 8;

/// Reserved x86 registers (not available for RISC-V register mapping).
/// - rbx: RvState pointer (callee-saved)
/// - r15: Memory base pointer (callee-saved)
/// - rsp: Stack pointer
/// - rax, rcx, rdx: Temporaries for complex operations (mul/div/shifts)
pub mod reserved {
    pub const STATE_PTR: &str = "rbx";
    pub const MEMORY_PTR: &str = "r15";
    pub const INSTRET: &str = "r10";
    pub const COLD_CACHE: &str = "r11";
}

/// Available x86 registers for hot RISC-V register mapping.
/// Order matters - callee-saved first (fewer save/restore), then caller-saved.
/// With 10 registers available, we can map most frequently-used RISC-V registers.
pub const AVAILABLE_REGS: [&str; 8] = [
    "r14", // callee-saved
    "r13", // callee-saved
    "r12", // callee-saved
    "rbp", // callee-saved (we save/restore in prologue)
    "rdi", // caller-saved, free after prologue (was arg0)
    "rsi", // caller-saved, free after prologue (was arg1)
    "r9",  // caller-saved
    "r8",  // caller-saved
];

/// 32-bit versions of available registers.
pub const AVAILABLE_REGS_32: [&str; 8] = [
    "r14d", "r13d", "r12d", "ebp", "edi", "esi", "r9d", "r8d",
];

/// Register mapping from RISC-V to x86.
#[derive(Clone, Debug)]
pub struct RegMap {
    /// RISC-V register -> x86 register name (or None if in memory).
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

    /// Get x86 register for RISC-V register (or None if in memory).
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

    /// Get 64-bit version of x86 register for RISC-V register.
    pub fn get_64(&self, rv_reg: u8) -> Option<&'static str> {
        if rv_reg == 0 {
            return None;
        }
        self.mapping.get(rv_reg as usize).copied().flatten()
    }

    /// Check if RISC-V register is mapped to an x86 register.
    pub fn is_hot(&self, rv_reg: u8) -> bool {
        rv_reg != 0
            && self
                .mapping
                .get(rv_reg as usize)
                .copied()
                .flatten()
                .is_some()
    }

    /// Iterator over (rv_reg, x86_reg) pairs for hot registers.
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

    /// Iterator over (rv_reg, x86_reg_64) pairs for hot registers.
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
        assert_eq!(map.get(1), Some("r14")); // ra -> r14
        assert_eq!(map.get(2), Some("r13")); // sp -> r13
        assert_eq!(map.get(10), Some("r12")); // a0 -> r12
        assert_eq!(map.get(11), Some("rbp")); // a1 -> rbp
        assert!(map.get(12).is_none()); // a2 not hot
    }

    #[test]
    fn test_reg_map_rv32() {
        let hot = vec![1, 2, 10];
        let map = RegMap::new(&hot, true);

        assert_eq!(map.get(1), Some("r14d"));
        assert_eq!(map.get(2), Some("r13d"));
        assert_eq!(map.get(10), Some("r12d"));
    }

    #[test]
    fn test_hot_regs_iterator_rv32() {
        let hot = vec![1, 10];
        let map = RegMap::new(&hot, true);

        let hot_regs: Vec<_> = map.hot_regs().collect();
        // Should return 32-bit register names for RV32
        assert_eq!(hot_regs, vec![(1, "r14d"), (10, "r13d")]);

        // hot_regs_64 should still return 64-bit names
        let hot_regs_64: Vec<_> = map.hot_regs_64().collect();
        assert_eq!(hot_regs_64, vec![(1, "r14"), (10, "r13")]);
    }

    #[test]
    fn test_hot_regs_iterator_rv64() {
        let hot = vec![1, 10];
        let map = RegMap::new(&hot, false);

        let hot_regs: Vec<_> = map.hot_regs().collect();
        // Should return 64-bit register names for RV64
        assert_eq!(hot_regs, vec![(1, "r14"), (10, "r13")]);
    }

    #[test]
    fn test_max_hot_regs() {
        // Test all 8 available registers
        let hot: Vec<u8> = (1..=8).collect(); // regs 1-8
        let map = RegMap::new(&hot, false);

        assert_eq!(map.get(1), Some("r14"));
        assert_eq!(map.get(2), Some("r13"));
        assert_eq!(map.get(3), Some("r12"));
        assert_eq!(map.get(4), Some("rbp"));
        assert_eq!(map.get(5), Some("rdi")); // New: was not available before
        assert_eq!(map.get(6), Some("rsi")); // New: was not available before
        assert_eq!(map.get(7), Some("r9"));
        assert_eq!(map.get(8), Some("r8"));

        // 11th register should not be mapped (only 10 available)
        let hot11: Vec<u8> = (1..=11).collect();
        let map11 = RegMap::new(&hot11, false);
        assert!(map11.get(11).is_none());
    }
}
