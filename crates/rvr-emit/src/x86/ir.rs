//! IR translation for x86-64 assembly (AT&T syntax).
//!
//! Translates IR expressions, statements, and terminators to x86-64 assembly.

use rvr_ir::{
    BinaryOp, Expr, InstrIR, ReadExpr, Stmt, Terminator, TernaryOp, UnaryOp, WriteTarget, Xlen,
};

use super::X86Emitter;
use super::registers::reserved;

/// Check if a statement (recursively) writes to Exited.
fn stmt_writes_to_exited<X: Xlen>(stmt: &Stmt<X>) -> bool {
    match stmt {
        Stmt::Write { target, .. } => matches!(target, WriteTarget::Exited),
        Stmt::If {
            then_stmts,
            else_stmts,
            ..
        } => {
            then_stmts.iter().any(stmt_writes_to_exited)
                || else_stmts.iter().any(stmt_writes_to_exited)
        }
        Stmt::ExternCall { .. } => false,
    }
}

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

    fn emit_instret_post_check(&mut self, instr: &InstrIR<X>, fall_pc: u64, current_pc: u64) {
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

    /// Emit an expression, returning which x86 register holds the result.
    pub(super) fn emit_expr(&mut self, expr: &Expr<X>, dest: &str) -> String {
        let suffix = self.suffix();
        if self.config.perf_mode {
            if matches!(
                expr,
                Expr::Read(ReadExpr::Csr(_))
                    | Expr::Read(ReadExpr::Cycle)
                    | Expr::Read(ReadExpr::Instret)
            ) {
                self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                return dest.to_string();
            }
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

    /// Emit a binary operation.
    pub(super) fn emit_binary_op(
        &mut self,
        op: BinaryOp,
        left: &Expr<X>,
        right: &Expr<X>,
        dest: &str,
    ) -> String {
        let temp1 = self.temp1();
        let temp2 = self.temp2();
        let suffix = self.suffix();

        if let Expr::Imm(imm) = right {
            let v = X::to_u64(*imm);
            let full_mask = if X::VALUE == 32 {
                u32::MAX as u64
            } else {
                u64::MAX
            };
            match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Or | BinaryOp::Xor if v == 0 => {
                    let left_reg = self.emit_expr(left, dest);
                    if left_reg != dest {
                        if X::VALUE == 32 {
                            self.emitf(format!(
                                "movl %{}, %{}",
                                self.reg_dword(&left_reg),
                                self.reg_dword(dest)
                            ));
                        } else {
                            self.emitf(format!("movq %{left_reg}, %{dest}"));
                        }
                    }
                    return dest.to_string();
                }
                BinaryOp::And if v == 0 => {
                    self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                    return dest.to_string();
                }
                BinaryOp::And if v == full_mask => {
                    let left_reg = self.emit_expr(left, dest);
                    if left_reg != dest {
                        if X::VALUE == 32 {
                            self.emitf(format!(
                                "movl %{}, %{}",
                                self.reg_dword(&left_reg),
                                self.reg_dword(dest)
                            ));
                        } else {
                            self.emitf(format!("movq %{left_reg}, %{dest}"));
                        }
                    }
                    return dest.to_string();
                }
                BinaryOp::Mul if v == 0 => {
                    self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                    return dest.to_string();
                }
                BinaryOp::Mul if v == 1 => {
                    let left_reg = self.emit_expr(left, dest);
                    if left_reg != dest {
                        if X::VALUE == 32 {
                            self.emitf(format!(
                                "movl %{}, %{}",
                                self.reg_dword(&left_reg),
                                self.reg_dword(dest)
                            ));
                        } else {
                            self.emitf(format!("movq %{left_reg}, %{dest}"));
                        }
                    }
                    return dest.to_string();
                }
                _ => {}
            }
        }

        // Fast path: in-place op on hot register
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Xor
                | BinaryOp::Mul
        ) {
            if let Expr::Read(ReadExpr::Reg(reg)) = left {
                if let Some(mapped) = self.reg_map.get(*reg) {
                    if mapped == dest {
                        let right_is_imm = matches!(right, Expr::Imm(_));
                        let right_val = if let Expr::Imm(imm) = right {
                            X::to_u64(*imm)
                        } else {
                            0
                        };
                        let imm_fits = right_val <= i32::MAX as u64;
                        match op {
                            BinaryOp::Add
                            | BinaryOp::Sub
                            | BinaryOp::And
                            | BinaryOp::Or
                            | BinaryOp::Xor => {
                                if right_is_imm && imm_fits {
                                    let op_str = match op {
                                        BinaryOp::Add => "add",
                                        BinaryOp::Sub => "sub",
                                        BinaryOp::And => "and",
                                        BinaryOp::Or => "or",
                                        BinaryOp::Xor => "xor",
                                        _ => unreachable!(),
                                    };
                                    self.emitf(format!(
                                        "{op_str}{suffix} ${}, %{dest}",
                                        right_val as i32
                                    ));
                                    return dest.to_string();
                                }
                            }
                            _ => {}
                        }

                        let right_reg = self.emit_expr(right, temp2);
                        match op {
                            BinaryOp::Add => {
                                self.emitf(format!("add{suffix} %{right_reg}, %{dest}"))
                            }
                            BinaryOp::Sub => {
                                self.emitf(format!("sub{suffix} %{right_reg}, %{dest}"))
                            }
                            BinaryOp::And => {
                                self.emitf(format!("and{suffix} %{right_reg}, %{dest}"))
                            }
                            BinaryOp::Or => self.emitf(format!("or{suffix} %{right_reg}, %{dest}")),
                            BinaryOp::Xor => {
                                self.emitf(format!("xor{suffix} %{right_reg}, %{dest}"))
                            }
                            BinaryOp::Mul => {
                                if X::VALUE == 32 {
                                    self.emitf(format!("imull %{right_reg}, %{dest}"));
                                } else {
                                    self.emitf(format!("imulq %{right_reg}, %{dest}"));
                                }
                            }
                            _ => {}
                        }
                        return dest.to_string();
                    }
                }
            }
        }

        let left_reg = self.emit_expr(left, temp1);
        if left_reg != temp1 {
            self.emitf(format!("mov{suffix} %{left_reg}, %{temp1}"));
        }

        // Handle shifts
        match op {
            BinaryOp::Sll
            | BinaryOp::Srl
            | BinaryOp::Sra
            | BinaryOp::SllW
            | BinaryOp::SrlW
            | BinaryOp::SraW => {
                let x86_op = match op {
                    BinaryOp::Sll | BinaryOp::SllW => "shl",
                    BinaryOp::Srl | BinaryOp::SrlW => "shr",
                    BinaryOp::Sra | BinaryOp::SraW => "sar",
                    _ => unreachable!(),
                };
                let is_word = matches!(op, BinaryOp::SllW | BinaryOp::SrlW | BinaryOp::SraW);

                if let Expr::Imm(imm) = right {
                    let mask = if is_word || X::VALUE == 32 {
                        0x1f
                    } else {
                        0x3f
                    };
                    let shift = (X::to_u64(*imm) & mask) as u8;

                    // LEA optimization: use lea for shift-left by 1, 2, or 3
                    // lea (%src,%src), %dest is faster than shl $1, %src
                    if matches!(op, BinaryOp::Sll) && shift <= 3 && shift > 0 {
                        if shift == 1 {
                            self.emitf(format!("lea{suffix} (%{temp1},%{temp1}), %{temp1}"));
                        } else if shift == 2 {
                            self.emitf(format!("lea{suffix} (,%{temp1},4), %{temp1}"));
                        } else {
                            self.emitf(format!("lea{suffix} (,%{temp1},8), %{temp1}"));
                        }
                        if dest != temp1 {
                            self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                        }
                        return dest.to_string();
                    }

                    if is_word {
                        self.emitf(format!("{x86_op}l ${shift}, %eax"));
                        self.emitf(format!("movslq %eax, %{dest}"));
                    } else {
                        self.emitf(format!("{x86_op}{suffix} ${shift}, %{temp1}"));
                        if dest != temp1 {
                            self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                        }
                    }
                } else {
                    // For variable shifts, evaluate shift amount first to avoid clobbering left
                    // Use temp3 (rdi) for intermediate, then move to cl
                    let temp3 = self.temp3();
                    let right_reg = self.emit_expr(right, temp3);
                    // Move shift amount to cl (required for variable shifts)
                    if right_reg != "rcx" && right_reg != "ecx" && right_reg != "cl" {
                        self.emitf(format!("movl %{}, %ecx", self.reg_dword(&right_reg)));
                    }
                    if is_word {
                        self.emitf(format!("{x86_op}l %cl, %eax"));
                        self.emitf(format!("movslq %eax, %{dest}"));
                    } else {
                        self.emitf(format!("{x86_op}{suffix} %cl, %{temp1}"));
                        if dest != temp1 {
                            self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                        }
                    }
                }
                return dest.to_string();
            }
            _ => {}
        }

        // Handle word ops
        match op {
            BinaryOp::AddW | BinaryOp::SubW => {
                let x86_op = if op == BinaryOp::AddW { "addl" } else { "subl" };
                if let Expr::Imm(imm) = right {
                    let v = X::to_u64(*imm) as i32;
                    self.emitf(format!("{x86_op} ${v}, %eax"));
                } else {
                    let right_reg = self.emit_expr(right, temp2);
                    self.emitf(format!("{x86_op} %{}, %eax", self.reg_dword(&right_reg)));
                }
                self.emitf(format!("movslq %eax, %{dest}"));
                return dest.to_string();
            }
            _ => {}
        }

        // Get right operand
        let right_is_imm = matches!(right, Expr::Imm(_));
        let right_val = if let Expr::Imm(imm) = right {
            format!("${}", X::to_u64(*imm) as i64)
        } else {
            let r = self.emit_expr(right, temp2);
            format!("%{r}")
        };

        match op {
            BinaryOp::Add => {
                // LEA optimization: use lea for add with small immediate
                if right_is_imm {
                    if let Expr::Imm(imm) = right {
                        let v = X::to_u64(*imm) as i64;
                        // LEA works well for small offsets that fit in disp32
                        if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
                            if X::VALUE == 32 {
                                self.emitf(format!("leal {}(%{temp1}), %{temp1}", v as i32));
                            } else {
                                self.emitf(format!("leaq {}(%{temp1}), %{temp1}", v as i32));
                            }
                        } else {
                            self.emitf(format!("add{suffix} {right_val}, %{temp1}"));
                        }
                    } else {
                        self.emitf(format!("add{suffix} {right_val}, %{temp1}"));
                    }
                } else {
                    self.emitf(format!("add{suffix} {right_val}, %{temp1}"));
                }
            }
            BinaryOp::Sub => self.emitf(format!("sub{suffix} {right_val}, %{temp1}")),
            BinaryOp::And => self.emitf(format!("and{suffix} {right_val}, %{temp1}")),
            BinaryOp::Or => self.emitf(format!("or{suffix} {right_val}, %{temp1}")),
            BinaryOp::Xor => self.emitf(format!("xor{suffix} {right_val}, %{temp1}")),
            BinaryOp::Mul => {
                if right_is_imm {
                    self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
                    self.emitf(format!("imul{suffix} %{temp2}, %{temp1}"));
                } else {
                    self.emitf(format!("imul{suffix} {right_val}, %{temp1}"));
                }
            }
            BinaryOp::MulW => {
                if right_is_imm {
                    self.emitf(format!("movl {right_val}, %ecx"));
                }
                self.emitf("imull %ecx, %eax".to_string());
                self.emitf(format!("movslq %eax, %{dest}"));
                return dest.to_string();
            }
            BinaryOp::MulH => {
                if right_is_imm {
                    self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
                }
                if X::VALUE == 32 {
                    self.emit("cdq");
                    self.emitf(format!("imull %{}", self.reg_dword(temp2)));
                    self.emitf(format!("movl %edx, %{dest}"));
                } else {
                    self.emit("cqo");
                    self.emitf(format!("imulq %{temp2}"));
                    self.emitf(format!("movq %rdx, %{dest}"));
                }
                return dest.to_string();
            }
            BinaryOp::MulHU => {
                if right_is_imm {
                    self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
                }
                if X::VALUE == 32 {
                    self.emitf(format!("mull %{}", self.reg_dword(temp2)));
                    self.emitf(format!("movl %edx, %{dest}"));
                } else {
                    self.emitf(format!("mulq %{temp2}"));
                    self.emitf(format!("movq %rdx, %{dest}"));
                }
                return dest.to_string();
            }
            BinaryOp::MulHSU => {
                if right_is_imm {
                    self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
                }
                if X::VALUE == 32 {
                    self.emitf(format!("mull %{}", self.reg_dword(temp2)));
                    self.emit("testl %eax, %eax");
                    let done = self.next_label("mulhsu_done");
                    self.emitf(format!("jns {done}"));
                    self.emitf(format!("subl %{}, %edx", self.reg_dword(temp2)));
                    self.emit_label(&done);
                    self.emitf(format!("movl %edx, %{dest}"));
                } else {
                    self.emitf(format!("mulq %{temp2}"));
                    self.emit("testq %rax, %rax");
                    let done = self.next_label("mulhsu_done");
                    self.emitf(format!("jns {done}"));
                    self.emitf(format!("subq %{temp2}, %rdx"));
                    self.emit_label(&done);
                    self.emitf(format!("movq %rdx, %{dest}"));
                }
                return dest.to_string();
            }
            BinaryOp::Div => {
                return self.emit_div_signed(right_is_imm, &right_val, temp2, dest);
            }
            BinaryOp::DivU => {
                return self.emit_div_unsigned(right_is_imm, &right_val, temp2, dest);
            }
            BinaryOp::Rem => {
                return self.emit_rem_signed(right_is_imm, &right_val, temp1, temp2, dest);
            }
            BinaryOp::RemU => {
                return self.emit_rem_unsigned(right_is_imm, &right_val, temp1, temp2, dest);
            }
            BinaryOp::DivW => {
                return self.emit_divw_signed(right_is_imm, &right_val, temp2, dest);
            }
            BinaryOp::DivUW => {
                return self.emit_divw_unsigned(right_is_imm, &right_val, temp2, dest);
            }
            BinaryOp::RemW => {
                return self.emit_remw_signed(right_is_imm, &right_val, temp2, dest);
            }
            BinaryOp::RemUW => {
                return self.emit_remw_unsigned(right_is_imm, &right_val, temp2, dest);
            }
            BinaryOp::Eq => {
                self.emitf(format!("cmp{suffix} {right_val}, %{temp1}"));
                self.emit("sete %al");
                self.emitf(format!("movzbl %al, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            BinaryOp::Ne => {
                self.emitf(format!("cmp{suffix} {right_val}, %{temp1}"));
                self.emit("setne %al");
                self.emitf(format!("movzbl %al, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            BinaryOp::Lt => {
                self.emitf(format!("cmp{suffix} {right_val}, %{temp1}"));
                self.emit("setl %al");
                self.emitf(format!("movzbl %al, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            BinaryOp::Ge => {
                self.emitf(format!("cmp{suffix} {right_val}, %{temp1}"));
                self.emit("setge %al");
                self.emitf(format!("movzbl %al, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            BinaryOp::Ltu => {
                self.emitf(format!("cmp{suffix} {right_val}, %{temp1}"));
                self.emit("setb %al");
                self.emitf(format!("movzbl %al, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            BinaryOp::Geu => {
                self.emitf(format!("cmp{suffix} {right_val}, %{temp1}"));
                self.emit("setae %al");
                self.emitf(format!("movzbl %al, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            _ => {
                self.emit_comment(&format!("unsupported binary op: {:?}", op));
            }
        }

        if dest != temp1 {
            self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
        }
        dest.to_string()
    }

    // Division helpers
    fn emit_div_signed(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        let suffix = self.suffix();
        if right_is_imm {
            self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
        }
        let skip = self.next_label("div_skip");
        let done = self.next_label("div_done");

        self.emitf(format!("test{suffix} %{temp2}, %{temp2}"));
        self.emitf(format!("jnz {skip}"));
        self.emitf(format!("mov{suffix} $-1, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&skip);

        if X::VALUE == 32 {
            let no_ov = self.next_label("div_no_ov");
            self.emit("cmpl $0x80000000, %eax");
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("cmpl $-1, %{}", self.reg_dword(temp2)));
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("movl $0x80000000, %{dest}"));
            self.emitf(format!("jmp {done}"));
            self.emit_label(&no_ov);
            self.emit("cdq");
            self.emitf(format!("idivl %{}", self.reg_dword(temp2)));
            self.emitf(format!("movl %eax, %{dest}"));
        } else {
            let no_ov = self.next_label("div_no_ov");
            self.emit("movabsq $0x8000000000000000, %rdx");
            self.emit("cmpq %rdx, %rax");
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("cmpq $-1, %{temp2}"));
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("movq %rdx, %{dest}"));
            self.emitf(format!("jmp {done}"));
            self.emit_label(&no_ov);
            self.emit("cqo");
            self.emitf(format!("idivq %{temp2}"));
            self.emitf(format!("movq %rax, %{dest}"));
        }
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_div_unsigned(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        let suffix = self.suffix();
        if right_is_imm {
            self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
        }
        let do_div = self.next_label("divu_do");
        let done = self.next_label("divu_done");

        self.emitf(format!("test{suffix} %{temp2}, %{temp2}"));
        self.emitf(format!("jnz {do_div}"));
        self.emitf(format!("mov{suffix} $-1, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&do_div);
        self.emit("xorl %edx, %edx");
        if X::VALUE == 32 {
            self.emitf(format!("divl %{}", self.reg_dword(temp2)));
            self.emitf(format!("movl %eax, %{dest}"));
        } else {
            self.emitf(format!("divq %{temp2}"));
            self.emitf(format!("movq %rax, %{dest}"));
        }
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_rem_signed(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp1: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        let suffix = self.suffix();
        if right_is_imm {
            self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
        }
        let skip = self.next_label("rem_skip");
        let done = self.next_label("rem_done");

        self.emitf(format!("test{suffix} %{temp2}, %{temp2}"));
        self.emitf(format!("jnz {skip}"));
        self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&skip);

        if X::VALUE == 32 {
            let no_ov = self.next_label("rem_no_ov");
            self.emit("cmpl $0x80000000, %eax");
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("cmpl $-1, %{}", self.reg_dword(temp2)));
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("xorl %{dest}, %{dest}"));
            self.emitf(format!("jmp {done}"));
            self.emit_label(&no_ov);
            self.emit("cdq");
            self.emitf(format!("idivl %{}", self.reg_dword(temp2)));
            self.emitf(format!("movl %edx, %{dest}"));
        } else {
            let no_ov = self.next_label("rem_no_ov");
            self.emit("movabsq $0x8000000000000000, %rdx");
            self.emit("cmpq %rdx, %rax");
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("cmpq $-1, %{temp2}"));
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("xorq %{dest}, %{dest}"));
            self.emitf(format!("jmp {done}"));
            self.emit_label(&no_ov);
            self.emit("cqo");
            self.emitf(format!("idivq %{temp2}"));
            self.emitf(format!("movq %rdx, %{dest}"));
        }
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_rem_unsigned(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp1: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        let suffix = self.suffix();
        if right_is_imm {
            self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
        }
        let do_div = self.next_label("remu_do");
        let done = self.next_label("remu_done");

        self.emitf(format!("test{suffix} %{temp2}, %{temp2}"));
        self.emitf(format!("jnz {do_div}"));
        self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&do_div);
        self.emit("xorl %edx, %edx");
        if X::VALUE == 32 {
            self.emitf(format!("divl %{}", self.reg_dword(temp2)));
            self.emitf(format!("movl %edx, %{dest}"));
        } else {
            self.emitf(format!("divq %{temp2}"));
            self.emitf(format!("movq %rdx, %{dest}"));
        }
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_divw_signed(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        if right_is_imm {
            self.emitf(format!("movl {right_val}, %{}", self.reg_dword(temp2)));
        }
        let skip = self.next_label("divw_skip");
        let done = self.next_label("divw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            self.reg_dword(temp2),
            self.reg_dword(temp2)
        ));
        self.emitf(format!("jnz {skip}"));
        self.emitf(format!("movq $-1, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&skip);

        let no_ov = self.next_label("divw_no_ov");
        self.emit("cmpl $0x80000000, %eax");
        self.emitf(format!("jne {no_ov}"));
        self.emitf(format!("cmpl $-1, %{}", self.reg_dword(temp2)));
        self.emitf(format!("jne {no_ov}"));
        self.emit("movl $0x80000000, %eax");
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&no_ov);
        self.emit("cdq");
        self.emitf(format!("idivl %{}", self.reg_dword(temp2)));
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_divw_unsigned(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        if right_is_imm {
            self.emitf(format!("movl {right_val}, %{}", self.reg_dword(temp2)));
        }
        let do_div = self.next_label("divuw_do");
        let done = self.next_label("divuw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            self.reg_dword(temp2),
            self.reg_dword(temp2)
        ));
        self.emitf(format!("jnz {do_div}"));
        self.emitf(format!("movq $-1, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&do_div);
        self.emit("xorl %edx, %edx");
        self.emitf(format!("divl %{}", self.reg_dword(temp2)));
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_remw_signed(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        if right_is_imm {
            self.emitf(format!("movl {right_val}, %{}", self.reg_dword(temp2)));
        }
        let skip = self.next_label("remw_skip");
        let done = self.next_label("remw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            self.reg_dword(temp2),
            self.reg_dword(temp2)
        ));
        self.emitf(format!("jnz {skip}"));
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&skip);

        let no_ov = self.next_label("remw_no_ov");
        self.emit("cmpl $0x80000000, %eax");
        self.emitf(format!("jne {no_ov}"));
        self.emitf(format!("cmpl $-1, %{}", self.reg_dword(temp2)));
        self.emitf(format!("jne {no_ov}"));
        self.emitf(format!("xorq %{dest}, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&no_ov);
        self.emit("cdq");
        self.emitf(format!("idivl %{}", self.reg_dword(temp2)));
        self.emitf(format!("movslq %edx, %{dest}"));
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_remw_unsigned(
        &mut self,
        right_is_imm: bool,
        right_val: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        if right_is_imm {
            self.emitf(format!("movl {right_val}, %{}", self.reg_dword(temp2)));
        }
        let do_div = self.next_label("remuw_do");
        let done = self.next_label("remuw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            self.reg_dword(temp2),
            self.reg_dword(temp2)
        ));
        self.emitf(format!("jnz {do_div}"));
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&do_div);
        self.emit("xorl %edx, %edx");
        self.emitf(format!("divl %{}", self.reg_dword(temp2)));
        self.emitf(format!("movslq %edx, %{dest}"));
        self.emit_label(&done);
        dest.to_string()
    }

    fn emit_extern_call(&mut self, fn_name: &str, args: &[Expr<X>]) -> String {
        self.save_hot_regs_to_state();

        let arg_regs_64 = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
        let arg_regs_32 = ["edi", "esi", "edx", "ecx", "r8d", "r9d"];

        let max_args = arg_regs_64.len();
        for (idx, arg) in args.iter().enumerate().take(max_args).rev() {
            let arg_reg = if X::VALUE == 32 {
                if matches!(arg, Expr::Var(name) if name == "state") {
                    arg_regs_64[idx]
                } else {
                    arg_regs_32[idx]
                }
            } else {
                arg_regs_64[idx]
            };

            match arg {
                Expr::Var(name) if name == "state" => {
                    self.emitf(format!("movq %{}, %{arg_reg}", reserved::STATE_PTR));
                }
                Expr::Read(ReadExpr::Reg(reg)) => {
                    let src = self.load_rv_to_temp(*reg, arg_reg);
                    if src != arg_reg {
                        if X::VALUE == 32 {
                            self.emitf(format!("movl %{src}, %{arg_reg}"));
                        } else {
                            self.emitf(format!("movq %{src}, %{arg_reg}"));
                        }
                    }
                }
                Expr::Imm(val) => {
                    let v = X::to_u64(*val);
                    if X::VALUE == 32 {
                        self.emitf(format!("movl ${}, %{arg_reg}", v as i32));
                    } else {
                        if v > 0x7fffffff {
                            self.emitf(format!("movabsq $0x{:x}, %{arg_reg}", v));
                        } else {
                            self.emitf(format!("movq $0x{:x}, %{arg_reg}", v));
                        }
                    }
                }
                _ => {
                    let tmp = self.emit_expr(arg, self.temp1());
                    if tmp != arg_reg {
                        if X::VALUE == 32 {
                            self.emitf(format!("movl %{tmp}, %{arg_reg}"));
                        } else {
                            self.emitf(format!("movq %{tmp}, %{arg_reg}"));
                        }
                    }
                }
            }
        }

        self.emitf(format!("call {fn_name}"));
        self.restore_hot_regs_from_state();
        if X::VALUE == 32 {
            "eax".to_string()
        } else {
            "rax".to_string()
        }
    }

    /// Emit a unary operation.
    pub(super) fn emit_unary_op(&mut self, op: UnaryOp, inner: &Expr<X>, dest: &str) -> String {
        let temp1 = self.temp1();
        let suffix = self.suffix();
        let inner_reg = self.emit_expr(inner, temp1);
        if inner_reg != temp1 {
            self.emitf(format!("mov{suffix} %{inner_reg}, %{temp1}"));
        }

        match op {
            UnaryOp::Neg => self.emitf(format!("neg{suffix} %{temp1}")),
            UnaryOp::Not => self.emitf(format!("not{suffix} %{temp1}")),
            UnaryOp::Sext8 => {
                if X::VALUE == 32 {
                    self.emitf(format!("movsbl %al, %{dest}"));
                } else {
                    self.emitf(format!("movsbq %al, %{dest}"));
                }
                return dest.to_string();
            }
            UnaryOp::Sext16 => {
                if X::VALUE == 32 {
                    self.emitf(format!("movswl %ax, %{dest}"));
                } else {
                    self.emitf(format!("movswq %ax, %{dest}"));
                }
                return dest.to_string();
            }
            UnaryOp::Sext32 => {
                self.emitf(format!("movslq %eax, %{dest}"));
                return dest.to_string();
            }
            UnaryOp::Zext8 => {
                self.emitf(format!("movzbl %al, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            UnaryOp::Zext16 => {
                self.emitf(format!("movzwl %ax, %{}", self.reg_dword(dest)));
                return dest.to_string();
            }
            UnaryOp::Zext32 => {
                self.emit("movl %eax, %eax");
                if dest != temp1 {
                    self.emitf(format!("movq %{temp1}, %{dest}"));
                }
                return dest.to_string();
            }
            UnaryOp::Clz => {
                let zero_label = self.next_label("clz_zero");
                let done_label = self.next_label("clz_done");
                if X::VALUE == 32 {
                    self.emit("testl %eax, %eax");
                    self.emitf(format!("jz {zero_label}"));
                    self.emit("bsrl %eax, %eax");
                    self.emit("xorl $31, %eax");
                    self.emitf(format!("jmp {done_label}"));
                    self.emit_label(&zero_label);
                    self.emit("movl $32, %eax");
                    self.emit_label(&done_label);
                } else {
                    self.emit("testq %rax, %rax");
                    self.emitf(format!("jz {zero_label}"));
                    self.emit("bsrq %rax, %rax");
                    self.emit("xorq $63, %rax");
                    self.emitf(format!("jmp {done_label}"));
                    self.emit_label(&zero_label);
                    self.emit("movq $64, %rax");
                    self.emit_label(&done_label);
                }
                if dest != temp1 {
                    self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                }
                return dest.to_string();
            }
            UnaryOp::Ctz => {
                let zero_label = self.next_label("ctz_zero");
                let done_label = self.next_label("ctz_done");
                if X::VALUE == 32 {
                    self.emit("testl %eax, %eax");
                    self.emitf(format!("jz {zero_label}"));
                    self.emit("bsfl %eax, %eax");
                    self.emitf(format!("jmp {done_label}"));
                    self.emit_label(&zero_label);
                    self.emit("movl $32, %eax");
                    self.emit_label(&done_label);
                } else {
                    self.emit("testq %rax, %rax");
                    self.emitf(format!("jz {zero_label}"));
                    self.emit("bsfq %rax, %rax");
                    self.emitf(format!("jmp {done_label}"));
                    self.emit_label(&zero_label);
                    self.emit("movq $64, %rax");
                    self.emit_label(&done_label);
                }
                if dest != temp1 {
                    self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                }
                return dest.to_string();
            }
            UnaryOp::Cpop => {
                if X::VALUE == 32 {
                    self.emit("popcntl %eax, %eax");
                } else {
                    self.emit("popcntq %rax, %rax");
                }
                if dest != temp1 {
                    self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                }
                return dest.to_string();
            }
            UnaryOp::Rev8 => {
                if X::VALUE == 32 {
                    self.emit("bswapl %eax");
                } else {
                    self.emit("bswapq %rax");
                }
                if dest != temp1 {
                    self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
                }
                return dest.to_string();
            }
            _ => {
                self.emit_comment(&format!("unary op {:?} simplified", op));
            }
        }

        if dest != temp1 {
            self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
        }
        dest.to_string()
    }

    /// Emit a statement.
    pub(super) fn emit_stmt(&mut self, stmt: &Stmt<X>) {
        let temp1 = self.temp1();
        let temp2 = self.temp2();
        let suffix = self.suffix();

        match stmt {
            Stmt::Write { target, value } => match target {
                WriteTarget::Reg(reg) => {
                    if *reg == 0 {
                        return;
                    }
                    self.cold_cache_invalidate(*reg);
                    if let Some(x86_reg) = self.reg_map.get(*reg) {
                        let val_reg = self.emit_expr(value, x86_reg);
                        if val_reg != x86_reg {
                            if X::VALUE == 32 {
                                self.emitf(format!(
                                    "movl %{}, %{}",
                                    self.reg_dword(&val_reg),
                                    self.reg_dword(x86_reg)
                                ));
                            } else {
                                self.emitf(format!("movq %{val_reg}, %{x86_reg}"));
                            }
                        }
                        self.emit_trace_reg_write(*reg, &val_reg);
                    } else {
                        let val_reg = self.emit_expr(value, temp1);
                        self.store_to_rv(*reg, &val_reg);
                        self.emit_trace_reg_write(*reg, &val_reg);
                    }
                }
                WriteTarget::Mem {
                    base,
                    offset,
                    width,
                } => {
                    let val_reg = self.emit_expr(value, temp2);
                    if val_reg != temp2 && val_reg != "rcx" && val_reg != "ecx" {
                        self.emitf(format!("mov{suffix} %{val_reg}, %{temp2}"));
                    }
                    self.emit_expr_as_addr(base);
                    if *offset != 0 {
                        self.emitf(format!("leaq {offset}(%rax), %rax"));
                    }
                    self.apply_address_mode("rax");
                    self.emit_trace_mem_access("rax", &temp2, *width, true);
                    let mem = format!("(%{}, %rax)", reserved::MEMORY_PTR);
                    let (sfx, reg) = match width {
                        1 => ("b", "cl"),
                        2 => ("w", "cx"),
                        4 => ("l", "ecx"),
                        8 => ("q", "rcx"),
                        _ => ("l", "ecx"),
                    };
                    self.emitf(format!("mov{sfx} %{reg}, {mem}"));
                }
                WriteTarget::Pc => {
                    let val_reg = self.emit_expr(value, temp1);
                    let pc_off = self.layout.offset_pc;
                    self.emitf(format!(
                        "mov{suffix} %{val_reg}, {}(%{})",
                        pc_off,
                        reserved::STATE_PTR
                    ));
                }
                WriteTarget::Exited => {
                    let off = self.layout.offset_has_exited;
                    self.emitf(format!("movb $1, {}(%{})", off, reserved::STATE_PTR));
                }
                WriteTarget::ExitCode => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_exit_code;
                    self.emitf(format!(
                        "movb %{}, {}(%{})",
                        self.reg_byte(&val_reg),
                        off,
                        reserved::STATE_PTR
                    ));
                }
                WriteTarget::Temp(idx) => {
                    let val_reg = self.emit_expr(value, temp1);
                    if let Some(offset) = self.temp_slot_offset(*idx) {
                        if X::VALUE == 32 {
                            self.emitf(format!(
                                "movl %{}, {}(%rsp)",
                                self.reg_dword(&val_reg),
                                offset
                            ));
                        } else {
                            self.emitf(format!("movq %{val_reg}, {}(%rsp)", offset));
                        }
                    } else {
                        self.emit_comment(&format!("temp {} out of range", idx));
                    }
                }
                WriteTarget::ResAddr => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_reservation_addr;
                    if X::VALUE == 32 {
                        self.emitf(format!(
                            "movl %{}, {}(%{})",
                            self.reg_dword(&val_reg),
                            off,
                            reserved::STATE_PTR
                        ));
                    } else {
                        self.emitf(format!(
                            "movq %{val_reg}, {}(%{})",
                            off,
                            reserved::STATE_PTR
                        ));
                    }
                }
                WriteTarget::ResValid => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_reservation_valid;
                    self.emitf(format!(
                        "movb %{}, {}(%{})",
                        self.reg_byte(&val_reg),
                        off,
                        reserved::STATE_PTR
                    ));
                }
                _ => self.emit_comment(&format!("unsupported write: {:?}", target)),
            },
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => {
                let cond_reg = self.emit_expr(cond, temp1);
                self.emitf(format!("test{suffix} %{cond_reg}, %{cond_reg}"));
                let else_label = self.next_label("if_else");
                let end_label = self.next_label("if_end");
                self.emitf(format!("jz {else_label}"));
                for s in then_stmts {
                    self.emit_stmt(s);
                }
                if !else_stmts.is_empty() {
                    self.emitf(format!("jmp {end_label}"));
                }
                self.emit_label(&else_label);
                for s in else_stmts {
                    self.emit_stmt(s);
                }
                if !else_stmts.is_empty() {
                    self.emit_label(&end_label);
                }
            }
            Stmt::ExternCall { fn_name, args } => {
                self.emit_comment(&format!("extern call: {fn_name}"));
                self.cold_cache = None;
                let _ = self.emit_extern_call(fn_name, args);
            }
        }
    }

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
