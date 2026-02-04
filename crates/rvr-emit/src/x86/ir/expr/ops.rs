use super::{BinaryOp, Expr, ReadExpr, UnaryOp, X86Emitter, Xlen, reserved};

struct BinaryEmitCtx<'a, X: Xlen> {
    right: &'a Expr<X>,
    right_is_imm: bool,
    right_val: &'a str,
    temp1: &'a str,
    temp2: &'a str,
    dest: &'a str,
    suffix: &'a str,
}

impl<X: Xlen> X86Emitter<X> {
    /// Emit a binary operation.
    #[allow(clippy::collapsible_if)]
    pub(super) fn emit_binary_op(
        &mut self,
        op: BinaryOp,
        left: &Expr<X>,
        right: &Expr<X>,
        dest: &str,
    ) -> String {
        if let Some(result) = self.emit_binary_op_imm_fast(op, left, right, dest) {
            return result;
        }
        if let Some(result) = self.emit_binary_op_in_place(op, left, right, dest) {
            return result;
        }

        let temp1 = Self::temp1();
        let temp2 = Self::temp2();
        let suffix = Self::suffix();

        let left_reg = self.emit_expr(left, temp1);
        if left_reg != temp1 {
            self.emitf(format!("mov{suffix} %{left_reg}, %{temp1}"));
        }

        if let Some(result) = self.emit_binary_shift(op, right, dest, temp1) {
            return result;
        }
        if let Some(result) = self.emit_binary_word(op, right, dest, temp2) {
            return result;
        }

        self.emit_binary_general(op, right, dest, temp1, temp2)
    }

    fn emit_move_left_to_dest(&mut self, left: &Expr<X>, dest: &str) -> String {
        let left_reg = self.emit_expr(left, dest);
        if left_reg != dest {
            if X::VALUE == 32 {
                self.emitf(format!(
                    "movl %{}, %{}",
                    Self::reg_dword(&left_reg),
                    Self::reg_dword(dest)
                ));
            } else {
                self.emitf(format!("movq %{left_reg}, %{dest}"));
            }
        }
        dest.to_string()
    }

