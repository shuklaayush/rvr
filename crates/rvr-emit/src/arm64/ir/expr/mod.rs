//! IR expression lowering for ARM64.

use rvr_ir::{BinaryOp, Expr, InstrIR, ReadExpr, Stmt, Terminator, TernaryOp, UnaryOp, WriteTarget, Xlen};

use crate::arm64::Arm64Emitter;
use crate::arm64::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
    fn signed_imm(&self, val: X::Reg) -> i64 {
        let v = X::to_u64(val);
        if X::VALUE == 64 {
            v as i64
        } else {
            let shift = 64 - X::VALUE as u32;
            ((v << shift) as i64) >> shift
        }
    }

    fn expr_needs_temp1(&self, expr: &Expr<X>) -> bool {
        match expr {
            Expr::Imm(_) => false,
            Expr::PcConst(_) => false,
            Expr::Var(_) => false,
            Expr::Read(ReadExpr::Reg(_))
            | Expr::Read(ReadExpr::Temp(_))
            | Expr::Read(ReadExpr::Csr(_))
            | Expr::Read(ReadExpr::Cycle)
            | Expr::Read(ReadExpr::Instret)
            | Expr::Read(ReadExpr::Pc)
            | Expr::Read(ReadExpr::TraceIdx)
            | Expr::Read(ReadExpr::PcIdx)
            | Expr::Read(ReadExpr::ResAddr)
            | Expr::Read(ReadExpr::ResValid)
            | Expr::Read(ReadExpr::Exited)
            | Expr::Read(ReadExpr::ExitCode) => false,
            Expr::Read(ReadExpr::Mem { .. }) | Expr::Read(ReadExpr::MemAddr { .. }) => true,
            Expr::Unary { .. }
            | Expr::Binary { .. }
            | Expr::Ternary { .. }
            | Expr::ExternCall { .. } => true,
        }
    }

    fn is_zero_expr(&self, expr: &Expr<X>) -> bool {
        match expr {
            Expr::Imm(v) => X::to_u64(*v) == 0,
            Expr::Read(ReadExpr::Reg(0)) => true,
            _ => false,
        }
    }

    fn emit_cmp_with_imm(&mut self, left_reg: &str, imm: i64) {
        if (0..=0xFFF).contains(&imm) {
            self.emitf(format!("cmp {left_reg}, #{imm}"));
        } else if imm < 0 && -imm <= 0xFFF {
            let abs = -imm;
            self.emitf(format!("cmn {left_reg}, #{abs}"));
        } else if let Some((imm12, shift12)) = self.addsub_imm_parts(imm) {
            let shift = if shift12 { ", lsl #12" } else { "" };
            if imm >= 0 {
                self.emitf(format!("cmp {left_reg}, #{imm12}{shift}"));
            } else {
                self.emitf(format!("cmn {left_reg}, #{imm12}{shift}"));
            }
        } else {
            let temp = self.temp2();
            self.load_imm(temp, imm as u64);
            let right_reg = if X::VALUE == 32 {
                self.reg_32(temp)
            } else {
                temp.to_string()
            };
            self.emitf(format!("cmp {left_reg}, {right_reg}"));
        }
    }

    fn emit_store_next_pc_imm(&mut self, next_pc: u64) {
        let pc_offset = self.layout.offset_pc;
        if X::VALUE == 32 {
            self.load_imm("w1", next_pc);
            self.emitf(format!("str w1, [{}, #{}]", reserved::STATE_PTR, pc_offset));
        } else {
            self.load_imm("x1", next_pc);
            self.emitf(format!("str x1, [{}, #{}]", reserved::STATE_PTR, pc_offset));
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

        // Increment instret counter (always 64-bit) in the cached register.
        self.emitf(format!(
            "add {}, {}, #1",
            reserved::INSTRET,
            reserved::INSTRET
        ));

        if !self.config.instret_mode.suspends() {
            return;
        }

        let continue_label = self.next_label("instret_ok");
        let target_offset = self.layout.offset_target_instret;
        self.emitf(format!(
            "ldr x2, [{}, #{}]",
            reserved::STATE_PTR,
            target_offset
        ));
        self.emitf(format!("cmp {}, x2", reserved::INSTRET));
        self.emitf(format!("b.lo {continue_label}"));

        match &instr.terminator {
            Terminator::Fall { target } => {
                let target_pc = target
                    .map(|t| self.inputs.resolve_address(X::to_u64(t)))
                    .unwrap_or(fall_pc);
                if target_pc != current_pc {
                    self.emit_store_next_pc_imm(target_pc);
                } else {
                    self.emit_store_next_pc_imm(fall_pc);
                }
                self.emit("b asm_exit");
            }
            Terminator::Jump { target } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                self.emit_store_next_pc_imm(target_pc);
                self.emit("b asm_exit");
            }
            Terminator::JumpDyn { addr, .. } => {
                let base_reg = self.emit_expr_as_addr(addr);
                if base_reg != "x0" {
                    self.emitf(format!("mov x0, {base_reg}"));
                }
                self.emit("and x0, x0, #-2");
                let pc_offset = self.layout.offset_pc;
                if X::VALUE == 32 {
                    self.emitf(format!("str w0, [{}, #{}]", reserved::STATE_PTR, pc_offset));
                } else {
                    self.emitf(format!("str x0, [{}, #{}]", reserved::STATE_PTR, pc_offset));
                }
                self.emit("b asm_exit");
            }
            Terminator::Branch {
                cond, target, fall, ..
            } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                let fall_target_pc = fall
                    .map(|f| self.inputs.resolve_address(X::to_u64(f)))
                    .unwrap_or(fall_pc);

                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                if !self.try_emit_compare_branch(cond, &target_label, false) {
                    let cond_reg = self.emit_expr(cond, self.temp1());
                    self.emitf(format!("cbnz {cond_reg}, {target_label}"));
                }
                self.emit_store_next_pc_imm(fall_target_pc);
                self.emitf(format!("b {done_label}"));
                self.emit_label(&target_label);
                self.emit_store_next_pc_imm(target_pc);
                self.emit_label(&done_label);
                self.emit("b asm_exit");
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
            "ldr x2, [{}, #{}]",
            reserved::STATE_PTR,
            target_offset
        ));
        self.emitf(format!("cmp {}, x2", reserved::INSTRET));
        self.emitf(format!("b.lo {continue_label}"));

        match &instr.terminator {
            Terminator::Fall { target } => {
                let target_pc = target
                    .map(|t| self.inputs.resolve_address(X::to_u64(t)))
                    .unwrap_or(fall_pc);
                if target_pc != current_pc {
                    self.emit_store_next_pc_imm(target_pc);
                } else {
                    self.emit_store_next_pc_imm(fall_pc);
                }
                self.emit("b asm_exit");
            }
            Terminator::Jump { target } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                self.emit_store_next_pc_imm(target_pc);
                self.emit("b asm_exit");
            }
            Terminator::JumpDyn { addr, .. } => {
                let base_reg = self.emit_expr_as_addr(addr);
                if base_reg != "x0" {
                    self.emitf(format!("mov x0, {base_reg}"));
                }
                self.emit("and x0, x0, #-2");
                let pc_offset = self.layout.offset_pc;
                if X::VALUE == 32 {
                    self.emitf(format!("str w0, [{}, #{}]", reserved::STATE_PTR, pc_offset));
                } else {
                    self.emitf(format!("str x0, [{}, #{}]", reserved::STATE_PTR, pc_offset));
                }
                self.emit("b asm_exit");
            }
            Terminator::Branch {
                cond, target, fall, ..
            } => {
                let target_pc = self.inputs.resolve_address(X::to_u64(*target));
                let fall_target_pc = fall
                    .map(|f| self.inputs.resolve_address(X::to_u64(f)))
                    .unwrap_or(fall_pc);

                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                if !self.try_emit_compare_branch(cond, &target_label, false) {
                    let cond_reg = self.emit_expr(cond, self.temp1());
                    self.emitf(format!("cbnz {cond_reg}, {target_label}"));
                }
                self.emit_store_next_pc_imm(fall_target_pc);
                self.emitf(format!("b {done_label}"));
                self.emit_label(&target_label);
                self.emit_store_next_pc_imm(target_pc);
                self.emit_label(&done_label);
                self.emit("b asm_exit");
            }
            Terminator::Exit { .. } | Terminator::Trap { .. } => {}
        }

        self.emit_label(&continue_label);
    }

    pub(super) fn try_emit_compare_branch(
        &mut self,
        cond: &Expr<X>,
        label: &str,
        invert: bool,
    ) -> bool {
        let (op, left, right) = match cond {
            Expr::Binary { op, left, right } => (op, left, right),
            _ => return false,
        };
        if matches!(op, BinaryOp::Or | BinaryOp::And) {
            let is_cmp = |expr: &Expr<X>| {
                matches!(
                    expr,
                    Expr::Binary {
                        op: BinaryOp::Eq
                            | BinaryOp::Ne
                            | BinaryOp::Lt
                            | BinaryOp::Ge
                            | BinaryOp::Ltu
                            | BinaryOp::Geu,
                        ..
                    }
                )
            };
            if is_cmp(left) && is_cmp(right) {
                let skip_label = self.next_label("bool_skip");
                match (op, invert) {
                    (BinaryOp::Or, false) => {
                        let _ = self.try_emit_compare_branch(left, label, false);
                        let _ = self.try_emit_compare_branch(right, label, false);
                        return true;
                    }
                    (BinaryOp::Or, true) => {
                        let _ = self.try_emit_compare_branch(left, &skip_label, false);
                        let _ = self.try_emit_compare_branch(right, &skip_label, false);
                        self.emitf(format!("b {label}"));
                        self.emit_label(&skip_label);
                        return true;
                    }
                    (BinaryOp::And, false) => {
                        let _ = self.try_emit_compare_branch(left, &skip_label, true);
                        let _ = self.try_emit_compare_branch(right, label, false);
                        self.emit_label(&skip_label);
                        return true;
                    }
                    (BinaryOp::And, true) => {
                        let _ = self.try_emit_compare_branch(left, label, true);
                        let _ = self.try_emit_compare_branch(right, label, true);
                        return true;
                    }
                    _ => {}
                }
            }
        }
        let cond_code = match (op, invert) {
            (BinaryOp::Eq, false) | (BinaryOp::Ne, true) => "eq",
            (BinaryOp::Ne, false) | (BinaryOp::Eq, true) => "ne",
            (BinaryOp::Lt, false) | (BinaryOp::Ge, true) => "lt",
            (BinaryOp::Ge, false) | (BinaryOp::Lt, true) => "ge",
            (BinaryOp::Ltu, false) | (BinaryOp::Geu, true) => "lo",
            (BinaryOp::Geu, false) | (BinaryOp::Ltu, true) => "hs",
            _ => return false,
        };

        let temp1 = self.temp1();
        let temp2 = self.temp2();
        let left_reg = self.emit_expr(left, temp1);
        let left_reg = if X::VALUE == 32 {
            self.reg_32(&left_reg)
        } else {
            left_reg
        };

        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) && self.is_zero_expr(right) {
            let branch = if cond_code == "eq" { "cbz" } else { "cbnz" };
            self.emitf(format!("{branch} {left_reg}, {label}"));
            return true;
        }

        let left_is_temp1 = left_reg == temp1 || left_reg == self.reg_32(temp1);
        let needs_spill = left_is_temp1 && self.expr_needs_temp1(right);
        let spill = if needs_spill {
            self.alloc_spill_slot()
        } else {
            None
        };
        if let Some(off) = spill {
            if X::VALUE == 32 {
                self.emitf(format!("str {}, [sp, #{}]", self.reg_32(&left_reg), off));
            } else {
                self.emitf(format!("str {left_reg}, [sp, #{}]", off));
            }
        }

        if self.is_zero_expr(right) {
            self.emit_cmp_with_imm(&left_reg, 0);
        } else if let Expr::Imm(v) = right.as_ref() {
            let imm = self.signed_imm(*v);
            self.emit_cmp_with_imm(&left_reg, imm);
        } else {
            let right_reg = self.emit_expr(right, temp2);
            let right_reg = if X::VALUE == 32 {
                self.reg_32(&right_reg)
            } else {
                right_reg
            };
            self.emitf(format!("cmp {left_reg}, {right_reg}"));
        }

        if let Some(off) = spill {
            if X::VALUE == 32 {
                self.emitf(format!("ldr {}, [sp, #{}]", self.reg_32(&left_reg), off));
            } else {
                self.emitf(format!("ldr {left_reg}, [sp, #{}]", off));
            }
            self.release_spill_slot();
        }

        self.emitf(format!("b.{cond_code} {label}"));
        true
    }

    fn addsub_imm_parts(&self, imm: i64) -> Option<(u16, bool)> {
        if imm == i64::MIN {
            return None;
        }
        let abs = imm.unsigned_abs();
        if abs <= 0xFFF {
            Some((abs as u16, false))
        } else if abs <= 0xFFF000 && (abs & 0xFFF) == 0 {
            Some(((abs >> 12) as u16, true))
        } else {
            None
        }
    }

    pub(super) fn cmp_from_temp_branch<'a>(
        &self,
        stmts: &'a [Stmt<X>],
        cond: &Expr<X>,
    ) -> Option<(&'a Expr<X>, &'a Expr<X>, BinaryOp)> {
        let temp_idx = match cond {
            Expr::Read(ReadExpr::Temp(idx)) => *idx,
            _ => return None,
        };
        let last = stmts.last()?;
        match last {
            Stmt::Write { target, value } => {
                if !matches!(target, WriteTarget::Temp(idx) if *idx == temp_idx) {
                    return None;
                }
                match value {
                    Expr::Binary { op, left, right } => match op {
                        BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Ge
                        | BinaryOp::Ltu
                        | BinaryOp::Geu => Some((left.as_ref(), right.as_ref(), *op)),
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Emit an expression for use as a 64-bit address.
    /// For RV32, ensures the result is zero-extended to 64-bit.
    pub(super) fn emit_expr_as_addr(&mut self, expr: &Expr<X>) -> String {
        match expr {
            Expr::Read(ReadExpr::Reg(reg)) => self.load_rv_as_addr(*reg, "x0"),
            Expr::Imm(val) => {
                let v = X::to_u64(*val);
                self.load_imm("x0", v);
                "x0".to_string()
            }
            _ => {
                let result = self.emit_expr(expr, self.temp1());
                let result64 = self.reg_64(&result);
                if result64 != "x0" {
                    self.emitf(format!("mov x0, {result64}"));
                }
                "x0".to_string()
            }
        }
    }

    /// Emit an expression, returning which ARM64 register holds the result.
    #[allow(clippy::collapsible_if)]
    pub(super) fn emit_expr(&mut self, expr: &Expr<X>, dest: &str) -> String {
        if self.config.perf_mode
            && matches!(
                expr,
                Expr::Read(ReadExpr::Csr(_))
                    | Expr::Read(ReadExpr::Cycle)
                    | Expr::Read(ReadExpr::Instret)
            )
        {
            self.emitf(format!("mov {dest}, #0"));
            return dest.to_string();
        }
        match expr {
            Expr::Imm(val) => {
                let v = X::to_u64(*val);
                if v == 0 {
                    self.emitf(format!("mov {dest}, #0"));
                } else {
                    self.load_imm(dest, v);
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
                let base_reg = self.emit_expr_as_addr(base);
                if *offset != 0 {
                    self.emit_add_offset("x0", &base_reg, (*offset).into());
                } else if base_reg != "x0" {
                    self.emitf(format!("mov x0, {base_reg}"));
                }
                self.apply_address_mode("x0");
                let mem = format!("{}, x0", reserved::MEMORY_PTR);
                self.emit_load_from_mem(&mem, dest, *width, *signed);
                self.emit_trace_mem_access("x0", dest, *width, false);
                dest.to_string()
            }
            Expr::Read(ReadExpr::MemAddr {
                addr,
                width,
                signed,
            }) => {
                let base_reg = self.emit_expr_as_addr(addr);
                if base_reg != "x0" {
                    self.emitf(format!("mov x0, {base_reg}"));
                }
                self.apply_address_mode("x0");
                let mem = format!("{}, x0", reserved::MEMORY_PTR);
                self.emit_load_from_mem(&mem, dest, *width, *signed);
                self.emit_trace_mem_access("x0", dest, *width, false);
                dest.to_string()
            }
            Expr::PcConst(val) => {
                let v = X::to_u64(*val);
                self.load_imm(dest, v);
                dest.to_string()
            }
            Expr::Read(ReadExpr::Csr(csr)) => {
                let instret_off = self.layout.offset_instret;
                match *csr {
                    // cycle/instret (user) and mcycle/minstret (machine)
                    0xC00 | 0xC02 | 0xB00 | 0xB02 => {
                        if self.config.instret_mode.counts() {
                            if X::VALUE == 32 {
                                let instret32 = self.reg_32(reserved::INSTRET);
                                self.emitf(format!("mov {dest}, {instret32}"));
                            } else {
                                self.emitf(format!("mov {dest}, {}", reserved::INSTRET));
                            }
                        } else {
                            self.emitf(format!(
                                "ldr {dest}, [{}, #{}]",
                                reserved::STATE_PTR,
                                instret_off
                            ));
                        }
                    }
                    // cycleh/instreth/mcycleh/minstreth (upper 32 bits for RV32)
                    0xC80 | 0xC82 | 0xB80 | 0xB82 if X::VALUE == 32 => {
                        if self.config.instret_mode.counts() {
                            let dest64 = self.reg_64(dest);
                            self.emitf(format!("lsr {dest64}, {}, #32", reserved::INSTRET));
                        } else {
                            self.emitf(format!(
                                "ldr {dest}, [{}, #{}]",
                                reserved::STATE_PTR,
                                instret_off + 4
                            ));
                        }
                    }
                    _ => {
                        self.emit_comment(&format!("CSR 0x{:03x} not implemented", csr));
                        self.emitf(format!("mov {dest}, #0"));
                    }
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::Cycle) | Expr::Read(ReadExpr::Instret) => {
                if self.config.instret_mode.counts() {
                    if X::VALUE == 32 {
                        let instret32 = self.reg_32(reserved::INSTRET);
                        self.emitf(format!("mov {dest}, {instret32}"));
                    } else {
                        self.emitf(format!("mov {dest}, {}", reserved::INSTRET));
                    }
                } else {
                    let instret_off = self.layout.offset_instret;
                    self.emitf(format!(
                        "ldr {dest}, [{}, #{}]",
                        reserved::STATE_PTR,
                        instret_off
                    ));
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::Pc) => {
                let pc_off = self.layout.offset_pc;
                self.emitf(format!(
                    "ldr {dest}, [{}, #{}]",
                    reserved::STATE_PTR,
                    pc_off
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::Exited) => {
                let off = self.layout.offset_has_exited;
                self.emitf(format!(
                    "ldrb {}, [{}, #{}]",
                    self.reg_32(dest),
                    reserved::STATE_PTR,
                    off
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::ExitCode) => {
                let off = self.layout.offset_exit_code;
                self.emitf(format!(
                    "ldrb {}, [{}, #{}]",
                    self.reg_32(dest),
                    reserved::STATE_PTR,
                    off
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::ResAddr) => {
                let off = self.layout.offset_reservation_addr;
                if X::VALUE == 32 {
                    self.emitf(format!(
                        "ldr {}, [{}, #{}]",
                        self.reg_32(dest),
                        reserved::STATE_PTR,
                        off
                    ));
                } else {
                    self.emitf(format!("ldr {dest}, [{}, #{}]", reserved::STATE_PTR, off));
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::ResValid) => {
                let off = self.layout.offset_reservation_valid;
                self.emitf(format!(
                    "ldrb {}, [{}, #{}]",
                    self.reg_32(dest),
                    reserved::STATE_PTR,
                    off
                ));
                dest.to_string()
            }
            Expr::Read(ReadExpr::Temp(idx)) => {
                if let Some(offset) = self.temp_slot_offset(*idx) {
                    if X::VALUE == 32 {
                        self.emitf(format!("ldr {}, [sp, #{}]", self.reg_32(dest), offset));
                    } else {
                        self.emitf(format!("ldr {dest}, [sp, #{}]", offset));
                    }
                } else {
                    self.emit_comment(&format!("temp {} out of range", idx));
                    self.emitf(format!("mov {dest}, #0"));
                }
                dest.to_string()
            }
            Expr::Var(name) => {
                if name == "state" {
                    let state = reserved::STATE_PTR;
                    if X::VALUE == 32 {
                        let dest32 = self.reg_32(dest);
                        self.emitf(format!("mov {dest32}, {}", self.reg_32(state)));
                    } else {
                        self.emitf(format!("mov {dest}, {state}"));
                    }
                } else {
                    self.emit_comment(&format!("unsupported var: {name}"));
                    self.emitf(format!("mov {dest}, #0"));
                }
                dest.to_string()
            }
            Expr::Binary { op, left, right } => self.emit_binary_op(*op, left, right, dest),
            Expr::Unary { op, expr: inner } => self.emit_unary_op(*op, inner, dest),
            Expr::ExternCall { name, args, .. } => {
                let ret = self.emit_extern_call(name, args);
                if ret != dest {
                    self.emitf(format!("mov {dest}, {ret}"));
                }
                dest.to_string()
            }
            Expr::Ternary {
                op: TernaryOp::Select,
                first: cond,
                second: then_val,
                third: else_val,
            } => {
                // Evaluate then/else into temp slots to avoid clobber.
                if let (Some(then_off), Some(else_off)) =
                    (self.temp_slot_offset(0), self.temp_slot_offset(1))
                {
                    let then_reg = self.emit_expr(then_val, self.temp1());
                    if X::VALUE == 32 {
                        self.emitf(format!(
                            "str {}, [sp, #{}]",
                            self.reg_32(&then_reg),
                            then_off
                        ));
                    } else {
                        self.emitf(format!("str {then_reg}, [sp, #{}]", then_off));
                    }

                    let else_reg = self.emit_expr(else_val, self.temp1());
                    if X::VALUE == 32 {
                        self.emitf(format!(
                            "str {}, [sp, #{}]",
                            self.reg_32(&else_reg),
                            else_off
                        ));
                    } else {
                        self.emitf(format!("str {else_reg}, [sp, #{}]", else_off));
                    }

                    let cond_reg = self.emit_expr(cond, self.temp1());

                    let (tmp1, tmp2) = if X::VALUE == 32 {
                        ("w1", "w2")
                    } else {
                        ("x1", "x2")
                    };
                    self.emitf(format!("ldr {tmp1}, [sp, #{}]", then_off));
                    self.emitf(format!("ldr {tmp2}, [sp, #{}]", else_off));
                    self.emitf(format!("cmp {cond_reg}, #0"));
                    self.emitf(format!("csel {dest}, {tmp1}, {tmp2}, ne"));
                } else {
                    self.emit_comment("select: temp slots unavailable");
                    self.emitf(format!("mov {dest}, #0"));
                }
                dest.to_string()
            }
            _ => {
                self.emit_comment(&format!("unsupported expr: {:?}", expr));
                self.emitf(format!("mov {dest}, #0"));
                dest.to_string()
            }
        }
    }

    /// Helper to emit a load from memory address in a register.
    fn emit_load_from_mem(&mut self, addr: &str, dest: &str, width: u8, signed: bool) {
        let dest32 = self.reg_32(dest);
        match (width, signed, X::VALUE) {
            (1, false, _) => self.emitf(format!("ldrb {dest32}, [{addr}]")),
            (1, true, 32) => self.emitf(format!("ldrsb {dest32}, [{addr}]")),
            (1, true, 64) => self.emitf(format!("ldrsb {dest}, [{addr}]")),
            (2, false, _) => self.emitf(format!("ldrh {dest32}, [{addr}]")),
            (2, true, 32) => self.emitf(format!("ldrsh {dest32}, [{addr}]")),
            (2, true, 64) => self.emitf(format!("ldrsh {dest}, [{addr}]")),
            (4, _, 32) => self.emitf(format!("ldr {dest32}, [{addr}]")),
            (4, false, 64) => self.emitf(format!("ldr {dest32}, [{addr}]")), // zero-extends
            (4, true, 64) => self.emitf(format!("ldrsw {dest}, [{addr}]")),
            (8, _, _) => self.emitf(format!("ldr {dest}, [{addr}]")),
            _ => self.emitf(format!("ldr {dest32}, [{addr}]")),
        }
    }

}

mod ops_binary;
mod ops_unary;
