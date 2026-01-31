//! RVR - RISC-V Static Recompiler
//!
//! Compiles RISC-V ELF binaries to optimized C code, then to native shared libraries.
//!
//! # Quick Start
//!
//! ```ignore
//! // Compile an ELF to a shared library (auto-detects RV32/RV64)
//! let lib_path = rvr::compile("program.elf".as_ref(), "output/".as_ref())?;
//! ```
//!
//! # Architecture
//!
//! RVR generates `.so` shared libraries that can be loaded by a host runtime.
//! The generated code:
//!
//! - Uses `preserve_none` calling convention for minimal overhead
//! - Passes hot registers as function arguments (configurable)
//! - Uses `[[clang::musttail]]` for guaranteed tail call optimization
//! - Generates C23 code with `constexpr`, typed constants, no macros
//!
//! ## Generated Interface
//!
//! The shared library exports:
//!
//! ```c
//! // Execute from a specific PC (returns exit code)
//! int rv_execute_from(RvState* state, uint32_t start_pc);
//!
//! // Initialize memory with embedded ELF segments
//! void rv_init_memory(RvState* state);
//!
//! // Free memory
//! void rv_free_memory(RvState* state);
//!
//! // Dispatch table for dynamic jumps
//! extern const rv_fn dispatch_table[];
//! ```
//!
//! ## State Structure
//!
//! The `RvState` struct is defined in the generated header. Key fields:
//!
//! - `memory`: Pointer to guest memory (allocated by host)
//! - `regs[32]`: General-purpose registers
//! - `pc`: Program counter
//! - `instret`: Retired instruction count (if enabled)
//! - `has_exited`, `exit_code`: Execution status
//!
//! # Examples
//!
//! ## Basic Compilation
//!
//! ```ignore
//! use rvr::{compile, CompileOptions, AddressMode, InstretMode};
//!
//! // Simple compilation
//! let lib = rvr::compile("prog.elf".as_ref(), "out/".as_ref())?;
//!
//! // With options
//! let options = CompileOptions::new()
//!     .with_instret_mode(InstretMode::Count)
//!     .with_address_mode(AddressMode::Bounds);
//! let lib = rvr::compile_with_options("prog.elf".as_ref(), "out/".as_ref(), options)?;
//! ```
//!
//! ## Custom Configuration
//!
//! ```ignore
//! use rvr::{EmitConfig, Recompiler, Rv64};
//!
//! let mut config = EmitConfig::<Rv64>::default();
//! config.hot_regs = vec![1, 2, 10, 11, 12]; // ra, sp, a0, a1, a2
//! config.memory_bits = 32; // 4GB address space
//! config.enable_lto = true;
//!
//! let recompiler = Recompiler::new(config);
//! let lib = recompiler.compile("prog.elf".as_ref(), "out/".as_ref(), 0)?;
//! ```
//!
//! ## Instruction Overrides
//!
//! Customize instruction behavior (e.g., ECALL handling):
//!
//! ```ignore
//! use rvr::{ElfImage, EmitConfig, Pipeline, Rv64};
//! use rvr_isa::{ExtensionRegistry, InstructionOverride, OP_ECALL, DecodedInstr};
//! use rvr_ir::{InstrIR, Terminator, Expr};
//!
//! struct MyEcallHandler;
//!
//! impl InstructionOverride<Rv64> for MyEcallHandler {
//!     fn lift(
//!         &self,
//!         instr: &DecodedInstr<Rv64>,
//!         _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
//!     ) -> InstrIR<Rv64> {
//!         // Custom ECALL: exit with a0 as code
//!         InstrIR::new(
//!             instr.pc, instr.size, instr.opid.pack(),
//!             Vec::new(),
//!             Terminator::exit(Expr::read(10)), // a0
//!         )
//!     }
//! }
//!
//! let registry = ExtensionRegistry::<Rv64>::standard()
//!     .with_override(OP_ECALL, MyEcallHandler);
//!
//! let data = std::fs::read("prog.elf")?;
//! let image = ElfImage::<Rv64>::parse(&data)?;
//! let mut pipeline = Pipeline::with_registry(image, EmitConfig::default(), registry);
//! ```
//!
//! ## Extension Registry (Builder Pattern)
//!
//! Enable only the RISC-V extensions you need:
//!
//! ```ignore
//! use rvr_isa::ExtensionRegistry;
//! use rvr::Rv64;
//!
//! // Start with base I extension, add only what you need
//! let registry = ExtensionRegistry::<Rv64>::base()
//!     .with_m()      // Integer multiply/divide
//!     .with_a()      // Atomics
//!     .with_c()      // Compressed (16-bit) instructions
//!     .with_zicsr(); // CSR access
//!
//! // Or use standard() for all common extensions
//! let full = ExtensionRegistry::<Rv64>::standard();
//!
//! // Typical Linux userspace configuration
//! let linux = ExtensionRegistry::<Rv64>::base()
//!     .with_c()       // Compressed first (for correct decode order)
//!     .with_m()       // Multiply/divide
//!     .with_a()       // Atomics
//!     .with_zicsr()   // CSR access
//!     .with_zba()     // Address generation
//!     .with_zbb();    // Basic bit manipulation
//! ```
//!
//! Available extensions:
//! - `with_m()` - Integer multiply/divide (M)
//! - `with_a()` - Atomics (A)
//! - `with_c()` - Compressed 16-bit instructions (C) - add first
//! - `with_zicsr()` - CSR read/write
//! - `with_zifencei()` - Instruction fence
//! - `with_zba()` - Address generation (Zba)
//! - `with_zbb()` - Basic bit manipulation (Zbb)
//! - `with_zbs()` - Single-bit operations (Zbs)
//! - `with_zbkb()` - Bitmanip for crypto (Zbkb)
//! - `with_zicond()` - Conditional operations (Zicond)
//!
//! ## Pipeline API (Low-Level)
//!
//! For fine-grained control over the compilation process:
//!
//! ```ignore
//! use rvr::{ElfImage, EmitConfig, Pipeline, Rv64};
//!
//! let data = std::fs::read("prog.elf")?;
//! let image = ElfImage::<Rv64>::parse(&data)?;
//!
//! let mut pipeline = Pipeline::new(image, EmitConfig::default());
//!
//! // Build CFG (decode, analyze, optimize)
//! pipeline.build_cfg()?;
//! println!("Blocks: {:?}", pipeline.stats());
//!
//! // Lift to IR
//! pipeline.lift_to_ir()?;
//!
//! // Inspect IR blocks
//! for (pc, block) in pipeline.ir_blocks() {
//!     println!("Block at {:#x}: {} instructions", pc, block.instructions.len());
//! }
//!
//! // Emit C code
//! pipeline.emit_c("out/".as_ref(), "prog")?;
//! ```
//!
//! # Crate Structure
//!
//! - `rvr` - High-level API (this crate)
//! - `rvr_elf` - ELF parsing
//! - `rvr_isa` - RISC-V instruction definitions, decoder, extension registry
//! - `rvr_ir` - Intermediate representation
//! - `rvr_cfg` - Control flow graph analysis
//! - `rvr_emit` - C code generation
//!
//! # Feature Flags
//!
//! Currently no optional Cargo features. RISC-V extensions are selected at
//! runtime via the `ExtensionRegistry` builder pattern (see above). The default
//! `Pipeline::new()` uses `ExtensionRegistry::standard()` which enables all
//! common extensions (I, M, A, C, Zicsr, Zifencei, Zba, Zbb, Zbs, Zbkb, Zicond).

