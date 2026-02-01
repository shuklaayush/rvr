//! Jump table and dispatch logic for ARM64 assembly.

use rvr_ir::Xlen;

use super::Arm64Emitter;
use super::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
    /// Emit a jump via the dispatch table.
    /// Assumes the target address is already in x0 (properly masked/cleared).
    pub(super) fn emit_dispatch_jump(&mut self) {
        // Load jump table base address
        self.emit("adrp x1, jump_table");
        self.emit("add x1, x1, :lo12:jump_table");

        let text_start = self.inputs.text_start;

        if X::VALUE == 32 {
            // RV32: PC is 32-bit
            self.load_imm("w2", text_start);
            self.emit("sub w0, w0, w2");
            self.emit("lsr w0, w0, #1"); // 2-byte slots for compressed support
        } else {
            // RV64: PC is 64-bit, compute index in 64-bit then truncate
            self.load_imm("x2", text_start);
            self.emit("sub x0, x0, x2");
            self.emit("lsr x0, x0, #1"); // Keep 64-bit until table lookup
        }

        // Load offset from jump table (32-bit relative offset)
        // x1 = jump_table base, w0 = index
        // Entry = jump_table[index] = 32-bit signed offset from jump_table
        self.emit("ldr w0, [x1, w0, uxtw #2]"); // w0 = *(jump_table + index * 4)

        // Add base to get absolute address
        self.emit("add x0, x1, w0, sxtw"); // x0 = jump_table + sign_extend(offset)

        // Jump to target
        self.emit("br x0");
    }

    /// Load PC from state into x0 for dispatch.
    pub(super) fn emit_load_pc_for_dispatch(&mut self) {
        let pc_offset = self.layout.offset_pc;
        if X::VALUE == 32 {
            // RV32: load 32-bit PC (zero-extends to x0)
            self.emitf(format!("ldr w0, [{}, #{}]", reserved::STATE_PTR, pc_offset));
        } else {
            // RV64: load 64-bit PC
            self.emitf(format!("ldr x0, [{}, #{}]", reserved::STATE_PTR, pc_offset));
        }
    }

    /// Emit the jump table in .rodata section.
    pub fn emit_jump_table(&mut self) {
        self.emit_raw(".section .rodata");
        self.emit_raw(".balign 4");
        self.emit_label("jump_table");

        let text_start = self.inputs.text_start;
        let pc_end = self.inputs.pc_end;

        // Generate entries for all 2-byte slots (for compressed instruction support)
        let mut pc = text_start;
        while pc < pc_end {
            let target = if self.inputs.valid_addresses.contains(&pc) {
                format!("asm_pc_{:x} - jump_table", pc)
            } else if let Some(&merged) = self.inputs.absorbed_to_merged.get(&pc) {
                format!("asm_pc_{:x} - jump_table", merged)
            } else {
                "asm_trap - jump_table".to_string()
            };
            self.emitf(format!(".word {target}"));
            pc += 2; // 2-byte slots for compressed instruction support
        }
        self.emit_blank();
    }
}
