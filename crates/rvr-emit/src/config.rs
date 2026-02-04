//! Emit configuration.
//!
//! Code generation configuration including hot register selection,
//! instret handling, and platform-specific defaults.

use std::marker::PhantomData;

use rvr_ir::Xlen;

use crate::arm64;
use crate::c::{TracerConfig, config as c_config};
use crate::x86;

// Import Compiler for convenience (used in EmitConfig)
pub use c_config::Compiler;

/// Number of registers for I extension.
pub const NUM_REGS_I: usize = 32;
/// Number of registers for E extension.
pub const NUM_REGS_E: usize = 16;

/// Get platform-specific default total slots for a given backend.
#[must_use]
pub const fn default_total_slots_for_backend(backend: Backend) -> usize {
    match backend {
        Backend::C => c_config::default_total_slots(),
        Backend::X86Asm => x86::HOT_REG_SLOTS,
        Backend::ARM64Asm => arm64::HOT_REG_SLOTS,
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

/// Instruction retirement counting mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InstretMode {
    /// No instruction counting.
    Off,
    /// Count instructions but don't suspend.
    #[default]
    Count,
    /// Count instructions and suspend at limit (checked at block boundaries).
    Suspend,
    /// Count instructions and suspend at limit (checked after every instruction).
    PerInstruction,
}

impl InstretMode {
    #[must_use]
    pub fn counts(&self) -> bool {
        *self != Self::Off
    }

    #[must_use]
    pub const fn suspends(&self) -> bool {
        matches!(self, Self::Suspend | Self::PerInstruction)
    }

    /// True if suspension check is emitted after every instruction.
    #[must_use]
    pub fn per_instruction(&self) -> bool {
        *self == Self::PerInstruction
    }

    /// Convert to C constant value for `RV_INSTRET_MODE` export.
    #[must_use]
    pub const fn as_c_mode(&self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Count => 1,
            Self::Suspend => 2,
            Self::PerInstruction => 3,
        }
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

/// Address translation mode for memory accesses.
///
/// Controls how guest virtual addresses are translated to physical addresses
/// in the emulator's memory buffer.
///
/// # Address Translation Semantics
///
/// | Mode      | Mask Address | Bounds Check | Trap on OOB |
/// |-----------|--------------|--------------|-------------|
/// | Unchecked | No           | No           | No (guards) |
/// | Wrap      | Yes          | No           | No          |
/// | Bounds    | Yes          | Yes          | Yes         |
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddressMode {
    /// Assume valid + passthrough. Guard pages catch OOB at runtime.
    Unchecked,
    /// Mask addresses to memory size (addresses wrap at boundary).
    /// Matches RISC-V sv39/sv48 address translation behavior.
    #[default]
    Wrap,
    /// Bounds check + trap + mask. Explicit trap on invalid addresses.
    Bounds,
}

impl AddressMode {
    /// Whether addresses should be masked to memory size.
    ///
    /// True for Wrap and Bounds modes. C emitters use `& MASK`, x86 uses `and`.
    #[must_use]
    pub const fn needs_mask(self) -> bool {
        matches!(self, Self::Wrap | Self::Bounds)
    }

    /// Whether addresses should be bounds-checked before access.
    ///
    /// True for Bounds mode only. C emitters use `if (out_of_bounds) trap()`,
    /// x86 uses `cmp; jbe ok; jmp trap; ok:`.
    #[must_use]
    pub fn needs_bounds_check(self) -> bool {
        self == Self::Bounds
    }

    /// Whether addresses are assumed valid (for optimizer hints).
    ///
    /// True for Unchecked mode. C emitters use `__builtin_assume()`.
    #[must_use]
    pub fn assumes_valid(self) -> bool {
        self == Self::Unchecked
    }
}

/// Code generation backend.
///
/// Controls the output format of the recompiler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    /// Emit C code, compile with clang/gcc.
    #[default]
    C,
    /// Emit x86-64 assembly, compile with gcc/as.
    X86Asm,
    /// Emit ARM64 assembly, compile with gcc/as.
    ARM64Asm,
}

/// Analysis mode for the compilation pipeline.
///
/// Controls how much CFG analysis is performed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnalysisMode {
    /// Full CFG analysis: block merging, absorption, optimizations.
    /// Best for C backend where LLVM benefits from larger functions.
    #[default]
    FullCfg,
    /// Basic mode: decode instructions, mark jump targets, no block merging.
    /// Faster compilation, sufficient for x86 backend.
    Basic,
}

