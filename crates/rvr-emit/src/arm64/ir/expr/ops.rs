use super::*;

impl<X: Xlen> Arm64Emitter<X> {
    /// Emit a binary operation.
    #[allow(clippy::collapsible_if)]
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
                        if X::VALUE == 32 && right_reg.starts_with("x") {
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
                    if X::VALUE == 32 && left_reg.starts_with("x") {
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
        if X::VALUE == 32 && left_reg.starts_with("x") {
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
        if X::VALUE == 32 && right_val.starts_with("x") {
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

    pub(crate) fn emit_extern_call(&mut self, fn_name: &str, args: &[Expr<X>]) -> String {
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
        if X::VALUE == 32 && dest.starts_with("w") && inner_reg.starts_with("x") {
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
}
