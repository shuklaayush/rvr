//! IR translation for ARM64 assembly.
//!
//! Translates IR expressions, statements, and terminators to ARM64 assembly.

use rvr_ir::{
    BinaryOp, BlockIR, Expr, InstrIR, ReadExpr, Stmt, Terminator, TernaryOp, UnaryOp, WriteTarget,
    Xlen,
};

use super::Arm64Emitter;
use super::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
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
                // Add memory base
                self.emitf(format!("add x0, x0, {}", reserved::MEMORY_PTR));
                self.emit_load_from_mem("x0", dest, *width, *signed);
                dest.to_string()
            }
            Expr::Read(ReadExpr::MemAddr {
                addr,
                width,
                signed,
            }) => {
                self.emit_expr_as_addr(addr);
                self.apply_address_mode("x0");
                self.emitf(format!("add x0, x0, {}", reserved::MEMORY_PTR));
                self.emit_load_from_mem("x0", dest, *width, *signed);
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
                    0xC00 | 0xC02 => {
                        // cycle/instret
                        self.emitf(format!(
                            "ldr {dest}, [{}, #{}]",
                            reserved::STATE_PTR,
                            instret_off
                        ));
                    }
                    0xC80 | 0xC82 if X::VALUE == 32 => {
                        // cycleh/instreth (upper 32 bits)
                        self.emitf(format!(
                            "ldr {dest}, [{}, #{}]",
                            reserved::STATE_PTR,
                            instret_off + 4
                        ));
                    }
                    _ => {
                        self.emit_comment(&format!("CSR 0x{:03x} not implemented", csr));
                        self.emitf(format!("mov {dest}, #0"));
                    }
                }
                dest.to_string()
            }
            Expr::Read(ReadExpr::Cycle) | Expr::Read(ReadExpr::Instret) => {
                let instret_off = self.layout.offset_instret;
                self.emitf(format!(
                    "ldr {dest}, [{}, #{}]",
                    reserved::STATE_PTR,
                    instret_off
                ));
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
            Expr::Binary { op, left, right } => self.emit_binary_op(*op, left, right, dest),
            Expr::Unary { op, expr: inner } => self.emit_unary_op(*op, inner, dest),
            Expr::Ternary {
                op: TernaryOp::Select,
                first: cond,
                second: then_val,
                third: else_val,
            } => {
                // Use correct register width for RV32 vs RV64
                let (tmp1, tmp2) = if X::VALUE == 32 {
                    ("w1", "w2")
                } else {
                    ("x1", "x2")
                };
                // Evaluate condition
                let cond_reg = self.emit_expr(cond, self.temp1());
                // Evaluate both values
                let then_reg = self.emit_expr(then_val, tmp1);
                if then_reg != tmp1 {
                    self.emitf(format!("mov {tmp1}, {then_reg}"));
                }
                let else_reg = self.emit_expr(else_val, tmp2);
                if else_reg != tmp2 {
                    self.emitf(format!("mov {tmp2}, {else_reg}"));
                }
                // Compare condition and select
                self.emitf(format!("cmp {cond_reg}, #0"));
                self.emitf(format!("csel {dest}, {tmp1}, {tmp2}, ne"));
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

        // Load left operand
        let left_reg = self.emit_expr(left, temp1);
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
                let right_reg = self.emit_expr(right, temp2);
                let t1_32 = self.reg_32(temp1);
                let r_32 = self.reg_32(&right_reg);
                self.emitf(format!("add {t1_32}, {t1_32}, {r_32}"));
                self.emitf(format!("sxtw {dest}, {t1_32}"));
                return dest.to_string();
            }
            BinaryOp::SubW => {
                let right_reg = self.emit_expr(right, temp2);
                let t1_32 = self.reg_32(temp1);
                let r_32 = self.reg_32(&right_reg);
                self.emitf(format!("sub {t1_32}, {t1_32}, {r_32}"));
                self.emitf(format!("sxtw {dest}, {t1_32}"));
                return dest.to_string();
            }
            BinaryOp::MulW => {
                let right_reg = self.emit_expr(right, temp2);
                let t1_32 = self.reg_32(temp1);
                let r_32 = self.reg_32(&right_reg);
                self.emitf(format!("mul {t1_32}, {t1_32}, {r_32}"));
                self.emitf(format!("sxtw {dest}, {t1_32}"));
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

        // Load right operand or use immediate
        let (right_is_imm, right_val) = if let Expr::Imm(imm) = right {
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

        match op {
            BinaryOp::Add => {
                self.emitf(format!("add {dest}, {temp1}, {right_val}"));
            }
            BinaryOp::Sub => {
                self.emitf(format!("sub {dest}, {temp1}, {right_val}"));
            }
            BinaryOp::And => {
                if right_is_imm {
                    // and with immediate needs special handling for large values
                    let right_reg = self.emit_expr(right, temp2);
                    self.emitf(format!("and {dest}, {temp1}, {right_reg}"));
                } else {
                    self.emitf(format!("and {dest}, {temp1}, {right_val}"));
                }
            }
            BinaryOp::Or => {
                if right_is_imm {
                    let right_reg = self.emit_expr(right, temp2);
                    self.emitf(format!("orr {dest}, {temp1}, {right_reg}"));
                } else {
                    self.emitf(format!("orr {dest}, {temp1}, {right_val}"));
                }
            }
            BinaryOp::Xor => {
                if right_is_imm {
                    let right_reg = self.emit_expr(right, temp2);
                    self.emitf(format!("eor {dest}, {temp1}, {right_reg}"));
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
            let shift_reg = self.emit_expr(right, self.temp2());
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
            let shift_reg = self.emit_expr(right, self.temp2());
            let shift32 = self.reg_32(&shift_reg);
            self.emitf(format!("{shift_op} {src32}, {src32}, {shift32}"));
        }
        // Sign extend result
        self.emitf(format!("sxtw {dest}, {src32}"));
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
        let right_reg = self.emit_expr(right, temp2);

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
                self.emitf(format!("mov {dest}, x2"));
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
        let right_reg = self.emit_expr(right, temp2);
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
                self.emitf(format!("sxtw {dest}, {left32}"));
            }
            _ => unreachable!(),
        }
        self.emitf(format!("b {done_label}"));
        self.emit_label(&skip_label);

        match op {
            BinaryOp::DivW => {
                self.emitf(format!("sdiv w2, {left32}, {right32}"));
                self.emitf(format!("sxtw {dest}, w2"));
            }
            BinaryOp::DivUW => {
                self.emitf(format!("udiv w2, {left32}, {right32}"));
                self.emitf(format!("sxtw {dest}, w2"));
            }
            BinaryOp::RemW => {
                self.emitf(format!("sdiv w2, {left32}, {right32}"));
                self.emitf(format!("msub w2, w2, {right32}, {left32}"));
                self.emitf(format!("sxtw {dest}, w2"));
            }
            BinaryOp::RemUW => {
                self.emitf(format!("udiv w2, {left32}, {right32}"));
                self.emitf(format!("msub w2, w2, {right32}, {left32}"));
                self.emitf(format!("sxtw {dest}, w2"));
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
        let right_reg = self.emit_expr(right, temp2);

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
                    self.emitf(format!("smull {dest64}, {left32}, {right32}"));
                    self.emitf(format!("asr {dest64}, {dest64}, #32"));
                    // If right was negative (as signed), we added 2^32 * left to the result
                    // Need to add it back: if right32 < 0, add left32 to high word
                    self.emitf(format!("cmp {right32}, #0"));
                    self.emitf(format!("csel w2, {left32}, wzr, lt"));
                    self.emitf(format!("add {dest}, {dest}, w2, sxtw"));
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
        let temp1 = self.temp1();
        let inner_reg = self.emit_expr(inner, temp1);
        if inner_reg != temp1 {
            self.emitf(format!("mov {temp1}, {inner_reg}"));
        }

        match op {
            UnaryOp::Neg => {
                self.emitf(format!("neg {dest}, {temp1}"));
            }
            UnaryOp::Not => {
                self.emitf(format!("mvn {dest}, {temp1}"));
            }
            UnaryOp::Sext8 => {
                self.emitf(format!("sxtb {dest}, {}", self.reg_32(temp1)));
            }
            UnaryOp::Sext16 => {
                self.emitf(format!("sxth {dest}, {}", self.reg_32(temp1)));
            }
            UnaryOp::Sext32 => {
                self.emitf(format!("sxtw {dest}, {}", self.reg_32(temp1)));
            }
            UnaryOp::Zext8 => {
                self.emitf(format!(
                    "uxtb {}, {}",
                    self.reg_32(dest),
                    self.reg_32(temp1)
                ));
            }
            UnaryOp::Zext16 => {
                self.emitf(format!(
                    "uxth {}, {}",
                    self.reg_32(dest),
                    self.reg_32(temp1)
                ));
            }
            UnaryOp::Zext32 => {
                // Moving w to x zero-extends automatically
                let src32 = self.reg_32(temp1);
                let dest32 = self.reg_32(dest);
                self.emitf(format!("mov {dest32}, {src32}"));
            }
            UnaryOp::Clz => {
                self.emitf(format!("clz {dest}, {temp1}"));
            }
            UnaryOp::Ctz => {
                // ctz = clz(rbit(x))
                self.emitf(format!("rbit {dest}, {temp1}"));
                self.emitf(format!("clz {dest}, {dest}"));
            }
            UnaryOp::Cpop => {
                // Population count - needs NEON or loop
                // For now use a simple approach with NEON if available
                self.emit_comment("cpop requires NEON; using fallback");
                // Fallback: store to stack, use fmov, cnt, then reduce
                // Simplified: just return 0 as placeholder
                self.emitf(format!("mov {dest}, #0"));
            }
            UnaryOp::Rev8 => {
                // Byte reverse
                self.emitf(format!("rev {dest}, {temp1}"));
            }
            _ => {
                self.emit_comment(&format!("unary op {:?} not implemented", op));
                self.emitf(format!("mov {dest}, {temp1}"));
            }
        }

        dest.to_string()
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
                    let val_reg = self.emit_expr(value, temp1);
                    self.store_to_rv(*reg, &val_reg);
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
                    self.emit_expr_as_addr(base);
                    if *offset != 0 {
                        self.emit_add_offset("x0", "x0", (*offset).into());
                    }
                    self.apply_address_mode("x0");

                    // HTIF handling: check for tohost write
                    if self.config.htif_enabled && (*width == 4 || *width == 8) {
                        self.emit_htif_check();
                    }

                    self.emitf(format!("add x0, x0, {}", reserved::MEMORY_PTR));
                    // Store
                    let val32 = self.reg_32(temp2);
                    match width {
                        1 => self.emitf(format!("strb {val32}, [x0]")),
                        2 => self.emitf(format!("strh {val32}, [x0]")),
                        4 => self.emitf(format!("str {val32}, [x0]")),
                        8 => self.emitf(format!("str {temp2}, [x0]")),
                        _ => self.emitf(format!("str {val32}, [x0]")),
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
                _ => self.emit_comment(&format!("unsupported write: {:?}", target)),
            },
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => {
                let cond_reg = self.emit_expr(cond, temp1);
                let else_label = self.next_label("if_else");
                let end_label = self.next_label("if_end");
                self.emitf(format!("cbz {cond_reg}, {else_label}"));
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
            Stmt::ExternCall { fn_name, .. } => {
                self.emit_comment(&format!("extern call: {fn_name}"));
            }
        }
    }

    /// Resolve a target PC, handling absorbed blocks.
    /// Returns the PC to branch to (either the original or the merged block).
    fn resolve_target_pc(&self, pc: u64) -> u64 {
        self.inputs
            .absorbed_to_merged
            .get(&pc)
            .copied()
            .unwrap_or(pc)
    }

    /// Emit a terminator.
    pub(super) fn emit_terminator(&mut self, term: &Terminator<X>, next_pc: u64, current_pc: u64) {
        let temp1 = self.temp1();

        match term {
            Terminator::Fall { target } => {
                if let Some(t) = target {
                    let target_pc = self.resolve_target_pc(X::to_u64(*t));
                    // Don't emit branch if target is the next instruction or current instruction
                    if target_pc != next_pc && target_pc != current_pc {
                        self.emitf(format!("b asm_pc_{:x}", target_pc));
                    }
                }
            }
            Terminator::Jump { target } => {
                let target_pc = self.resolve_target_pc(X::to_u64(*target));
                // Don't emit a self-loop (would be a bug in IR)
                if target_pc != current_pc {
                    self.emitf(format!("b asm_pc_{:x}", target_pc));
                }
            }
            Terminator::JumpDyn { addr, .. } => {
                self.emit_expr_as_addr(addr);
                self.emit("and x0, x0, #-2"); // Clear lowest bit
                self.emit_dispatch_jump();
            }
            Terminator::Branch {
                cond, target, fall, ..
            } => {
                let cond_reg = self.emit_expr(cond, temp1);
                let target_pc = self.resolve_target_pc(X::to_u64(*target));
                self.emitf(format!("cbnz {cond_reg}, asm_pc_{:x}", target_pc));
                if let Some(f) = fall {
                    let fall_pc = self.resolve_target_pc(X::to_u64(*f));
                    if fall_pc != next_pc {
                        self.emitf(format!("b asm_pc_{:x}", fall_pc));
                    }
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
    pub(super) fn emit_instruction(&mut self, instr: &InstrIR<X>, is_last_in_block: bool) {
        let pc = X::to_u64(instr.pc);
        let next_pc = pc + instr.size as u64;
        self.emit_instret_increment(1, pc);
        for stmt in &instr.statements {
            self.emit_stmt(stmt);
        }
        // Only emit terminator control flow for the last instruction in a block.
        // Non-last instructions with Fall terminators should just fall through naturally.
        if is_last_in_block || instr.terminator.is_control_flow() {
            self.emit_terminator(&instr.terminator, next_pc, pc);
        }
    }

    /// Emit code for all blocks.
    pub fn emit_blocks(&mut self, blocks: &[(u64, BlockIR<X>)]) {
        self.emit_raw("// Generated code blocks");
        self.emit_blank();
        for (pc, block) in blocks {
            if self.label_pcs.contains(pc) {
                self.emit_pc_label(*pc);
            }
            let num_instructions = block.instructions.len();
            for (i, instr) in block.instructions.iter().enumerate() {
                let instr_pc = X::to_u64(instr.pc);
                if instr_pc != *pc && self.label_pcs.contains(&instr_pc) {
                    self.emit_pc_label(instr_pc);
                }
                let is_last = i == num_instructions - 1;
                self.emit_instruction(instr, is_last);
            }
        }
    }
}