// Core types - always available
pub use rvr_elf::{ElfImage, get_elf_xlen};
pub use rvr_emit::c::TracerConfig;
pub use rvr_emit::{
    AddressMode, AnalysisMode, Backend, Compiler, EmitConfig, FixedAddressConfig, InstretMode,
    SyscallMode,
};
pub use rvr_isa::{Rv32, Rv64, Xlen};

// CSR constants for use with Runner::get_csr/set_csr
pub use rvr_isa::extensions::{CSR_CYCLE, CSR_INSTRET, CSR_TIME};

pub mod perf;
mod pipeline;
pub use pipeline::{Pipeline, PipelineStats};

mod runner;
pub use runner::{PerfCounters, RunError, RunResult, RunResultWithPerf, Runner};

pub mod bench;
pub mod gdb;
pub mod metrics;
pub mod tests;

use std::marker::PhantomData;
use std::path::Path;
use std::process::{Command, Stdio};

use rvr_isa::ExtensionRegistry;
use rvr_isa::syscalls::{LinuxHandler, SyscallAbi};
use thiserror::Error;
use tracing::{debug, error, info_span, warn};

/// Recompiler errors.
#[derive(Error, Debug)]
pub enum Error {
    #[error("ELF error: {0}")]
    Elf(#[from] rvr_elf::ElfError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XLEN mismatch: expected {expected}, got {actual}")]
    XlenMismatch { expected: u8, actual: u8 },
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    #[error("No program loaded")]
    NoProgramLoaded,
    #[error("No code segment containing entry point 0x{0:x}")]
    NoCodeSegment(u64),
    #[error("CFG not built: call build_cfg before {0}")]
    CfgNotBuilt(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

/// RISC-V recompiler.
pub struct Recompiler<X: Xlen> {
    config: EmitConfig<X>,
    quiet: bool,
    export_functions: bool,
    _marker: PhantomData<X>,
}

impl<X: Xlen> Recompiler<X> {
    /// Create a new recompiler with the given configuration.
    pub fn new(config: EmitConfig<X>) -> Self {
        Self {
            config,
            quiet: false,
            export_functions: false,
            _marker: PhantomData,
        }
    }

    /// Create a recompiler with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(EmitConfig::default())
    }

    /// Set syscall handling mode.
    pub fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.config.syscall_mode = mode;
        self
    }

    /// Set the C compiler to use (clang or gcc).
    pub fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.config.compiler = compiler;
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
        self.config.export_functions = enabled;
        self
    }

