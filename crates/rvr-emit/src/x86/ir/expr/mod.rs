//! IR expression lowering for x86-64.

use rvr_ir::{BinaryOp, Expr, InstrIR, ReadExpr, Terminator, TernaryOp, UnaryOp, Xlen};

use crate::x86::X86Emitter;
use crate::x86::registers::reserved;

impl<X: Xlen> X86Emitter<X> {
    /// Emit an expression for use as a 64-bit address.
    /// For RV32, ensures the result is zero-extended to 64-bit.
    pub(super) fn emit_expr_as_addr(&mut self, expr: &Expr<X>) -> String {
        match expr {
            Expr::Read(ReadExpr::Reg(reg)) => self.load_rv_as_addr(*reg, "rax"),
            Expr::Imm(val) => {
                let v = X::to_u64(*val);
                if X::VALUE == 32 {
                    self.emitf(format!("movl $0x{:x}, %eax", v as u32));
                } else {
                    if v > 0x7fffffff {
                        self.emitf(format!("movabsq $0x{:x}, %rax", v));
                    } else {
                        self.emitf(format!("movq $0x{:x}, %rax", v));
                    }
                }
                "rax".to_string()
            }
            _ => {
                if X::VALUE == 32 {
                    let result = self.emit_expr(expr, "eax");
                    if result != "eax" {
                        let result32 = self.temp_dword(&result);
                        self.emitf(format!("movl %{result32}, %eax"));
                    }
                } else {
                    let result = self.emit_expr(expr, "rax");
                    if result != "rax" {
                        self.emitf(format!("movq %{result}, %rax"));
                    }
                }
                "rax".to_string()
            }
        }
    }

    fn emit_store_next_pc_imm(&mut self, next_pc: u64) {
        let pc_offset = self.layout.offset_pc;
        if X::VALUE == 32 {
            self.emitf(format!(
                "movl $0x{:x}, {}(%{})",
                next_pc as u32,
                pc_offset,
                reserved::STATE_PTR
            ));
        } else if next_pc > 0x7fffffff {
            self.emitf(format!("movabsq $0x{:x}, %rdx", next_pc));
            self.emitf(format!(
                "movq %rdx, {}(%{})",
                pc_offset,
                reserved::STATE_PTR
            ));
        } else {
            self.emitf(format!(
                "movq $0x{:x}, {}(%{})",
                next_pc,
                pc_offset,
                reserved::STATE_PTR
            ));
        }
    }

