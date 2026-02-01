//! Assembly structure: header, prologue, epilogue, runtime wrapper (AT&T syntax).

use rvr_ir::Xlen;

use super::X86Emitter;
use super::registers::reserved;

impl<X: Xlen> X86Emitter<X> {
    /// Emit the assembly file header.
    pub fn emit_header(&mut self) {
        // AT&T syntax is the default, no directive needed
        self.emit_raw(".code64");
        self.emit_blank();
        self.emit_raw("# Constants");
        self.emitf(format!(".set PC_OFFSET, {}", self.layout.offset_pc));
        self.emitf(format!(
            ".set INSTRET_OFFSET, {}",
            self.layout.offset_instret
        ));
        self.emitf(format!(
            ".set TARGET_INSTRET_OFFSET, {}",
            self.layout.offset_target_instret
        ));
        self.emitf(format!(
            ".set HAS_EXITED_OFFSET, {}",
            self.layout.offset_has_exited
        ));
        self.emitf(format!(
            ".set EXIT_CODE_OFFSET, {}",
            self.layout.offset_exit_code
        ));
        self.emitf(format!(".set MEMORY_OFFSET, {}", self.layout.offset_memory));

        // Fixed address constants if enabled
        if let Some(fixed) = self.config.fixed_addresses {
            self.emitf(format!(".set RV_STATE_ADDR, 0x{:x}", fixed.state_addr));
            self.emitf(format!(".set RV_MEMORY_ADDR, 0x{:x}", fixed.memory_addr));
        }

        self.emit_blank();
    }

    /// Emit the .text section with asm_run entry point.
    pub fn emit_text_section(&mut self) {
        self.emit_raw(".section .text");
        self.emit_raw(".global asm_run");
        self.emit_raw(".type asm_run, @function");
        self.emit_blank();
    }