    /// Get the configuration.
    pub fn config(&self) -> &EmitConfig<X> {
        &self.config
    }

    /// Compile an ELF file to a shared library.
    ///
    /// If `jobs` is 0, auto-detects based on CPU count.
    pub fn compile(
        &self,
        elf_path: &Path,
        output_dir: &Path,
        jobs: usize,
    ) -> Result<std::path::PathBuf> {
        // First lift to source (C or x86 assembly)
        let _source_path = self.lift(elf_path, output_dir)?;

        let lib_name = output_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rv");

        // Compile based on backend
        match self.config.backend {
            Backend::C => {
                // Compile C to .so (compiler choice is already in the Makefile via config)
                compile_c_to_shared(output_dir, jobs, self.quiet)?;
            }
            Backend::X86Asm => {
                // Assemble x86 to .so
                compile_x86_to_shared(output_dir, lib_name, &self.config.compiler, self.quiet)?;
            }
        }

        let lib_path = output_dir.join(format!("lib{}.so", lib_name));
        Ok(lib_path)
    }

    /// Lift an ELF file to source code (C or x86 assembly, depending on backend).
    pub fn lift(&self, elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
        // Load ELF
        let data = std::fs::read(elf_path)?;
        let image = ElfImage::<X>::parse(&data)?;

        // Create output directory if it doesn't exist
        std::fs::create_dir_all(output_dir)?;

        // Build pipeline with syscall handler selection.
        let registry = match self.config.syscall_mode {
            SyscallMode::BareMetal => ExtensionRegistry::standard(),
            SyscallMode::Linux => {
                let abi = if image.is_rve() {
                    SyscallAbi::Embedded
                } else {
                    SyscallAbi::Standard
                };
                ExtensionRegistry::standard().with_syscall_handler(LinuxHandler::new(abi))
            }
        };
        let mut pipeline = Pipeline::<X>::with_registry(image, self.config.clone(), registry);

        // Add function symbols as extra entry points if requested
        if self.export_functions {
            pipeline.add_function_symbols_as_entry_points();
        }

        // Build CFG (InstructionTable → BlockTable → optimizations)
        pipeline.build_cfg()?;

        // Lift to IR
        pipeline.lift_to_ir()?;

        let base_name = output_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rv");

        // Emit based on backend
        match self.config.backend {
            Backend::C => {
                // Load debug info for #line directives (if enabled and ELF has debug info)
                if self.config.emit_line_info
                    && let Some(path_str) = elf_path.to_str()
                    && let Err(e) = pipeline.load_debug_info(path_str)
                {
                    warn!(error = %e, "failed to load debug info (continuing without #line directives)");
                }

                pipeline.emit_c(output_dir, base_name)?;
                Ok(output_dir.join(format!("{}_part0.c", base_name)))
            }
            Backend::X86Asm => {
                pipeline.emit_x86(output_dir, base_name)?;
                Ok(output_dir.join(format!("{}.s", base_name)))
            }
        }
    }
}

