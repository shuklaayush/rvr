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
    /// Use backend defaults for analysis mode (CFG for C, linear for asm).
    pub analysis_mode_auto: bool,
    /// Address translation mode.
    pub address_mode: AddressMode,
    /// Enable HTIF (Host-Target Interface) for riscv-tests.
    pub htif: bool,
    /// Print HTIF stdout (guest console output).
    pub htif_verbose: bool,
    /// Emit #line directives with source locations (requires debug info in ELF).
    /// Defaults to true (matching EmitConfig).
    pub line_info: bool,
    /// Export functions mode: compile for calling exported functions rather than running from entry point.
    /// Adds all function symbols as entry points for CFG analysis.
    pub export_functions: bool,
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
    /// Suppress compilation output (make commands, etc).
    pub quiet: bool,
    /// Fixed addresses for state and memory (optional).
    /// When set, state/memory are accessed via compile-time constant addresses.
    pub fixed_addresses: Option<FixedAddressConfig>,
    /// Perf mode (disable instret/CSR reads).
    pub perf_mode: bool,
    /// Enable superblock formation (merging fall-through blocks after branches).
    /// Disable for differential testing to ensure dispatch works at all block boundaries.
    pub enable_superblock: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            analysis_mode: AnalysisMode::default(),
            analysis_mode_auto: true,
            address_mode: AddressMode::default(),
            htif: false,
            htif_verbose: false,
            line_info: true, // Match EmitConfig default
            export_functions: false,
            instret_mode: InstretMode::default(),
            jobs: 0,
            tracer_config: TracerConfig::default(),
            syscall_mode: SyscallMode::default(),
            compiler: Compiler::default(),
            quiet: false,
            fixed_addresses: None,
            perf_mode: false,
            enable_superblock: true, // Enabled by default for performance
        }
    }
}

impl CompileOptions {
    /// Create default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set code generation backend.
    pub fn with_backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }

    /// Set analysis mode.
    pub fn with_analysis_mode(mut self, mode: AnalysisMode) -> Self {
        self.analysis_mode = mode;
        self.analysis_mode_auto = false;
        self
    }

    /// Use backend defaults for analysis mode (CFG for C, linear for asm).
    pub fn with_analysis_mode_auto(mut self, enabled: bool) -> Self {
        self.analysis_mode_auto = enabled;
        self
    }

    /// Set address translation mode.
    pub fn with_address_mode(mut self, mode: AddressMode) -> Self {
        self.address_mode = mode;
        self
    }

    /// Set HTIF enabled.
    pub fn with_htif(mut self, enabled: bool) -> Self {
        self.htif = enabled;
        self
    }

    /// Set HTIF verbose (print guest stdout).
    pub fn with_htif_verbose(mut self, verbose: bool) -> Self {
        self.htif_verbose = verbose;
        self
    }

    /// Set line_info enabled (for #line directives).
    pub fn with_line_info(mut self, enabled: bool) -> Self {
        self.line_info = enabled;
        self
    }

    /// Set instret mode.
    pub fn with_instret_mode(mut self, mode: InstretMode) -> Self {
        self.instret_mode = mode;
        self
    }

    /// Set number of parallel compile jobs (0 = auto-detect).
    pub fn with_jobs(mut self, jobs: usize) -> Self {
        self.jobs = jobs;
        self
    }

    /// Set tracer configuration.
    pub fn with_tracer_config(mut self, config: TracerConfig) -> Self {
        self.tracer_config = config;
        self
    }

    /// Set syscall handling mode.
    pub fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.syscall_mode = mode;
        self
    }

    /// Set the C compiler to use.
    pub fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.compiler = compiler;
        self
    }

    /// Suppress compilation output.
    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Enable export functions mode for calling exported functions.
    ///
    /// When enabled, all function symbols are added as CFG entry points,
    /// and RV_EXPORT_FUNCTIONS metadata is set in the compiled library.
    pub fn with_export_functions(mut self, enabled: bool) -> Self {
        self.export_functions = enabled;
        self
    }

    /// Set fixed addresses for state and memory.
    ///
    /// When enabled, state/memory are accessed via compile-time constant addresses
    /// instead of function arguments. Requires runtime to map at these addresses.
    pub fn with_fixed_addresses(mut self, config: FixedAddressConfig) -> Self {
        self.fixed_addresses = Some(config);
        self
    }

    /// Enable perf mode (disable instret/CSR reads).
    pub fn with_perf_mode(mut self, enabled: bool) -> Self {
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
    pub fn with_superblock(mut self, enabled: bool) -> Self {
        self.enable_superblock = enabled;
        self
    }

    /// Apply options to EmitConfig.
    fn apply<X: Xlen>(&self, config: &mut EmitConfig<X>) {
        config.backend = self.backend;
        config.analysis_mode = if self.analysis_mode_auto {
            match self.backend {
                Backend::C => AnalysisMode::FullCfg,
                _ => AnalysisMode::Basic,
            }
        } else {
            self.analysis_mode
        };
        config.address_mode = self.address_mode;
        config.htif_enabled = self.htif;
        config.htif_verbose = self.htif_verbose;
        config.emit_line_info = self.line_info;
        config.instret_mode = self.instret_mode;
        config.tracer_config = self.tracer_config.clone();
        config.compiler = self.compiler.clone();
        config.syscall_mode = self.syscall_mode;
        config.fixed_addresses = self.fixed_addresses;
        config.perf_mode = self.perf_mode;
        config.enable_superblock = self.enable_superblock;
        if self.perf_mode {
            config.instret_mode = InstretMode::Off;
        }
        // Re-compute hot registers based on backend (x86 has different slot count than C)
        config.reinit_hot_regs_for_backend();
    }

    /// Check if line info is enabled.
    pub fn has_line_info(&self) -> bool {
        self.line_info
    }
}