    /// Emit the function prologue.
    /// Arguments: rdi = RvState*, rsi = memory base (unless fixed addresses)
    pub fn emit_prologue(&mut self) {
        self.emit_label("asm_run");
        self.emit_comment("Save callee-saved registers");
        self.emit("pushq %rbp");
        self.emit("pushq %rbx");
        self.emit("pushq %r12");
        self.emit("pushq %r13");
        self.emit("pushq %r14");
        self.emit("pushq %r15");
        let stack_bytes = 8 + X86Emitter::<X>::TEMP_STACK_BYTES;
        self.emit_comment("Align stack to 16 bytes and reserve temp slots");
        self.emitf(format!("subq ${}, %rsp", stack_bytes));
        self.emit_blank();

        self.emit_comment("Setup pointers");
        if self.config.fixed_addresses.is_some() {
            self.emit_comment("Fixed address mode");
            self.emitf(format!("movabsq $RV_STATE_ADDR, %{}", reserved::STATE_PTR));
            self.emitf(format!(
                "movabsq $RV_MEMORY_ADDR, %{}",
                reserved::MEMORY_PTR
            ));
        } else {
            // Normal mode: state and memory passed as arguments
            self.emitf(format!("movq %rdi, %{}", reserved::STATE_PTR)); // rbx = state
            self.emitf(format!("movq %rsi, %{}", reserved::MEMORY_PTR)); // r15 = memory
        }
        self.emit_blank();

        self.emit_comment("Load hot registers from state");
        let hot_regs: Vec<_> = self.reg_map.hot_regs().collect();
        let suffix = self.suffix();
        for (rv_reg, x86_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "mov{suffix} {offset}(%{}), %{x86_reg}",
                reserved::STATE_PTR
            ));
        }
        self.emit_blank();

        self.emit_comment("Jump to starting PC via dispatch table");
        self.emit_load_pc_for_dispatch();
        self.emit_dispatch_jump();
        self.emit_blank();
    }

    /// Emit the function epilogue.
    pub fn emit_epilogue(&mut self) {
        self.emit_label("asm_exit");
        self.emit_comment("Save hot registers back to state");
        let hot_regs: Vec<_> = self.reg_map.hot_regs().collect();
        let suffix = self.suffix();
        for (rv_reg, x86_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "mov{suffix} %{x86_reg}, {offset}(%{})",
                reserved::STATE_PTR
            ));
        }
        self.emit_blank();

        self.emit_comment("Restore stack and callee-saved registers");
        let stack_bytes = 8 + X86Emitter::<X>::TEMP_STACK_BYTES;
        self.emitf(format!("addq ${}, %rsp", stack_bytes));
        self.emit("popq %r15");
        self.emit("popq %r14");
        self.emit("popq %r13");
        self.emit("popq %r12");
        self.emit("popq %rbx");
        self.emit("popq %rbp");
        self.emit("ret");
        self.emit_blank();

        self.emit_label("asm_trap");
        self.emit_comment("Trap handler - set exit flag and exit (exit_code stays 0)");
        let has_exited = self.layout.offset_has_exited;
        self.emitf(format!("movb $1, {}(%{})", has_exited, reserved::STATE_PTR));
        self.emit("jmp asm_exit");
        self.emit_blank();
    }

    /// Emit rv_execute_from wrapper that the runner expects.
    pub(super) fn emit_runtime_wrapper(&mut self) {
        self.emit_raw(".global rv_execute_from");
        self.emit_raw(".type rv_execute_from, @function");
        self.emit_label("rv_execute_from");
        self.emit_comment("rv_execute_from(RvState* state, uint64_t start_pc)");
        self.emit_comment("  rdi = state, rsi = start_pc");
        self.emit_blank();

        // Store start_pc to state->pc
        let pc_offset = self.layout.offset_pc;
        if X::VALUE == 32 {
            self.emitf(format!("movl %esi, {}(%rdi)", pc_offset));
        } else {
            self.emitf(format!("movq %rsi, {}(%rdi)", pc_offset));
        }

        // Load memory pointer from state for the asm_run call (non-fixed mode)
        if self.config.fixed_addresses.is_none() {
            let memory_offset = self.layout.offset_memory;
            self.emit_comment("Load memory pointer from state");
            self.emitf(format!("movq {}(%rdi), %rsi", memory_offset));
        }

        // Call asm_run
        self.emit("call asm_run");

        // Return has_exited
        let has_exited_offset = self.layout.offset_has_exited;
        self.emitf(format!("movzbl {}(%rdi), %eax", has_exited_offset));
        self.emit("ret");
        self.emit_blank();
    }

    /// Emit metadata constants required by the runner.
    pub(super) fn emit_metadata_constants(&mut self) {
        self.emit_raw(".section .rodata");
        self.emit_blank();

        // RV_TRACER_KIND
        use crate::c::TracerKind;
        let tracer_kind = self
            .config
            .tracer_config
            .builtin_kind()
            .unwrap_or(TracerKind::None)
            .as_c_kind();
        self.emit_raw(".global RV_TRACER_KIND");
        self.emit_label("RV_TRACER_KIND");
        self.emitf(format!(".long {}", tracer_kind));
        self.emit_blank();

        // RV_EXPORT_FUNCTIONS
        let export_functions = if self.config.export_functions {
            1u32
        } else {
            0
        };
        self.emit_raw(".global RV_EXPORT_FUNCTIONS");
        self.emit_label("RV_EXPORT_FUNCTIONS");
        self.emitf(format!(".long {}", export_functions));
        self.emit_blank();

        // RV_INSTRET_MODE
        let instret_mode = self.config.instret_mode.as_c_mode();
        self.emit_raw(".global RV_INSTRET_MODE");
        self.emit_label("RV_INSTRET_MODE");
        self.emitf(format!(".long {}", instret_mode));
        self.emit_blank();

        // Fixed addresses (if enabled)
        if let Some(fixed) = self.config.fixed_addresses {
            self.emit_raw(".global RV_FIXED_STATE_ADDR");
            self.emit_label("RV_FIXED_STATE_ADDR");
            self.emitf(format!(".quad 0x{:x}", fixed.state_addr));
            self.emit_blank();

            self.emit_raw(".global RV_FIXED_MEMORY_ADDR");
            self.emit_label("RV_FIXED_MEMORY_ADDR");
            self.emitf(format!(".quad 0x{:x}", fixed.memory_addr));
            self.emit_blank();
        }
    }
}
