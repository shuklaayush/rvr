//! RVR - RISC-V Recompiler
//!
//! Compiles RISC-V ELF binaries to optimized C code, then to native shared libraries.
//!
//! # Example
//!
//! ```ignore
//! use rvr::{Recompiler, Rv64};
//!
//! let recompiler = Recompiler::<Rv64>::with_defaults();
//! let compiled = recompiler.compile("program.elf", "output/")?;
//! ```
//!
//! # API Overview
//!
//! The main entry point is [`Recompiler`], which provides high-level methods:
//! - [`lift`](Recompiler::lift): ELF → C code
//! - [`compile`](Recompiler::compile): ELF → C code → native shared library
//!
//! For lower-level control, use [`Pipeline`] directly with custom [`EmitConfig`].
//!
//! # Re-exports
//!
//! This crate re-exports commonly needed types. For advanced usage, access the
//! underlying crates directly: `rvr_elf`, `rvr_isa`, `rvr_ir`, `rvr_cfg`, `rvr_emit`.

// Core types - always available
pub use rvr_elf::{ElfImage, get_elf_xlen};
pub use rvr_isa::{Xlen, Rv32, Rv64};
pub use rvr_emit::{EmitConfig, InstretMode, TracerConfig};

mod pipeline;
pub use pipeline::{Pipeline, PipelineStats};

use std::marker::PhantomData;
use std::path::Path;

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

/// RISC-V recompiler.
pub struct Recompiler<X: Xlen> {
    config: EmitConfig<X>,
    _marker: PhantomData<X>,
}

impl<X: Xlen> Recompiler<X> {
    /// Create a new recompiler with the given configuration.
    pub fn new(config: EmitConfig<X>) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }

    /// Create a recompiler with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(EmitConfig::default())
    }

    /// Get the configuration.
    pub fn config(&self) -> &EmitConfig<X> {
        &self.config
    }

    /// Compile an ELF file to a shared library.
    ///
    /// If `jobs` is 0, auto-detects based on CPU count.
    pub fn compile(&self, elf_path: &Path, output_dir: &Path, jobs: usize) -> Result<std::path::PathBuf> {
        // First lift to C source
        let _c_path = self.lift(elf_path, output_dir)?;

        // Then compile C to .so
        compile_c_to_shared(output_dir, jobs)?;

        // Return the path to the shared library
        let lib_name = output_dir.file_name()
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

        // Build pipeline
        let mut pipeline = Pipeline::<X>::new(image, self.config.clone());

        // Build CFG (InstructionTable → BlockTable → optimizations)
        pipeline.build_cfg()?;

        // Lift to IR
        pipeline.lift_to_ir()?;

        // Emit C code
        let base_name = output_dir.file_name()
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

    dispatch_by_xlen(xlen, || {
        let mut config = EmitConfig::<Rv32>::default();
        options.apply(&mut config);
        let recompiler = Recompiler::<Rv32>::new(config);
        recompiler.compile(elf_path, output_dir, options.jobs)
    }, || {
        let mut config = EmitConfig::<Rv64>::default();
        options.apply(&mut config);
        let recompiler = Recompiler::<Rv64>::new(config);
        recompiler.compile(elf_path, output_dir, options.jobs)
    })
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

    dispatch_by_xlen(xlen, || {
        let mut config = EmitConfig::<Rv32>::default();
        options.apply(&mut config);
        let recompiler = Recompiler::<Rv32>::new(config);
        recompiler.lift(elf_path, output_dir)
    }, || {
        let mut config = EmitConfig::<Rv64>::default();
        options.apply(&mut config);
        let recompiler = Recompiler::<Rv64>::new(config);
        recompiler.lift(elf_path, output_dir)
    })
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
fn compile_c_to_shared(output_dir: &Path, jobs: usize) -> Result<()> {
    use std::process::Command;

    let makefile_path = output_dir.join("Makefile");
    if !makefile_path.exists() {
        return Err(Error::CompilationFailed("Makefile not found".to_string()));
    }

    let job_count = if jobs == 0 {
        num_cpus::get().saturating_sub(2).max(1)
    } else {
        jobs
    };

    let status = Command::new("make")
        .arg("-C")
        .arg(output_dir)
        .arg("-j")
        .arg(job_count.to_string())
        .arg("shared")
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
