//! C backend configuration.
//!
//! Compiler configuration, platform-specific hot register slot counts,
//! and argument slot calculations for the C code generation backend.

use std::str::FromStr;

use super::TracerConfig;
use crate::InstretMode;

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

/// x86_64 preserve_none: 12 argument registers available.
/// R12, R13, R14, R15, RDI, RSI, RDX, RCX, R8, R9, R11, RAX.
/// Only RSP and RBP are callee-saved.
/// See: https://clang.llvm.org/docs/AttributeReference.html#preserve-none
///
/// Using all 12 args with preserve_none + musttail can exhaust LLVM's register allocator
/// under LTO (e.g., lld reports "ran out of registers during register allocation").
/// We reserve one slot to keep register pressure manageable at tail-call sites.
pub const X86_64_DEFAULT_TOTAL_SLOTS: usize = 11;

/// AArch64 preserve_none: 24 argument registers.
/// X20-X28 (9), X0-X7 (8), X9-X15 (7). Only LR and FP are callee-saved.
/// See: https://clang.llvm.org/docs/AttributeReference.html#preserve-none
pub const AARCH64_DEFAULT_TOTAL_SLOTS: usize = 24;

/// Fixed slots when instret counting is enabled (state + memory + instret).
pub const FIXED_SLOTS_WITH_INSTRET: usize = 3;

/// Fixed slots when instret counting is disabled (state + memory).
pub const FIXED_SLOTS_NO_INSTRET: usize = 2;

/// Get platform-specific default total slots for C backend.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
