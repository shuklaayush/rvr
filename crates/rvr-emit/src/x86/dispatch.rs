//! Jump table and dispatch logic for x86-64 assembly (AT&T syntax).

use rvr_ir::Xlen;

use super::X86Emitter;
use super::registers::reserved;

impl<X: Xlen> X86Emitter<X> {
    /// Emit a jump via the dispatch table.
    /// Assumes the target address is already in rax (properly masked/cleared).
    pub(super) fn emit_dispatch_jump(&mut self) {
        let text_start = self.inputs.text_start;
        let text_size = self.inputs.pc_end - text_start;

        // Range check: trap if target < text_start or target >= pc_end
        // This prevents out-of-bounds jump table access (e.g., ra=0 on return)
        let text_start32 = u32::try_from(text_start).expect("text start fits in u32");
        let text_size32 = u32::try_from(text_size).expect("text size fits in u32");
        if X::VALUE == 32 {
            // RV32: check 32-bit range
            self.emitf(format!("subl $0x{text_start32:x}, %eax"));
            self.emitf(format!("cmpl $0x{text_size32:x}, %eax"));
            self.emit("jae asm_trap"); // unsigned >= text_size means out of range
            self.emit("shrl $1, %eax");
        } else {
            // RV64: check 64-bit range
            self.emitf(format!("movl $0x{text_start32:x}, %edx"));
            self.emit("subq %rdx, %rax");
            self.emitf(format!("movl $0x{text_size32:x}, %edx"));
            self.emit("cmpq %rdx, %rax");
            self.emit("jae asm_trap"); // unsigned >= text_size means out of range
            self.emit("shrq $1, %rax");
        }

        // Load jump table base
        self.emit("leaq jump_table(%rip), %rcx");

        // Load offset from jump table and add base
        self.emit("movslq (%rcx,%rax,4), %rax");
        self.emit("addq %rcx, %rax");
        self.emit("jmp *%rax");
    }

    /// Load PC from state into rax for dispatch.
    pub(super) fn emit_load_pc_for_dispatch(&mut self) {
        let pc_offset = self.layout.offset_pc;
        if X::VALUE == 32 {
            // RV32: load 32-bit PC into eax (zero-extends to rax)
            self.emitf(format!(
                "movl {}(%{}), %eax",
                pc_offset,
                reserved::STATE_PTR
            ));
        } else {
            // RV64: load 64-bit PC into rax
            self.emitf(format!(
                "movq {}(%{}), %rax",
                pc_offset,
                reserved::STATE_PTR
            ));
        }
    }

    /// Emit the jump table in .rodata section.
    pub fn emit_jump_table(&mut self) {
        self.emit_raw(".section .rodata");
        self.emit_raw(".align 4");
        self.emit_label("jump_table");

        let text_start = self.inputs.text_start;
        let pc_end = self.inputs.pc_end;

        // Generate entries for all 2-byte slots
        let mut pc = text_start;
        while pc < pc_end {
            let target = if self.inputs.valid_addresses.contains(&pc) {
                format!("asm_pc_{pc:x} - jump_table")
            } else if let Some(&merged) = self.inputs.absorbed_to_merged.get(&pc) {
                format!("asm_pc_{merged:x} - jump_table")
            } else {
                "asm_trap - jump_table".to_string()
            };
            self.emitf(format!(".long {target}"));
            pc += 2; // 2-byte slots for compressed instruction support
        }
        self.emit_blank();
    }
}
