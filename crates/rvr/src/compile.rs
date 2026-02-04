use std::path::Path;

use rvr_emit::c::TracerConfig;
use rvr_emit::{
    AddressMode, AnalysisMode, Backend, Compiler, EmitConfig, FixedAddressConfig, InstretMode,
    SyscallMode,
};
use rvr_isa::{Rv32, Rv64, Xlen};
use tracing::warn;

use crate::{Error, Recompiler, Result};

/// Options for compile/lift operations.
#[derive(Clone, Debug)]
pub struct CompileOptions {
    /// Code generation backend.
    pub backend: Backend,
    /// Analysis mode (full CFG or linear scan).
    pub analysis_mode: AnalysisMode,
    /// Address translation mode.
    pub address_mode: AddressMode,
    /// Instruction retirement mode.
    pub instret_mode: InstretMode,
    /// Number of parallel compile jobs (0 = auto-detect based on CPU count).
    pub jobs: usize,
    /// Tracer configuration.
    pub tracer_config: TracerConfig,
    /// Syscall handling mode.
    pub syscall_mode: SyscallMode,
    /// C compiler to use.
    pub compiler: Compiler,
    /// Fixed addresses for state and memory (optional).
    /// When set, state/memory are accessed via compile-time constant addresses.
    pub fixed_addresses: Option<FixedAddressConfig>,
    /// Compile-time flags for toggles and optional features.
    pub flags: CompileFlags,
}

/// Toggle flags for compile options.
#[derive(Clone, Copy, Debug, Default)]
pub struct CompileFlags(u16);

impl CompileFlags {
    const ANALYSIS_MODE_AUTO: u16 = 1 << 0;
    const HTIF: u16 = 1 << 1;
    const HTIF_VERBOSE: u16 = 1 << 2;
    const LINE_INFO: u16 = 1 << 3;
    const EXPORT_FUNCTIONS: u16 = 1 << 4;
    const QUIET: u16 = 1 << 5;
    const PERF_MODE: u16 = 1 << 6;
    const SUPERBLOCK: u16 = 1 << 7;

    const fn set_flag(&mut self, flag: u16, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }

    const fn has_flag(self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }

    #[must_use]
    pub const fn analysis_mode_auto(self) -> bool {
        self.has_flag(Self::ANALYSIS_MODE_AUTO)
    }

    pub const fn set_analysis_mode_auto(&mut self, enabled: bool) {
        self.set_flag(Self::ANALYSIS_MODE_AUTO, enabled);
    }

    #[must_use]
    pub const fn htif(self) -> bool {
        self.has_flag(Self::HTIF)
    }

    pub const fn set_htif(&mut self, enabled: bool) {
        self.set_flag(Self::HTIF, enabled);
    }

    #[must_use]
    pub const fn htif_verbose(self) -> bool {
        self.has_flag(Self::HTIF_VERBOSE)
    }

    pub const fn set_htif_verbose(&mut self, enabled: bool) {
        self.set_flag(Self::HTIF_VERBOSE, enabled);
    }

    #[must_use]
    pub const fn line_info(self) -> bool {
        self.has_flag(Self::LINE_INFO)
    }

    pub const fn set_line_info(&mut self, enabled: bool) {
        self.set_flag(Self::LINE_INFO, enabled);
    }

    #[must_use]
    pub const fn export_functions(self) -> bool {
        self.has_flag(Self::EXPORT_FUNCTIONS)
    }

    pub const fn set_export_functions(&mut self, enabled: bool) {
        self.set_flag(Self::EXPORT_FUNCTIONS, enabled);
    }

    #[must_use]
    pub const fn quiet(self) -> bool {
        self.has_flag(Self::QUIET)
    }

    pub const fn set_quiet(&mut self, enabled: bool) {
        self.set_flag(Self::QUIET, enabled);
    }

    #[must_use]
    pub const fn perf_mode(self) -> bool {
        self.has_flag(Self::PERF_MODE)
    }

    pub const fn set_perf_mode(&mut self, enabled: bool) {
        self.set_flag(Self::PERF_MODE, enabled);
    }

    #[must_use]
    pub const fn enable_superblock(self) -> bool {
        self.has_flag(Self::SUPERBLOCK)
    }