/// Options for compile/lift operations.
#[derive(Clone, Debug)]
pub struct CompileOptions {
    /// Code generation backend.
    pub backend: Backend,
    /// Analysis mode (full CFG or linear scan).
    pub analysis_mode: AnalysisMode,
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
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            analysis_mode: AnalysisMode::default(),
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

    /// Apply options to EmitConfig.
    fn apply<X: Xlen>(&self, config: &mut EmitConfig<X>) {
        config.backend = self.backend;
        config.analysis_mode = self.analysis_mode;
        config.address_mode = self.address_mode;
        config.htif_enabled = self.htif;
        config.htif_verbose = self.htif_verbose;
        config.emit_line_info = self.line_info;
        config.instret_mode = self.instret_mode;
        config.tracer_config = self.tracer_config.clone();
        config.compiler = self.compiler.clone();
        config.syscall_mode = self.syscall_mode;
        config.fixed_addresses = self.fixed_addresses;
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
    let xlen = get_elf_xlen(&data)?;

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
    let xlen = get_elf_xlen(&data)?;

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

/// Compile C source to shared library.
///
/// If `jobs` is 0, auto-detects based on CPU count.
/// Note: The compiler is set in the Makefile (generated with the chosen CC).
fn compile_c_to_shared(output_dir: &Path, jobs: usize, quiet: bool) -> Result<()> {
    let _span = info_span!("compile_c").entered();

    let makefile_path = output_dir.join("Makefile");
    if !makefile_path.exists() {
        error!(path = %makefile_path.display(), "Makefile not found");
        return Err(Error::CompilationFailed("Makefile not found".to_string()));
    }

    let job_count = if jobs == 0 {
        num_cpus::get().saturating_sub(2).max(1)
    } else {
        jobs
    };

    debug!(dir = %output_dir.display(), jobs = job_count, "running make");

    let mut cmd = Command::new("make");
    cmd.arg("-C")
        .arg(output_dir)
        .arg("-j")
        .arg(job_count.to_string())
        .arg("shared");

    // Always capture output so we can show errors on failure
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| {
        error!(error = %e, "failed to run make");
        Error::CompilationFailed(format!("Failed to run make: {}", e))
    })?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Log full output for debugging
        if !stderr.is_empty() {
            error!(exit_code = code, dir = %output_dir.display(), stderr = %stderr, "make failed");
        } else if !stdout.is_empty() {
            error!(exit_code = code, dir = %output_dir.display(), stdout = %stdout, "make failed");
        } else {
            error!(exit_code = code, dir = %output_dir.display(), "make failed");
        }
        // Include first line of error in the error message for quick visibility
        let first_error = stderr
            .lines()
            .next()
            .or_else(|| stdout.lines().next())
            .unwrap_or("unknown error");
        return Err(Error::CompilationFailed(format!(
            "make failed: {}",
            first_error
        )));
    } else if !quiet {
        // In non-quiet mode, show stdout (compilation progress)
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            for line in stdout.lines() {
                debug!("{}", line);
            }
        }
    }

    Ok(())
}

