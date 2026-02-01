//! IR translation for ARM64 assembly.
//!
//! Translates IR expressions, statements, and terminators to ARM64 assembly.

use rvr_ir::{
    BinaryOp, Expr, InstrIR, ReadExpr, Stmt, Terminator, TernaryOp, UnaryOp, WriteTarget, Xlen,
};

use super::Arm64Emitter;
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
        if imm >= 0 && imm <= 0xFFF {
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

    fn try_emit_compare_branch(&mut self, cond: &Expr<X>, label: &str, invert: bool) -> bool {
        let (op, left, right) = match cond {
            Expr::Binary { op, left, right } => (op, left, right),
            _ => return false,
        };
        if matches!(op, BinaryOp::Or | BinaryOp::And) {
            let is_cmp = |expr: &Expr<X>| match expr {
                Expr::Binary { op, .. } => match op {
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Ge
                    | BinaryOp::Ltu
                    | BinaryOp::Geu => true,
                    _ => false,
                },
                _ => false,
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
        let abs = imm.abs() as u64;
        if abs <= 0xFFF {
            Some((abs as u16, false))
        } else if abs <= 0xFFF000 && (abs & 0xFFF) == 0 {
            Some(((abs >> 12) as u16, true))
        } else {
            None
        }
    }

    fn cmp_from_temp_branch<'a>(
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
    pub(super) fn emit_expr(&mut self, expr: &Expr<X>, dest: &str) -> String {
        if self.config.perf_mode {
            if matches!(
                expr,
                Expr::Read(ReadExpr::Csr(_))
                    | Expr::Read(ReadExpr::Cycle)
                    | Expr::Read(ReadExpr::Instret)
            ) {
                self.emitf(format!("mov {dest}, #0"));
                return dest.to_string();
            }
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
                        self.emitf(format!("mov {dest}, {left_reg}"));
                    }
                    return dest.to_string();
                }
                BinaryOp::And if v == 0 => {
                    self.emitf(format!("mov {dest}, #0"));
                    return dest.to_string();
                }
                BinaryOp::And if v == full_mask => {
                    let left_reg = self.emit_expr(left, dest);
                    if left_reg != dest {
                        self.emitf(format!("mov {dest}, {left_reg}"));
                    }
                    return dest.to_string();
                }
                BinaryOp::Mul if v == 0 => {
                    self.emitf(format!("mov {dest}, #0"));
                    return dest.to_string();
                }
                BinaryOp::Mul if v == 1 => {
                    let left_reg = self.emit_expr(left, dest);
                    if left_reg != dest {
                        self.emitf(format!("mov {dest}, {left_reg}"));
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
                    if self.reg_64(mapped) == self.reg_64(dest) {
                        let dest_reg = mapped;
                        let right_is_imm = matches!(right, Expr::Imm(_));
                        let right_val = if let Expr::Imm(imm) = right {
                            X::to_u64(*imm)
                        } else {
                            0
                        };
                        match op {
                            BinaryOp::Add | BinaryOp::Sub => {
                                if right_is_imm {
                                    let signed = self.signed_imm(X::from_u64(right_val));
                                    if let Some((imm12, shift12)) = self.addsub_imm_parts(signed) {
                                        let shift = if shift12 { ", lsl #12" } else { "" };
                                        match op {
                                            BinaryOp::Add if signed >= 0 => {
                                                self.emitf(format!(
                                                    "add {dest_reg}, {dest_reg}, #{imm12}{shift}"
                                                ));
                                                return dest.to_string();
                                            }
                                            BinaryOp::Add => {
                                                self.emitf(format!(
                                                    "sub {dest_reg}, {dest_reg}, #{imm12}{shift}"
                                                ));
                                                return dest.to_string();
                                            }
                                            BinaryOp::Sub if signed >= 0 => {
                                                self.emitf(format!(
                                                    "sub {dest_reg}, {dest_reg}, #{imm12}{shift}"
                                                ));
                                                return dest.to_string();
                                            }
                                            BinaryOp::Sub => {
                                                self.emitf(format!(
                                                    "add {dest_reg}, {dest_reg}, #{imm12}{shift}"
                                                ));
                                                return dest.to_string();
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            BinaryOp::And | BinaryOp::Or | BinaryOp::Xor => {
                                if right_is_imm && self.is_logical_imm(right_val, X::VALUE == 32) {
                                    let op_str = match op {
                                        BinaryOp::And => "and",
                                        BinaryOp::Or => "orr",
                                        BinaryOp::Xor => "eor",
                                        _ => unreachable!(),
                                    };
                                    self.emitf(format!(
                                        "{op_str} {dest_reg}, {dest_reg}, #{right_val}"
                                    ));
                                    return dest.to_string();
                                }
                            }
                            _ => {}
                        }

                        let mut right_reg = self.emit_expr(right, temp2);
                        if X::VALUE == 32 && right_reg.starts_with('x') {
                            right_reg = self.reg_32(&right_reg);
                        }
                        match op {
                            BinaryOp::Add => {
                                self.emitf(format!("add {dest_reg}, {dest_reg}, {right_reg}"))
                            }
                            BinaryOp::Sub => {
                                self.emitf(format!("sub {dest_reg}, {dest_reg}, {right_reg}"))
                            }
                            BinaryOp::And => {
                                self.emitf(format!("and {dest_reg}, {dest_reg}, {right_reg}"))
                            }
                            BinaryOp::Or => {
                                self.emitf(format!("orr {dest_reg}, {dest_reg}, {right_reg}"))
                            }
                            BinaryOp::Xor => {
                                self.emitf(format!("eor {dest_reg}, {dest_reg}, {right_reg}"))
                            }
                            BinaryOp::Mul => {
                                self.emitf(format!("mul {dest_reg}, {dest_reg}, {right_reg}"))
                            }
                            _ => {}
                        }
                        return dest.to_string();
                    }
                }
            }
        }

        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Xor
                | BinaryOp::Mul
        ) {
            if let Expr::Read(ReadExpr::Reg(left_reg)) = left {
                if let Some(left_mapped) = self.reg_map.get(*left_reg) {
                    let left_src = if X::VALUE == 32 {
                        self.reg_32(left_mapped)
                    } else {
                        left_mapped.to_string()
                    };
                    if let Expr::Read(ReadExpr::Reg(right_reg)) = right {
                        if let Some(right_mapped) = self.reg_map.get(*right_reg) {
                            let right_src = if X::VALUE == 32 {
                                self.reg_32(right_mapped)
                            } else {
                                right_mapped.to_string()
                            };
                            let op_str = match op {
                                BinaryOp::Add => "add",
                                BinaryOp::Sub => "sub",
                                BinaryOp::And => "and",
                                BinaryOp::Or => "orr",
                                BinaryOp::Xor => "eor",
                                BinaryOp::Mul => "mul",
                                _ => "",
                            };
                            if !op_str.is_empty() {
                                self.emitf(format!("{op_str} {dest}, {left_src}, {right_src}"));
                                return dest.to_string();
                            }
                        }
                    } else if let Expr::Imm(imm) = right {
                        let signed = self.signed_imm(*imm);
                        match op {
                            BinaryOp::Add | BinaryOp::Sub => {
                                if let Some((imm12, shift12)) = self.addsub_imm_parts(signed) {
                                    let shift = if shift12 { ", lsl #12" } else { "" };
                                    match op {
                                        BinaryOp::Add if signed >= 0 => {
                                            self.emitf(format!(
                                                "add {dest}, {left_src}, #{imm12}{shift}"
                                            ));
                                            return dest.to_string();
                                        }
                                        BinaryOp::Add => {
                                            self.emitf(format!(
                                                "sub {dest}, {left_src}, #{imm12}{shift}"
                                            ));
                                            return dest.to_string();
                                        }
                                        BinaryOp::Sub if signed >= 0 => {
                                            self.emitf(format!(
                                                "sub {dest}, {left_src}, #{imm12}{shift}"
                                            ));
                                            return dest.to_string();
                                        }
                                        BinaryOp::Sub => {
                                            self.emitf(format!(
                                                "add {dest}, {left_src}, #{imm12}{shift}"
                                            ));
                                            return dest.to_string();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            BinaryOp::And | BinaryOp::Or | BinaryOp::Xor => {
                                let v = X::to_u64(*imm);
                                if self.is_logical_imm(v, X::VALUE == 32) {
                                    let op_str = match op {
                                        BinaryOp::And => "and",
                                        BinaryOp::Or => "orr",
                                        BinaryOp::Xor => "eor",
                                        _ => "",
                                    };
                                    if !op_str.is_empty() {
                                        self.emitf(format!("{op_str} {dest}, {left_src}, #{v}"));
                                        return dest.to_string();
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            if let Expr::Imm(imm) = right {
                let signed = self.signed_imm(*imm);
                if let Some((imm12, shift12)) = self.addsub_imm_parts(signed) {
                    let shift = if shift12 { ", lsl #12" } else { "" };
                    let mut left_reg = self.emit_expr(left, temp1);
                    if X::VALUE == 32 && left_reg.starts_with('x') {
                        left_reg = self.reg_32(&left_reg);
                    }
                    match op {
                        BinaryOp::Add if signed >= 0 => {
                            self.emitf(format!("add {dest}, {left_reg}, #{imm12}{shift}"));
                            return dest.to_string();
                        }
                        BinaryOp::Add => {
                            self.emitf(format!("sub {dest}, {left_reg}, #{imm12}{shift}"));
                            return dest.to_string();
                        }
                        BinaryOp::Sub if signed >= 0 => {
                            self.emitf(format!("sub {dest}, {left_reg}, #{imm12}{shift}"));
                            return dest.to_string();
                        }
                        BinaryOp::Sub => {
                            self.emitf(format!("add {dest}, {left_reg}, #{imm12}{shift}"));
                            return dest.to_string();
                        }
                        _ => {}
                    }
                }
            }
        }

        // Load left operand
        let mut left_reg = self.emit_expr(left, temp1);
        if X::VALUE == 32 && left_reg.starts_with('x') {
            left_reg = self.reg_32(&left_reg);
        }
        if left_reg != temp1 {
            self.emitf(format!("mov {temp1}, {left_reg}"));
        }
        // Handle special cases
        match op {
            // Shifts
            BinaryOp::Sll | BinaryOp::Srl | BinaryOp::Sra => {
                return self.emit_shift_op(op, right, temp1, dest);
            }
            BinaryOp::SllW | BinaryOp::SrlW | BinaryOp::SraW => {
                return self.emit_shift_word_op(op, right, temp1, dest);
            }
            // Word operations (RV64 only)
            BinaryOp::AddW => {
                let left_spill = self.maybe_spill_left(right, temp1);
                let right_reg = self.emit_expr(right, temp2);
                let t1_32 = self.reg_32(temp1);
                let r_32 = self.reg_32(&right_reg);
                self.restore_spilled_left(left_spill, temp1);
                self.emitf(format!("add {t1_32}, {t1_32}, {r_32}"));
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, {t1_32}"));
                return dest.to_string();
            }
            BinaryOp::SubW => {
                let left_spill = self.maybe_spill_left(right, temp1);
                let right_reg = self.emit_expr(right, temp2);
                let t1_32 = self.reg_32(temp1);
                let r_32 = self.reg_32(&right_reg);
                self.restore_spilled_left(left_spill, temp1);
                self.emitf(format!("sub {t1_32}, {t1_32}, {r_32}"));
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, {t1_32}"));
                return dest.to_string();
            }
            BinaryOp::MulW => {
                let left_spill = self.maybe_spill_left(right, temp1);
                let right_reg = self.emit_expr(right, temp2);
                let t1_32 = self.reg_32(temp1);
                let r_32 = self.reg_32(&right_reg);
                self.restore_spilled_left(left_spill, temp1);
                self.emitf(format!("mul {t1_32}, {t1_32}, {r_32}"));
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, {t1_32}"));
                return dest.to_string();
            }
            // Division
            BinaryOp::Div | BinaryOp::DivU | BinaryOp::Rem | BinaryOp::RemU => {
                return self.emit_div_op(op, right, temp1, temp2, dest);
            }
            BinaryOp::DivW | BinaryOp::DivUW | BinaryOp::RemW | BinaryOp::RemUW => {
                return self.emit_div_word_op(op, right, temp1, temp2, dest);
            }
            // Multiplication high bits
            BinaryOp::MulH | BinaryOp::MulHU | BinaryOp::MulHSU => {
                return self.emit_mulh_op(op, right, temp1, temp2, dest);
            }
            _ => {}
        }

        let left_spill = self.maybe_spill_left(right, temp1);

        // Load right operand or use immediate
        let (right_is_imm, mut right_val) = if let Expr::Imm(imm) = right {
            let v = X::to_u64(*imm);
            if v <= 0xFFF {
                (true, format!("#{v}"))
            } else {
                let right_reg = self.emit_expr(right, temp2);
                (false, right_reg)
            }
        } else {
            let right_reg = self.emit_expr(right, temp2);
            (false, right_reg)
        };
        if X::VALUE == 32 && right_val.starts_with('x') {
            right_val = self.reg_32(&right_val);
        }
        self.restore_spilled_left(left_spill, temp1);

        match op {
            BinaryOp::Add => {
                self.emitf(format!("add {dest}, {temp1}, {right_val}"));
            }
            BinaryOp::Sub => {
                self.emitf(format!("sub {dest}, {temp1}, {right_val}"));
            }
            BinaryOp::And => {
                if right_is_imm {
                    if let Expr::Imm(imm) = right {
                        let v = X::to_u64(*imm);
                        if self.is_logical_imm(v, X::VALUE == 32) {
                            self.emitf(format!("and {dest}, {temp1}, #{v}"));
                        } else {
                            let right_reg = self.emit_expr(right, temp2);
                            self.emitf(format!("and {dest}, {temp1}, {right_reg}"));
                        }
                    } else {
                        let right_reg = self.emit_expr(right, temp2);
                        self.emitf(format!("and {dest}, {temp1}, {right_reg}"));
                    }
                } else {
                    self.emitf(format!("and {dest}, {temp1}, {right_val}"));
                }
            }
            BinaryOp::Or => {
                if right_is_imm {
                    if let Expr::Imm(imm) = right {
                        let v = X::to_u64(*imm);
                        if self.is_logical_imm(v, X::VALUE == 32) {
                            self.emitf(format!("orr {dest}, {temp1}, #{v}"));
                        } else {
                            let right_reg = self.emit_expr(right, temp2);
                            self.emitf(format!("orr {dest}, {temp1}, {right_reg}"));
                        }
                    } else {
                        let right_reg = self.emit_expr(right, temp2);
                        self.emitf(format!("orr {dest}, {temp1}, {right_reg}"));
                    }
                } else {
                    self.emitf(format!("orr {dest}, {temp1}, {right_val}"));
                }
            }
            BinaryOp::Xor => {
                if right_is_imm {
                    if let Expr::Imm(imm) = right {
                        let v = X::to_u64(*imm);
                        if self.is_logical_imm(v, X::VALUE == 32) {
                            self.emitf(format!("eor {dest}, {temp1}, #{v}"));
                        } else {
                            let right_reg = self.emit_expr(right, temp2);
                            self.emitf(format!("eor {dest}, {temp1}, {right_reg}"));
                        }
                    } else {
                        let right_reg = self.emit_expr(right, temp2);
                        self.emitf(format!("eor {dest}, {temp1}, {right_reg}"));
                    }
                } else {
                    self.emitf(format!("eor {dest}, {temp1}, {right_val}"));
                }
            }
            BinaryOp::Mul => {
                if right_is_imm {
                    let right_reg = self.emit_expr(right, temp2);
                    self.emitf(format!("mul {dest}, {temp1}, {right_reg}"));
                } else {
                    self.emitf(format!("mul {dest}, {temp1}, {right_val}"));
                }
            }
            // Comparisons
            BinaryOp::Eq => {
                self.emitf(format!("cmp {temp1}, {right_val}"));
                self.emitf(format!("cset {dest}, eq"));
            }
            BinaryOp::Ne => {
                self.emitf(format!("cmp {temp1}, {right_val}"));
                self.emitf(format!("cset {dest}, ne"));
            }
            BinaryOp::Lt => {
                self.emitf(format!("cmp {temp1}, {right_val}"));
                self.emitf(format!("cset {dest}, lt"));
            }
            BinaryOp::Ge => {
                self.emitf(format!("cmp {temp1}, {right_val}"));
                self.emitf(format!("cset {dest}, ge"));
            }
            BinaryOp::Ltu => {
                self.emitf(format!("cmp {temp1}, {right_val}"));
                self.emitf(format!("cset {dest}, lo"));
            }
            BinaryOp::Geu => {
                self.emitf(format!("cmp {temp1}, {right_val}"));
                self.emitf(format!("cset {dest}, hs"));
            }
            _ => {
                self.emit_comment(&format!("unsupported binary op: {:?}", op));
            }
        }

        dest.to_string()
    }

    fn emit_extern_call(&mut self, fn_name: &str, args: &[Expr<X>]) -> String {
        // Save hot regs to state before calling out.
        self.save_hot_regs_to_state();

        // Load arguments in reverse order to avoid clobbering temp registers.
        let max_args = 8usize;
        for (idx, arg) in args.iter().enumerate().take(max_args).rev() {
            let arg_reg = if X::VALUE == 32 {
                if matches!(arg, Expr::Var(name) if name == "state") {
                    match idx {
                        0 => "x0",
                        1 => "x1",
                        2 => "x2",
                        3 => "x3",
                        4 => "x4",
                        5 => "x5",
                        6 => "x6",
                        7 => "x7",
                        _ => unreachable!(),
                    }
                } else {
                    match idx {
                        0 => "w0",
                        1 => "w1",
                        2 => "w2",
                        3 => "w3",
                        4 => "w4",
                        5 => "w5",
                        6 => "w6",
                        7 => "w7",
                        _ => unreachable!(),
                    }
                }
            } else {
                match idx {
                    0 => "x0",
                    1 => "x1",
                    2 => "x2",
                    3 => "x3",
                    4 => "x4",
                    5 => "x5",
                    6 => "x6",
                    7 => "x7",
                    _ => unreachable!(),
                }
            };

            match arg {
                Expr::Var(name) if name == "state" => {
                    self.emitf(format!("mov {arg_reg}, {}", reserved::STATE_PTR));
                }
                Expr::Read(ReadExpr::Reg(reg)) => {
                    let src = self.load_rv_to_temp(*reg, arg_reg);
                    if src != arg_reg {
                        self.emitf(format!("mov {arg_reg}, {src}"));
                    }
                }
                Expr::Imm(val) => {
                    let v = X::to_u64(*val);
                    self.load_imm(arg_reg, v);
                }
                _ => {
                    let tmp = self.emit_expr(arg, self.temp1());
                    if tmp != arg_reg {
                        self.emitf(format!("mov {arg_reg}, {tmp}"));
                    }
                }
            }
        }

        self.emitf(format!("bl {fn_name}"));
        self.restore_hot_regs_from_state();
        if X::VALUE == 32 {
            "w0".to_string()
        } else {
            "x0".to_string()
        }
    }

    fn maybe_spill_left(&mut self, right: &Expr<X>, left_reg: &str) -> Option<usize> {
        if !self.expr_needs_temp1(right) {
            return None;
        }
        let temp1 = self.temp1();
        let left_is_temp1 = left_reg == temp1 || left_reg == self.reg_32(temp1);
        if !left_is_temp1 {
            return None;
        }
        let offset = self.alloc_spill_slot();
        if let Some(off) = offset {
            if X::VALUE == 32 {
                self.emitf(format!("str {}, [sp, #{}]", self.reg_32(left_reg), off));
            } else {
                self.emitf(format!("str {left_reg}, [sp, #{}]", off));
            }
        }
        offset
    }

    fn restore_spilled_left(&mut self, spill: Option<usize>, left_reg: &str) {
        if let Some(off) = spill {
            if X::VALUE == 32 {
                self.emitf(format!("ldr {}, [sp, #{}]", self.reg_32(left_reg), off));
            } else {
                self.emitf(format!("ldr {left_reg}, [sp, #{}]", off));
            }
            self.release_spill_slot();
        }
    }

    fn emit_shift_op(&mut self, op: BinaryOp, right: &Expr<X>, src: &str, dest: &str) -> String {
        let shift_op = match op {
            BinaryOp::Sll => "lsl",
            BinaryOp::Srl => "lsr",
            BinaryOp::Sra => "asr",
            _ => unreachable!(),
        };

        if let Expr::Imm(imm) = right {
            let mask = if X::VALUE == 32 { 0x1f } else { 0x3f };
            let shift = (X::to_u64(*imm) & mask) as u8;
            self.emitf(format!("{shift_op} {dest}, {src}, #{shift}"));
        } else {
            let spill = self.alloc_spill_slot();
            if let Some(offset) = spill {
                if X::VALUE == 32 {
                    self.emitf(format!("str {}, [sp, #{}]", self.reg_32(src), offset));
                } else {
                    self.emitf(format!("str {src}, [sp, #{}]", offset));
                }
            }
            let shift_reg = self.emit_expr(right, self.temp2());
            if let Some(offset) = spill {
                if X::VALUE == 32 {
                    self.emitf(format!("ldr {}, [sp, #{}]", self.reg_32(src), offset));
                } else {
                    self.emitf(format!("ldr {src}, [sp, #{}]", offset));
                }
                self.release_spill_slot();
            }
            self.emitf(format!("{shift_op} {dest}, {src}, {shift_reg}"));
        }
        dest.to_string()
    }

    fn emit_shift_word_op(
        &mut self,
        op: BinaryOp,
        right: &Expr<X>,
        src: &str,
        dest: &str,
    ) -> String {
        let shift_op = match op {
            BinaryOp::SllW => "lsl",
            BinaryOp::SrlW => "lsr",
            BinaryOp::SraW => "asr",
            _ => unreachable!(),
        };

        let src32 = self.reg_32(src);

        if let Expr::Imm(imm) = right {
            let shift = (X::to_u64(*imm) & 0x1f) as u8;
            self.emitf(format!("{shift_op} {src32}, {src32}, #{shift}"));
        } else {
            let spill = self.alloc_spill_slot();
            if let Some(offset) = spill {
                self.emitf(format!("str {src32}, [sp, #{}]", offset));
            }
            let shift_reg = self.emit_expr(right, self.temp2());
            let shift32 = self.reg_32(&shift_reg);
            if let Some(offset) = spill {
                self.emitf(format!("ldr {src32}, [sp, #{}]", offset));
                self.release_spill_slot();
            }
            self.emitf(format!("{shift_op} {src32}, {src32}, {shift32}"));
        }
        // Sign extend result
        let dest64 = self.reg_64(dest);
        self.emitf(format!("sxtw {dest64}, {src32}"));
        dest.to_string()
    }

    fn emit_div_op(
        &mut self,
        op: BinaryOp,
        right: &Expr<X>,
        left_reg: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        let left_spill = self.maybe_spill_left(right, left_reg);
        let right_reg = self.emit_expr(right, temp2);
        self.restore_spilled_left(left_spill, left_reg);

        // Check for division by zero
        let skip_label = self.next_label("div_ok");
        let done_label = self.next_label("div_done");

        self.emitf(format!("cbnz {right_reg}, {skip_label}"));
        // Division by zero: return -1 for div, dividend for rem
        match op {
            BinaryOp::Div | BinaryOp::DivU => {
                self.emitf(format!("mov {dest}, #-1"));
            }
            BinaryOp::Rem | BinaryOp::RemU => {
                self.emitf(format!("mov {dest}, {left_reg}"));
            }
            _ => unreachable!(),
        }
        self.emitf(format!("b {done_label}"));
        self.emit_label(&skip_label);

        match op {
            BinaryOp::Div => {
                // Check for overflow: INT_MIN / -1
                let no_ov_label = self.next_label("div_no_ov");
                if X::VALUE == 32 {
                    self.load_imm("w2", 0x80000000);
                    self.emitf(format!("cmp {left_reg}, w2"));
                } else {
                    self.load_imm("x2", 0x8000000000000000u64);
                    self.emitf(format!("cmp {left_reg}, x2"));
                }
                self.emitf(format!("b.ne {no_ov_label}"));
                self.emitf(format!("cmn {right_reg}, #1")); // compare with -1
                self.emitf(format!("b.ne {no_ov_label}"));
                // Overflow: return INT_MIN
                if X::VALUE == 32 {
                    self.emitf(format!("mov {dest}, w2"));
                } else {
                    self.emitf(format!("mov {dest}, x2"));
                }
                self.emitf(format!("b {done_label}"));
                self.emit_label(&no_ov_label);
                self.emitf(format!("sdiv {dest}, {left_reg}, {right_reg}"));
            }
            BinaryOp::DivU => {
                self.emitf(format!("udiv {dest}, {left_reg}, {right_reg}"));
            }
            BinaryOp::Rem => {
                // Check for overflow
                let no_ov_label = self.next_label("rem_no_ov");
                if X::VALUE == 32 {
                    self.load_imm("w2", 0x80000000);
                    self.emitf(format!("cmp {left_reg}, w2"));
                } else {
                    self.load_imm("x2", 0x8000000000000000u64);
                    self.emitf(format!("cmp {left_reg}, x2"));
                }
                self.emitf(format!("b.ne {no_ov_label}"));
                self.emitf(format!("cmn {right_reg}, #1"));
                self.emitf(format!("b.ne {no_ov_label}"));
                // Overflow: return 0
                self.emitf(format!("mov {dest}, #0"));
                self.emitf(format!("b {done_label}"));
                self.emit_label(&no_ov_label);
                // remainder = dividend - (quotient * divisor)
                // Use correct register width for RV32 vs RV64
                if X::VALUE == 32 {
                    self.emitf(format!("sdiv w2, {left_reg}, {right_reg}"));
                    self.emitf(format!("msub {dest}, w2, {right_reg}, {left_reg}"));
                } else {
                    self.emitf(format!("sdiv x2, {left_reg}, {right_reg}"));
                    self.emitf(format!("msub {dest}, x2, {right_reg}, {left_reg}"));
                }
            }
            BinaryOp::RemU => {
                if X::VALUE == 32 {
                    self.emitf(format!("udiv w2, {left_reg}, {right_reg}"));
                    self.emitf(format!("msub {dest}, w2, {right_reg}, {left_reg}"));
                } else {
                    self.emitf(format!("udiv x2, {left_reg}, {right_reg}"));
                    self.emitf(format!("msub {dest}, x2, {right_reg}, {left_reg}"));
                }
            }
            _ => unreachable!(),
        }

        self.emit_label(&done_label);
        dest.to_string()
    }

    fn emit_div_word_op(
        &mut self,
        op: BinaryOp,
        right: &Expr<X>,
        left_reg: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        let left_spill = self.maybe_spill_left(right, left_reg);
        let right_reg = self.emit_expr(right, temp2);
        self.restore_spilled_left(left_spill, left_reg);
        let left32 = self.reg_32(left_reg);
        let right32 = self.reg_32(&right_reg);

        let skip_label = self.next_label("divw_ok");
        let done_label = self.next_label("divw_done");

        self.emitf(format!("cbnz {right32}, {skip_label}"));
        match op {
            BinaryOp::DivW | BinaryOp::DivUW => {
                self.emitf(format!("mov {dest}, #-1"));
            }
            BinaryOp::RemW | BinaryOp::RemUW => {
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, {left32}"));
            }
            _ => unreachable!(),
        }
        self.emitf(format!("b {done_label}"));
        self.emit_label(&skip_label);

        match op {
            BinaryOp::DivW => {
                self.emitf(format!("sdiv w2, {left32}, {right32}"));
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, w2"));
            }
            BinaryOp::DivUW => {
                self.emitf(format!("udiv w2, {left32}, {right32}"));
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, w2"));
            }
            BinaryOp::RemW => {
                self.emitf(format!("sdiv w2, {left32}, {right32}"));
                self.emitf(format!("msub w2, w2, {right32}, {left32}"));
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, w2"));
            }
            BinaryOp::RemUW => {
                self.emitf(format!("udiv w2, {left32}, {right32}"));
                self.emitf(format!("msub w2, w2, {right32}, {left32}"));
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, w2"));
            }
            _ => unreachable!(),
        }

        self.emit_label(&done_label);
        dest.to_string()
    }

    fn emit_mulh_op(
        &mut self,
        op: BinaryOp,
        right: &Expr<X>,
        left_reg: &str,
        temp2: &str,
        dest: &str,
    ) -> String {
        let left_spill = self.maybe_spill_left(right, left_reg);
        let right_reg = self.emit_expr(right, temp2);
        self.restore_spilled_left(left_spill, left_reg);

        if X::VALUE == 32 {
            // RV32: use smull/umull (32x32 -> 64) then extract high 32 bits
            let left32 = self.reg_32(left_reg);
            let right32 = self.reg_32(&right_reg);
            let dest64 = self.reg_64(dest);
            match op {
                BinaryOp::MulH => {
                    // Signed 32x32 -> 64, then arithmetic shift right 32
                    self.emitf(format!("smull {dest64}, {left32}, {right32}"));
                    self.emitf(format!("asr {dest64}, {dest64}, #32"));
                }
                BinaryOp::MulHU => {
                    // Unsigned 32x32 -> 64, then logical shift right 32
                    self.emitf(format!("umull {dest64}, {left32}, {right32}"));
                    self.emitf(format!("lsr {dest64}, {dest64}, #32"));
                }
                BinaryOp::MulHSU => {
                    // Signed * Unsigned: sign-extend left, zero-extend right, multiply
                    // smull sign-extends both, so we need to correct
                    let tmp = self.temp3();
                    let tmp32 = self.reg_32(tmp);
                    self.emitf(format!("mov {tmp32}, {left32}"));
                    self.emitf(format!("smull {dest64}, {left32}, {right32}"));
                    self.emitf(format!("asr {dest64}, {dest64}, #32"));
                    // If right was negative (as signed), we added 2^32 * left to the result
                    // Need to add it back: if right32 < 0, add left32 to high word
                    self.emitf(format!("cmp {right32}, #0"));
                    self.emitf(format!("csel {tmp32}, {tmp32}, wzr, lt"));
                    self.emitf(format!("add {dest64}, {dest64}, {tmp32}, sxtw"));
                }
                _ => unreachable!(),
            }
        } else {
            // RV64: use smulh/umulh directly
            match op {
                BinaryOp::MulH => {
                    // Signed high multiplication
                    self.emitf(format!("smulh {dest}, {left_reg}, {right_reg}"));
                }
                BinaryOp::MulHU => {
                    // Unsigned high multiplication
                    self.emitf(format!("umulh {dest}, {left_reg}, {right_reg}"));
                }
                BinaryOp::MulHSU => {
                    // Signed * Unsigned high - no direct instruction
                    // Result = umulh(a, b) - (a < 0 ? b : 0)
                    self.emitf(format!("umulh {dest}, {left_reg}, {right_reg}"));
                    // If left is negative, subtract right from result
                    self.emitf(format!("cmp {left_reg}, #0"));
                    self.emitf(format!("csel x2, {right_reg}, xzr, lt"));
                    self.emitf(format!("sub {dest}, {dest}, x2"));
                }
                _ => unreachable!(),
            }
        }
        dest.to_string()
    }

    /// Emit a unary operation.
    pub(super) fn emit_unary_op(&mut self, op: UnaryOp, inner: &Expr<X>, dest: &str) -> String {
        let mut inner_reg = self.emit_expr(inner, dest);
        if X::VALUE == 32 && dest.starts_with('w') && inner_reg.starts_with('x') {
            inner_reg = self.reg_32(&inner_reg);
        }
        if inner_reg != dest {
            self.emitf(format!("mov {dest}, {inner_reg}"));
        }

        match op {
            UnaryOp::Neg => {
                self.emitf(format!("neg {dest}, {dest}"));
            }
            UnaryOp::Not => {
                self.emitf(format!("mvn {dest}, {dest}"));
            }
            UnaryOp::Sext8 => {
                self.emitf(format!("sxtb {dest}, {}", self.reg_32(dest)));
            }
            UnaryOp::Sext16 => {
                self.emitf(format!("sxth {dest}, {}", self.reg_32(dest)));
            }
            UnaryOp::Sext32 => {
                let dest64 = self.reg_64(dest);
                self.emitf(format!("sxtw {dest64}, {}", self.reg_32(dest)));
            }
            UnaryOp::Zext8 => {
                self.emitf(format!("uxtb {}, {}", self.reg_32(dest), self.reg_32(dest)));
            }
            UnaryOp::Zext16 => {
                self.emitf(format!("uxth {}, {}", self.reg_32(dest), self.reg_32(dest)));
            }
            UnaryOp::Zext32 => {
                // Moving w to x zero-extends automatically
                let src32 = self.reg_32(dest);
                let dest32 = self.reg_32(dest);
                self.emitf(format!("mov {dest32}, {src32}"));
            }
            UnaryOp::Clz => {
                self.emitf(format!("clz {dest}, {dest}"));
            }
            UnaryOp::Ctz => {
                // ctz = clz(rbit(x))
                self.emitf(format!("rbit {dest}, {dest}"));
                self.emitf(format!("clz {dest}, {dest}"));
            }
            UnaryOp::Cpop => {
                if X::VALUE == 32 {
                    let dest32 = self.reg_32(dest);
                    self.emit_cpop32(&dest32);
                } else {
                    self.emit_cpop64(dest);
                }
            }
            UnaryOp::Clz32 => {
                let dest32 = self.reg_32(dest);
                self.emitf(format!("clz {dest32}, {dest32}"));
            }
            UnaryOp::Ctz32 => {
                let dest32 = self.reg_32(dest);
                self.emitf(format!("rbit {dest32}, {dest32}"));
                self.emitf(format!("clz {dest32}, {dest32}"));
            }
            UnaryOp::Cpop32 => {
                let dest32 = self.reg_32(dest);
                self.emit_cpop32(&dest32);
            }
            UnaryOp::Orc8 => {
                if X::VALUE == 32 {
                    let dest32 = self.reg_32(dest);
                    self.emit_orc8_32(&dest32);
                } else {
                    self.emit_orc8_64(dest);
                }
            }
            UnaryOp::Rev8 => {
                // Byte reverse
                self.emitf(format!("rev {dest}, {dest}"));
            }
            _ => {
                self.emit_comment(&format!("unary op {:?} not implemented", op));
                self.emitf(format!("mov {dest}, {dest}"));
            }
        }

        dest.to_string()
    }

    fn emit_cpop64(&mut self, dest: &str) {
        let tmp = self.temp3();
        self.emitf(format!("lsr {tmp}, {dest}, #1"));
        self.emitf(format!("and {tmp}, {tmp}, #0x5555555555555555"));
        self.emitf(format!("sub {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #2"));
        self.emitf(format!("and {tmp}, {tmp}, #0x3333333333333333"));
        self.emitf(format!("and {dest}, {dest}, #0x3333333333333333"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #4"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("and {dest}, {dest}, #0x0f0f0f0f0f0f0f0f"));
        self.emitf(format!("lsr {tmp}, {dest}, #8"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #16"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #32"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("and {dest}, {dest}, #0x7f"));
    }

    fn emit_cpop32(&mut self, dest32: &str) {
        let tmp = self.temp3();
        let tmp32 = self.reg_32(tmp);
        self.emitf(format!("lsr {tmp32}, {dest32}, #1"));
        self.emitf(format!("and {tmp32}, {tmp32}, #0x55555555"));
        self.emitf(format!("sub {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #2"));
        self.emitf(format!("and {tmp32}, {tmp32}, #0x33333333"));
        self.emitf(format!("and {dest32}, {dest32}, #0x33333333"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #4"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("and {dest32}, {dest32}, #0x0f0f0f0f"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #8"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #16"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("and {dest32}, {dest32}, #0x3f"));
    }

    fn emit_orc8_64(&mut self, dest: &str) {
        let tmp = self.temp3();
        self.emitf(format!("lsr {tmp}, {dest}, #1"));
        self.emitf(format!("orr {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #2"));
        self.emitf(format!("orr {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #4"));
        self.emitf(format!("orr {dest}, {dest}, {tmp}"));
        self.emitf(format!("and {dest}, {dest}, #0x0101010101010101"));
        self.emitf(format!("mov {tmp}, #0xff"));
        self.emitf(format!("mul {dest}, {dest}, {tmp}"));
    }

    fn emit_orc8_32(&mut self, dest32: &str) {
        let tmp = self.temp3();
        let tmp32 = self.reg_32(tmp);
        self.emitf(format!("lsr {tmp32}, {dest32}, #1"));
        self.emitf(format!("orr {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #2"));
        self.emitf(format!("orr {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #4"));
        self.emitf(format!("orr {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("and {dest32}, {dest32}, #0x01010101"));
        self.emitf(format!("mov {tmp32}, #0xff"));
        self.emitf(format!("mul {dest32}, {dest32}, {tmp32}"));
    }

    /// Emit a statement.
    pub(super) fn emit_stmt(&mut self, stmt: &Stmt<X>) {
        let temp1 = self.temp1();
        let temp2 = self.temp2();

        match stmt {
            Stmt::Write { target, value } => match target {
                WriteTarget::Reg(reg) => {
                    if *reg == 0 {
                        return;
                    }
                    self.cold_cache_invalidate(*reg);
                    if let Some(arm_reg) = self.reg_map.get(*reg) {
                        let val_reg = self.emit_expr(value, arm_reg);
                        if val_reg != arm_reg {
                            self.emitf(format!("mov {arm_reg}, {val_reg}"));
                        }
                    } else {
                        let val_reg = self.emit_expr(value, temp1);
                        self.store_to_rv(*reg, &val_reg);
                    }
                }
                WriteTarget::Mem {
                    base,
                    offset,
                    width,
                } => {
                    // First evaluate value to temp2
                    let val_reg = self.emit_expr(value, temp2);
                    // Check if val_reg is temp2 (x1/w1) - use exact match, not starts_with
                    // to avoid matching x10/w10 etc.
                    let is_temp2 = val_reg == "x1" || val_reg == "w1";
                    if val_reg != temp2 && !is_temp2 {
                        self.emitf(format!("mov {temp2}, {val_reg}"));
                    }
                    // Then evaluate address
                    let base_reg = self.emit_expr_as_addr(base);
                    if *offset != 0 {
                        self.emit_add_offset("x0", &base_reg, (*offset).into());
                    } else if base_reg != "x0" {
                        self.emitf(format!("mov x0, {base_reg}"));
                    }
                    self.apply_address_mode("x0");

                    // HTIF handling: check for tohost write
                    let htif_done_label =
                        if self.config.htif_enabled && (*width == 4 || *width == 8) {
                            Some(self.emit_htif_check())
                        } else {
                            None
                        };

                    // Store
                    let val32 = self.reg_32(temp2);
                    let mem = format!("{}, x0", reserved::MEMORY_PTR);
                    match width {
                        1 => self.emitf(format!("strb {val32}, [{mem}]")),
                        2 => self.emitf(format!("strh {val32}, [{mem}]")),
                        4 => self.emitf(format!("str {val32}, [{mem}]")),
                        8 => self.emitf(format!("str {temp2}, [{mem}]")),
                        _ => self.emitf(format!("str {val32}, [{mem}]")),
                    }

                    // Emit the done label for HTIF syscall handling
                    if let Some(label) = htif_done_label {
                        self.emit_label(&label);
                    }
                }
                WriteTarget::Pc => {
                    let val_reg = self.emit_expr(value, temp1);
                    let pc_off = self.layout.offset_pc;
                    self.emitf(format!(
                        "str {val_reg}, [{}, #{}]",
                        reserved::STATE_PTR,
                        pc_off
                    ));
                }
                WriteTarget::Exited => {
                    let off = self.layout.offset_has_exited;
                    self.emit("mov w0, #1");
                    self.emitf(format!("strb w0, [{}, #{}]", reserved::STATE_PTR, off));
                }
                WriteTarget::ExitCode => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_exit_code;
                    self.emitf(format!(
                        "strb {}, [{}, #{}]",
                        self.reg_32(&val_reg),
                        reserved::STATE_PTR,
                        off
                    ));
                }
                WriteTarget::Temp(idx) => {
                    let val_reg = self.emit_expr(value, temp1);
                    if let Some(offset) = self.temp_slot_offset(*idx) {
                        if X::VALUE == 32 {
                            self.emitf(format!("str {}, [sp, #{}]", self.reg_32(&val_reg), offset));
                        } else {
                            self.emitf(format!("str {val_reg}, [sp, #{}]", offset));
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
                            "str {}, [{}, #{}]",
                            self.reg_32(&val_reg),
                            reserved::STATE_PTR,
                            off
                        ));
                    } else {
                        self.emitf(format!(
                            "str {val_reg}, [{}, #{}]",
                            reserved::STATE_PTR,
                            off
                        ));
                    }
                }
                WriteTarget::ResValid => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_reservation_valid;
                    self.emitf(format!(
                        "strb {}, [{}, #{}]",
                        self.reg_32(&val_reg),
                        reserved::STATE_PTR,
                        off
                    ));
                }
                _ => self.emit_comment(&format!("unsupported write: {:?}", target)),
            },
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => {
                let else_label = self.next_label("if_else");
                let end_label = self.next_label("if_end");
                if !self.try_emit_compare_branch(cond, &else_label, true) {
                    let cond_reg = self.emit_expr(cond, temp1);
                    self.emitf(format!("cbz {cond_reg}, {else_label}"));
                }
                for s in then_stmts {
                    self.emit_stmt(s);
                }
                if !else_stmts.is_empty() {
                    self.emitf(format!("b {end_label}"));
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
    pub(super) fn emit_instruction(
        &mut self,
        instr: &InstrIR<X>,
        is_last_in_block: bool,
        fall_pc: u64,
    ) {
        let pc = X::to_u64(instr.pc);
        self.emit_instret_increment(1, pc);

        // Check if any statement might set has_exited (e.g., exit syscall)
        let might_exit = instr.statements.iter().any(stmt_writes_to_exited);
        let mut skip_last_temp_cmp = false;
        let mut cmp_for_branch: Option<(Expr<X>, Expr<X>, BinaryOp)> = None;
        if is_last_in_block {
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