    pub const fn set_enable_superblock(&mut self, enabled: bool) {
        self.set_flag(Self::SUPERBLOCK, enabled);
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        let mut flags = CompileFlags::default();
        flags.set_analysis_mode_auto(true);
        flags.set_line_info(true);
        flags.set_enable_superblock(true);
        Self {
            backend: Backend::default(),
            analysis_mode: AnalysisMode::default(),
            address_mode: AddressMode::default(),
            instret_mode: InstretMode::default(),
            jobs: 0,
            tracer_config: TracerConfig::default(),
            syscall_mode: SyscallMode::default(),
            compiler: Compiler::default(),
            fixed_addresses: None,
            flags,
        }
    }
}

impl CompileOptions {
    /// Create default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set code generation backend.
    #[must_use]
    pub const fn with_backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }

    /// Set analysis mode.
    #[must_use]
    pub const fn with_analysis_mode(mut self, mode: AnalysisMode) -> Self {
        self.analysis_mode = mode;
        self.flags.set_analysis_mode_auto(false);
        self
    }

    /// Use backend defaults for analysis mode (CFG for C, linear for asm).
    #[must_use]
    pub const fn with_analysis_mode_auto(mut self, enabled: bool) -> Self {
        self.flags.set_analysis_mode_auto(enabled);
        self
    }

    /// Set address translation mode.
    #[must_use]
    pub const fn with_address_mode(mut self, mode: AddressMode) -> Self {
        self.address_mode = mode;
        self
    }

    /// Set HTIF enabled.
    #[must_use]
    pub const fn with_htif(mut self, enabled: bool) -> Self {
        self.flags.set_htif(enabled);
        self
    }

    /// Set HTIF verbose (print guest stdout).
    #[must_use]
    pub const fn with_htif_verbose(mut self, verbose: bool) -> Self {
        self.flags.set_htif_verbose(verbose);
        self
    }

    /// Set `line_info` enabled (for `#line` directives).
    #[must_use]
    pub const fn with_line_info(mut self, enabled: bool) -> Self {
        self.flags.set_line_info(enabled);
        self
    }

    /// Set instret mode.
    #[must_use]
    pub const fn with_instret_mode(mut self, mode: InstretMode) -> Self {
        self.instret_mode = mode;
        self
    }

    /// Set number of parallel compile jobs (0 = auto-detect).
    #[must_use]
    pub const fn with_jobs(mut self, jobs: usize) -> Self {
        self.jobs = jobs;
        self
    }

    /// Set tracer configuration.
    #[must_use]
    pub fn with_tracer_config(mut self, tracer_config: TracerConfig) -> Self {
        self.tracer_config = tracer_config;
        self
    }

    /// Set syscall handling mode.
    #[must_use]
    pub const fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.syscall_mode = mode;
        self
    }

    /// Set the C compiler to use.
    #[must_use]
    pub fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.compiler = compiler;
        self
    }

    /// Suppress compilation output.
    #[must_use]
    pub const fn with_quiet(mut self, quiet: bool) -> Self {
        self.flags.set_quiet(quiet);
        self
    }

    /// Enable export functions mode for calling exported functions.
    ///
    /// When enabled, all function symbols are added as CFG entry points,
    /// and `RV_EXPORT_FUNCTIONS` metadata is set in the compiled library.
    #[must_use]
    pub const fn with_export_functions(mut self, enabled: bool) -> Self {
        self.flags.set_export_functions(enabled);
        self
    }

    /// Set fixed addresses for state and memory.
    ///
    /// When enabled, state/memory are accessed via compile-time constant addresses
    /// instead of function arguments. Requires runtime to map at these addresses.
    #[must_use]
    pub const fn with_fixed_addresses(mut self, config: FixedAddressConfig) -> Self {
        self.fixed_addresses = Some(config);
        self
    }

    /// Enable perf mode (disable instret/CSR reads).
    #[must_use]
    pub const fn with_perf_mode(mut self, enabled: bool) -> Self {
        self.flags.set_perf_mode(enabled);
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
        self.flags.set_enable_superblock(enabled);
        self
    }

    /// Apply options to `EmitConfig`.
    fn apply<X: Xlen>(&self, config: &mut EmitConfig<X>) {
        config.backend = self.backend;
        config.analysis_mode = if self.flags.analysis_mode_auto() {
            match self.backend {
                Backend::C => AnalysisMode::FullCfg,
                _ => AnalysisMode::Basic,
            }
        } else {
            self.analysis_mode
        };
        config.address_mode = self.address_mode;
        config.flags.set_htif_enabled(self.flags.htif());
        config.flags.set_htif_verbose(self.flags.htif_verbose());
        config.flags.set_emit_line_info(self.flags.line_info());
        config.instret_mode = self.instret_mode;
        config.tracer_config = self.tracer_config.clone();
        config.compiler = self.compiler.clone();
        config.syscall_mode = self.syscall_mode;
        config.fixed_addresses = self.fixed_addresses;
        config.perf_mode = self.flags.perf_mode();
        config.enable_superblock = self.flags.enable_superblock();
        if self.flags.perf_mode() {
            config.instret_mode = InstretMode::Off;
        }
        // Re-compute hot registers based on backend (x86 has different slot count than C)
        config.reinit_hot_regs_for_backend();
    }

    /// Check if line info is enabled.
    #[must_use]
    pub const fn has_line_info(&self) -> bool {
        self.flags.line_info()
    }

    #[must_use]
    pub const fn quiet(&self) -> bool {
        self.flags.quiet()
    }

    #[must_use]
    pub const fn export_functions(&self) -> bool {
        self.flags.export_functions()
    }
}

