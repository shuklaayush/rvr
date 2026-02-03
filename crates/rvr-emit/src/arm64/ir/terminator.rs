use rvr_ir::{BinaryOp, Expr, InstrIR, Terminator, Xlen};

use crate::arm64::Arm64Emitter;
use crate::arm64::registers::reserved;
use super::stmt_writes_to_exited;

impl<X: Xlen> Arm64Emitter<X> {
    /// Emit a terminator, using the actual fall-through PC from the output stream.
    pub(super) fn emit_terminator(&mut self, term: &Terminator<X>, fall_pc: u64, current_pc: u64) {
        let temp1 = self.temp1();

        match term {
            Terminator::Fall { target } => {
                let target_pc = target
                    .map(|t| self.inputs.resolve_address(X::to_u64(t)))
                    .unwrap_or(fall_pc);
                if target.is_some() && !self.inputs.is_valid_address(target_pc) {
                    self.emit("b asm_trap");
                    return;
                }
                // Don't emit branch if target is the next emitted instruction or current instruction
                if target_pc != fall_pc && target_pc != current_pc {
                    self.emitf(format!("b asm_pc_{:x}", target_pc));
                }
            }
            Terminator::Jump { target } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                if !self.inputs.is_valid_address(target_pc) {
                    self.emit("b asm_trap");
                    return;
                }
                // Don't emit a self-loop (would be a bug in IR)
                if target_pc != current_pc {
                    self.emitf(format!("b asm_pc_{:x}", target_pc));
                }
            }
            Terminator::JumpDyn { addr, .. } => {
                let base_reg = self.emit_expr_as_addr(addr);
                if base_reg != "x0" {
                    self.emitf(format!("mov x0, {base_reg}"));
                }
                self.emit("and x0, x0, #-2"); // Clear lowest bit
                self.emit_dispatch_jump();
            }
            Terminator::Branch {
                cond, target, fall, ..
            } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                let target_label = if self.inputs.is_valid_address(target_pc) {
                    format!("asm_pc_{:x}", target_pc)
                } else {
                    "asm_trap".to_string()
                };
                if !self.try_emit_compare_branch(cond, &target_label, false) {
                    let cond_reg = self.emit_expr(cond, temp1);
                    self.emitf(format!("cbnz {cond_reg}, {target_label}"));
                }
                let fall_target_pc = fall
                    .map(|f| self.inputs.resolve_address(X::to_u64(f)))
                    .unwrap_or(fall_pc);
                if fall.is_some() && !self.inputs.is_valid_address(fall_target_pc) {
                    self.emit("b asm_trap");
                    return;
                }
                if fall_target_pc != fall_pc {
                    self.emitf(format!("b asm_pc_{:x}", fall_target_pc));
                }
            }
            Terminator::Exit { code } => {
                let code_reg = self.emit_expr(code, temp1);
                let has_exited = self.layout.offset_has_exited;
                let exit_code = self.layout.offset_exit_code;
                self.emit("mov w0, #1");
                self.emitf(format!(
                    "strb w0, [{}, #{}]",
                    reserved::STATE_PTR,
                    has_exited
                ));
                self.emitf(format!(
                    "strb {}, [{}, #{}]",
                    self.reg_32(&code_reg),
                    reserved::STATE_PTR,
                    exit_code
                ));
                self.emit("b asm_exit");
            }
            Terminator::Trap { message } => {
                self.emit_comment(&format!("trap: {message}"));
                self.emit("b asm_trap");
            }
        }
    }

    /// Emit a single instruction from IR.
    #[allow(clippy::collapsible_if)]
    pub(super) fn emit_instruction(
        &mut self,
        instr: &InstrIR<X>,
        is_last_in_block: bool,
        fall_pc: u64,
    ) {
        let pc = X::to_u64(instr.pc);
        self.emit_trace_pc(pc, instr.raw);
        if !self.config.instret_mode.per_instruction() {
            self.emit_instret_increment(1, pc);
        }

        // Check if any statement might set has_exited (e.g., exit syscall)
        let might_exit = instr.statements.iter().any(stmt_writes_to_exited);
        let mut skip_last_temp_cmp = false;
        let mut cmp_for_branch: Option<(Expr<X>, Expr<X>, BinaryOp)> = None;
        if is_last_in_block && !self.config.instret_mode.per_instruction() {
            if let Terminator::Branch { cond, .. } = &instr.terminator {
                if let Some((left, right, op)) = self.cmp_from_temp_branch(&instr.statements, cond)
                {
                    skip_last_temp_cmp = true;
                    cmp_for_branch = Some((left.clone(), right.clone(), op));
                }
            }
        }

        let stmt_count = instr.statements.len();
        for (idx, stmt) in instr.statements.iter().enumerate() {
            if skip_last_temp_cmp && idx + 1 == stmt_count {
                continue;
            }
            self.emit_stmt(stmt);
        }

        // If the instruction might set has_exited, check and branch to asm_exit
        if might_exit {
            let has_exited_off = self.layout.offset_has_exited;
            self.emitf(format!(
                "ldrb w0, [{}, #{}]",
                reserved::STATE_PTR,
                has_exited_off
            ));
            self.emit("cbnz w0, asm_exit");
        }

        // Per-instruction suspend: increment + check after executing the instruction body.
        if self.config.instret_mode.per_instruction() {
            self.emit_instret_post_check(instr, fall_pc, pc);
        }

        if is_last_in_block && self.config.instret_mode.suspends()
            && !self.config.instret_mode.per_instruction()
        {
            self.emit_instret_suspend_check(instr, fall_pc, pc);
        }

        // Use fall_pc from output stream to keep inlined/absorbed ranges correct.
        if is_last_in_block {
            if let Some((left, right, op)) = cmp_for_branch {
                if let Terminator::Branch { target, fall, .. } = &instr.terminator {
                    let cond_expr = Expr::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                    let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                    let target_label = if self.inputs.is_valid_address(target_pc) {
                        format!("asm_pc_{:x}", target_pc)
                    } else {
                        "asm_trap".to_string()
                    };
                    if !self.try_emit_compare_branch(&cond_expr, &target_label, false) {
                        let cond_reg = self.emit_expr(&cond_expr, self.temp1());
                        self.emitf(format!("cbnz {cond_reg}, {target_label}"));
                    }
                    let fall_target_pc = fall
                        .map(|f| self.inputs.resolve_address(X::to_u64(f)))
                        .unwrap_or(fall_pc);
                    if fall.is_some() && !self.inputs.is_valid_address(fall_target_pc) {
                        self.emit("b asm_trap");
                        return;
                    }
                    if fall_target_pc != fall_pc {
                        self.emitf(format!("b asm_pc_{:x}", fall_target_pc));
                    }
                } else {
                    self.emit_terminator(&instr.terminator, fall_pc, pc);
                }
            } else {
                self.emit_terminator(&instr.terminator, fall_pc, pc);
            }
        } else {
            match instr.terminator {
                Terminator::Branch { .. } => {
                    self.emit_terminator(&instr.terminator, fall_pc, pc);
                }
                Terminator::Fall { target } => {
                    let target_pc = target.map(|t| X::to_u64(t)).unwrap_or(fall_pc);
                    if target_pc != fall_pc {
                        self.emit_terminator(&instr.terminator, fall_pc, pc);
                    }
                }
                _ => {}
            }
        }
    }

    /// Emit code for a linear instruction stream.
    pub fn emit_instructions(&mut self, instrs: &[InstrIR<X>]) {
        self.emit_raw("// Generated code instructions");
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
