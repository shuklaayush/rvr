//! x86-64 assembly emission for RISC-V recompiler.
//!
//! Generates Intel-syntax x86-64 assembly that can be compiled with GCC/as.
//! Unlike the C emitter which uses blocks-as-functions, this emits a linear
//! instruction stream with labels for jump targets.
//!
//! # Module Structure
//!
//! - `emitter` - Core emission helpers and register access
//! - `dispatch` - Jump table and dispatch logic
//! - `prologue` - Header, prologue, epilogue, runtime wrapper
//! - `instructions` - RISC-V instruction emission
//! - `ir` - IR translation (expressions, statements, terminators)
//! - `registers` - Register mapping

mod dispatch;
mod emitter;
mod instructions;
mod ir;
mod prologue;
mod registers;

pub use registers::{HOT_REG_SLOTS, RegMap};

use std::collections::HashSet;
use std::path::Path;

use rvr_ir::{InstrIR, Xlen};

use crate::RvStateLayout;
use crate::config::EmitConfig;
use crate::inputs::EmitInputs;

/// x86-64 assembly emitter.
pub struct X86Emitter<X: Xlen> {
    /// Emit configuration.
    pub(self) config: EmitConfig<X>,
    /// Emit inputs (entry point, valid addresses, etc).
    pub(self) inputs: EmitInputs,
    /// Accumulated assembly text.
    pub(self) asm: String,
    /// PCs that need labels (jump targets from branches/jalr).
    pub(self) label_pcs: HashSet<u64>,
    /// `RvState` layout (field offsets).
    pub(self) layout: RvStateLayout,
    /// Register mapping.
    pub(self) reg_map: RegMap,
    /// Memory mask for address translation.
    pub(self) memory_mask: u64,
    /// Counter for generating unique labels.
    pub(self) label_counter: usize,
    /// Cached cold register (RV reg number) stored in `COLD_CACHE`.
    pub(self) cold_cache: Option<u8>,
}

impl<X: Xlen> X86Emitter<X> {
    /// Create a new x86 emitter.
    #[must_use]
    pub fn new(config: EmitConfig<X>, inputs: EmitInputs) -> Self {
        let layout = RvStateLayout::new::<X>(&config);
        let is_rv32 = X::VALUE == 32;
        let reg_map = RegMap::new(&config.hot_regs, is_rv32);
        let memory_mask = (1u64 << config.memory_bits) - 1;

        // Collect all valid PCs as label targets
        let mut label_pcs = inputs.valid_addresses.clone();
        // Also add absorbed->merged targets
        for &merged in inputs.absorbed_to_merged.values() {
            label_pcs.insert(merged);
        }

        Self {
            config,
            inputs,
            asm: String::with_capacity(1024 * 1024), // 1MB initial
            label_pcs,
            layout,
            reg_map,
            memory_mask,
            label_counter: 0,
            cold_cache: None,
        }
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Generate complete assembly file from a linear instruction stream.
    pub fn generate_instructions(&mut self, instrs: &[InstrIR<X>]) {
        self.emit_header();
        self.emit_text_section();
        self.emit_runtime_wrapper();
        self.emit_prologue();
        self.emit_instructions(instrs);
        self.emit_epilogue();
        self.emit_jump_table();
        self.emit_metadata_constants();
    }

    /// Get the accumulated assembly.
    #[must_use]
    pub fn assembly(&self) -> &str {
        &self.asm
    }

    /// Write assembly to a file.
    ///
    /// # Errors
    ///
    /// Returns any I/O error returned by `std::fs::write`.
    pub fn write_asm(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, &self.asm)
    }

    /// Get the layout.
    #[must_use]
    pub const fn layout(&self) -> &RvStateLayout {
        &self.layout
    }

    /// Get the config.
    #[must_use]
    pub const fn config(&self) -> &EmitConfig<X> {
        &self.config
    }

    /// Get the inputs.
    #[must_use]
    pub const fn inputs(&self) -> &EmitInputs {
        &self.inputs
    }

    /// Get the register map.
    #[must_use]
    pub const fn reg_map(&self) -> &RegMap {
        &self.reg_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    fn test_inputs() -> EmitInputs {
        EmitInputs {
            entry_point: 0x8000_0000,
            text_start: 0x8000_0000,
            pc_end: 0x8000_0100,
            valid_addresses: [0x8000_0000u64, 0x8000_0004, 0x8000_0008]
                .into_iter()
                .collect(),
            absorbed_to_merged: std::collections::HashMap::new(),
            initial_brk: 0x8000_1000,
        }
    }

    #[test]
    fn test_emitter_creation() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = X86Emitter::new(config, test_inputs());
        assert!(emitter.assembly().is_empty());
    }

    #[test]
    fn test_emit_header() {
        let config = EmitConfig::<Rv64>::default();
        let mut emitter = X86Emitter::new(config, test_inputs());
        emitter.emit_header();
        let asm = emitter.assembly();
        // AT&T syntax - no .intel_syntax directive
        assert!(asm.contains(".code64"));
        assert!(asm.contains("PC_OFFSET"));
    }

    #[test]
    fn test_emit_prologue() {
        let config = EmitConfig::<Rv64>::default();
        let mut emitter = X86Emitter::new(config, test_inputs());
        emitter.emit_prologue();
        let asm = emitter.assembly();
        assert!(asm.contains("asm_run:"));
        // AT&T syntax uses %reg and reversed operand order
        assert!(asm.contains("pushq %rbp"));
        assert!(asm.contains("movq %rdi, %rbx"));
    }

    #[test]
    fn test_emit_epilogue() {
        let config = EmitConfig::<Rv64>::default();
        let mut emitter = X86Emitter::new(config, test_inputs());
        emitter.emit_epilogue();
        let asm = emitter.assembly();
        assert!(asm.contains("asm_exit:"));
        assert!(asm.contains("ret"));
        assert!(asm.contains("asm_trap:"));
    }

    #[test]
    fn test_emit_jump_table() {
        let config = EmitConfig::<Rv64>::default();
        let mut emitter = X86Emitter::new(config, test_inputs());
        emitter.emit_jump_table();
        let asm = emitter.assembly();
        assert!(asm.contains("jump_table:"));
        assert!(asm.contains(".long asm_pc_80000000"));
    }

    #[test]
    fn test_emit_add() {
        let config = EmitConfig::<Rv64>::default();
        let mut emitter = X86Emitter::new(config, test_inputs());
        emitter.emit_add(10, 11, 12);
        let asm = emitter.assembly();
        assert!(asm.contains("add") || asm.contains("mov"));
    }

    #[test]
    fn test_layout() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = X86Emitter::new(config, test_inputs());
        let layout = emitter.layout();
        assert_eq!(layout.reg_offset(0), 0);
        assert_eq!(layout.reg_offset(1), 8);
    }

    #[test]
    fn test_full_generation() {
        let config = EmitConfig::<Rv64>::default();
        let mut emitter = X86Emitter::new(config, test_inputs());
        emitter.generate_instructions(&[]);
        let asm = emitter.assembly();

        // AT&T syntax (no intel directive)
        assert!(asm.contains(".code64"));
        assert!(asm.contains("asm_run:"));
        assert!(asm.contains("asm_exit:"));
        assert!(asm.contains("asm_trap:"));
        assert!(asm.contains("jump_table:"));

        eprintln!("\n=== Generated Assembly ===\n{asm}");
    }
}
