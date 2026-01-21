//! ELF parser for RISC-V binaries.

mod constants;
pub mod debug;
mod file;
mod header;
mod image;

pub use constants::*;
pub use debug::DebugInfo;
pub use file::*;
pub use header::*;
pub use image::*;

use thiserror::Error;

/// ELF parsing errors.
#[derive(Error, Debug)]
pub enum ElfError {
    #[error("ELF data too small")]
    TooSmall,
    #[error("Invalid ELF magic number")]
    InvalidMagic,
    #[error("Only little-endian ELF supported")]
    NotLittleEndian,
    #[error("ELF XLEN mismatch: expected {expected}, got {actual}")]
    XlenMismatch { expected: u8, actual: u8 },
    #[error("Unsupported ELF class: {0}")]
    UnsupportedClass(u8),
    #[error("Section header out of bounds")]
    SectionOutOfBounds,
    #[error("Program header out of bounds")]
    ProgramOutOfBounds,
    #[error("Segment extends beyond file")]
    SegmentBeyondFile,
    #[error("Virtual address overflow")]
    VirtualAddressOverflow,
    #[error("No loadable segments found")]
    NoLoadableSegments,
    #[error("Too many loadable segments")]
    TooManySegments,
    #[error("Overlapping virtual address ranges")]
    OverlappingSegments,
}

pub type Result<T> = std::result::Result<T, ElfError>;
