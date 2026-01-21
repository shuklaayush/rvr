//! Emit configuration.
//!
//! Code generation configuration including hot register selection,
//! instret handling, and platform-specific defaults.

use std::marker::PhantomData;
use std::str::FromStr;

use rvr_ir::Xlen;

use crate::tracer::TracerConfig;

/// Number of registers for I extension.
pub const NUM_REGS_I: usize = 32;
/// Number of registers for E extension.
pub const NUM_REGS_E: usize = 16;

/// C compiler to use for generated code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Compiler {
    /// Clang (default) - supports C23, preserve_none, musttail, thin LTO.
    #[default]
    Clang,
    /// GCC - uses C2x, standard LTO.
    Gcc,
}

impl Compiler {
    /// Command name for this compiler.
    pub fn command(&self) -> &'static str {
        match self {
            Self::Clang => "clang",
            Self::Gcc => "gcc",
        }
    }

    /// Check if this compiler is clang.
    pub fn is_clang(&self) -> bool {
        matches!(self, Self::Clang)
    }
}

impl FromStr for Compiler {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "clang" => Ok(Self::Clang),
            "gcc" => Ok(Self::Gcc),
            _ => Err(format!(
                "unknown compiler '{}', expected 'clang' or 'gcc'",
                s
            )),
        }
    }
}

impl std::fmt::Display for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command())
    }
}

/// Register priority order for hot register selection.
/// Higher priority registers are chosen first when slots are limited.
/// x0 (zero) is excluded since it's always 0.
pub const REG_PRIORITY: [u8; 31] = [
    // Highest priority - used constantly
    1, // ra
    2, // sp
    // Function arguments (a0-a7)
    10, 11, 12, 13, 14, 15, 16, 17, // a0-a7
    // Temporaries (t0-t2)
    5, 6, 7, // t0-t2
    // Temporaries (t3-t6)
    28, 29, 30, 31, // t3-t6
    // Saved registers (s0-s1)
    8, 9, // s0-s1
    // Saved registers (s2-s11)
    18, 19, 20, 21, 22, 23, 24, 25, 26, 27, // s2-s11
    // Lowest priority - rarely used
    3, // gp
    4, // tp
];

/// x86_64: 10 slots = 8 hot regs (optimal based on benchmarking).
/// More causes register exhaustion, fewer increases memory access overhead.
pub const X86_64_DEFAULT_TOTAL_SLOTS: usize = 10;

/// AArch64: 31 GPRs (x0-x30), minus SP and ~7 for compiler temps = ~23 usable.
pub const AARCH64_DEFAULT_TOTAL_SLOTS: usize = 23;

/// Fixed slots when instret counting is enabled (state + instret).
pub const FIXED_SLOTS_WITH_INSTRET: usize = 2;

/// Fixed slots when instret counting is disabled (state only).
pub const FIXED_SLOTS_NO_INSTRET: usize = 1;

/// Get platform-specific default total slots.
pub fn default_total_slots() -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        X86_64_DEFAULT_TOTAL_SLOTS
    }
    #[cfg(target_arch = "aarch64")]
    {
        AARCH64_DEFAULT_TOTAL_SLOTS
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        X86_64_DEFAULT_TOTAL_SLOTS
    }
}

/// Compute number of hot RISC-V registers from total argument slots.
pub fn compute_num_hot_regs(
    total_slots: usize,
    instret_mode: InstretMode,
    tracer_config: &TracerConfig,
) -> usize {
    let fixed = if instret_mode.counts() {
        FIXED_SLOTS_WITH_INSTRET
    } else {
        FIXED_SLOTS_NO_INSTRET
    };
    // Tracer vars that pass to block functions take up argument slots
    let extra = tracer_config.passed_vars.len();
    total_slots.saturating_sub(fixed + extra)
}

/// Instruction retirement counting mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InstretMode {
    /// No instruction counting.
    Off,
    /// Count instructions but don't suspend.
    #[default]
    Count,
    /// Count instructions and suspend at limit.
    Suspend,
}

impl InstretMode {
    pub fn counts(&self) -> bool {
        *self != Self::Off
    }

    pub fn suspends(&self) -> bool {
        *self == Self::Suspend
    }
}

/// Syscall handling mode for ECALL instructions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyscallMode {
    /// Bare-metal syscalls (exit only).
    #[default]
    BareMetal,
    /// Linux-style syscalls (brk/mmap/read/write, etc).
    Linux,
}

/// Code generation configuration.
#[derive(Clone, Debug)]
pub struct EmitConfig<X: Xlen> {
    /// Number of registers: 32 for I extension, 16 for E extension.
    pub num_regs: usize,
    /// Registers passed as arguments (hot registers).
    pub hot_regs: Vec<u8>,
    /// Enable address bounds checking.
    pub addr_check: bool,
    /// Instruction retirement mode.
    pub instret_mode: InstretMode,
    /// Emit comments in generated C code.
    pub emit_comments: bool,
    /// Emit #line directives for source-level debugging.
    pub emit_line_info: bool,
    /// Enable tohost check (for riscv-tests).
    pub tohost_enabled: bool,
    /// Memory address bits (default 32).
    pub memory_bits: u8,
    /// Tracer configuration.
    pub tracer_config: TracerConfig,
    /// C compiler to use.
    pub compiler: Compiler,
    /// Syscall handling mode.
    pub syscall_mode: SyscallMode,
    _marker: PhantomData<X>,
}

impl<X: Xlen> Default for EmitConfig<X> {
    fn default() -> Self {
        Self::standard()
    }
}

