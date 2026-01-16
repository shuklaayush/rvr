//! RVR - RISC-V Recompiler
//!
//! Compiles RISC-V ELF binaries to optimized C code, then to native shared libraries.
//!
//! # Example
//!
//! ```ignore
//! use rvr::{Recompiler, Rv64, EmitConfig};
//!
//! let recompiler = Recompiler::<Rv64>::new(EmitConfig::default());
//! let compiled = recompiler.compile("program.elf", "output/")?;
//! ```

// Re-export from sub-crates
pub use rvr_elf::{ElfError, ElfImage, MemorySegment, get_elf_xlen};
pub use rvr_isa::{
    OpId, Xlen, Rv32, Rv64,
    EXT_I, EXT_M, EXT_A, EXT_C, EXT_ZICSR, EXT_ZIFENCEI, EXT_CUSTOM,
    NUM_REGS_I, NUM_REGS_E, NUM_CSRS,
    DecodedInstr, InstrArgs, decode,
    CompositeDecoder, InstructionExtension,
};
pub use rvr_ir::{
    Expr, ExprKind, Space, Stmt, Terminator, BranchHint,
    InstrIR, BlockIR, IRBuilder,
};
pub use rvr_cfg::{BlockTable, CfgAnalyzer, CfgResult, CodeView};
pub use rvr_emit::{EmitConfig, InstretMode, TracerConfig, TracerKind, CEmitter};

mod pipeline;
pub use pipeline::*;

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
    pub fn compile(&self, elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
        // First lift to C source
        let _c_path = self.lift(elf_path, output_dir)?;

        // Then compile C to .so
        compile_c_to_shared(output_dir)?;

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

        // Run CFG analysis
        pipeline.analyze_cfg();

        // Lift to IR
        pipeline.lift_to_ir();

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

/// Compile an ELF file, auto-detecting XLEN from the ELF header.
pub fn compile(elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
    let data = std::fs::read(elf_path)?;
    let xlen = get_elf_xlen(&data)?;

    match xlen {
        32 => {
            let recompiler = Recompiler::<Rv32>::with_defaults();
            recompiler.compile(elf_path, output_dir)
        }
        64 => {
            let recompiler = Recompiler::<Rv64>::with_defaults();
            recompiler.compile(elf_path, output_dir)
        }
        _ => Err(Error::XlenMismatch {
            expected: 32,
            actual: xlen,
        }),
    }
}

/// Lift an ELF file to C source code, auto-detecting XLEN.
pub fn lift_to_c(elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
    let data = std::fs::read(elf_path)?;
    let xlen = get_elf_xlen(&data)?;

    match xlen {
        32 => {
            let recompiler = Recompiler::<Rv32>::with_defaults();
            recompiler.lift(elf_path, output_dir)
        }
        64 => {
            let recompiler = Recompiler::<Rv64>::with_defaults();
            recompiler.lift(elf_path, output_dir)
        }
        _ => Err(Error::XlenMismatch {
            expected: 32,
            actual: xlen,
        }),
    }
}

/// Compile C source to shared library.
fn compile_c_to_shared(output_dir: &Path) -> Result<()> {
    use std::process::Command;

    let makefile_path = output_dir.join("Makefile");
    if !makefile_path.exists() {
        return Err(Error::CompilationFailed("Makefile not found".to_string()));
    }

    let status = Command::new("make")
        .arg("-C")
        .arg(output_dir)
        .arg("-j")
        .arg(num_cpus::get().saturating_sub(2).max(1).to_string())
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