/// Compile an ELF file, auto-detecting XLEN from the ELF header.
///
/// # Errors
/// Returns an error if the ELF cannot be read or compilation fails.
pub fn compile(elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
    let options = CompileOptions::default();
    compile_with_options(elf_path, output_dir, &options)
}

/// Compile an ELF file with options, auto-detecting XLEN from the ELF header.
///
/// # Errors
/// Returns an error if the ELF cannot be read or compilation fails.
pub fn compile_with_options(
    elf_path: &Path,
    output_dir: &Path,
    options: &CompileOptions,
) -> Result<std::path::PathBuf> {
    let data = std::fs::read(elf_path)?;
    let xlen = rvr_elf::get_elf_xlen(&data)?;

    dispatch_by_xlen(
        xlen,
        || {
            let mut config = EmitConfig::<Rv32>::default();
            options.apply(&mut config);
            let recompiler = Recompiler::<Rv32>::new(config)
                .with_quiet(options.quiet())
                .with_export_functions(options.export_functions());
            recompiler.compile(elf_path, output_dir, options.jobs)
        },
        || {
            let mut config = EmitConfig::<Rv64>::default();
            options.apply(&mut config);
            let recompiler = Recompiler::<Rv64>::new(config)
                .with_quiet(options.quiet())
                .with_export_functions(options.export_functions());
            recompiler.compile(elf_path, output_dir, options.jobs)
        },
    )
}

/// Lift an ELF file to C source code, auto-detecting XLEN.
///
/// # Errors
/// Returns an error if the ELF cannot be read or lifting fails.
pub fn lift_to_c(elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
    let options = CompileOptions::default();
    lift_to_c_with_options(elf_path, output_dir, &options)
}

/// Lift an ELF file to C source code with options, auto-detecting XLEN.
///
/// # Errors
/// Returns an error if the ELF cannot be read or lifting fails.
pub fn lift_to_c_with_options(
    elf_path: &Path,
    output_dir: &Path,
    options: &CompileOptions,
) -> Result<std::path::PathBuf> {
    let data = std::fs::read(elf_path)?;
    let xlen = rvr_elf::get_elf_xlen(&data)?;

    dispatch_by_xlen(
        xlen,
        || {
            let mut config = EmitConfig::<Rv32>::default();
            options.apply(&mut config);
            let recompiler =
                Recompiler::<Rv32>::new(config).with_export_functions(options.export_functions());
            recompiler.lift(elf_path, output_dir)
        },
        || {
            let mut config = EmitConfig::<Rv64>::default();
            options.apply(&mut config);
            let recompiler =
                Recompiler::<Rv64>::new(config).with_export_functions(options.export_functions());
            recompiler.lift(elf_path, output_dir)
        },
    )
}

fn dispatch_by_xlen<R>(
    xlen: u8,
    rv32: impl FnOnce() -> Result<R>,
    rv64: impl FnOnce() -> Result<R>,
) -> Result<R> {
    match xlen {
        32 => rv32(),
        64 => rv64(),
        _ => {
            warn!(xlen = xlen, "unsupported XLEN (expected 32 or 64)");
            Err(Error::XlenMismatch {
                expected: 32,
                actual: xlen,
            })
        }
    }
}