/// Fixed address configuration for state and memory.
///
/// When enabled, state and memory are accessed via compile-time constant addresses
/// instead of being passed as function arguments. This frees up argument registers
/// for hot values but requires the runtime to map memory at these exact addresses.
///
/// Default addresses are chosen to minimize collision with typical ASLR mappings:
/// - Above 4GB mark (avoid 32-bit conflicts)
/// - Below typical mmap regions (~0x7f... on Linux)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedAddressConfig {
    /// Fixed address for `RvState` struct.
    pub state_addr: u64,
    /// Fixed address for guest memory base.
    pub memory_addr: u64,
}

impl Default for FixedAddressConfig {
    fn default() -> Self {
        Self {
            state_addr: 0x10_0000_0000,  // 64 GB
            memory_addr: 0x20_0000_0000, // 128 GB
        }
    }
}

/// Codegen feature flags for emitters.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmitFlags(u32);

impl EmitFlags {
    const EMIT_COMMENTS: u32 = 1 << 0;
    const EMIT_LINE_INFO: u32 = 1 << 1;
    const HTIF_ENABLED: u32 = 1 << 2;
    const HTIF_VERBOSE: u32 = 1 << 3;

    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    const fn contains(self, mask: u32) -> bool {
        (self.0 & mask) != 0
    }

    const fn set(&mut self, mask: u32, enabled: bool) {
        if enabled {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }

    #[must_use]
    pub const fn emit_comments(self) -> bool {
        self.contains(Self::EMIT_COMMENTS)
    }

    pub const fn set_emit_comments(&mut self, enabled: bool) {
        self.set(Self::EMIT_COMMENTS, enabled);
    }

    #[must_use]
    pub const fn emit_line_info(self) -> bool {
        self.contains(Self::EMIT_LINE_INFO)
    }

    pub const fn set_emit_line_info(&mut self, enabled: bool) {
        self.set(Self::EMIT_LINE_INFO, enabled);
    }

    #[must_use]
    pub const fn htif_enabled(self) -> bool {
        self.contains(Self::HTIF_ENABLED)
    }

    pub const fn set_htif_enabled(&mut self, enabled: bool) {
        self.set(Self::HTIF_ENABLED, enabled);
    }

    #[must_use]
    pub const fn htif_verbose(self) -> bool {
        self.contains(Self::HTIF_VERBOSE)
    }

    pub const fn set_htif_verbose(&mut self, enabled: bool) {
        self.set(Self::HTIF_VERBOSE, enabled);
    }
}

/// Code generation configuration.
#[derive(Clone, Debug)]
pub struct EmitConfig<X: Xlen> {
    /// Number of registers: 32 for I extension, 16 for E extension.
    pub num_regs: usize,
    /// Registers passed as arguments (hot registers).
    pub hot_regs: Vec<u8>,
    /// Code generation backend (C or x86 assembly).
    pub backend: Backend,
    /// Analysis mode (full CFG or linear scan).
    pub analysis_mode: AnalysisMode,
    /// Address translation mode.
    pub address_mode: AddressMode,
    /// Instruction retirement mode.
    pub instret_mode: InstretMode,
    /// Code generation feature flags.
    pub flags: EmitFlags,
    /// Memory address bits (default 32).
    pub memory_bits: u8,
    /// Tracer configuration.
    pub tracer_config: TracerConfig,
    /// C compiler to use.
    pub compiler: Compiler,
    /// Syscall handling mode.
    pub syscall_mode: SyscallMode,
    /// Export functions mode: compiled for calling exported functions rather than running from entry point.
    pub export_functions: bool,
    /// Fixed addresses for state and memory (optional).
    /// When set, state/memory are not passed as arguments but accessed via constant addresses.
    pub fixed_addresses: Option<FixedAddressConfig>,
    /// Perf mode: disable instret/CSR reads for benchmarking.
    pub perf_mode: bool,
    /// Enable superblock formation (merging fall-through blocks after branches).
    /// Disable for differential testing to ensure dispatch works at all block boundaries.
    pub enable_superblock: bool,
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
        let mut flags = EmitFlags::empty();
        flags.set_emit_comments(true);
        flags.set_emit_line_info(true);
        flags.set_htif_enabled(false);
        flags.set_htif_verbose(false);

