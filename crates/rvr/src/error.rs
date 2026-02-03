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
