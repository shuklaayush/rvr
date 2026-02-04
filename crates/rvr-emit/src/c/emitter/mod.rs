//! C code emitter for RISC-V recompiler.
//!
//! Generates C code from RISC-V IR blocks with:
//! - Explicit musttail calls for all control flow (including fall-through)
//! - Branch emits both taken and not-taken paths
//! - `save_to_state` on all exit paths
//! - Optional tracing hooks (`trace_block`, `trace_pc`, `trace_branch`_*)
//! - Optional tohost handling for riscv-tests

use rvr_ir::Xlen;

use super::signature::{FnSignature, state_ref};
use crate::config::EmitConfig;
use crate::inputs::EmitInputs;

/// C code emitter.
pub struct CEmitter<X: Xlen> {
    pub config: EmitConfig<X>,
    pub inputs: EmitInputs,
    /// Function signature for block functions.
    pub sig: FnSignature,
    /// Output buffer.
    pub out: String,
    /// Register type name ("`uint32_t`" or "`uint64_t`").
    reg_type: &'static str,
    /// Signed register type ("`int32_t`" or "`int64_t`").
    signed_type: &'static str,
    /// Current instruction PC.
    current_pc: u64,
    /// Current instruction op (packed `OpId` for tracing).
    current_op: u16,
    /// Current instruction raw bytes (for spike tracer).
    current_raw: u32,
    /// Instruction index within block (for instret).
    instr_idx: usize,
}

impl<X: Xlen> CEmitter<X> {
    /// Create a new emitter.
    #[must_use]
    pub fn new(config: EmitConfig<X>, inputs: EmitInputs) -> Self {
        let (reg_type, signed_type) = if X::VALUE == 64 {
            ("uint64_t", "int64_t")
        } else {
            ("uint32_t", "int32_t")
        };
        let sig = FnSignature::new(&config);

        Self {
            config,
            inputs,
            sig,
            out: String::with_capacity(4096),
            reg_type,
            signed_type,
            current_pc: 0,
            current_op: 0,
            current_raw: 0,
            instr_idx: 0,
        }
    }

    /// Reset output buffer.
    pub fn reset(&mut self) {
        self.out.clear();
        self.current_pc = 0;
        self.current_op = 0;
        self.current_raw = 0;
        self.instr_idx = 0;
    }

    /// Get output string.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.out
    }

    /// Take output string, consuming the emitter.
    #[must_use]
    pub fn take_output(self) -> String {
        self.out
    }

    /// Check if address is valid.
    pub(super) fn is_valid_address(&self, addr: u64) -> bool {
        self.inputs.is_valid_address(addr)
    }

    /// Format address as hex.
    pub(super) fn fmt_addr(addr: u64) -> String {
        if X::VALUE == 64 {
            format!("0x{addr:016x}ULL")
        } else {
            format!("0x{addr:08x}u")
        }
    }

    /// Format PC for comments (no suffix, lowercase hex).
    pub(super) fn fmt_pc_comment(pc: u64) -> String {
        format!("0x{pc:x}")
    }

    /// Write indented line.
    pub(super) fn writeln(&mut self, indent: usize, s: &str) {
        for _ in 0..indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    /// Write without indent.
    pub(super) fn write(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// Get state reference expression.
    pub(super) const fn state_ref(&self) -> &'static str {
        state_ref(self.sig.fixed_addresses)
    }

    /// Check if using fixed addresses (no memory parameter).
    pub(super) const fn uses_fixed_addresses(&self) -> bool {
        self.sig.fixed_addresses
    }
}

mod block;
mod expr;
mod terminator;

#[cfg(test)]
mod tests;
