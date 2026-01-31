//! Emit configuration.
//!
//! Code generation configuration including hot register selection,
//! instret handling, and platform-specific defaults.

use std::marker::PhantomData;
use std::str::FromStr;

use rvr_ir::Xlen;

use crate::c::TracerConfig;

/// Number of registers for I extension.
pub const NUM_REGS_I: usize = 32;
/// Number of registers for E extension.
pub const NUM_REGS_E: usize = 16;

/// C compiler to use for generated code.
///
/// Accepts any compiler command (e.g., "clang", "clang-20", "gcc-13").
/// Clang vs GCC is auto-detected from the command name to determine flags:
/// - Clang: C23, thin LTO, preserve_none, musttail
/// - GCC: C2x, standard LTO
///
/// For clang, the linker (lld) version is auto-derived from the compiler
/// command (e.g., "clang-20" → "lld-20"). Use `with_linker()` to override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compiler {
    command: String,
    linker: Option<String>,
}

impl Compiler {
    /// Create a compiler with the given command.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            linker: None,
        }
    }

    /// Default clang compiler.
    pub fn clang() -> Self {
        Self::new("clang")
    }

    /// Default gcc compiler.
    pub fn gcc() -> Self {
        Self::new("gcc")
    }

    /// Set explicit linker command (overrides auto-derivation).
    pub fn with_linker(mut self, linker: impl Into<String>) -> Self {
        self.linker = Some(linker.into());
        self
    }

    /// Command to invoke.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Check if this is a clang-based compiler (for flag selection).
    ///
    /// Returns true if the command contains "clang".
    pub fn is_clang(&self) -> bool {
        self.command.contains("clang")
    }

    /// Get the linker to use with `-fuse-ld=`.
    ///
    /// For clang, returns the linker (explicit or auto-derived from compiler version).
    /// For gcc, returns None (uses system default linker).
    ///
    /// Auto-derivation extracts version suffix from compiler command:
    /// - "clang" → "lld"
    /// - "clang-20" → "lld-20"
    /// - "/opt/llvm/bin/clang-18" → "lld-18"
    pub fn linker(&self) -> Option<String> {
        if !self.is_clang() {
            return None;
        }

        if let Some(ref linker) = self.linker {
            return Some(linker.clone());
        }

        Some(format!("lld{}", self.version_suffix()))
    }

    /// Get llvm-addr2line command, auto-derived from compiler version.
    ///
    /// Auto-derivation extracts version suffix from compiler command:
    /// - "clang" → "llvm-addr2line"
    /// - "clang-20" → "llvm-addr2line-20"
    /// - "/opt/llvm/bin/clang-18" → "llvm-addr2line-18"
    /// - "gcc" → "llvm-addr2line" (fallback)
    pub fn addr2line(&self) -> String {
        format!("llvm-addr2line{}", self.version_suffix())
    }

    /// Extract version suffix from compiler command.
    ///
    /// - "clang" → ""
    /// - "clang-20" → "-20"
    /// - "/opt/llvm/bin/clang-18" → "-18"
    /// - "gcc-13" → "" (no version for gcc)
    fn version_suffix(&self) -> &str {
        if !self.is_clang() {
            return "";
        }

        // Extract the basename first (handle paths like /opt/llvm/bin/clang-20)
        let basename = self.command.rsplit('/').next().unwrap_or(&self.command);

        // Extract version suffix: "clang-20" → "-20", "clang" → ""
        basename.strip_prefix("clang").unwrap_or("")
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::clang()
    }
}

impl FromStr for Compiler {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("compiler command cannot be empty".to_string());
        }
        Ok(Self::new(s))
    }
}

impl std::fmt::Display for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command)
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

