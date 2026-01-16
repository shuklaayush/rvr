//! C code emission for RISC-V recompiler.

mod config;
mod tracer;

pub use config::*;
pub use tracer::*;

use rvr_isa::Xlen;

/// C code emitter.
pub struct CEmitter<X: Xlen> {
    config: EmitConfig<X>,
}

impl<X: Xlen> CEmitter<X> {
    pub fn new(config: EmitConfig<X>) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &EmitConfig<X> {
        &self.config
    }
}
