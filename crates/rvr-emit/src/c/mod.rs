//! C code emission backend.
//!
//! Generates C code that can be compiled with clang/gcc.
//! Uses blocks-as-functions with musttail for tail call optimization.

pub mod config;
mod dispatch;
mod emitter;
mod header;
mod htif;
mod memory;
mod project;
mod signature;
mod syscalls;
mod tracer;
mod tracers;

pub use config::*;
pub use dispatch::*;
pub use emitter::*;
pub use header::*;
pub use htif::*;
pub use memory::*;
pub use project::*;
pub use signature::*;
pub use syscalls::*;
pub use tracer::*;
