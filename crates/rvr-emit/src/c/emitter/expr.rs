//! Expression and statement rendering for the C emitter.

use rvr_ir::{BinaryOp, Expr, ReadExpr, Stmt, TernaryOp, UnaryOp, WriteTarget, Xlen};

use crate::htif::TOHOST_ADDR;

use super::CEmitter;

impl<X: Xlen> CEmitter<X> {
    pub(crate) fn render_expr(&self, expr: &Expr<X>) -> String {
        match expr {
            Expr::Imm(val) => Self::render_imm(*val),
            Expr::Read(read) => self.render_read(read),
            Expr::PcConst(pc) => Self::fmt_addr(X::to_u64(*pc)),
            Expr::Var(name) => self.render_var(name),
            Expr::Unary { op, expr } => self.render_unary_expr(*op, expr),
            Expr::Binary { op, left, right } => self.render_binary_expr(*op, left, right),
            Expr::Ternary {
                op,
                first,
                second,
                third,
            } => self.render_ternary_expr(*op, first, second, third),
            Expr::ExternCall { name, args, .. } => self.render_extern_call(name, args),
        }
    }

    fn render_imm(val: X::Reg) -> String {
        let val = X::to_u64(val);
        if X::VALUE == 64 {
            format!("0x{val:x}ULL")
        } else {
            format!("0x{val:x}u")
        }
    }

    fn render_var(&self, name: &str) -> String {
        if name == "state" && self.sig.fixed_addresses {
            self.state_ref().to_string()
        } else {
            name.to_string()
        }
    }

    fn render_unary_expr(&self, op: UnaryOp, expr: &Expr<X>) -> String {
        let o = self.render_expr(expr);
        match op {
            UnaryOp::Not => format!("(~{o})"),
            UnaryOp::Neg => format!("(-{o})"),
            UnaryOp::Sext8 => format!("(({})(int8_t){})", self.reg_type, o),
            UnaryOp::Sext16 => format!("(({})(int16_t){})", self.reg_type, o),
            UnaryOp::Sext32 => {
                if X::VALUE == 64 {
                    format!("((uint64_t)(int64_t)(int32_t){o})")
                } else {
                    o
                }
            }
            UnaryOp::Zext8 => format!("(({})(uint8_t){})", self.reg_type, o),
            UnaryOp::Zext16 => format!("(({})(uint16_t){})", self.reg_type, o),
            UnaryOp::Zext32 => {
                if X::VALUE == 64 {
                    format!("((uint64_t)(uint32_t){o})")
                } else {
                    o
                }
            }
            UnaryOp::Clz => {
                if X::VALUE == 64 {
                    format!("({o} ? __builtin_clzll({o}) : 64)")
                } else {
                    format!("({o} ? __builtin_clz({o}) : 32)")
                }
            }
            UnaryOp::Ctz => {
                if X::VALUE == 64 {
                    format!("({o} ? __builtin_ctzll({o}) : 64)")
                } else {
                    format!("({o} ? __builtin_ctz({o}) : 32)")
                }
            }
            UnaryOp::Cpop => {
                if X::VALUE == 64 {
                    format!("__builtin_popcountll({o})")
                } else {
                    format!("__builtin_popcount({o})")
                }
            }
            UnaryOp::Clz32 => {
                format!("((uint64_t)((uint32_t){o} ? __builtin_clz((uint32_t){o}) : 32))")
            }
            UnaryOp::Ctz32 => {
                format!("((uint64_t)((uint32_t){o} ? __builtin_ctz((uint32_t){o}) : 32))")
            }
            UnaryOp::Cpop32 => format!("((uint64_t)__builtin_popcount((uint32_t){o}))"),
            UnaryOp::Orc8 => {
                if X::VALUE == 64 {
                    format!("rv_orc_b64({o})")
                } else {
                    format!("rv_orc_b32({o})")
                }
            }
            UnaryOp::Rev8 => {
                if X::VALUE == 64 {
                    format!("__builtin_bswap64({o})")
                } else {
                    format!("__builtin_bswap32({o})")
                }
            }
            UnaryOp::Brev8 => {
                if X::VALUE == 64 {
                    format!("rv_brev8_64({o})")
                } else {
                    format!("rv_brev8_32({o})")
                }
            }
            UnaryOp::Zip => format!("rv_zip32({o})"),
            UnaryOp::Unzip => format!("rv_unzip32({o})"),
        }
    }

