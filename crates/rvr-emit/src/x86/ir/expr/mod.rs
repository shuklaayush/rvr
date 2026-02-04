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
                    let v32 = u32::try_from(v).unwrap_or(0);
                    self.emitf(format!("movl $0x{v32:x}, %eax"));
                } else if v > 0x7fff_ffff {
                    self.emitf(format!("movabsq $0x{v:x}, %rax"));
                } else {
                    self.emitf(format!("movq $0x{v:x}, %rax"));
                }
                "rax".to_string()
            }
            _ => {
                if X::VALUE == 32 {
                    let result = self.emit_expr(expr, "eax");
                    if result != "eax" {
                        let result32 = Self::temp_dword(&result);
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
                u32::try_from(next_pc).unwrap_or(0),
                pc_offset,
                reserved::STATE_PTR
            ));
        } else if next_pc > 0x7fff_ffff {
            self.emitf(format!("movabsq $0x{next_pc:x}, %rdx"));
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
                let target_pc =
                    target.map_or(fall_pc, |t| self.inputs.resolve_address(X::to_u64(t)));
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
                let fall_target_pc =
                    fall.map_or(fall_pc, |f| self.inputs.resolve_address(X::to_u64(f)));
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

                let suffix = Self::suffix();
                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                let cond_reg = self.emit_expr(cond, Self::temp1());
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
                let target_pc =
                    target.map_or(fall_pc, |t| self.inputs.resolve_address(X::to_u64(t)));
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
                let fall_target_pc =
                    fall.map_or(fall_pc, |f| self.inputs.resolve_address(X::to_u64(f)));
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

                let suffix = Self::suffix();
                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                let cond_reg = self.emit_expr(cond, Self::temp1());
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
        let suffix = Self::suffix();
        if self.config.perf_mode
            && matches!(
                expr,
                Expr::Read(ReadExpr::Csr(_) | ReadExpr::Cycle | ReadExpr::Instret)
            )
        {
            self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
            return dest.to_string();
        }

        match expr {
            Expr::Imm(val) => self.emit_expr_imm(*val, dest),
            Expr::Read(ReadExpr::Reg(reg)) => self.load_rv_to_temp(*reg, dest),
            Expr::Read(ReadExpr::Mem {
                base,
                offset,
                width,
                signed,
            }) => self.emit_expr_mem(base, *offset, *width, *signed, dest),
            Expr::Read(ReadExpr::MemAddr {
                addr,
                width,
                signed,
            }) => self.emit_expr_mem_addr(addr, *width, *signed, dest),
            Expr::PcConst(val) => self.emit_expr_pc_const(*val, dest),
            Expr::Read(ReadExpr::Csr(csr)) => self.emit_expr_csr(*csr, dest),
            Expr::Read(ReadExpr::Cycle | ReadExpr::Instret) => self.emit_expr_instret(dest),
            Expr::Read(ReadExpr::Pc) => self.emit_expr_pc(dest),
            Expr::Read(ReadExpr::Exited) => {
                self.emit_expr_state_u8(self.layout.offset_has_exited, dest)
            }
            Expr::Read(ReadExpr::ExitCode) => {
                self.emit_expr_state_u8(self.layout.offset_exit_code, dest)
            }
            Expr::Read(ReadExpr::ResAddr) => self.emit_expr_reservation_addr(dest),
            Expr::Read(ReadExpr::ResValid) => {
                self.emit_expr_state_u8(self.layout.offset_reservation_valid, dest)
            }
            Expr::Read(ReadExpr::Temp(idx)) => self.emit_expr_temp(*idx, dest),
            Expr::Var(name) => self.emit_expr_var(name, dest),
            Expr::Binary { op, left, right } => self.emit_binary_op(*op, left, right, dest),
            Expr::Unary { op, expr: inner } => self.emit_unary_op(*op, inner, dest),
            Expr::ExternCall { name, args, .. } => self.emit_expr_extern_call(name, args, dest),
            Expr::Ternary {
                op: TernaryOp::Select,
                first: cond,
                second: then_val,
                third: else_val,
            } => self.emit_expr_select(cond, then_val, else_val, dest),
            _ => self.emit_expr_unsupported(expr, dest),
        }
    }

    fn emit_expr_imm(&mut self, val: X::Reg, dest: &str) -> String {
        let suffix = Self::suffix();
        let v = X::to_u64(val);
        if v == 0 {
            self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
        } else if X::VALUE == 32 {
            let v32 = i32::try_from(v).unwrap_or(0);
            self.emitf(format!("movl ${v32}, %{dest}"));
        } else if v > 0x7fff_ffff {
            self.emitf(format!("movabsq $0x{v:x}, %{dest}"));
        } else {
            self.emitf(format!("movq $0x{v:x}, %{dest}"));
        }
        dest.to_string()
    }

    fn emit_expr_mem(
        &mut self,
        base: &Expr<X>,
        offset: i16,
        width: u8,
        signed: bool,
        dest: &str,
    ) -> String {
        self.emit_expr_as_addr(base);
        if offset != 0 {
            self.emitf(format!("leaq {offset}(%rax), %rax"));
        }
        self.apply_address_mode("rax");
        let mem = format!("(%{}, %rax)", reserved::MEMORY_PTR);
        self.emit_load_from_mem(&mem, dest, width, signed);
        self.emit_trace_mem_access("rax", dest, width, false);
        dest.to_string()
    }

    fn emit_expr_mem_addr(
        &mut self,
        addr: &Expr<X>,
        width: u8,
        signed: bool,
        dest: &str,
    ) -> String {
        self.emit_expr_as_addr(addr);
        self.apply_address_mode("rax");
        let mem = format!("(%{}, %rax)", reserved::MEMORY_PTR);
        self.emit_load_from_mem(&mem, dest, width, signed);
        self.emit_trace_mem_access("rax", dest, width, false);
        dest.to_string()
    }

    fn emit_expr_pc_const(&mut self, val: X::Reg, dest: &str) -> String {
        let v = X::to_u64(val);
        if X::VALUE == 32 {
            let v32 = u32::try_from(v).unwrap_or(0);
            self.emitf(format!("movl $0x{v32:x}, %{dest}"));
        } else if v > 0x7fff_ffff {
            self.emitf(format!("movabsq $0x{v:x}, %{dest}"));
        } else {
            self.emitf(format!("movq $0x{v:x}, %{dest}"));
        }
        dest.to_string()
    }

    fn emit_expr_csr(&mut self, csr: u16, dest: &str) -> String {
        let suffix = Self::suffix();
        let instret_off = self.layout.offset_instret;
        match csr {
            0xC00 | 0xC02 | 0xB00 | 0xB02 => {
                if self.config.instret_mode.counts() {
                    if X::VALUE == 32 {
                        self.emitf(format!(
                            "movl %{}, %{}",
                            Self::reg_dword(reserved::INSTRET),
                            Self::reg_dword(dest)
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
            0xC80 | 0xC82 | 0xB80 | 0xB82 if X::VALUE == 32 => {
                if self.config.instret_mode.counts() {
                    self.emitf(format!("movq %{}, %rdx", reserved::INSTRET));
                    self.emit("shrq $32, %rdx");
                    self.emitf(format!("movl %edx, %{}", Self::reg_dword(dest)));
                } else {
                    self.emitf(format!(
                        "movl {}(%{}), %{dest}",
                        instret_off + 4,
                        reserved::STATE_PTR
                    ));
                }
            }
            _ => {
                self.emit_comment(&format!("CSR 0x{csr:03x} not implemented"));
                self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
            }
        }
        dest.to_string()
    }

    fn emit_expr_instret(&mut self, dest: &str) -> String {
        let suffix = Self::suffix();
        if self.config.instret_mode.counts() {
            if X::VALUE == 32 {
                self.emitf(format!(
                    "movl %{}, %{}",
                    Self::reg_dword(reserved::INSTRET),
                    Self::reg_dword(dest)
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

    fn emit_expr_pc(&mut self, dest: &str) -> String {
        let suffix = Self::suffix();
        let pc_off = self.layout.offset_pc;
        self.emitf(format!(
            "mov{suffix} {}(%{}), %{dest}",
            pc_off,
            reserved::STATE_PTR
        ));
        dest.to_string()
    }

    fn emit_expr_state_u8(&mut self, offset: usize, dest: &str) -> String {
        self.emitf(format!(
            "movzbl {}(%{}), %{}",
            offset,
            reserved::STATE_PTR,
            Self::reg_dword(dest)
        ));
        dest.to_string()
    }

    fn emit_expr_reservation_addr(&mut self, dest: &str) -> String {
        let off = self.layout.offset_reservation_addr;
        if X::VALUE == 32 {
            self.emitf(format!(
                "movl {}(%{}), %{}",
                off,
                reserved::STATE_PTR,
                Self::reg_dword(dest)
            ));
        } else {
            self.emitf(format!("movq {}(%{}), %{dest}", off, reserved::STATE_PTR));
        }
        dest.to_string()
    }

    fn emit_expr_temp(&mut self, idx: u8, dest: &str) -> String {
        let suffix = Self::suffix();
        if let Some(offset) = Self::temp_slot_offset(idx) {
            if X::VALUE == 32 {
                self.emitf(format!("movl {}(%rsp), %{}", offset, Self::reg_dword(dest)));
            } else {
                self.emitf(format!("movq {offset}(%rsp), %{dest}"));
            }
        } else {
            self.emit_comment(&format!("temp {idx} out of range"));
            self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
        }
        dest.to_string()
    }

    fn emit_expr_var(&mut self, name: &str, dest: &str) -> String {
        let suffix = Self::suffix();
        if name == "state" {
            let dest64 = Self::reg_qword(dest);
            self.emitf(format!("movq %{}, %{dest64}", reserved::STATE_PTR));
            dest64.to_string()
        } else {
            self.emit_comment(&format!("unsupported var: {name}"));
            self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
            dest.to_string()
        }
    }

    fn emit_expr_extern_call(&mut self, name: &str, args: &[Expr<X>], dest: &str) -> String {
        let ret = self.emit_extern_call(name, args);
        if ret != dest {
            if X::VALUE == 32 {
                self.emitf(format!("movl %{ret}, %{}", Self::reg_dword(dest)));
            } else {
                self.emitf(format!("movq %{ret}, %{dest}"));
            }
        }
        dest.to_string()
    }

    fn emit_expr_select(
        &mut self,
        cond: &Expr<X>,
        then_val: &Expr<X>,
        else_val: &Expr<X>,
        dest: &str,
    ) -> String {
        let suffix = Self::suffix();
        let temp1 = Self::temp1();
        let temp2 = Self::temp2();
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
                Self::reg_dword(&else_reg),
                Self::reg_dword(temp1)
            ));
        } else {
            self.emitf(format!("cmovzq %{else_reg}, %{temp1}"));
        }
        if dest != temp1 {
            self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
        }
        dest.to_string()
    }

    fn emit_expr_unsupported(&mut self, expr: &Expr<X>, dest: &str) -> String {
        let suffix = Self::suffix();
        self.emit_comment(&format!("unsupported expr: {expr:?}"));
        self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
        dest.to_string()
    }

    /// Helper to emit a load from memory operand
    fn emit_load_from_mem(&mut self, mem: &str, dest: &str, width: u8, signed: bool) {
        match (width, signed, X::VALUE) {
            (1, false, _) => self.emitf(format!("movzbl {mem}, %{}", Self::reg_dword(dest))),
            (1, true, 32) => self.emitf(format!("movsbl {mem}, %{dest}")),
            (1, true, 64) => self.emitf(format!("movsbq {mem}, %{dest}")),
            (2, false, _) => self.emitf(format!("movzwl {mem}, %{}", Self::reg_dword(dest))),
            (2, true, 32) => self.emitf(format!("movswl {mem}, %{dest}")),
            (2, true, 64) => self.emitf(format!("movswq {mem}, %{dest}")),
            (4, false, 64) => self.emitf(format!("movl {mem}, %{}", Self::reg_dword(dest))),
            (4, true, 64) => self.emitf(format!("movslq {mem}, %{dest}")),
            (8, _, _) => self.emitf(format!("movq {mem}, %{dest}")),
            _ => self.emitf(format!("movl {mem}, %{dest}")),
        }
    }
}

mod ops;
