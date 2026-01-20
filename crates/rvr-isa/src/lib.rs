//! RISC-V instruction set definitions and decoder.
//!
//! This crate provides instruction decoding, lifting to IR, and disassembly
//! for RISC-V extensions. Each extension (I, M, A, C, Zicsr) is self-contained
//! in its own module under `extensions/`.

mod encode;
pub mod extensions;
pub mod syscalls;
mod types;

pub use encode::*;
pub use extensions::*;
pub use types::*;

/// Decode an instruction using the standard RISC-V extensions.
///
/// This is a convenience wrapper around `CompositeDecoder::standard().decode()`.
pub fn decode<X: Xlen>(bytes: &[u8], pc: X::Reg) -> Option<DecodedInstr<X>> {
    CompositeDecoder::<X>::standard().decode(bytes, pc)
}