        Self {
            num_regs,
            hot_regs: Vec::new(),
            backend: Backend::default(),
            analysis_mode: AnalysisMode::default(),
            address_mode: AddressMode::default(),
            instret_mode: InstretMode::Count,
            flags,
            memory_bits: 32,
            tracer_config: TracerConfig::none(),
            compiler: Compiler::default(),
            syscall_mode: SyscallMode::default(),
            export_functions: false,
            fixed_addresses: None,
            perf_mode: false,
            enable_superblock: true, // Enabled by default for performance
            _marker: PhantomData,
        }
    }

    /// Create config with specified register count and platform-optimized hot registers.
    ///
    /// # Panics
    ///
    /// Panics if `num_regs` is not a supported register count.
    #[must_use]
    pub fn new(num_regs: usize) -> Self {
        assert!(num_regs == NUM_REGS_I || num_regs == NUM_REGS_E);
        let mut config = Self::base(num_regs);
        config.init_hot_regs(c_config::default_total_slots());
        config
    }

    /// Create config with platform-optimized defaults.
    ///
    /// This initializes hot registers based on platform-specific total slots
    /// and the given tracer configuration.
    #[must_use]
    pub fn with_defaults(num_regs: usize, total_slots: usize, tracer_config: TracerConfig) -> Self {
        let mut config = Self::base(num_regs);
        config.tracer_config = tracer_config;
        config.init_hot_regs(total_slots);
        config
    }

    /// Create config with standard platform defaults.
    #[must_use]
    pub fn standard() -> Self {
        Self::with_defaults(
            NUM_REGS_I,
            c_config::default_total_slots(),
            TracerConfig::none(),
        )
    }