/// Compile x86 assembly to shared library.
///
/// On non-x86 hosts, uses clang for cross-compilation with:
/// - `--target=x86_64-unknown-linux-gnu` for x86 target
/// - `-fuse-ld=lld` for cross-linking
/// - `-nostdlib` since generated code is self-contained
fn compile_x86_to_shared(
    output_dir: &Path,
    base_name: &str,
    compiler: &Compiler,
    quiet: bool,
) -> Result<()> {
    let _span = info_span!("compile_x86").entered();

    let asm_path = output_dir.join(format!("{}.s", base_name));
    let obj_path = output_dir.join(format!("{}.o", base_name));
    let lib_path = output_dir.join(format!("lib{}.so", base_name));

    if !asm_path.exists() {
        return Err(Error::CompilationFailed(format!(
            "Assembly file not found: {}",
            asm_path.display()
        )));
    }

    // Check if we need cross-compilation (non-x86 host)
    let is_x86_host = cfg!(target_arch = "x86_64") || cfg!(target_arch = "x86");
    let needs_cross = !is_x86_host;

    let cc = if needs_cross {
        // On non-x86 hosts, must use clang for cross-compilation
        "clang"
    } else {
        compiler.command()
    };

    debug!(asm = %asm_path.display(), compiler = %cc, cross = %needs_cross, "assembling");

    // Assemble: cc -c -fPIC -o foo.o foo.s
    let mut asm_cmd = Command::new(cc);

    if needs_cross {
        // Cross-compilation: use clang with explicit x86 target
        asm_cmd.args(["--target=x86_64-unknown-linux-gnu", "-c", "-fPIC"]);
    } else {
        // AT&T syntax works with both GCC and LLVM's integrated assembler
        asm_cmd.args(["-c", "-fPIC"]);
    }

    asm_cmd.arg("-o").arg(&obj_path).arg(&asm_path);

    let asm_output = asm_cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::CompilationFailed(format!("Failed to run {}: {}", cc, e)))?;

    if !asm_output.status.success() {
        let stderr = String::from_utf8_lossy(&asm_output.stderr);
        error!(stderr = %stderr, "assembly failed");
        return Err(Error::CompilationFailed(format!(
            "Assembly failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        )));
    }

    debug!(obj = %obj_path.display(), "linking");

    // Link to shared library
    let mut link_cmd = Command::new(cc);

    if needs_cross {
        // Cross-linking: use lld and no stdlib (our code is self-contained)
        link_cmd.args([
            "--target=x86_64-unknown-linux-gnu",
            "-fuse-ld=lld",
            "-nostdlib",
            "-shared",
            "-Wl,-z,noexecstack",
        ]);
    } else {
        link_cmd.args(["-shared", "-Wl,-z,noexecstack"]);
        // Use configured linker for clang (e.g., lld, lld-20)
        if let Some(linker) = compiler.linker() {
            link_cmd.arg(format!("-fuse-ld={}", linker));
        }
    }

    link_cmd.arg("-o").arg(&lib_path).arg(&obj_path);

    let link_output = link_cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::CompilationFailed(format!("Failed to link: {}", e)))?;

    if !link_output.status.success() {
        let stderr = String::from_utf8_lossy(&link_output.stderr);
        error!(stderr = %stderr, "linking failed");
        return Err(Error::CompilationFailed(format!(
            "Linking failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        )));
    }

    if !quiet {
        debug!(lib = %lib_path.display(), cross = %needs_cross, "compiled x86 shared library");
    }

    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_recompiler_creation() {
        let _recompiler = Recompiler::<Rv64>::with_defaults();
    }
}
