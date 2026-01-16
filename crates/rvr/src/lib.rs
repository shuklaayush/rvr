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
};
pub use rvr_ir::{
    Expr, ExprKind, Space, Stmt, Terminator, BranchHint,
    InstrIR, BlockIR, IRBuilder,
};
pub use rvr_cfg::BlockTable;
pub use rvr_emit::{EmitConfig, InstretMode, TracerConfig, TracerKind, CEmitter};

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
    pub fn compile(&self, _elf_path: &Path, _output_dir: &Path) -> Result<std::path::PathBuf> {
        // TODO: Implement full compilation pipeline
        todo!("Compilation pipeline not yet implemented")
    }

    /// Lift an ELF file to C source code (without compilation).
    pub fn lift(&self, _elf_path: &Path, _output_dir: &Path) -> Result<std::path::PathBuf> {
        // TODO: Implement lifting pipeline
        todo!("Lifting pipeline not yet implemented")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recompiler_creation() {
        let _recompiler = Recompiler::<Rv64>::with_defaults();
    }
}
