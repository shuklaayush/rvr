//! Assembly structure: header, prologue, epilogue, runtime wrapper for ARM64.

use rvr_ir::Xlen;

use super::Arm64Emitter;
use super::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
    /// Emit the assembly file header.
    pub fn emit_header(&mut self) {
        self.emit_raw(".arch armv8-a");
        self.emit_blank();
        self.emit_raw("// Constants");
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
        self.emit_raw(".type asm_run, %function");
        self.emit_blank();
    }

    /// Emit the function prologue.
    /// Arguments: x0 = RvState*, x1 = memory base (unless fixed addresses)
    pub fn emit_prologue(&mut self) {
        self.emit_label("asm_run");
        self.emit_comment("Save callee-saved registers");
        // Save frame pointer and link register
        self.emit("stp x29, x30, [sp, #-16]!");
        // Save x19-x28 (callee-saved)
        self.emit("stp x19, x20, [sp, #-16]!");
        self.emit("stp x21, x22, [sp, #-16]!");
        self.emit("stp x23, x24, [sp, #-16]!");
        self.emit("stp x25, x26, [sp, #-16]!");
        self.emit("stp x27, x28, [sp, #-16]!");
        // Also save x3-x18 since we use them for hot registers
        self.emit("stp x3, x4, [sp, #-16]!");
        self.emit("stp x5, x6, [sp, #-16]!");
        self.emit("stp x7, x8, [sp, #-16]!");
        self.emit("stp x9, x10, [sp, #-16]!");
        self.emit("stp x11, x12, [sp, #-16]!");
        self.emit("stp x13, x14, [sp, #-16]!");
        self.emit("stp x15, x16, [sp, #-16]!");
        self.emit("stp x17, x18, [sp, #-16]!");
        if Arm64Emitter::<X>::TEMP_STACK_BYTES > 0 {
            self.emitf(format!(
                "sub sp, sp, #{}",
                Arm64Emitter::<X>::TEMP_STACK_BYTES
            ));
        }
        self.emit_blank();

        self.emit_comment("Setup pointers");
        if let Some(fixed) = self.config.fixed_addresses {
            self.emit_comment("Fixed address mode");
            // Use load_imm for constant addresses (adrp/add only works with symbols)
            self.load_imm("x19", fixed.state_addr);
            self.load_imm("x20", fixed.memory_addr);
        } else {
            // Normal mode: state and memory passed as arguments
            self.emitf(format!("mov {}, x0", reserved::STATE_PTR)); // x19 = state
            self.emitf(format!("mov {}, x1", reserved::MEMORY_PTR)); // x20 = memory
        }
        self.emit_blank();

        self.emit_comment("Load hot registers from state");
        let hot_regs: Vec<_> = self.reg_map.hot_regs().collect();
        for (rv_reg, arm_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "ldr {arm_reg}, [{}, #{}]",
                reserved::STATE_PTR,
                offset
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
        for (rv_reg, arm_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "str {arm_reg}, [{}, #{}]",
                reserved::STATE_PTR,
                offset
            ));
        }
        self.emit_blank();

        self.emit_comment("Restore callee-saved registers");
        if Arm64Emitter::<X>::TEMP_STACK_BYTES > 0 {
            self.emitf(format!(
                "add sp, sp, #{}",
                Arm64Emitter::<X>::TEMP_STACK_BYTES
            ));
        }
        // Restore in reverse order
        self.emit("ldp x17, x18, [sp], #16");
        self.emit("ldp x15, x16, [sp], #16");
        self.emit("ldp x13, x14, [sp], #16");
        self.emit("ldp x11, x12, [sp], #16");
        self.emit("ldp x9, x10, [sp], #16");
        self.emit("ldp x7, x8, [sp], #16");
        self.emit("ldp x5, x6, [sp], #16");
        self.emit("ldp x3, x4, [sp], #16");
        self.emit("ldp x27, x28, [sp], #16");
        self.emit("ldp x25, x26, [sp], #16");
        self.emit("ldp x23, x24, [sp], #16");
        self.emit("ldp x21, x22, [sp], #16");
        self.emit("ldp x19, x20, [sp], #16");
        self.emit("ldp x29, x30, [sp], #16");
        self.emit("ret");
        self.emit_blank();

        self.emit_label("asm_trap");
        self.emit_comment("Trap handler - set exit flag and exit");
        let has_exited = self.layout.offset_has_exited;
        let exit_code = self.layout.offset_exit_code;
        self.emit("mov w0, #1");
        self.emitf(format!(
            "strb w0, [{}, #{}]",
            reserved::STATE_PTR,
            has_exited
        ));
        self.emitf(format!(
            "strb w0, [{}, #{}]",
            reserved::STATE_PTR,
            exit_code
        ));
        self.emit("b asm_exit");
        self.emit_blank();
    }

    /// Emit rv_execute_from wrapper that the runner expects.
    pub(super) fn emit_runtime_wrapper(&mut self) {
        self.emit_raw(".global rv_execute_from");
        self.emit_raw(".type rv_execute_from, %function");
        self.emit_label("rv_execute_from");
        self.emit_comment("rv_execute_from(RvState* state, uint64_t start_pc)");
        self.emit_comment("  x0 = state, x1 = start_pc");
        self.emit_blank();

        // Save frame pointer, link register, and state pointer
        self.emit("stp x29, x30, [sp, #-16]!");
        self.emit("mov x29, sp");
        self.emit("str x0, [sp, #-16]!"); // Save state pointer

        // Store start_pc to state->pc
        let pc_offset = self.layout.offset_pc;
        if X::VALUE == 32 {
            self.emitf(format!("str w1, [x0, #{}]", pc_offset));
        } else {
            self.emitf(format!("str x1, [x0, #{}]", pc_offset));
        }

        // Load memory pointer from state for the asm_run call (non-fixed mode)
        if self.config.fixed_addresses.is_none() {
            let memory_offset = self.layout.offset_memory;
            self.emit_comment("Load memory pointer from state");
            self.emitf(format!("ldr x1, [x0, #{}]", memory_offset));
        }

        // Call asm_run
        self.emit("bl asm_run");

        // Restore state pointer and return has_exited
        self.emit("ldr x0, [sp], #16"); // Restore state pointer
        let has_exited_offset = self.layout.offset_has_exited;
        self.emitf(format!("ldrb w0, [x0, #{}]", has_exited_offset));
        self.emit("ldp x29, x30, [sp], #16");
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
        self.emitf(format!(".word {}", tracer_kind));
        self.emit_blank();

        // RV_EXPORT_FUNCTIONS
        let export_functions = if self.config.export_functions {
            1u32
        } else {
            0
        };
        self.emit_raw(".global RV_EXPORT_FUNCTIONS");
        self.emit_label("RV_EXPORT_FUNCTIONS");
        self.emitf(format!(".word {}", export_functions));
        self.emit_blank();

        // RV_INSTRET_MODE
        let instret_mode = self.config.instret_mode.as_c_mode();
        self.emit_raw(".global RV_INSTRET_MODE");
        self.emit_label("RV_INSTRET_MODE");
        self.emitf(format!(".word {}", instret_mode));
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