/// x86_64 preserve_none: 12 argument registers available (C backend).
/// R12, R13, R14, R15, RDI, RSI, RDX, RCX, R8, R9, R11, RAX.
/// Only RSP and RBP are callee-saved.
/// See: https://clang.llvm.org/docs/AttributeReference.html#preserve-none
///
/// TODO: File LLVM bug - using all 12 causes register allocation failure during LTO.
///
/// Error: `ld.lld-21: error: <unknown>:0:0: ran out of registers during register
/// allocation in function 'B_...'`
///
/// preserve_none + 12 register args + musttail creates extreme register pressure.
/// At tail call sites, we need 12 args + scratch for dispatch table + computed index.
/// LTO transforms may create points where all values must be live simultaneously.
/// This shouldn't be a hard error - LLVM should always be able to spill pure C code.
/// Workaround: use 11 slots instead of 12.
pub const X86_64_DEFAULT_TOTAL_SLOTS: usize = 11;

/// x86_64 assembly backend: 10 GPRs available for hot registers.
///
/// Reserved: rbx (state ptr), r15 (memory ptr), rsp (stack), rax/rcx/rdx (temps)
/// Available: r14, r13, r12, rbp, rdi, rsi, r11, r10, r9, r8
///
/// Comparison to PolkaVM (13 regs): They use rax/rdx as hot regs, but that
/// requires spilling during mul/div. We keep rax/rcx/rdx as dedicated temps
/// for simpler codegen.
pub const X86_64_ASM_HOT_REG_SLOTS: usize = 10;

/// AArch64 preserve_none: 24 argument registers.
/// X20-X28 (9), X0-X7 (8), X9-X15 (7). Only LR and FP are callee-saved.
/// See: https://clang.llvm.org/docs/AttributeReference.html#preserve-none
pub const AARCH64_DEFAULT_TOTAL_SLOTS: usize = 24;

/// Fixed slots when instret counting is enabled (state + memory + instret).
pub const FIXED_SLOTS_WITH_INSTRET: usize = 3;