    fn render_binary_expr(&self, op: BinaryOp, left: &Expr<X>, right: &Expr<X>) -> String {
        let l = self.render_expr(left);
        let r = self.render_expr(right);
        if let Some(result) = self.render_binary_basic(op, &l, &r) {
            return result;
        }
        if let Some(result) = Self::render_binary_div_rem(op, &l, &r) {
            return result;
        }
        if let Some(result) = Self::render_binary_word(op, &l, &r) {
            return result;
        }
        if let Some(result) = Self::render_binary_mul_high(op, &l, &r) {
            return result;
        }
        self.render_binary_pack(op, &l, &r)
            .unwrap_or_else(|| unreachable!("unsupported binary op: {op:?}"))
    }

    fn render_binary_basic(&self, op: BinaryOp, l: &str, r: &str) -> Option<String> {
        let result = match op {
            BinaryOp::Add => format!("({l} + {r})"),
            BinaryOp::Sub => format!("({l} - {r})"),
            BinaryOp::Mul => format!("({l} * {r})"),
            BinaryOp::And => format!("({l} & {r})"),
            BinaryOp::Or => format!("({l} | {r})"),
            BinaryOp::Xor => format!("({l} ^ {r})"),
            BinaryOp::Sll => format!("({l} << {r})"),
            BinaryOp::Srl => format!("({l} >> {r})"),
            BinaryOp::Sra => format!(
                "(({})(({}){}  >> {}))",
                self.reg_type, self.signed_type, l, r
            ),
            BinaryOp::Eq => format!("{l} == {r}"),
            BinaryOp::Ne => format!("{l} != {r}"),
            BinaryOp::Lt => format!("({}){} < ({}){}", self.signed_type, l, self.signed_type, r),
            BinaryOp::Ge => format!("({}){} >= ({}){}", self.signed_type, l, self.signed_type, r),
            BinaryOp::Ltu => format!("{l} < {r}"),
            BinaryOp::Geu => format!("{l} >= {r}"),
            _ => return None,
        };
        Some(result)
    }

    fn render_binary_div_rem(op: BinaryOp, l: &str, r: &str) -> Option<String> {
        let result = match op {
            BinaryOp::Div => {
                if X::VALUE == 64 {
                    format!("rv_div64({l}, {r})")
                } else {
                    format!("rv_div({l}, {r})")
                }
            }
            BinaryOp::DivU => {
                if X::VALUE == 64 {
                    format!("rv_divu64({l}, {r})")
                } else {
                    format!("rv_divu({l}, {r})")
                }
            }
            BinaryOp::Rem => {
                if X::VALUE == 64 {
                    format!("rv_rem64({l}, {r})")
                } else {
                    format!("rv_rem({l}, {r})")
                }
            }
            BinaryOp::RemU => {
                if X::VALUE == 64 {
                    format!("rv_remu64({l}, {r})")
                } else {
                    format!("rv_remu({l}, {r})")
                }
            }
            _ => return None,
        };
        Some(result)
    }