/// Compile an ELF file, auto-detecting XLEN from the ELF header.
pub fn compile(elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
    compile_with_options(elf_path, output_dir, CompileOptions::default())
}

/// Compile an ELF file with options, auto-detecting XLEN from the ELF header.
pub fn compile_with_options(
    elf_path: &Path,
    output_dir: &Path,
    options: CompileOptions,
) -> Result<std::path::PathBuf> {
    let data = std::fs::read(elf_path)?;
    let xlen = rvr_elf::get_elf_xlen(&data)?;

    dispatch_by_xlen(
        xlen,
        || {
            let mut config = EmitConfig::<Rv32>::default();
            options.apply(&mut config);
            let recompiler = Recompiler::<Rv32>::new(config)
                .with_quiet(options.quiet)
                .with_export_functions(options.export_functions);
            recompiler.compile(elf_path, output_dir, options.jobs)
        },
        || {
            let mut config = EmitConfig::<Rv64>::default();
            options.apply(&mut config);
            let recompiler = Recompiler::<Rv64>::new(config)
                .with_quiet(options.quiet)
                .with_export_functions(options.export_functions);
            recompiler.compile(elf_path, output_dir, options.jobs)
        },
    )
}

/// Lift an ELF file to C source code, auto-detecting XLEN.
pub fn lift_to_c(elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
    lift_to_c_with_options(elf_path, output_dir, CompileOptions::default())
}

/// Lift an ELF file to C source code with options, auto-detecting XLEN.
pub fn lift_to_c_with_options(
    elf_path: &Path,
    output_dir: &Path,
    options: CompileOptions,
) -> Result<std::path::PathBuf> {
    let data = std::fs::read(elf_path)?;
    let xlen = rvr_elf::get_elf_xlen(&data)?;

    dispatch_by_xlen(
        xlen,
        || {
            let mut config = EmitConfig::<Rv32>::default();
            options.apply(&mut config);
            let recompiler =
                Recompiler::<Rv32>::new(config).with_export_functions(options.export_functions);
            recompiler.lift(elf_path, output_dir)
        },
        || {
            let mut config = EmitConfig::<Rv64>::default();
            options.apply(&mut config);
            let recompiler =
                Recompiler::<Rv64>::new(config).with_export_functions(options.export_functions);
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
