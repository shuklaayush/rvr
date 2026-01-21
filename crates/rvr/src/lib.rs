//! RVR - RISC-V Static Recompiler
//!
//! Compiles RISC-V ELF binaries to optimized C code, then to native shared libraries.
//! Designed for integration with Mojo-based execution environments.
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
//! RVR generates `.so` shared libraries that can be loaded by a host runtime
//! (typically Mojo). The generated code:
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
//! The `RvState` struct is defined in the generated header and must match
//! the Mojo `RvState` layout. Key fields:
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
//! use rvr::{compile, CompileOptions, InstretMode};
//!
//! // Simple compilation
//! let lib = rvr::compile("prog.elf".as_ref(), "out/".as_ref())?;
//!
//! // With options
//! let options = CompileOptions::new()
//!     .with_instret_mode(InstretMode::Count)
//!     .with_addr_check(true);
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
pub use rvr_elf::{get_elf_xlen, ElfImage};
pub use rvr_emit::{EmitConfig, InstretMode, TracerConfig};
pub use rvr_isa::{Rv32, Rv64, Xlen};

mod pipeline;
pub use pipeline::{Pipeline, PipelineStats};

mod runner;
pub use runner::{RunError, RunResult, Runner};

pub mod bench;

use std::marker::PhantomData;
use std::path::Path;

use rvr_isa::syscalls::{LinuxHandler, SyscallAbi};
use rvr_isa::ExtensionRegistry;
use thiserror::Error;

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

/// Syscall handling mode for ECALL instructions.
#[derive(Clone, Copy, Debug, Default)]
pub enum SyscallMode {
    /// Bare-metal syscalls (exit only).
    #[default]
    BareMetal,
    /// Linux-style syscalls (brk/mmap/read/write, etc).
    Linux,
}

/// RISC-V recompiler.
pub struct Recompiler<X: Xlen> {
    config: EmitConfig<X>,
    syscall_mode: SyscallMode,
    compiler: Option<String>,
    quiet: bool,
    _marker: PhantomData<X>,
}

impl<X: Xlen> Recompiler<X> {
    /// Create a new recompiler with the given configuration.
    pub fn new(config: EmitConfig<X>) -> Self {
        Self {
            config,
            syscall_mode: SyscallMode::BareMetal,
            compiler: None,
            quiet: false,
            _marker: PhantomData,
        }
    }

    /// Create a recompiler with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(EmitConfig::default())
    }

    /// Set syscall handling mode.
    pub fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.syscall_mode = mode;
        self
    }

    /// Set the C compiler to use (e.g. "clang" or "gcc").
    pub fn with_compiler(mut self, compiler: impl Into<String>) -> Self {
        self.compiler = Some(compiler.into());
        self
    }

    /// Suppress compilation output.
    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
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
        // First lift to C source
        let _c_path = self.lift(elf_path, output_dir)?;

        // Then compile C to .so
        compile_c_to_shared(output_dir, jobs, self.compiler.as_deref(), self.quiet)?;

        // Return the path to the shared library
        let lib_name = output_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rv");
        let lib_path = output_dir.join(format!("lib{}.so", lib_name));
        Ok(lib_path)
    }

    /// Lift an ELF file to C source code (without compilation).
    pub fn lift(&self, elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
        // Load ELF
        let data = std::fs::read(elf_path)?;
        let image = ElfImage::<X>::parse(&data)?;

        // Create output directory if it doesn't exist
        std::fs::create_dir_all(output_dir)?;

        // Build pipeline with syscall handler selection.
        let registry = match self.syscall_mode {
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

        // Build CFG (InstructionTable → BlockTable → optimizations)
        pipeline.build_cfg()?;

        // Lift to IR
        pipeline.lift_to_ir()?;

        // Emit C code
        let base_name = output_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rv");
        pipeline.emit_c(output_dir, base_name)?;

        // Return path to main C file
        let c_path = output_dir.join(format!("{}_part0.c", base_name));
        Ok(c_path)
    }
}