    fn render_binary_word(op: BinaryOp, l: &str, r: &str) -> Option<String> {
        let result = match op {
            BinaryOp::AddW => {
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){l} + (uint32_t){r}))")
            }
            BinaryOp::SubW => {
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){l} - (uint32_t){r}))")
            }
            BinaryOp::MulW => {
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){l} * (uint32_t){r}))")
            }
            BinaryOp::DivW => format!("rv_divw((int32_t){l}, (int32_t){r})"),
            BinaryOp::DivUW => format!("rv_divuw((uint32_t){l}, (uint32_t){r})"),
            BinaryOp::RemW => format!("rv_remw((int32_t){l}, (int32_t){r})"),
            BinaryOp::RemUW => format!("rv_remuw((uint32_t){l}, (uint32_t){r})"),
            BinaryOp::SllW => {
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){l} << {r}))")
            }
            BinaryOp::SrlW => {
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){l} >> {r}))")
            }
            BinaryOp::SraW => format!("((uint64_t)(int64_t)((int32_t){l} >> {r}))"),
            _ => return None,
        };
        Some(result)
    }

    fn render_binary_mul_high(op: BinaryOp, l: &str, r: &str) -> Option<String> {
        let result = match op {
            BinaryOp::MulH => {
                if X::VALUE == 64 {
                    format!("rv_mulh64({l}, {r})")
                } else {
                    format!("rv_mulh({l}, {r})")
                }
            }
            BinaryOp::MulHSU => {
                if X::VALUE == 64 {
                    format!("rv_mulhsu64({l}, {r})")
                } else {
                    format!("rv_mulhsu({l}, {r})")
                }
            }
            BinaryOp::MulHU => {
                if X::VALUE == 64 {
                    format!("rv_mulhu64({l}, {r})")
                } else {
                    format!("rv_mulhu({l}, {r})")
                }
            }
            _ => return None,
        };
        Some(result)
    }

    fn render_binary_pack(&self, op: BinaryOp, l: &str, r: &str) -> Option<String> {
        let result = match op {
            BinaryOp::Pack => {
                if X::VALUE == 64 {
                    format!("(((uint64_t)(uint32_t){l}) | ((uint64_t)(uint32_t){r} << 32))")
                } else {
                    format!("(((uint32_t)(uint16_t){l}) | ((uint32_t)(uint16_t){r} << 16))")
                }
            }
            BinaryOp::Pack8 => format!(
                "((({})(uint8_t){}) | (({})(uint8_t){} << 8))",
                self.reg_type, l, self.reg_type, r
            ),
            BinaryOp::Pack16 => format!(
                "((int64_t)(int32_t)(((uint32_t)(uint16_t){l}) | ((uint32_t)(uint16_t){r} << 16)))"
            ),
            _ => return None,
        };
        Some(result)
    }

    fn render_ternary_expr(
        &self,
        op: TernaryOp,
        first: &Expr<X>,
        second: &Expr<X>,
        third: &Expr<X>,
    ) -> String {
        match op {
            TernaryOp::Select => {
                let c = self.render_expr(first);
                let t = self.render_expr(second);
                let e = self.render_expr(third);
                format!("({c} ? {t} : {e})")
            }
        }
    }

    fn render_extern_call(&self, name: &str, args: &[Expr<X>]) -> String {
        let args: Vec<String> = args.iter().map(|a| self.render_expr(a)).collect();
        format!("{name}({})", args.join(", "))
    }

    /// Render read expression.
    fn render_read(&self, expr: &ReadExpr<X>) -> String {
        match expr {
            ReadExpr::Reg(reg) => self.render_read_reg(*reg),
            ReadExpr::Mem {
                base,
                offset,
                width,
                signed,
            } => self.render_read_mem(base, *offset, *width, *signed),
            ReadExpr::MemAddr {
                addr,
                width,
                signed,
            } => self.render_read_mem_addr(addr, *width, *signed),
            ReadExpr::Csr(csr) => self.render_read_csr(*csr),
            _ => self.render_read_simple(expr),
        }
    }

    fn render_read_reg(&self, reg: u8) -> String {
        if reg == 0 {
            if X::VALUE == 64 {
                "0x0ULL".to_string()
            } else {
                "0".to_string()
            }
        } else if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            let state = self.state_ref();
            if self.sig.is_hot_reg(reg) {
                let val = self.sig.reg_read(reg);
                format!("trd_regval(&{state}->tracer, {pc_lit}, {op_lit}, {reg}, {val})")
            } else {
                format!("trd_reg(&{state}->tracer, {pc_lit}, {op_lit}, {state}, {reg})")
            }
        } else {
            self.sig.reg_read(reg)
        }
    }

    fn render_read_mem(&self, base: &Expr<X>, offset: i16, width: u8, signed: bool) -> String {
        let base = self.render_expr(base);
        self.render_mem_read(&base, offset, width, signed)
    }

    fn render_read_mem_addr(&self, addr: &Expr<X>, width: u8, signed: bool) -> String {
        let base = self.render_expr(addr);
        self.render_mem_read(&base, 0, width, signed)
    }

    fn render_mem_read(&self, base: &str, offset: i16, width: u8, signed: bool) -> String {
        if self.config.has_tracing() {
            let load_fn = Self::mem_trace_read_fn(width, signed);
            let pc_lit = Self::fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            let state = self.state_ref();
            format!("{load_fn}(&{state}->tracer, {pc_lit}, {op_lit}, memory, {base}, {offset})")
        } else if self.uses_fixed_addresses() {
            let load_fn = Self::mem_read_fn(width, signed);
            format!("{load_fn}({base}, {offset})")
        } else {
            let load_fn = Self::mem_read_fn(width, signed);
            format!("{load_fn}(memory, {base}, {offset})")
        }
    }

    const fn mem_trace_read_fn(width: u8, signed: bool) -> &'static str {
        match (width, signed, X::VALUE) {
            (1, true, _) => "trd_mem_i8",
            (1, false, _) => "trd_mem_u8",
            (2, true, _) => "trd_mem_i16",
            (2, false, _) => "trd_mem_u16",
            (4, true, 64) => "trd_mem_i32",
            (8, _, _) => "trd_mem_u64",
            _ => "trd_mem_u32",
        }
    }

    const fn mem_read_fn(width: u8, signed: bool) -> &'static str {
        match (width, signed, X::VALUE) {
            (1, true, _) => "rd_mem_i8",
            (1, false, _) => "rd_mem_u8",
            (2, true, _) => "rd_mem_i16",
            (2, false, _) => "rd_mem_u16",
            (4, true, 64) => "rd_mem_i32",
            (8, _, _) => "rd_mem_u64",
            _ => "rd_mem_u32",
        }
    }

    fn render_read_csr(&self, csr: u16) -> String {
        if self.config.perf_mode {
            return "0".to_string();
        }
        let state = self.state_ref();
        let state_arg = if self.uses_fixed_addresses() {
            String::new()
        } else {
            format!("{state}, ")
        };
        if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            if self.config.instret_mode.counts() {
                format!(
                    "trd_csr(&{state}->tracer, {pc_lit}, {op_lit}, {state_arg}0x{csr:x}, instret)"
                )
            } else {
                format!("trd_csr(&{state}->tracer, {pc_lit}, {op_lit}, {state_arg}0x{csr:x})")
            }
        } else if self.config.instret_mode.counts() {
            format!("rd_csr({state_arg}0x{csr:x}, instret)")
        } else {
            format!("rd_csr({state_arg}0x{csr:x})")
        }
    }

    fn render_read_simple(&self, expr: &ReadExpr<X>) -> String {
        match expr {
            ReadExpr::Pc => format!("{}->pc", self.state_ref()),
            ReadExpr::Cycle => {
                if self.config.perf_mode {
                    "0".to_string()
                } else {
                    format!("{}->cycle", self.state_ref())
                }
            }
            ReadExpr::Instret => {
                if self.config.perf_mode {
                    "0".to_string()
                } else {
                    format!("{}->instret", self.state_ref())
                }
            }
            ReadExpr::Temp(idx) => format!("_t{idx}"),
            ReadExpr::TraceIdx => format!("{}->trace_idx", self.state_ref()),
            ReadExpr::PcIdx => format!("{}->pc_idx", self.state_ref()),
            ReadExpr::ResAddr => format!("{}->reservation_addr", self.state_ref()),
            ReadExpr::ResValid => format!("{}->reservation_valid", self.state_ref()),
            ReadExpr::Exited => format!("{}->exited", self.state_ref()),
            ReadExpr::ExitCode => format!("{}->exit_code", self.state_ref()),
            _ => "0".to_string(),
        }
    }
    // ============= Statement rendering =============

    /// Render statement.
    pub(crate) fn render_stmt(&mut self, stmt: &Stmt<X>, indent: usize) {
        match stmt {
            Stmt::Write { target, value } => self.render_write_stmt(target, value, indent),
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => self.render_if_stmt(cond, then_stmts, else_stmts, indent),
            Stmt::ExternCall { fn_name, args } => self.render_extern_stmt(fn_name, args, indent),
        }
    }

    fn render_write_stmt(&mut self, target: &WriteTarget<X>, value: &Expr<X>, indent: usize) {
        let value_str = self.render_expr(value);
        let state = self.state_ref();
        match target {
            WriteTarget::Reg(reg) => self.render_write_reg(*reg, &value_str, state, indent),
            WriteTarget::Mem {
                base,
                offset,
                width,
            } => self.render_write_mem(base, *offset, *width, &value_str, state, indent),
            WriteTarget::Csr(csr) => self.render_write_csr(*csr, &value_str, state, indent),
            WriteTarget::Pc => self.writeln(indent, &format!("{state}->pc = {value_str};")),
            WriteTarget::Temp(idx) => self.writeln(
                indent,
                &format!("{} _t{} = {};", self.reg_type, idx, value_str),
            ),
            WriteTarget::ResAddr => {
                self.writeln(indent, &format!("{state}->reservation_addr = {value_str};"));
            }
            WriteTarget::ResValid => {
                self.writeln(
                    indent,
                    &format!("{state}->reservation_valid = {value_str};"),
                );
            }
            WriteTarget::Exited => {
                self.writeln(indent, &format!("{state}->has_exited = {value_str};"));
            }
            WriteTarget::ExitCode => {
                self.writeln(indent, &format!("{state}->exit_code = {value_str};"));
            }
        }
    }

    fn render_write_reg(&mut self, reg: u8, value_str: &str, state: &str, indent: usize) {
        if reg == 0 {
            return;
        }
        if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            if self.sig.is_hot_reg(reg) {
                let name = self.sig.reg_read(reg);
                self.writeln(
                    indent,
                    &format!(
                        "{name} = twr_regval(&{state}->tracer, {pc_lit}, {op_lit}, {reg}, {value_str});"
                    ),
                );
            } else {
                self.writeln(
                    indent,
                    &format!(
                        "twr_reg(&{state}->tracer, {pc_lit}, {op_lit}, {state}, {reg}, {value_str});"
                    ),
                );
            }
        } else {
            let code = self.sig.reg_write(reg, value_str);
            if !code.is_empty() {
                self.writeln(indent, &code);
            }
        }
    }

    fn render_write_mem(
        &mut self,
        base: &Expr<X>,
        offset: i16,
        width: u8,
        value_str: &str,
        state: &str,
        indent: usize,
    ) {
        let base_str = self.render_expr(base);

        if self.config.htif_enabled() && (width == 4 || width == 8) {
            self.render_mem_write_tohost(&base_str, offset, value_str, width, indent);
        } else if self.config.has_tracing() {
            let store_fn = Self::mem_trace_write_fn(width);
            let pc_lit = Self::fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            self.writeln(
                indent,
                &format!(
                    "{store_fn}(&{state}->tracer, {pc_lit}, {op_lit}, memory, {base_str}, {offset}, {value_str});"
                ),
            );
        } else if self.uses_fixed_addresses() {
            let store_fn = Self::mem_write_fn(width);
            self.writeln(
                indent,
                &format!("{store_fn}({base_str}, {offset}, {value_str});"),
            );
        } else {
            let store_fn = Self::mem_write_fn(width);
            self.writeln(
                indent,
                &format!("{store_fn}(memory, {base_str}, {offset}, {value_str});"),
            );
        }
    }

    fn render_write_csr(&mut self, csr: u16, value_str: &str, state: &str, indent: usize) {
        let state_arg = if self.uses_fixed_addresses() {
            String::new()
        } else {
            format!("{state}, ")
        };
        if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            self.writeln(
                indent,
                &format!(
                    "twr_csr(&{state}->tracer, {pc_lit}, {op_lit}, {state_arg}0x{csr:x}, {value_str});"
                ),
            );
        } else {
            self.writeln(
                indent,
                &format!("wr_csr({state_arg}0x{csr:x}, {value_str});"),
            );
        }
    }

    const fn mem_trace_write_fn(width: u8) -> &'static str {
        match width {
            1 => "twr_mem_u8",
            2 => "twr_mem_u16",
            8 => "twr_mem_u64",
            _ => "twr_mem_u32",
        }
    }

    const fn mem_write_fn(width: u8) -> &'static str {
        match width {
            1 => "wr_mem_u8",
            2 => "wr_mem_u16",
            8 => "wr_mem_u64",
            _ => "wr_mem_u32",
        }
    }

    fn render_if_stmt(
        &mut self,
        cond: &Expr<X>,
        then_stmts: &[Stmt<X>],
        else_stmts: &[Stmt<X>],
        indent: usize,
    ) {
        let cond_str = self.render_expr(cond);
        self.writeln(indent, &format!("if ({cond_str}) {{"));
        for s in then_stmts {
            self.render_stmt(s, indent + 1);
        }
        if !else_stmts.is_empty() {
            self.writeln(indent, "} else {");
            for s in else_stmts {
                self.render_stmt(s, indent + 1);
            }
        }
        self.writeln(indent, "}");
    }

    fn render_extern_stmt(&mut self, fn_name: &str, args: &[Expr<X>], indent: usize) {
        let args_str: Vec<String> = args.iter().map(|a| self.render_expr(a)).collect();
        self.writeln(indent, &format!("{fn_name}({});", args_str.join(", ")));
    }

    /// Render memory write with tohost check.
    ///
    /// When writing to TOHOST address, calls `handle_tohost_write` and checks for exit.
    fn render_mem_write_tohost(
        &mut self,
        base: &str,
        offset: i16,
        value: &str,
        width: u8,
        indent: usize,
    ) {
        let pc_lit = Self::fmt_addr(self.current_pc);
        let save_to_state = self.sig.save_to_state.clone();

        // Build instret update if needed
        let instret_update = if self.config.instret_mode.counts() {
            format!("instret += {};", self.instr_idx + 1)
        } else {
            String::new()
        };

        // Build save_to_state call if needed
        let save_call = if save_to_state.is_empty() {
            String::new()
        } else {
            format!(" {save_to_state}")
        };

        let state = self.state_ref();

        // Generate the tohost check
        self.writeln(
            indent,
            &format!("if (unlikely((uint32_t){base} + {offset} == 0x{TOHOST_ADDR:x}u)) {{"),
        );
        self.writeln(
            indent + 1,
            &format!("handle_tohost_write({state}, {value});"),
        );
        self.writeln(
            indent + 1,
            &format!("if (unlikely({state}->has_exited)) {{"),
        );
        self.writeln(indent + 2, &format!("{state}->pc = {pc_lit};"));
        if !instret_update.is_empty() || !save_call.is_empty() {
            self.writeln(indent + 2, &format!("{instret_update}{save_call}"));
        }
        self.writeln(indent + 2, "return;");
        self.writeln(indent + 1, "}");
        self.writeln(indent, "} else {");

        // Select correct store function based on width
        let (store_fn, tstore_fn) = match width {
            8 => ("wr_mem_u64", "twr_mem_u64"),
            _ => ("wr_mem_u32", "twr_mem_u32"),
        };

        if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            self.writeln(
                indent + 1,
                &format!(
                    "{tstore_fn}(&{state}->tracer, {pc_lit}, {op_lit}, memory, {base}, {offset}, {value});"
                ),
            );
        } else if self.uses_fixed_addresses() {
            self.writeln(
                indent + 1,
                &format!("{store_fn}({base}, {offset}, {value});"),
            );
        } else {
            self.writeln(
                indent + 1,
                &format!("{store_fn}(memory, {base}, {offset}, {value});"),
            );
        }
        self.writeln(indent, "}");
    }
}
