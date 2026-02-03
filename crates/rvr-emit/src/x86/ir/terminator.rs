use rvr_ir::{InstrIR, Terminator, Xlen};

use crate::x86::X86Emitter;
use crate::x86::registers::reserved;
use super::stmt_writes_to_exited;

impl<X: Xlen> X86Emitter<X> {
    /// Emit a terminator.
    pub(super) fn emit_terminator(&mut self, term: &Terminator<X>, fall_pc: u64) {
        let temp1 = self.temp1();
        let suffix = self.suffix();

        match term {
            Terminator::Fall { target } => {
                let target_pc = target
                    .map(|t| self.inputs.resolve_address(X::to_u64(t)))
                    .unwrap_or(fall_pc);
                if target.is_some() && !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    return;
                }
                if target_pc != fall_pc {
                    self.emitf(format!("jmp asm_pc_{:x}", target_pc));
                }
            }
            Terminator::Jump { target } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                if !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    return;
                }
                self.emitf(format!("jmp asm_pc_{:x}", target_pc));
            }
            Terminator::JumpDyn { addr, .. } => {
                self.emit_expr_as_addr(addr);
                self.emit("andq $-2, %rax");
                self.emit_dispatch_jump();
            }
            Terminator::Branch {
                cond, target, fall, ..
            } => {
                let cond_reg = self.emit_expr(cond, temp1);
                self.emitf(format!("test{suffix} %{cond_reg}, %{cond_reg}"));
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                if self.inputs.is_valid_address(target_pc) {
                    self.emitf(format!("jnz asm_pc_{:x}", target_pc));
                } else {
                    self.emit("jnz asm_trap");
                }
                let fall_target_pc = fall
                    .map(|f| self.inputs.resolve_address(X::to_u64(f)))
                    .unwrap_or(fall_pc);
                if fall.is_some() && !self.inputs.is_valid_address(fall_target_pc) {
                    self.emit("jmp asm_trap");
                    return;
                }
                if fall_target_pc != fall_pc {
                    self.emitf(format!("jmp asm_pc_{:x}", fall_target_pc));
                }
            }
            Terminator::Exit { code } => {
                let code_reg = self.emit_expr(code, temp1);
                let has_exited = self.layout.offset_has_exited;
                let exit_code = self.layout.offset_exit_code;
                self.emitf(format!("movb $1, {}(%{})", has_exited, reserved::STATE_PTR));
                self.emitf(format!(
                    "movb %{}, {}(%{})",
                    self.reg_byte(&code_reg),
                    exit_code,
                    reserved::STATE_PTR
                ));
                self.emit("jmp asm_exit");
            }
            Terminator::Trap { message } => {
                self.emit_comment(&format!("trap: {message}"));
                self.emit("jmp asm_trap");
            }
        }
    }

    /// Emit a single instruction from IR.
    pub(super) fn emit_instruction(&mut self, instr: &InstrIR<X>, is_last: bool, fall_pc: u64) {
        let pc = X::to_u64(instr.pc);
        self.emit_trace_pc(pc, instr.raw);
        if !self.config.instret_mode.per_instruction() {
            self.emit_instret_increment(1, pc);
        }

        // Check if any statement might set has_exited (e.g., exit syscall)
        let might_exit = instr.statements.iter().any(stmt_writes_to_exited);

        for stmt in &instr.statements {
            self.emit_stmt(stmt);
        }

        // If the instruction might set has_exited, check and branch to asm_exit
        if might_exit {
            let has_exited_off = self.layout.offset_has_exited;
            let suffix = self.suffix();
            self.emitf(format!(
                "cmpb $0, {}({})",
                has_exited_off,
                reserved::STATE_PTR
            ));
            self.emit(&format!("jne{suffix} asm_exit"));
        }

        if self.config.instret_mode.per_instruction() {
            self.emit_instret_post_check(instr, fall_pc, pc);
        }

        if is_last && self.config.instret_mode.suspends()
            && !self.config.instret_mode.per_instruction()
        {
            self.emit_instret_suspend_check(instr, fall_pc, pc);
        }

        if is_last {
            self.emit_terminator(&instr.terminator, fall_pc);
        } else {
            match instr.terminator {
                Terminator::Branch { .. } => self.emit_terminator(&instr.terminator, fall_pc),
                Terminator::Fall { target } => {
                    let target_pc = target.map(|t| X::to_u64(t)).unwrap_or(fall_pc);
                    if target_pc != fall_pc {
                        self.emit_terminator(&instr.terminator, fall_pc);
                    }
                }
                _ => {}
            }
        }
    }

    /// Emit code for a linear instruction stream.
    pub fn emit_instructions(&mut self, instrs: &[InstrIR<X>]) {
        self.emit_raw("# Generated code instructions");
        self.emit_blank();
        for (i, instr) in instrs.iter().enumerate() {
            let pc = X::to_u64(instr.pc);
            if self.label_pcs.contains(&pc) {
                self.emit_pc_label(pc);
            }
            let fall_pc = if i + 1 < instrs.len() {
                X::to_u64(instrs[i + 1].pc)
            } else {
                pc + instr.size as u64
            };
            self.emit_instruction(instr, true, fall_pc);
        }
    }
}