/// Options for compile/lift operations.
#[derive(Clone, Debug, Default)]
pub struct CompileOptions {
    /// Enable address bounds checking.
    pub addr_check: bool,
    /// Enable tohost check (for riscv-tests).
    pub tohost: bool,
    /// Instruction retirement mode.
    pub instret_mode: InstretMode,
    /// Number of parallel compile jobs (0 = auto-detect based on CPU count).
    pub jobs: usize,
    /// Tracer configuration.
    pub tracer_config: TracerConfig,
    /// Syscall handling mode.
    pub syscall_mode: SyscallMode,
    /// Optional C compiler override (e.g. "clang").
    pub compiler: Option<String>,
    /// Suppress compilation output (make commands, etc).
    pub quiet: bool,
}

impl CompileOptions {
    /// Create default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set address checking.
    pub fn with_addr_check(mut self, enabled: bool) -> Self {
        self.addr_check = enabled;
        self
    }

    /// Set tohost enabled.
    pub fn with_tohost(mut self, enabled: bool) -> Self {
        self.tohost = enabled;
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

    /// Set the C compiler to use (e.g. "clang" or "gcc").
    pub fn with_compiler(mut self, compiler: impl Into<String>) -> Self {
        self.compiler = Some(compiler.into());
        self
    }

    /// Suppress compilation output.
    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Apply options to EmitConfig.
    fn apply<X: Xlen>(&self, config: &mut EmitConfig<X>) {
        config.addr_check = self.addr_check;
        config.tohost_enabled = self.tohost;
        config.instret_mode = self.instret_mode;
        config.tracer_config = self.tracer_config.clone();
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
            let mut recompiler = Recompiler::<Rv32>::new(config)
                .with_syscall_mode(options.syscall_mode)
                .with_quiet(options.quiet);
            if let Some(compiler) = &options.compiler {
                recompiler = recompiler.with_compiler(compiler.clone());
            }
            recompiler.compile(elf_path, output_dir, options.jobs)
        },
        || {
            let mut config = EmitConfig::<Rv64>::default();
            options.apply(&mut config);
            let mut recompiler = Recompiler::<Rv64>::new(config)
                .with_syscall_mode(options.syscall_mode)
                .with_quiet(options.quiet);
            if let Some(compiler) = &options.compiler {
                recompiler = recompiler.with_compiler(compiler.clone());
            }
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
            let mut recompiler =
                Recompiler::<Rv32>::new(config).with_syscall_mode(options.syscall_mode);
            if let Some(compiler) = &options.compiler {
                recompiler = recompiler.with_compiler(compiler.clone());
            }
            recompiler.lift(elf_path, output_dir)
        },
        || {
            let mut config = EmitConfig::<Rv64>::default();
            options.apply(&mut config);
            let mut recompiler =
                Recompiler::<Rv64>::new(config).with_syscall_mode(options.syscall_mode);
            if let Some(compiler) = &options.compiler {
                recompiler = recompiler.with_compiler(compiler.clone());
            }
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
        _ => Err(Error::XlenMismatch {
            expected: 32,
            actual: xlen,
        }),
    }
}

/// Compile C source to shared library.
///
/// If `jobs` is 0, auto-detects based on CPU count.
fn compile_c_to_shared(
    output_dir: &Path,
    jobs: usize,
    compiler: Option<&str>,
    quiet: bool,
) -> Result<()> {
    use std::process::{Command, Stdio};

    let makefile_path = output_dir.join("Makefile");
    if !makefile_path.exists() {
        return Err(Error::CompilationFailed("Makefile not found".to_string()));
    }

    let job_count = if jobs == 0 {
        num_cpus::get().saturating_sub(2).max(1)
    } else {
        jobs
    };

    let mut cmd = Command::new("make");
    cmd.arg("-C")
        .arg(output_dir)
        .arg("-j")
        .arg(job_count.to_string());
    if let Some(cc) = compiler {
        cmd.arg(format!("CC={}", cc));
    }
    cmd.arg("shared");

    if quiet {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = cmd
        .status()
        .map_err(|e| Error::CompilationFailed(format!("Failed to run make: {}", e)))?;

    if !status.success() {
        return Err(Error::CompilationFailed("make failed".to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recompiler_creation() {
        let _recompiler = Recompiler::<Rv64>::with_defaults();
    }
}