impl<X: Xlen> EmitConfig<X> {
    /// Create base config without hot registers (internal use).
    fn base(num_regs: usize) -> Self {
        Self {
            num_regs,
            hot_regs: Vec::new(),
            addr_check: false,
            instret_mode: InstretMode::Count,
            emit_comments: true,
            emit_line_info: false,
            tohost_enabled: false,
            memory_bits: 32,
            tracer_config: TracerConfig::none(),
            compiler: Compiler::default(),
            syscall_mode: SyscallMode::default(),
            _marker: PhantomData,
        }
    }

    /// Create config with specified register count and platform-optimized hot registers.
    pub fn new(num_regs: usize) -> Self {
        assert!(num_regs == NUM_REGS_I || num_regs == NUM_REGS_E);
        let mut config = Self::base(num_regs);
        config.init_hot_regs(default_total_slots());
        config
    }

    /// Create config with platform-optimized defaults.
    ///
    /// This initializes hot registers based on platform-specific total slots
    /// and the given tracer configuration.
    pub fn with_defaults(num_regs: usize, total_slots: usize, tracer_config: TracerConfig) -> Self {
        let mut config = Self::base(num_regs);
        config.tracer_config = tracer_config;
        config.init_hot_regs(total_slots);
        config
    }

    /// Create config with standard platform defaults.
    pub fn standard() -> Self {
        Self::with_defaults(NUM_REGS_I, default_total_slots(), TracerConfig::none())
    }

    /// Initialize hot register list from total_slots.
    ///
    /// Only includes registers that exist (< num_regs) for E extension support.
    pub fn init_hot_regs(&mut self, total_slots: usize) {
        let num_hot_regs =
            compute_num_hot_regs(total_slots, self.instret_mode, &self.tracer_config);
        self.hot_regs.clear();

        let mut count = 0;
        for &reg in &REG_PRIORITY {
            if count >= num_hot_regs {
                break;
            }
            // Skip registers that don't exist in E extension
            if (reg as usize) < self.num_regs {
                self.hot_regs.push(reg);
                count += 1;
            }
        }
    }

    /// Check if register index is valid.
    pub fn is_valid_reg(&self, reg: u8) -> bool {
        (reg as usize) < self.num_regs
    }

    /// Check if register is in hot list.
    pub fn is_hot_reg(&self, reg: u8) -> bool {
        reg != 0 && self.hot_regs.contains(&reg)
    }

    /// Number of hot registers.
    pub fn num_hot_regs(&self) -> usize {
        self.hot_regs.len()
    }

    /// Check if tracing is enabled.
    pub fn has_tracing(&self) -> bool {
        !self.tracer_config.is_none()
    }

    /// Set address checking.
    pub fn with_addr_check(mut self, enabled: bool) -> Self {
        self.addr_check = enabled;
        self
    }

    /// Set tracer configuration.
    pub fn with_tracer(mut self, config: TracerConfig) -> Self {
        self.tracer_config = config;
        self
    }

    /// Set instret mode.
    pub fn with_instret_mode(mut self, mode: InstretMode) -> Self {
        self.instret_mode = mode;
        self
    }

    /// Set tohost enabled.
    pub fn with_tohost(mut self, enabled: bool) -> Self {
        self.tohost_enabled = enabled;
        self
    }

    /// Set C compiler.
    pub fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.compiler = compiler;
        self
    }

    /// Set emit_line_info (for #line directives).
    pub fn with_line_info(mut self, enabled: bool) -> Self {
        self.emit_line_info = enabled;
        self
    }

    /// Set syscall mode.
    pub fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.syscall_mode = mode;
        self
    }

    /// Bytes per register based on XLEN.
    pub fn reg_bytes(&self) -> usize {
        X::REG_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_default_config() {
        let config = EmitConfig::<Rv64>::default();
        assert_eq!(config.num_regs, 32);
        // default() now initializes hot registers (same as standard())
        assert!(!config.hot_regs.is_empty());
        assert!(config.instret_mode.counts());
    }

    #[test]
    fn test_standard_config() {
        let config = EmitConfig::<Rv64>::standard();
        assert_eq!(config.num_regs, 32);
        assert!(!config.hot_regs.is_empty());
    }

    #[test]
    fn test_hot_regs_init() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.instret_mode = InstretMode::Count;
        config.init_hot_regs(10);
        // 10 slots - 2 (state + instret) = 8 hot regs
        assert_eq!(config.hot_regs.len(), 8);
        // First should be ra (1)
        assert_eq!(config.hot_regs[0], 1);
        // Second should be sp (2)
        assert_eq!(config.hot_regs[1], 2);
    }

    #[test]
    fn test_hot_regs_no_instret() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.instret_mode = InstretMode::Off;
        config.init_hot_regs(10);
        // 10 slots - 1 (state only) = 9 hot regs
        assert_eq!(config.hot_regs.len(), 9);
    }

    #[test]
    fn test_is_hot_reg() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.hot_regs = vec![1, 2, 10];
        assert!(config.is_hot_reg(1));
        assert!(config.is_hot_reg(2));
        assert!(config.is_hot_reg(10));
        assert!(!config.is_hot_reg(0)); // x0 is never hot
        assert!(!config.is_hot_reg(3));
    }

    #[test]
    fn test_compute_num_hot_regs() {
        let tracer = TracerConfig::none();
        assert_eq!(compute_num_hot_regs(10, InstretMode::Count, &tracer), 8);
        assert_eq!(compute_num_hot_regs(10, InstretMode::Off, &tracer), 9);
        assert_eq!(compute_num_hot_regs(10, InstretMode::Suspend, &tracer), 8);
    }
}
