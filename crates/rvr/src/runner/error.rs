//! Runner error types.

use thiserror::Error;

/// Runner error type.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("failed to load library: {0}")]
    LoadError(#[from] libloading::Error),

    #[error("shared library not found: {0}")]
    LibraryNotFound(String),

    #[error("ELF file not found: {0}")]
    ElfNotFound(String),

    #[error("failed to find symbol '{0}': {1}")]
    SymbolNotFound(String, libloading::Error),

    #[error("function not found: {0}")]
    FunctionNotFound(String),

    #[error("memory allocation failed: {0}")]
    MemoryAllocationFailed(#[from] rvr_state::MemoryError),

    #[error("ELF parsing failed: {0}")]
    ElfError(#[from] rvr_elf::ElfError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("execution error: exit code {0}")]
    ExecutionError(u8),

    #[error("tracer setup failed: {0}")]
    TracerSetupFailed(String),

    #[error("state file error: {0}")]
    StateError(String),
}
