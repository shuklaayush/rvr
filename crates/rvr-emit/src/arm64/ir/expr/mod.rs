//! IR expression lowering for ARM64.

mod ops_binary;
mod ops_unary;

use rvr_ir::{
    BinaryOp, Expr, InstrIR, ReadExpr, Stmt, Terminator, TernaryOp, UnaryOp, WriteTarget, Xlen,
};

use crate::arm64::Arm64Emitter;
use crate::arm64::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
    fn signed_imm(val: X::Reg) -> i64 {
        let v = X::to_u64(val);
        if X::VALUE == 64 {
            i64::from_ne_bytes(v.to_ne_bytes())
        } else {
            let shift = 64 - u32::from(X::VALUE);
            let shifted = v << shift;
            let signed = i64::from_ne_bytes(shifted.to_ne_bytes());
            signed >> shift
        }
    }

    const fn expr_needs_temp1(expr: &Expr<X>) -> bool {
        match expr {
            Expr::Imm(_)
            | Expr::PcConst(_)
            | Expr::Var(_)
            | Expr::Read(
                ReadExpr::Reg(_)
                | ReadExpr::Temp(_)
                | ReadExpr::Csr(_)
                | ReadExpr::Cycle
                | ReadExpr::Instret
                | ReadExpr::Pc
                | ReadExpr::TraceIdx
                | ReadExpr::PcIdx
                | ReadExpr::ResAddr
                | ReadExpr::ResValid
                | ReadExpr::Exited
                | ReadExpr::ExitCode,
            ) => false,
            Expr::Read(ReadExpr::Mem { .. } | ReadExpr::MemAddr { .. })
            | Expr::Unary { .. }
            | Expr::Binary { .. }
            | Expr::Ternary { .. }
            | Expr::ExternCall { .. } => true,
        }
    }

    fn is_zero_expr(expr: &Expr<X>) -> bool {
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
        } else if let Some((imm12, shift12)) = Self::addsub_imm_parts(imm) {
            let shift = if shift12 { ", lsl #12" } else { "" };
            if imm >= 0 {
                self.emitf(format!("cmp {left_reg}, #{imm12}{shift}"));
            } else {
                self.emitf(format!("cmn {left_reg}, #{imm12}{shift}"));
            }
        } else {
            let temp = Self::temp2();
            self.load_imm(temp, u64::from_ne_bytes(imm.to_ne_bytes()));
            let right_reg = if X::VALUE == 32 {
                Self::reg_32(temp)
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
                let target_pc =
                    target.map_or(fall_pc, |t| self.inputs.resolve_address(X::to_u64(t)));
                if target_pc == current_pc {
                    self.emit_store_next_pc_imm(fall_pc);
                } else {
                    self.emit_store_next_pc_imm(target_pc);
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
                let fall_target_pc =
                    fall.map_or(fall_pc, |f| self.inputs.resolve_address(X::to_u64(f)));

                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                if !self.try_emit_compare_branch(cond, &target_label, false) {
                    let cond_reg = self.emit_expr(cond, Self::temp1());
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
                let target_pc =
                    target.map_or(fall_pc, |t| self.inputs.resolve_address(X::to_u64(t)));
                if target_pc == current_pc {
                    self.emit_store_next_pc_imm(fall_pc);
                } else {
                    self.emit_store_next_pc_imm(target_pc);
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
                let fall_target_pc =
                    fall.map_or(fall_pc, |f| self.inputs.resolve_address(X::to_u64(f)));

                let target_label = self.next_label("instret_target");
                let done_label = self.next_label("instret_done");
                if !self.try_emit_compare_branch(cond, &target_label, false) {
                    let cond_reg = self.emit_expr(cond, Self::temp1());
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
        let Expr::Binary { op, left, right } = cond else {
            return false;
        };
        if let Some(handled) = self.try_emit_compare_logic(*op, left, right, label, invert) {
            return handled;
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

        let temp1 = Self::temp1();
        let temp2 = Self::temp2();
        let left_reg = self.emit_expr(left, temp1);
        let left_reg = if X::VALUE == 32 {
            Self::reg_32(&left_reg)
        } else {
            left_reg
        };

        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) && Self::is_zero_expr(right) {
            let branch = if cond_code == "eq" { "cbz" } else { "cbnz" };
            self.emitf(format!("{branch} {left_reg}, {label}"));
            return true;
        }

        let left_is_temp1 = left_reg == temp1 || left_reg == Self::reg_32(temp1);
        let needs_spill = left_is_temp1 && Self::expr_needs_temp1(right);
        let spill = if needs_spill {
            self.alloc_spill_slot()
        } else {
            None
        };
        if let Some(off) = spill {
            if X::VALUE == 32 {
                self.emitf(format!("str {}, [sp, #{}]", Self::reg_32(&left_reg), off));
            } else {
                self.emitf(format!("str {left_reg}, [sp, #{off}]"));
            }
        }

        if Self::is_zero_expr(right) {
            self.emit_cmp_with_imm(&left_reg, 0);
        } else if let Expr::Imm(v) = right.as_ref() {
            let imm = Self::signed_imm(*v);
            self.emit_cmp_with_imm(&left_reg, imm);
        } else {
            let right_reg = self.emit_expr(right, temp2);
            let right_reg = if X::VALUE == 32 {
                Self::reg_32(&right_reg)
            } else {
                right_reg
            };
            self.emitf(format!("cmp {left_reg}, {right_reg}"));
        }

        if let Some(off) = spill {
            if X::VALUE == 32 {
                self.emitf(format!("ldr {}, [sp, #{}]", Self::reg_32(&left_reg), off));
            } else {
                self.emitf(format!("ldr {left_reg}, [sp, #{off}]"));
            }
            self.release_spill_slot();
        }

        self.emitf(format!("b.{cond_code} {label}"));
        true
    }

    fn try_emit_compare_logic(
        &mut self,
        op: BinaryOp,
        left: &Expr<X>,
        right: &Expr<X>,
        label: &str,
        invert: bool,
    ) -> Option<bool> {
        if !matches!(op, BinaryOp::Or | BinaryOp::And) {
            return None;
        }
        if !Self::is_cmp_expr(left) || !Self::is_cmp_expr(right) {
            return None;
        }

        let skip_label = self.next_label("bool_skip");
        match (op, invert) {
            (BinaryOp::Or, false) => {
                let _ = self.try_emit_compare_branch(left, label, false);
                let _ = self.try_emit_compare_branch(right, label, false);
                Some(true)
            }
            (BinaryOp::Or, true) => {
                let _ = self.try_emit_compare_branch(left, &skip_label, false);
                let _ = self.try_emit_compare_branch(right, &skip_label, false);
                self.emitf(format!("b {label}"));
                self.emit_label(&skip_label);
                Some(true)
            }
            (BinaryOp::And, false) => {
                let _ = self.try_emit_compare_branch(left, &skip_label, true);
                let _ = self.try_emit_compare_branch(right, label, false);
                self.emit_label(&skip_label);
                Some(true)
            }
            (BinaryOp::And, true) => {
                let _ = self.try_emit_compare_branch(left, label, true);
                let _ = self.try_emit_compare_branch(right, label, true);
                Some(true)
            }
            _ => Some(false),
        }
    }

    const fn is_cmp_expr(expr: &Expr<X>) -> bool {
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
    }

    fn addsub_imm_parts(imm: i64) -> Option<(u16, bool)> {
        if imm == i64::MIN {
            return None;
        }
        let abs = imm.unsigned_abs();
        if abs <= 0xFFF {
            Some((u16::try_from(abs).ok()?, false))
        } else if abs <= 0x00FF_F000 && abs.trailing_zeros() >= 12 {
            Some((u16::try_from(abs >> 12).ok()?, true))
        } else {
            None
        }
    }

    pub(super) fn cmp_from_temp_branch<'a>(
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
                let result = self.emit_expr(expr, Self::temp1());
                let result64 = Self::reg_64(&result);
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
                Expr::Read(ReadExpr::Csr(_) | ReadExpr::Cycle | ReadExpr::Instret)
            )
        {
            self.emitf(format!("mov {dest}, #0"));
            return dest.to_string();
        }
        match expr {
            Expr::Imm(val) => self.emit_expr_imm(*val, dest),
            Expr::Read(read) => self.emit_expr_read(read, dest),
            Expr::PcConst(val) => self.emit_expr_pc_const(*val, dest),
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
        }
    }

    fn emit_expr_imm(&mut self, val: X::Reg, dest: &str) -> String {
        let v = X::to_u64(val);
        if v == 0 {
            self.emitf(format!("mov {dest}, #0"));
        } else {
            self.load_imm(dest, v);
        }
        dest.to_string()
    }

    fn emit_expr_pc_const(&mut self, val: X::Reg, dest: &str) -> String {
        let v = X::to_u64(val);
        self.load_imm(dest, v);
        dest.to_string()
    }

    fn emit_expr_read(&mut self, read: &ReadExpr<X>, dest: &str) -> String {
        match read {
            ReadExpr::Reg(reg) => self.load_rv_to_temp(*reg, dest),
            ReadExpr::Mem {
                base,
                offset,
                width,
                signed,
            } => self.emit_expr_read_mem(base, *offset, *width, *signed, dest),
            ReadExpr::MemAddr {
                addr,
                width,
                signed,
            } => self.emit_expr_read_mem_addr(addr, *width, *signed, dest),
            ReadExpr::Csr(csr) => self.emit_expr_read_csr(*csr, dest),
            ReadExpr::Cycle | ReadExpr::Instret => self.emit_expr_read_instret(dest),
            ReadExpr::Pc => self.emit_expr_read_pc(dest),
            ReadExpr::Exited => self.emit_expr_read_exited(dest),
            ReadExpr::ExitCode => self.emit_expr_read_exit_code(dest),
            ReadExpr::ResAddr => self.emit_expr_read_res_addr(dest),
            ReadExpr::ResValid => self.emit_expr_read_res_valid(dest),
            ReadExpr::Temp(idx) => self.emit_expr_read_temp(*idx, dest),
            _ => self.emit_expr_unsupported(&Expr::Read(read.clone()), dest),
        }
    }

    fn emit_expr_read_mem(
        &mut self,
        base: &Expr<X>,
        offset: i16,
        width: u8,
        signed: bool,
        dest: &str,
    ) -> String {
        let base_reg = self.emit_expr_as_addr(base);
        if offset != 0 {
            self.emit_add_offset("x0", &base_reg, offset.into());
        } else if base_reg != "x0" {
            self.emitf(format!("mov x0, {base_reg}"));
        }
        self.apply_address_mode("x0");
        let mem = format!("{}, x0", reserved::MEMORY_PTR);
        self.emit_load_from_mem(&mem, dest, width, signed);
        self.emit_trace_mem_access("x0", dest, width, false);
        dest.to_string()
    }

    fn emit_expr_read_mem_addr(
        &mut self,
        addr: &Expr<X>,
        width: u8,
        signed: bool,
        dest: &str,
    ) -> String {
        let base_reg = self.emit_expr_as_addr(addr);
        if base_reg != "x0" {
            self.emitf(format!("mov x0, {base_reg}"));
        }
        self.apply_address_mode("x0");
        let mem = format!("{}, x0", reserved::MEMORY_PTR);
        self.emit_load_from_mem(&mem, dest, width, signed);
        self.emit_trace_mem_access("x0", dest, width, false);
        dest.to_string()
    }

    fn emit_expr_read_csr(&mut self, csr: u16, dest: &str) -> String {
        let instret_off = self.layout.offset_instret;
        match csr {
            0xC00 | 0xC02 | 0xB00 | 0xB02 => {
                if self.config.instret_mode.counts() {
                    if X::VALUE == 32 {
                        let instret32 = Self::reg_32(reserved::INSTRET);
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
            0xC80 | 0xC82 | 0xB80 | 0xB82 if X::VALUE == 32 => {
                if self.config.instret_mode.counts() {
                    let dest64 = Self::reg_64(dest);
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
                self.emit_comment(&format!("CSR 0x{csr:03x} not implemented"));
                self.emitf(format!("mov {dest}, #0"));
            }
        }
        dest.to_string()
    }

    fn emit_expr_read_instret(&mut self, dest: &str) -> String {
        if self.config.instret_mode.counts() {
            if X::VALUE == 32 {
                let instret32 = Self::reg_32(reserved::INSTRET);
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

    fn emit_expr_read_pc(&mut self, dest: &str) -> String {
        let pc_off = self.layout.offset_pc;
        self.emitf(format!(
            "ldr {dest}, [{}, #{}]",
            reserved::STATE_PTR,
            pc_off
        ));
        dest.to_string()
    }

    fn emit_expr_read_exited(&mut self, dest: &str) -> String {
        let off = self.layout.offset_has_exited;
        self.emitf(format!(
            "ldrb {}, [{}, #{}]",
            Self::reg_32(dest),
            reserved::STATE_PTR,
            off
        ));
        dest.to_string()
    }

    fn emit_expr_read_exit_code(&mut self, dest: &str) -> String {
        let off = self.layout.offset_exit_code;
        self.emitf(format!(
            "ldrb {}, [{}, #{}]",
            Self::reg_32(dest),
            reserved::STATE_PTR,
            off
        ));
        dest.to_string()
    }

    fn emit_expr_read_res_addr(&mut self, dest: &str) -> String {
        let off = self.layout.offset_reservation_addr;
        if X::VALUE == 32 {
            self.emitf(format!(
                "ldr {}, [{}, #{}]",
                Self::reg_32(dest),
                reserved::STATE_PTR,
                off
            ));
        } else {
            self.emitf(format!("ldr {dest}, [{}, #{}]", reserved::STATE_PTR, off));
        }
        dest.to_string()
    }

    fn emit_expr_read_res_valid(&mut self, dest: &str) -> String {
        let off = self.layout.offset_reservation_valid;
        self.emitf(format!(
            "ldrb {}, [{}, #{}]",
            Self::reg_32(dest),
            reserved::STATE_PTR,
            off
        ));
        dest.to_string()
    }

    fn emit_expr_read_temp(&mut self, idx: u8, dest: &str) -> String {
        if let Some(offset) = Self::temp_slot_offset(idx) {
            if X::VALUE == 32 {
                self.emitf(format!("ldr {}, [sp, #{}]", Self::reg_32(dest), offset));
            } else {
                self.emitf(format!("ldr {dest}, [sp, #{offset}]"));
            }
        } else {
            self.emit_comment(&format!("temp {idx} out of range"));
            self.emitf(format!("mov {dest}, #0"));
        }
        dest.to_string()
    }

    fn emit_expr_var(&mut self, name: &str, dest: &str) -> String {
        if name == "state" {
            let state = reserved::STATE_PTR;
            if X::VALUE == 32 {
                let dest32 = Self::reg_32(dest);
                self.emitf(format!("mov {dest32}, {}", Self::reg_32(state)));
            } else {
                self.emitf(format!("mov {dest}, {state}"));
            }
        } else {
            self.emit_comment(&format!("unsupported var: {name}"));
            self.emitf(format!("mov {dest}, #0"));
        }
        dest.to_string()
    }

    fn emit_expr_extern_call(&mut self, name: &str, args: &[Expr<X>], dest: &str) -> String {
        let ret = self.emit_extern_call(name, args);
        if ret != dest {
            self.emitf(format!("mov {dest}, {ret}"));
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
        if let (Some(then_off), Some(else_off)) =
            (Self::temp_slot_offset(0), Self::temp_slot_offset(1))
        {
            let then_reg = self.emit_expr(then_val, Self::temp1());
            if X::VALUE == 32 {
                self.emitf(format!(
                    "str {}, [sp, #{}]",
                    Self::reg_32(&then_reg),
                    then_off
                ));
            } else {
                self.emitf(format!("str {then_reg}, [sp, #{then_off}]"));
            }

            let else_reg = self.emit_expr(else_val, Self::temp1());
            if X::VALUE == 32 {
                self.emitf(format!(
                    "str {}, [sp, #{}]",
                    Self::reg_32(&else_reg),
                    else_off
                ));
            } else {
                self.emitf(format!("str {else_reg}, [sp, #{else_off}]"));
            }

            let cond_reg = self.emit_expr(cond, Self::temp1());

            let (tmp1, tmp2) = if X::VALUE == 32 {
                ("w1", "w2")
            } else {
                ("x1", "x2")
            };
            self.emitf(format!("ldr {tmp1}, [sp, #{then_off}]"));
            self.emitf(format!("ldr {tmp2}, [sp, #{else_off}]"));
            self.emitf(format!("cmp {cond_reg}, #0"));
            self.emitf(format!("csel {dest}, {tmp1}, {tmp2}, ne"));
        } else {
            self.emit_comment("select: temp slots unavailable");
            self.emitf(format!("mov {dest}, #0"));
        }
        dest.to_string()
    }

    fn emit_expr_unsupported(&mut self, expr: &Expr<X>, dest: &str) -> String {
        self.emit_comment(&format!("unsupported expr: {expr:?}"));
        self.emitf(format!("mov {dest}, #0"));
        dest.to_string()
    }

    /// Helper to emit a load from memory address in a register.
    fn emit_load_from_mem(&mut self, addr: &str, dest: &str, width: u8, signed: bool) {
        let dest32 = Self::reg_32(dest);
        match (width, signed, X::VALUE) {
            (1, false, _) => self.emitf(format!("ldrb {dest32}, [{addr}]")),
            (1, true, 32) => self.emitf(format!("ldrsb {dest32}, [{addr}]")),
            (1, true, 64) => self.emitf(format!("ldrsb {dest}, [{addr}]")),
            (2, false, _) => self.emitf(format!("ldrh {dest32}, [{addr}]")),
            (2, true, 32) => self.emitf(format!("ldrsh {dest32}, [{addr}]")),
            (2, true, 64) => self.emitf(format!("ldrsh {dest}, [{addr}]")),
            (4, true, 64) => self.emitf(format!("ldrsw {dest}, [{addr}]")),
            (8, _, _) => self.emitf(format!("ldr {dest}, [{addr}]")),
            _ => self.emitf(format!("ldr {dest32}, [{addr}]")),
        }
    }
}