    fn emit_binary_op_imm_fast(
        &mut self,
        op: BinaryOp,
        left: &Expr<X>,
        right: &Expr<X>,
        dest: &str,
    ) -> Option<String> {
        let Expr::Imm(imm) = right else {
            return None;
        };
        let v = X::to_u64(*imm);
        let full_mask = if X::VALUE == 32 {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        };

        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Or | BinaryOp::Xor if v == 0 => {
                Some(self.emit_move_left_to_dest(left, dest))
            }
            BinaryOp::And | BinaryOp::Mul if v == 0 => {
                let suffix = Self::suffix();
                self.emitf(format!("xor{suffix} %{dest}, %{dest}"));
                Some(dest.to_string())
            }
            BinaryOp::And if v == full_mask => Some(self.emit_move_left_to_dest(left, dest)),
            BinaryOp::Mul if v == 1 => Some(self.emit_move_left_to_dest(left, dest)),
            _ => None,
        }
    }

    fn emit_binary_op_in_place(
        &mut self,
        op: BinaryOp,
        left: &Expr<X>,
        right: &Expr<X>,
        dest: &str,
    ) -> Option<String> {
        if !matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Xor
                | BinaryOp::Mul
        ) {
            return None;
        }

        let Expr::Read(ReadExpr::Reg(reg)) = left else {
            return None;
        };
        let mapped = self.reg_map.get(*reg)?;
        if mapped != dest {
            return None;
        }

        let suffix = Self::suffix();
        let right_is_imm = matches!(right, Expr::Imm(_));
        let right_val = if let Expr::Imm(imm) = right {
            X::to_u64(*imm)
        } else {
            0
        };
        let imm_fits = i32::try_from(right_val).is_ok();

        if matches!(
            op,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::And | BinaryOp::Or | BinaryOp::Xor
        ) && right_is_imm
            && imm_fits
        {
            let right_i32 = i32::try_from(right_val).unwrap_or(0);
            let op_str = match op {
                BinaryOp::Add => "add",
                BinaryOp::Sub => "sub",
                BinaryOp::And => "and",
                BinaryOp::Or => "or",
                BinaryOp::Xor => "xor",
                _ => unreachable!(),
            };
            self.emitf(format!("{op_str}{suffix} ${right_i32}, %{dest}"));
            return Some(dest.to_string());
        }

        let temp2 = Self::temp2();
        let right_reg = self.emit_expr(right, temp2);
        match op {
            BinaryOp::Add => self.emitf(format!("add{suffix} %{right_reg}, %{dest}")),
            BinaryOp::Sub => self.emitf(format!("sub{suffix} %{right_reg}, %{dest}")),
            BinaryOp::And => self.emitf(format!("and{suffix} %{right_reg}, %{dest}")),
            BinaryOp::Or => self.emitf(format!("or{suffix} %{right_reg}, %{dest}")),
            BinaryOp::Xor => self.emitf(format!("xor{suffix} %{right_reg}, %{dest}")),
            BinaryOp::Mul => {
                if X::VALUE == 32 {
                    self.emitf(format!("imull %{right_reg}, %{dest}"));
                } else {
                    self.emitf(format!("imulq %{right_reg}, %{dest}"));
                }
            }
            _ => {}
        }

        Some(dest.to_string())
    }

    fn emit_binary_shift(
        &mut self,
        op: BinaryOp,
        right: &Expr<X>,
        dest: &str,
        temp1: &str,
    ) -> Option<String> {
        if !matches!(
            op,
            BinaryOp::Sll
                | BinaryOp::Srl
                | BinaryOp::Sra
                | BinaryOp::SllW
                | BinaryOp::SrlW
                | BinaryOp::SraW
        ) {
            return None;
        }

        let suffix = Self::suffix();
        let x86_op = match op {
            BinaryOp::Sll | BinaryOp::SllW => "shl",
            BinaryOp::Srl | BinaryOp::SrlW => "shr",
            BinaryOp::Sra | BinaryOp::SraW => "sar",
            _ => unreachable!(),
        };
        let is_word = matches!(op, BinaryOp::SllW | BinaryOp::SrlW | BinaryOp::SraW);

        if let Expr::Imm(imm) = right {
            let mask = if is_word || X::VALUE == 32 {
                0x1f_u64
            } else {
                0x3f_u64
            };
            let shift = u8::try_from(X::to_u64(*imm) & mask).unwrap_or(0);

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
                return Some(dest.to_string());
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
            let temp3 = Self::temp3();
            let right_reg = self.emit_expr(right, temp3);
            if right_reg != "rcx" && right_reg != "ecx" && right_reg != "cl" {
                self.emitf(format!("movl %{}, %ecx", Self::reg_dword(&right_reg)));
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

        Some(dest.to_string())
    }

    fn emit_binary_word(
        &mut self,
        op: BinaryOp,
        right: &Expr<X>,
        dest: &str,
        temp2: &str,
    ) -> Option<String> {
        let (BinaryOp::AddW | BinaryOp::SubW) = op else {
            return None;
        };
        let x86_op = if op == BinaryOp::AddW { "addl" } else { "subl" };

        if let Expr::Imm(imm) = right {
            let v = i32::try_from(X::to_u64(*imm)).unwrap_or(0);
            self.emitf(format!("{x86_op} ${v}, %eax"));
        } else {
            let right_reg = self.emit_expr(right, temp2);
            self.emitf(format!("{x86_op} %{}, %eax", Self::reg_dword(&right_reg)));
        }
        self.emitf(format!("movslq %eax, %{dest}"));
        Some(dest.to_string())
    }

    fn emit_binary_general(
        &mut self,
        op: BinaryOp,
        right: &Expr<X>,
        dest: &str,
        temp1: &str,
        temp2: &str,
    ) -> String {
        let suffix = Self::suffix();
        let right_is_imm = matches!(right, Expr::Imm(_));
        let right_val = if let Expr::Imm(imm) = right {
            let imm_i64 = X::to_u64(*imm).cast_signed();
            format!("${imm_i64}")
        } else {
            let r = self.emit_expr(right, temp2);
            format!("%{r}")
        };
        let ctx = BinaryEmitCtx {
            right,
            right_is_imm,
            right_val: &right_val,
            temp1,
            temp2,
            dest,
            suffix,
        };

        if let Some(result) = self.emit_binary_arith(op, &ctx) {
            return result;
        }
        if let Some(result) =
            self.emit_binary_mul_special(op, right_is_imm, &right_val, temp2, dest, suffix)
        {
            return result;
        }
        if let Some(result) =
            self.emit_binary_div_rem(op, right_is_imm, &right_val, temp1, temp2, dest)
        {
            return result;
        }
        if let Some(result) = self.emit_binary_compare(op, &right_val, temp1, dest, suffix) {
            return result;
        }

        self.emit_comment(&format!("unsupported binary op: {op:?}"));
        self.finish_binary(dest, temp1, suffix)
    }

    fn emit_binary_arith(&mut self, op: BinaryOp, ctx: &BinaryEmitCtx<'_, X>) -> Option<String> {
        match op {
            BinaryOp::Add => {
                if ctx.right_is_imm {
                    if let Expr::Imm(imm) = ctx.right {
                        let v = X::to_u64(*imm).cast_signed();
                        if let Ok(v32) = i32::try_from(v) {
                            if X::VALUE == 32 {
                                self.emitf(format!("leal {v32}(%{}), %{}", ctx.temp1, ctx.temp1));
                            } else {
                                self.emitf(format!("leaq {v32}(%{}), %{}", ctx.temp1, ctx.temp1));
                            }
                        } else {
                            self.emitf(format!(
                                "add{} {}, %{}",
                                ctx.suffix, ctx.right_val, ctx.temp1
                            ));
                        }
                    } else {
                        self.emitf(format!(
                            "add{} {}, %{}",
                            ctx.suffix, ctx.right_val, ctx.temp1
                        ));
                    }
                } else {
                    self.emitf(format!(
                        "add{} {}, %{}",
                        ctx.suffix, ctx.right_val, ctx.temp1
                    ));
                }
                Some(self.finish_binary(ctx.dest, ctx.temp1, ctx.suffix))
            }
            BinaryOp::Sub => {
                self.emitf(format!(
                    "sub{} {}, %{}",
                    ctx.suffix, ctx.right_val, ctx.temp1
                ));
                Some(self.finish_binary(ctx.dest, ctx.temp1, ctx.suffix))
            }
            BinaryOp::And => {
                self.emitf(format!(
                    "and{} {}, %{}",
                    ctx.suffix, ctx.right_val, ctx.temp1
                ));
                Some(self.finish_binary(ctx.dest, ctx.temp1, ctx.suffix))
            }
            BinaryOp::Or => {
                self.emitf(format!(
                    "or{} {}, %{}",
                    ctx.suffix, ctx.right_val, ctx.temp1
                ));
                Some(self.finish_binary(ctx.dest, ctx.temp1, ctx.suffix))
            }
            BinaryOp::Xor => {
                self.emitf(format!(
                    "xor{} {}, %{}",
                    ctx.suffix, ctx.right_val, ctx.temp1
                ));
                Some(self.finish_binary(ctx.dest, ctx.temp1, ctx.suffix))
            }
            BinaryOp::Mul => {
                if ctx.right_is_imm {
                    self.emitf(format!(
                        "mov{} {}, %{}",
                        ctx.suffix, ctx.right_val, ctx.temp2
                    ));
                    self.emitf(format!("imul{} %{}, %{}", ctx.suffix, ctx.temp2, ctx.temp1));
                } else {
                    self.emitf(format!(
                        "imul{} {}, %{}",
                        ctx.suffix, ctx.right_val, ctx.temp1
                    ));
                }
                Some(self.finish_binary(ctx.dest, ctx.temp1, ctx.suffix))
            }
            _ => None,
        }
    }

    fn emit_binary_mul_special(
        &mut self,
        op: BinaryOp,
        right_is_imm: bool,
        right_val: &str,
        temp2: &str,
        dest: &str,
        suffix: &str,
    ) -> Option<String> {
        match op {
            BinaryOp::MulW => {
                if right_is_imm {
                    self.emitf(format!("movl {right_val}, %ecx"));
                }
                self.emitf("imull %ecx, %eax".to_string());
                self.emitf(format!("movslq %eax, %{dest}"));
                Some(dest.to_string())
            }
            BinaryOp::MulH => {
                if right_is_imm {
                    self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
                }
                if X::VALUE == 32 {
                    self.emit("cdq");
                    self.emitf(format!("imull %{}", Self::reg_dword(temp2)));
                    self.emitf(format!("movl %edx, %{dest}"));
                } else {
                    self.emit("cqo");
                    self.emitf(format!("imulq %{temp2}"));
                    self.emitf(format!("movq %rdx, %{dest}"));
                }
                Some(dest.to_string())
            }
            BinaryOp::MulHU => {
                if right_is_imm {
                    self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
                }
                if X::VALUE == 32 {
                    self.emitf(format!("mull %{}", Self::reg_dword(temp2)));
                    self.emitf(format!("movl %edx, %{dest}"));
                } else {
                    self.emitf(format!("mulq %{temp2}"));
                    self.emitf(format!("movq %rdx, %{dest}"));
                }
                Some(dest.to_string())
            }
            BinaryOp::MulHSU => {
                if right_is_imm {
                    self.emitf(format!("mov{suffix} {right_val}, %{temp2}"));
                }
                if X::VALUE == 32 {
                    self.emitf(format!("mull %{}", Self::reg_dword(temp2)));
                    self.emit("testl %eax, %eax");
                    let done = self.next_label("mulhsu_done");
                    self.emitf(format!("jns {done}"));
                    self.emitf(format!("subl %{}, %edx", Self::reg_dword(temp2)));
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
                Some(dest.to_string())
            }
            _ => None,
        }
    }

    fn emit_binary_div_rem(
        &mut self,
        op: BinaryOp,
        right_is_imm: bool,
        right_val: &str,
        temp1: &str,
        temp2: &str,
        dest: &str,
    ) -> Option<String> {
        match op {
            BinaryOp::Div => Some(self.emit_div_signed(right_is_imm, right_val, temp2, dest)),
            BinaryOp::DivU => Some(self.emit_div_unsigned(right_is_imm, right_val, temp2, dest)),
            BinaryOp::Rem => {
                Some(self.emit_rem_signed(right_is_imm, right_val, temp1, temp2, dest))
            }
            BinaryOp::RemU => {
                Some(self.emit_rem_unsigned(right_is_imm, right_val, temp1, temp2, dest))
            }
            BinaryOp::DivW => Some(self.emit_divw_signed(right_is_imm, right_val, temp2, dest)),
            BinaryOp::DivUW => Some(self.emit_divw_unsigned(right_is_imm, right_val, temp2, dest)),
            BinaryOp::RemW => Some(self.emit_remw_signed(right_is_imm, right_val, temp2, dest)),
            BinaryOp::RemUW => Some(self.emit_remw_unsigned(right_is_imm, right_val, temp2, dest)),
            _ => None,
        }
    }

    fn emit_binary_compare(
        &mut self,
        op: BinaryOp,
        right_val: &str,
        temp1: &str,
        dest: &str,
        suffix: &str,
    ) -> Option<String> {
        let setcc = match op {
            BinaryOp::Eq => "sete",
            BinaryOp::Ne => "setne",
            BinaryOp::Lt => "setl",
            BinaryOp::Ge => "setge",
            BinaryOp::Ltu => "setb",
            BinaryOp::Geu => "setae",
            _ => return None,
        };
        self.emitf(format!("cmp{suffix} {right_val}, %{temp1}"));
        self.emitf(format!("{setcc} %al"));
        self.emitf(format!("movzbl %al, %{}", Self::reg_dword(dest)));
        Some(dest.to_string())
    }

    fn finish_binary(&mut self, dest: &str, temp1: &str, suffix: &str) -> String {
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
        let suffix = Self::suffix();
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

        let no_ov = self.next_label("div_no_ov");
        if X::VALUE == 32 {
            self.emit("cmpl $0x80000000, %eax");
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("cmpl $-1, %{}", Self::reg_dword(temp2)));
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("movl $0x80000000, %{dest}"));
            self.emitf(format!("jmp {done}"));
            self.emit_label(&no_ov);
            self.emit("cdq");
            self.emitf(format!("idivl %{}", Self::reg_dword(temp2)));
            self.emitf(format!("movl %eax, %{dest}"));
        } else {
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
        let suffix = Self::suffix();
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
            self.emitf(format!("divl %{}", Self::reg_dword(temp2)));
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
        let suffix = Self::suffix();
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

        let no_ov = self.next_label("rem_no_ov");
        if X::VALUE == 32 {
            self.emit("cmpl $0x80000000, %eax");
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("cmpl $-1, %{}", Self::reg_dword(temp2)));
            self.emitf(format!("jne {no_ov}"));
            self.emitf(format!("xorl %{dest}, %{dest}"));
            self.emitf(format!("jmp {done}"));
            self.emit_label(&no_ov);
            self.emit("cdq");
            self.emitf(format!("idivl %{}", Self::reg_dword(temp2)));
            self.emitf(format!("movl %edx, %{dest}"));
        } else {
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
        let suffix = Self::suffix();
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
            self.emitf(format!("divl %{}", Self::reg_dword(temp2)));
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
            self.emitf(format!("movl {right_val}, %{}", Self::reg_dword(temp2)));
        }
        let skip = self.next_label("divw_skip");
        let done = self.next_label("divw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            Self::reg_dword(temp2),
            Self::reg_dword(temp2)
        ));
        self.emitf(format!("jnz {skip}"));
        self.emitf(format!("movq $-1, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&skip);

        let no_ov = self.next_label("divw_no_ov");
        self.emit("cmpl $0x80000000, %eax");
        self.emitf(format!("jne {no_ov}"));
        self.emitf(format!("cmpl $-1, %{}", Self::reg_dword(temp2)));
        self.emitf(format!("jne {no_ov}"));
        self.emit("movl $0x80000000, %eax");
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&no_ov);
        self.emit("cdq");
        self.emitf(format!("idivl %{}", Self::reg_dword(temp2)));
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
            self.emitf(format!("movl {right_val}, %{}", Self::reg_dword(temp2)));
        }
        let do_div = self.next_label("divuw_do");
        let done = self.next_label("divuw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            Self::reg_dword(temp2),
            Self::reg_dword(temp2)
        ));
        self.emitf(format!("jnz {do_div}"));
        self.emitf(format!("movq $-1, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&do_div);
        self.emit("xorl %edx, %edx");
        self.emitf(format!("divl %{}", Self::reg_dword(temp2)));
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
            self.emitf(format!("movl {right_val}, %{}", Self::reg_dword(temp2)));
        }
        let skip = self.next_label("remw_skip");
        let done = self.next_label("remw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            Self::reg_dword(temp2),
            Self::reg_dword(temp2)
        ));
        self.emitf(format!("jnz {skip}"));
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&skip);

        let no_ov = self.next_label("remw_no_ov");
        self.emit("cmpl $0x80000000, %eax");
        self.emitf(format!("jne {no_ov}"));
        self.emitf(format!("cmpl $-1, %{}", Self::reg_dword(temp2)));
        self.emitf(format!("jne {no_ov}"));
        self.emitf(format!("xorq %{dest}, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&no_ov);
        self.emit("cdq");
        self.emitf(format!("idivl %{}", Self::reg_dword(temp2)));
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
            self.emitf(format!("movl {right_val}, %{}", Self::reg_dword(temp2)));
        }
        let do_div = self.next_label("remuw_do");
        let done = self.next_label("remuw_done");

        self.emitf(format!(
            "testl %{}, %{}",
            Self::reg_dword(temp2),
            Self::reg_dword(temp2)
        ));
        self.emitf(format!("jnz {do_div}"));
        self.emitf(format!("movslq %eax, %{dest}"));
        self.emitf(format!("jmp {done}"));
        self.emit_label(&do_div);
        self.emit("xorl %edx, %edx");
        self.emitf(format!("divl %{}", Self::reg_dword(temp2)));
        self.emitf(format!("movslq %edx, %{dest}"));
        self.emit_label(&done);
        dest.to_string()
    }

    pub(crate) fn emit_extern_call(&mut self, fn_name: &str, args: &[Expr<X>]) -> String {
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
                        let v32 = i32::try_from(v).unwrap_or(0);
                        self.emitf(format!("movl ${v32}, %{arg_reg}"));
                    } else if v > 0x7fff_ffff {
                        self.emitf(format!("movabsq $0x{v:x}, %{arg_reg}"));
                    } else {
                        self.emitf(format!("movq $0x{v:x}, %{arg_reg}"));
                    }
                }
                _ => {
                    let tmp = self.emit_expr(arg, Self::temp1());
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
        let temp1 = Self::temp1();
        let suffix = Self::suffix();
        let inner_reg = self.emit_expr(inner, temp1);
        if inner_reg != temp1 {
            self.emitf(format!("mov{suffix} %{inner_reg}, %{temp1}"));
        }

        if matches!(op, UnaryOp::Neg | UnaryOp::Not) {
            match op {
                UnaryOp::Neg => self.emitf(format!("neg{suffix} %{temp1}")),
                UnaryOp::Not => self.emitf(format!("not{suffix} %{temp1}")),
                _ => {}
            }
            return self.finish_unary(dest, temp1, suffix);
        }

        if let Some(result) = self.emit_unary_extend(op, dest, temp1) {
            return result;
        }
        if let Some(result) = self.emit_unary_bitcount(op, dest, temp1, suffix) {
            return result;
        }
        if let Some(result) = self.emit_unary_misc(op, dest, temp1, suffix) {
            return result;
        }

        self.emit_comment(&format!("unary op {op:?} simplified"));
        self.finish_unary(dest, temp1, suffix)
    }

    fn emit_unary_extend(&mut self, op: UnaryOp, dest: &str, temp1: &str) -> Option<String> {
        match op {
            UnaryOp::Sext8 => {
                if X::VALUE == 32 {
                    self.emitf(format!("movsbl %al, %{dest}"));
                } else {
                    self.emitf(format!("movsbq %al, %{dest}"));
                }
                Some(dest.to_string())
            }
            UnaryOp::Sext16 => {
                if X::VALUE == 32 {
                    self.emitf(format!("movswl %ax, %{dest}"));
                } else {
                    self.emitf(format!("movswq %ax, %{dest}"));
                }
                Some(dest.to_string())
            }
            UnaryOp::Sext32 => {
                self.emitf(format!("movslq %eax, %{dest}"));
                Some(dest.to_string())
            }
            UnaryOp::Zext8 => {
                self.emitf(format!("movzbl %al, %{}", Self::reg_dword(dest)));
                Some(dest.to_string())
            }
            UnaryOp::Zext16 => {
                self.emitf(format!("movzwl %ax, %{}", Self::reg_dword(dest)));
                Some(dest.to_string())
            }
            UnaryOp::Zext32 => {
                self.emit("movl %eax, %eax");
                if dest != temp1 {
                    self.emitf(format!("movq %{temp1}, %{dest}"));
                }
                Some(dest.to_string())
            }
            _ => None,
        }
    }

    fn emit_unary_bitcount(
        &mut self,
        op: UnaryOp,
        dest: &str,
        temp1: &str,
        suffix: &str,
    ) -> Option<String> {
        match op {
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
                } else {
                    self.emit("testq %rax, %rax");
                    self.emitf(format!("jz {zero_label}"));
                    self.emit("bsrq %rax, %rax");
                    self.emit("xorq $63, %rax");
                    self.emitf(format!("jmp {done_label}"));
                    self.emit_label(&zero_label);
                    self.emit("movq $64, %rax");
                }
                self.emit_label(&done_label);
                Some(self.finish_unary(dest, temp1, suffix))
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
                } else {
                    self.emit("testq %rax, %rax");
                    self.emitf(format!("jz {zero_label}"));
                    self.emit("bsfq %rax, %rax");
                    self.emitf(format!("jmp {done_label}"));
                    self.emit_label(&zero_label);
                    self.emit("movq $64, %rax");
                }
                self.emit_label(&done_label);
                Some(self.finish_unary(dest, temp1, suffix))
            }
            _ => None,
        }
    }

    fn emit_unary_misc(
        &mut self,
        op: UnaryOp,
        dest: &str,
        temp1: &str,
        suffix: &str,
    ) -> Option<String> {
        match op {
            UnaryOp::Cpop => {
                if X::VALUE == 32 {
                    self.emit("popcntl %eax, %eax");
                } else {
                    self.emit("popcntq %rax, %rax");
                }
                Some(self.finish_unary(dest, temp1, suffix))
            }
            UnaryOp::Rev8 => {
                if X::VALUE == 32 {
                    self.emit("bswapl %eax");
                } else {
                    self.emit("bswapq %rax");
                }
                Some(self.finish_unary(dest, temp1, suffix))
            }
            _ => None,
        }
    }

    fn finish_unary(&mut self, dest: &str, temp1: &str, suffix: &str) -> String {
        if dest != temp1 {
            self.emitf(format!("mov{suffix} %{temp1}, %{dest}"));
        }
        dest.to_string()
    }
}