    /// Initialize hot register list with the specified number of hot registers.
    ///
    /// Only includes registers that exist (< `num_regs`) for E extension support.
    fn init_hot_regs_count(&mut self, num_hot_regs: usize) {
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

    /// Initialize hot register list from total argument slots (C backend).
    ///
    /// For C backend: subtracts fixed slots (state, memory, instret) from total.
    /// Only includes registers that exist (< `num_regs`) for E extension support.
    pub fn init_hot_regs(&mut self, total_slots: usize) {
        let num_hot_regs = c_config::compute_num_hot_regs(
            total_slots,
            self.instret_mode,
            &self.tracer_config,
            self.fixed_addresses.is_some(),
        );
        self.init_hot_regs_count(num_hot_regs);
    }

    /// Re-initialize hot registers based on the current backend.
    ///
    /// For C backend: uses platform-specific argument slots minus fixed slots.
    /// For x86/ARM64 backends: uses all available GPRs (state/memory use dedicated regs).
    pub fn reinit_hot_regs_for_backend(&mut self) {
        match self.backend {
            Backend::C => {
                let total_slots = c_config::default_total_slots();
                self.init_hot_regs(total_slots);
            }
            Backend::X86Asm => {
                // x86 uses dedicated registers for state (rbx) and memory (r15),
                // so all hot reg slots are available for RISC-V registers
                self.init_hot_regs_count(x86::HOT_REG_SLOTS);
            }
            Backend::ARM64Asm => {
                // ARM64 uses dedicated registers for state (x19) and memory (x20),
                // so all hot reg slots are available for RISC-V registers
                self.init_hot_regs_count(arm64::HOT_REG_SLOTS);
            }
        }
    }

    /// Check if register index is valid.
    #[must_use]
    pub const fn is_valid_reg(&self, reg: u8) -> bool {
        (reg as usize) < self.num_regs
    }

    /// Check if register is in hot list.
    #[must_use]
    pub fn is_hot_reg(&self, reg: u8) -> bool {
        reg != 0 && self.hot_regs.contains(&reg)
    }

    /// Number of hot registers.
    #[must_use]
    pub const fn num_hot_regs(&self) -> usize {
        self.hot_regs.len()
    }

    /// Check if tracing is enabled.
    #[must_use]
    pub const fn has_tracing(&self) -> bool {
        !self.tracer_config.is_none()
    }

    /// Check if emit comments is enabled.
    #[must_use]
    pub const fn emit_comments(&self) -> bool {
        self.flags.emit_comments()
    }

    /// Check if emit line info is enabled.
    #[must_use]
    pub const fn emit_line_info(&self) -> bool {
        self.flags.emit_line_info()
    }

    /// Check if HTIF is enabled.
    #[must_use]
    pub const fn htif_enabled(&self) -> bool {
        self.flags.htif_enabled()
    }

    /// Check if HTIF verbose is enabled.
    #[must_use]
    pub const fn htif_verbose(&self) -> bool {
        self.flags.htif_verbose()
    }

    /// Set address translation mode.
    #[must_use]
    pub const fn with_address_mode(mut self, mode: AddressMode) -> Self {
        self.address_mode = mode;
        self
    }

    /// Set tracer configuration.
    #[must_use]
    pub fn with_tracer(mut self, config: TracerConfig) -> Self {
        self.tracer_config = config;
        self
    }

    /// Set instret mode.
    #[must_use]
    pub const fn with_instret_mode(mut self, mode: InstretMode) -> Self {
        self.instret_mode = mode;
        self
    }

    /// Set tohost enabled.
    #[must_use]
    pub const fn with_tohost(mut self, enabled: bool) -> Self {
        self.flags.set_htif_enabled(enabled);
        self
    }

    /// Set HTIF verbose (print guest stdout).
    #[must_use]
    pub const fn with_htif_verbose(mut self, verbose: bool) -> Self {
        self.flags.set_htif_verbose(verbose);
        self
    }

    /// Set C compiler.
    #[must_use]
    pub fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.compiler = compiler;
        self
    }

    /// Set `emit_line_info` (for #line directives).
    #[must_use]
    pub const fn with_line_info(mut self, enabled: bool) -> Self {
        self.flags.set_emit_line_info(enabled);
        self
    }

    /// Set syscall mode.
    #[must_use]
    pub const fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.syscall_mode = mode;
        self
    }

    /// Set fixed addresses for state and memory.
    ///
    /// When enabled, state/memory are accessed via compile-time constant addresses
    /// instead of function arguments. Requires runtime to map at these addresses.
    #[must_use]
    pub fn with_fixed_addresses(mut self, config: FixedAddressConfig) -> Self {
        self.fixed_addresses = Some(config);
        // Re-compute hot registers since fixed_addresses affects the calculation
        self.init_hot_regs(c_config::default_total_slots());
        self
    }

    /// Enable perf mode (disables instret and CSR reads).
    #[must_use]
    pub const fn with_perf_mode(mut self, enabled: bool) -> Self {
        self.perf_mode = enabled;
        if enabled {
            self.instret_mode = InstretMode::Off;
        }
        self
    }

    /// Enable or disable superblock formation.
    ///
    /// Superblocks merge fall-through blocks after branches for better performance,
    /// but prevent dispatch to mid-block addresses. Disable for differential testing.
    #[must_use]
    pub const fn with_superblock(mut self, enabled: bool) -> Self {
        self.enable_superblock = enabled;
        self
    }

    /// Check if fixed addresses are enabled.
    #[must_use]
    pub const fn has_fixed_addresses(&self) -> bool {
        self.fixed_addresses.is_some()
    }

    /// Bytes per register based on XLEN.
    #[must_use]
    pub const fn reg_bytes(&self) -> usize {
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
        // 10 slots - 3 (state + memory + instret) = 7 hot regs
        assert_eq!(config.hot_regs.len(), 7);
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
        // 10 slots - 2 (state + memory) = 8 hot regs
        assert_eq!(config.hot_regs.len(), 8);
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
}