/// Fixed slots when instret counting is disabled (state + memory).
pub const FIXED_SLOTS_NO_INSTRET: usize = 2;

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
///
/// Fixed slots depend on configuration:
/// - With fixed addresses: only instret (if enabled) takes a slot
/// - Without fixed addresses: state + memory + instret (if enabled)
pub fn compute_num_hot_regs(
    total_slots: usize,
    instret_mode: InstretMode,
    tracer_config: &TracerConfig,
    fixed_addresses: bool,
) -> usize {
    let fixed = if fixed_addresses {
        // Only instret takes an argument slot (state/memory are constants)
        if instret_mode.counts() { 1 } else { 0 }
    } else if instret_mode.counts() {
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

    /// Convert to C constant value for RV_INSTRET_MODE export.
    pub fn as_c_mode(&self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Count => 1,
            Self::Suspend => 2,
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
    pub fn needs_mask(self) -> bool {
        matches!(self, Self::Wrap | Self::Bounds)
    }

    /// Whether addresses should be bounds-checked before access.
    ///
    /// True for Bounds mode only. C emitters use `if (out_of_bounds) trap()`,
    /// x86 uses `cmp; jbe ok; jmp trap; ok:`.
    pub fn needs_bounds_check(self) -> bool {
        self == Self::Bounds
    }

    /// Whether addresses are assumed valid (for optimizer hints).
    ///
    /// True for Unchecked mode. C emitters use `__builtin_assume()`.
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
    /// Fixed address for RvState struct.
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
    /// Emit comments in generated C code.
    pub emit_comments: bool,
    /// Emit #line directives for source-level debugging.
    pub emit_line_info: bool,
    /// Enable HTIF (Host-Target Interface) for riscv-tests.
    pub htif_enabled: bool,
    /// Print HTIF stdout (guest console output).
    pub htif_verbose: bool,
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
            backend: Backend::default(),
            analysis_mode: AnalysisMode::default(),
            address_mode: AddressMode::default(),
            instret_mode: InstretMode::Count,
            emit_comments: true,
            emit_line_info: true,
            htif_enabled: false,
            htif_verbose: false,
            memory_bits: 32,
            tracer_config: TracerConfig::none(),
            compiler: Compiler::default(),
            syscall_mode: SyscallMode::default(),
            export_functions: false,
            fixed_addresses: None,
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
        let num_hot_regs = compute_num_hot_regs(
            total_slots,
            self.instret_mode,
            &self.tracer_config,
            self.fixed_addresses.is_some(),
        );
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

    /// Set address translation mode.
    pub fn with_address_mode(mut self, mode: AddressMode) -> Self {
        self.address_mode = mode;
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
        self.htif_enabled = enabled;
        self
    }

    /// Set HTIF verbose (print guest stdout).
    pub fn with_htif_verbose(mut self, verbose: bool) -> Self {
        self.htif_verbose = verbose;
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

    /// Set fixed addresses for state and memory.
    ///
    /// When enabled, state/memory are accessed via compile-time constant addresses
    /// instead of function arguments. Requires runtime to map at these addresses.
    pub fn with_fixed_addresses(mut self, config: FixedAddressConfig) -> Self {
        self.fixed_addresses = Some(config);
        // Re-compute hot registers since fixed_addresses affects the calculation
        self.init_hot_regs(default_total_slots());
        self
    }

    /// Check if fixed addresses are enabled.
    pub fn has_fixed_addresses(&self) -> bool {
        self.fixed_addresses.is_some()
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

    #[test]
    fn test_compute_num_hot_regs() {
        let tracer = TracerConfig::none();
        // Without fixed addresses: 10 - 3 (state + memory + instret) = 7
        assert_eq!(
            compute_num_hot_regs(10, InstretMode::Count, &tracer, false),
            7
        );
        // Without fixed addresses: 10 - 2 (state + memory) = 8
        assert_eq!(
            compute_num_hot_regs(10, InstretMode::Off, &tracer, false),
            8
        );
        // Without fixed addresses: 10 - 3 (state + memory + instret) = 7
        assert_eq!(
            compute_num_hot_regs(10, InstretMode::Suspend, &tracer, false),
            7
        );

        // With fixed addresses: 10 - 1 (instret only) = 9
        assert_eq!(
            compute_num_hot_regs(10, InstretMode::Count, &tracer, true),
            9
        );
        // With fixed addresses: 10 - 0 = 10
        assert_eq!(
            compute_num_hot_regs(10, InstretMode::Off, &tracer, true),
            10
        );
    }

    #[test]
    fn test_compiler_linker_derivation() {
        // Basic clang -> lld
        let c = Compiler::new("clang");
        assert_eq!(c.linker(), Some("lld".to_string()));

        // Versioned clang-20 -> lld-20
        let c = Compiler::new("clang-20");
        assert_eq!(c.linker(), Some("lld-20".to_string()));

        // Path with version
        let c = Compiler::new("/opt/llvm/bin/clang-18");
        assert_eq!(c.linker(), Some("lld-18".to_string()));

        // GCC has no lld
        let c = Compiler::new("gcc");
        assert_eq!(c.linker(), None);

        let c = Compiler::new("gcc-13");
        assert_eq!(c.linker(), None);

        // Explicit linker override
        let c = Compiler::new("clang-20").with_linker("lld-19");
        assert_eq!(c.linker(), Some("lld-19".to_string()));
    }

    #[test]
    fn test_compiler_addr2line_derivation() {
        // Basic clang -> llvm-addr2line
        let c = Compiler::new("clang");
        assert_eq!(c.addr2line(), "llvm-addr2line");

        // Versioned clang-20 -> llvm-addr2line-20
        let c = Compiler::new("clang-20");
        assert_eq!(c.addr2line(), "llvm-addr2line-20");

        // Path with version
        let c = Compiler::new("/opt/llvm/bin/clang-18");
        assert_eq!(c.addr2line(), "llvm-addr2line-18");

        // GCC uses llvm-addr2line without version
        let c = Compiler::new("gcc");
        assert_eq!(c.addr2line(), "llvm-addr2line");

        let c = Compiler::new("gcc-13");
        assert_eq!(c.addr2line(), "llvm-addr2line");
    }
}