    pub(super) fn emit_instret_post_check(
        &mut self,
        instr: &InstrIR<X>,
        fall_pc: u64,
        current_pc: u64,
    ) {
        if !self.config.instret_mode.counts() {
            return;
        }

        self.emitf(format!("addq $1, %{}", reserved::INSTRET));

        if !self.config.instret_mode.suspends() {
            return;
        }

        let continue_label = self.next_label("instret_ok");
        let target_offset = self.layout.offset_target_instret;
        self.emitf(format!(
            "movq {}(%{}), %rdx",
            target_offset,
            reserved::STATE_PTR
        ));
        self.emitf(format!("cmpq %rdx, %{}", reserved::INSTRET));
        self.emitf(format!("jb {continue_label}"));

        match &instr.terminator {
            Terminator::Fall { target } => {
                let target_pc = target
                    .map(|t| self.inputs.resolve_address(X::to_u64(t)))
                    .unwrap_or(fall_pc);
                if target.is_some() && !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }
                let next_pc = if target_pc == current_pc {
                    fall_pc
                } else {
                    target_pc
                };
                self.emit_store_next_pc_imm(next_pc);
                self.emit("jmp asm_exit");
            }
            Terminator::Jump { target } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                if !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }
                self.emit_store_next_pc_imm(target_pc);
                self.emit("jmp asm_exit");
            }
            Terminator::JumpDyn { addr, .. } => {
                self.emit_expr_as_addr(addr);
                self.emit("andq $-2, %rax");
                let pc_offset = self.layout.offset_pc;
                if X::VALUE == 32 {
                    self.emitf(format!(
                        "movl %eax, {}(%{})",
                        pc_offset,
                        reserved::STATE_PTR
                    ));
                } else {
                    self.emitf(format!(
                        "movq %rax, {}(%{})",
                        pc_offset,
                        reserved::STATE_PTR
                    ));
                }
                self.emit("jmp asm_exit");
            }
            Terminator::Branch {
                cond, target, fall, ..
            } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                let fall_target_pc = fall
                    .map(|f| self.inputs.resolve_address(X::to_u64(f)))
                    .unwrap_or(fall_pc);
                if fall.is_some() && !self.inputs.is_valid_address(fall_target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }
                if !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }

                let suffix = self.suffix();
                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                let cond_reg = self.emit_expr(cond, self.temp1());
                self.emitf(format!("test{suffix} %{cond_reg}, %{cond_reg}"));
                self.emitf(format!("jnz {target_label}"));
                self.emit_store_next_pc_imm(fall_target_pc);
                self.emitf(format!("jmp {done_label}"));
                self.emit_label(&target_label);
                self.emit_store_next_pc_imm(target_pc);
                self.emit_label(&done_label);
                self.emit("jmp asm_exit");
            }
            Terminator::Exit { .. } | Terminator::Trap { .. } => {
                // Let the terminator handle exit/trap semantics.
            }
        }

        self.emit_label(&continue_label);
    }

    pub(super) fn emit_instret_suspend_check(
        &mut self,
        instr: &InstrIR<X>,
        fall_pc: u64,
        current_pc: u64,
    ) {
        if !self.config.instret_mode.suspends() || self.config.instret_mode.per_instruction() {
            return;
        }

        let continue_label = self.next_label("instret_ok");
        let target_offset = self.layout.offset_target_instret;
        self.emitf(format!(
            "movq {}(%{}), %rdx",
            target_offset,
            reserved::STATE_PTR
        ));
        self.emitf(format!("cmpq %rdx, %{}", reserved::INSTRET));
        self.emitf(format!("jb {continue_label}"));

        match &instr.terminator {
            Terminator::Fall { target } => {
                let target_pc = target
                    .map(|t| self.inputs.resolve_address(X::to_u64(t)))
                    .unwrap_or(fall_pc);
                if target.is_some() && !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }
                let next_pc = if target_pc == current_pc {
                    fall_pc
                } else {
                    target_pc
                };
                self.emit_store_next_pc_imm(next_pc);
                self.emit("jmp asm_exit");
            }
            Terminator::Jump { target } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                if !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }
                self.emit_store_next_pc_imm(target_pc);
                self.emit("jmp asm_exit");
            }
            Terminator::JumpDyn { addr, .. } => {
                self.emit_expr_as_addr(addr);
                self.emit("andq $-2, %rax");
                let pc_offset = self.layout.offset_pc;
                if X::VALUE == 32 {
                    self.emitf(format!(
                        "movl %eax, {}(%{})",
                        pc_offset,
                        reserved::STATE_PTR
                    ));
                } else {
                    self.emitf(format!(
                        "movq %rax, {}(%{})",
                        pc_offset,
                        reserved::STATE_PTR
                    ));
                }
                self.emit("jmp asm_exit");
            }
            Terminator::Branch {
                cond, target, fall, ..
            } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                let fall_target_pc = fall
                    .map(|f| self.inputs.resolve_address(X::to_u64(f)))
                    .unwrap_or(fall_pc);
                if fall.is_some() && !self.inputs.is_valid_address(fall_target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }
                if !self.inputs.is_valid_address(target_pc) {
                    self.emit("jmp asm_trap");
                    self.emit_label(&continue_label);
                    return;
                }

                let suffix = self.suffix();
                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                let cond_reg = self.emit_expr(cond, self.temp1());
                self.emitf(format!("test{suffix} %{cond_reg}, %{cond_reg}"));
                self.emitf(format!("jnz {target_label}"));
                self.emit_store_next_pc_imm(fall_target_pc);
                self.emitf(format!("jmp {done_label}"));
                self.emit_label(&target_label);
                self.emit_store_next_pc_imm(target_pc);
                self.emit_label(&done_label);
                self.emit("jmp asm_exit");
            }
            Terminator::Exit { .. } | Terminator::Trap { .. } => {}
        }

        self.emit_label(&continue_label);
    }

    /// Emit an expression, returning which x86 register holds the result.
    #[allow(clippy::collapsible_if)]
    pub(super) fn emit_expr(&mut self, expr: &Expr<X>, dest: &str) -> String {
        let suffix = self.suffix();
        if self.config.perf_mode
            && matches!(
                expr,
                Expr::Read(ReadExpr::Csr(_))
                    | Expr::Read(ReadExpr::Cycle)
                    | Expr::Read(ReadExpr::Instret)
            )
        {
            self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
            return dest.to_string();
        }
        match expr {
            Expr::Imm(val) => {
                let v = X::to_u64(*val);
                if v == 0 {
                    self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                } else if X::VALUE == 32 {
                    self.emitf(format!("movl ${}, %{dest}", v as i32));
                } else {
                    if v > 0x7fffffff {
                        self.emitf(format!("movabsq $0x{:x}, %{dest}", v));
                    } else {
                        self.emitf(format!("movq $0x{:x}, %{dest}", v));
                    }
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::Reg(reg)) => self.load_rv_to_temp(*reg, dest),
            Expr::Read(ReadExpr::Mem {
                base,
                offset,
                width,
                signed,
            }) => {
                self.emit_expr_as_addr(base);
                if *offset != 0 {
                    self.emitf(format!("leaq {offset}(%rax), %rax"));
                }
                self.apply_address_mode("rax");
                let mem = format!("(%{}, %rax)", reserved::MEMORY_PTR);
                self.emit_load_from_mem(&mem, dest, *width, *signed);
                self.emit_trace_mem_access("rax", dest, *width, false);
                dest.to_string()
            }
            Expr::Read(ReadExpr::MemAddr {
                addr,
                width,
                signed,
            }) => {
                self.emit_expr_as_addr(addr);
                self.apply_address_mode("rax");
                let mem = format!("(%{}, %rax)", reserved::MEMORY_PTR);
                self.emit_load_from_mem(&mem, dest, *width, *signed);
                self.emit_trace_mem_access("rax", dest, *width, false);
                dest.to_string()
            }
            Expr::PcConst(val) => {
                let v = X::to_u64(*val);
                if X::VALUE == 32 {
                    self.emitf(format!("movl $0x{:x}, %{dest}", v as u32));
                } else {
                    if v > 0x7fffffff {
                        self.emitf(format!("movabsq $0x{:x}, %{dest}", v));
                    } else {
                        self.emitf(format!("movq $0x{:x}, %{dest}", v));
                    }
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::Csr(csr)) => {
                let instret_off = self.layout.offset_instret;
                match *csr {
                    // cycle/instret (user) and mcycle/minstret (machine)
                    0xC00 | 0xC02 | 0xB00 | 0xB02 => {
                        if self.config.instret_mode.counts() {
                            if X::VALUE == 32 {
                                self.emitf(format!(
                                    "movl %{}, %{}",
                                    self.reg_dword(reserved::INSTRET),
                                    self.reg_dword(dest)
                                ));
                            } else {
                                self.emitf(format!("movq %{}, %{dest}", reserved::INSTRET));
                            }
                        } else {
                            self.emitf(format!(
                                "mov{suffix} {}(%{}), %{dest}",
                                instret_off,
                                reserved::STATE_PTR
                            ));
                        }
                    }
                    // cycleh/instreth/mcycleh/minstreth (upper 32 bits for RV32)
                    0xC80 | 0xC82 | 0xB80 | 0xB82 if X::VALUE == 32 => {
                        if self.config.instret_mode.counts() {
                            self.emitf(format!("movq %{}, %rdx", reserved::INSTRET));
                            self.emit("shrq $32, %rdx");
                            self.emitf(format!("movl %edx, %{}", self.reg_dword(dest)));
                        } else {
                            self.emitf(format!(
                                "movl {}(%{}), %{dest}",
                                instret_off + 4,
                                reserved::STATE_PTR
                            ));
                        }
                    }
                    _ => {
                        self.emit_comment(&format!("CSR 0x{:03x} not implemented", csr));
                        self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                    }
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::Cycle) | Expr::Read(ReadExpr::Instret) => {
                if self.config.instret_mode.counts() {
                    if X::VALUE == 32 {
                        self.emitf(format!(
                            "movl %{}, %{}",
                            self.reg_dword(reserved::INSTRET),
                            self.reg_dword(dest)
                        ));
                    } else {
                        self.emitf(format!("movq %{}, %{dest}", reserved::INSTRET));
                    }
                } else {
                    let instret_off = self.layout.offset_instret;
                    self.emitf(format!(
                        "mov{suffix} {}(%{}), %{dest}",
                        instret_off,
                        reserved::STATE_PTR
                    ));
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::Pc) => {
                let pc_off = self.layout.offset_pc;
                self.emitf(format!(
                    "mov{suffix} {}(%{}), %{dest}",
                    pc_off,
                    reserved::STATE_PTR
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::Exited) => {
                let off = self.layout.offset_has_exited;
                self.emitf(format!(
                    "movzbl {}(%{}), %{}",
                    off,
                    reserved::STATE_PTR,
                    self.reg_dword(dest)
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::ExitCode) => {
                let off = self.layout.offset_exit_code;
                self.emitf(format!(
                    "movzbl {}(%{}), %{}",
                    off,
                    reserved::STATE_PTR,
                    self.reg_dword(dest)
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::ResAddr) => {
                let off = self.layout.offset_reservation_addr;
                if X::VALUE == 32 {
                    self.emitf(format!(
                        "movl {}(%{}), %{}",
                        off,
                        reserved::STATE_PTR,
                        self.reg_dword(dest)
                    ));
                } else {
                    self.emitf(format!("movq {}(%{}), %{dest}", off, reserved::STATE_PTR));
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::ResValid) => {
                let off = self.layout.offset_reservation_valid;
                self.emitf(format!(
                    "movzbl {}(%{}), %{}",
                    off,
                    reserved::STATE_PTR,
                    self.reg_dword(dest)
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::Temp(idx)) => {
                if let Some(offset) = self.temp_slot_offset(*idx) {
                    if X::VALUE == 32 {
                        self.emitf(format!("movl {}(%rsp), %{}", offset, self.reg_dword(dest)));
                    } else {
                        self.emitf(format!("movq {}(%rsp), %{dest}", offset));
                    }
                } else {
                    self.emit_comment(&format!("temp {} out of range", idx));
                    self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                }
                dest.to_string()
            }
            Expr::Var(name) => {
                if name == "state" {
                    let dest64 = self.reg_qword(dest);
                    self.emitf(format!("movq %{}, %{dest64}", reserved::STATE_PTR));
                    dest64.to_string()
                } else {
                    self.emit_comment(&format!("unsupported var: {name}"));
                    self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                    dest.to_string()
                }
            }
            Expr::Binary { op, left, right } => self.emit_binary_op(*op, left, right, dest),
            Expr::Unary { op, expr: inner } => self.emit_unary_op(*op, inner, dest),
            Expr::ExternCall { name, args, .. } => {
                let ret = self.emit_extern_call(name, args);
                if ret != dest {
                    if X::VALUE == 32 {
                        self.emitf(format!("movl %{ret}, %{}", self.reg_dword(dest)));
                    } else {
                        self.emitf(format!("movq %{ret}, %{dest}"));
                    }
                }
                dest.to_string()
            }
            Expr::Ternary {
                op: TernaryOp::Select,
                first: cond,
                second: then_val,
                third: else_val,
            } => {
                let temp1 = self.temp1();
                let temp2 = self.temp2();
                let cond_reg = self.emit_expr(cond, temp1);
                self.emitf(format!("movq %{cond_reg}, %rdx"));
                let then_reg = self.emit_expr(then_val, temp1);
                if then_reg != temp1 {
                    self.emitf(format!("mov{suffix} %{then_reg}, %{temp1}"));
                }
                let else_reg = self.emit_expr(else_val, temp2);
                self.emit("testq %rdx, %rdx");
                if X::VALUE == 32 {
                    self.emitf(format!(
                        "cmovzl %{}, %{}",
                        self.reg_dword(&else_reg),
                        self.reg_dword(temp1)
                    ));
                } else {
                    self.emitf(format!("cmovzq %{else_reg}, %{temp1}"));
                }
                if dest != temp1 {
                    self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                }
                dest.to_string()
            }
            _ => {
                self.emit_comment(&format!("unsupported expr: {:?}", expr));
                self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                dest.to_string()
            }
        }
    }

    /// Helper to emit a load from memory operand
    fn emit_load_from_mem(&mut self, mem: &str, dest: &str, width: u8, signed: bool) {
        match (width, signed, X::VALUE) {
            (1, false, _) => self.emitf(format!("movzbl {mem}, %{}", self.reg_dword(dest))),
            (1, true, 32) => self.emitf(format!("movsbl {mem}, %{dest}")),
            (1, true, 64) => self.emitf(format!("movsbq {mem}, %{dest}")),
            (2, false, _) => self.emitf(format!("movzwl {mem}, %{}", self.reg_dword(dest))),
            (2, true, 32) => self.emitf(format!("movswl {mem}, %{dest}")),
            (2, true, 64) => self.emitf(format!("movswq {mem}, %{dest}")),
            (4, _, 32) => self.emitf(format!("movl {mem}, %{dest}")),
            (4, false, 64) => self.emitf(format!("movl {mem}, %{}", self.reg_dword(dest))),
            (4, true, 64) => self.emitf(format!("movslq {mem}, %{dest}")),
            (8, _, _) => self.emitf(format!("movq {mem}, %{dest}")),
            _ => self.emitf(format!("movl {mem}, %{dest}")),
        }
    }

}

mod ops;
