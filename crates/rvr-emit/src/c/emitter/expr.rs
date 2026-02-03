//! Expression and statement rendering for the C emitter.

use rvr_ir::{BinaryOp, Expr, ReadExpr, Stmt, TernaryOp, UnaryOp, WriteTarget, Xlen};

use crate::htif::TOHOST_ADDR;

use super::CEmitter;

impl<X: Xlen> CEmitter<X> {
    pub(crate) fn render_expr(&self, expr: &Expr<X>) -> String {
        match expr {
            Expr::Imm(val) => {
                let val = X::to_u64(*val);
                if X::VALUE == 64 {
                    format!("0x{:x}ULL", val)
                } else {
                    format!("0x{:x}u", val)
                }
            }
            Expr::Read(read) => self.render_read(read),
            Expr::PcConst(pc) => self.fmt_addr(X::to_u64(*pc)),
            Expr::Var(name) => {
                // Handle "state" specially in fixed address mode
                if name == "state" && self.sig.fixed_addresses {
                    self.state_ref().to_string()
                } else {
                    name.clone()
                }
            }
            Expr::Unary { op, expr } => {
                let o = self.render_expr(expr);
                match op {
                    UnaryOp::Not => format!("(~{})", o),
                    UnaryOp::Neg => format!("(-{})", o),
                    UnaryOp::Sext8 => format!("(({})(int8_t){})", self.reg_type, o),
                    UnaryOp::Sext16 => format!("(({})(int16_t){})", self.reg_type, o),
                    UnaryOp::Sext32 => {
                        if X::VALUE == 64 {
                            format!("((uint64_t)(int64_t)(int32_t){})", o)
                        } else {
                            o
                        }
                    }
                    UnaryOp::Zext8 => format!("(({})(uint8_t){})", self.reg_type, o),
                    UnaryOp::Zext16 => format!("(({})(uint16_t){})", self.reg_type, o),
                    UnaryOp::Zext32 => {
                        if X::VALUE == 64 {
                            format!("((uint64_t)(uint32_t){})", o)
                        } else {
                            o
                        }
                    }
                    UnaryOp::Clz => {
                        if X::VALUE == 64 {
                            format!("({} ? __builtin_clzll({}) : 64)", o, o)
                        } else {
                            format!("({} ? __builtin_clz({}) : 32)", o, o)
                        }
                    }
                    UnaryOp::Ctz => {
                        if X::VALUE == 64 {
                            format!("({} ? __builtin_ctzll({}) : 64)", o, o)
                        } else {
                            format!("({} ? __builtin_ctz({}) : 32)", o, o)
                        }
                    }
                    UnaryOp::Cpop => {
                        if X::VALUE == 64 {
                            format!("__builtin_popcountll({})", o)
                        } else {
                            format!("__builtin_popcount({})", o)
                        }
                    }
                    UnaryOp::Clz32 => format!(
                        "((uint64_t)((uint32_t){} ? __builtin_clz((uint32_t){}) : 32))",
                        o, o
                    ),
                    UnaryOp::Ctz32 => format!(
                        "((uint64_t)((uint32_t){} ? __builtin_ctz((uint32_t){}) : 32))",
                        o, o
                    ),
                    UnaryOp::Cpop32 => format!("((uint64_t)__builtin_popcount((uint32_t){}))", o),
                    UnaryOp::Orc8 => {
                        if X::VALUE == 64 {
                            format!("rv_orc_b64({})", o)
                        } else {
                            format!("rv_orc_b32({})", o)
                        }
                    }
                    UnaryOp::Rev8 => {
                        if X::VALUE == 64 {
                            format!("__builtin_bswap64({})", o)
                        } else {
                            format!("__builtin_bswap32({})", o)
                        }
                    }
                    UnaryOp::Brev8 => {
                        if X::VALUE == 64 {
                            format!("rv_brev8_64({})", o)
                        } else {
                            format!("rv_brev8_32({})", o)
                        }
                    }
                    UnaryOp::Zip => format!("rv_zip32({})", o),
                    UnaryOp::Unzip => format!("rv_unzip32({})", o),
                }
            }
            Expr::Binary { op, left, right } => {
                let l = self.render_expr(left);
                let r = self.render_expr(right);
                match op {
                    BinaryOp::Add => format!("({} + {})", l, r),
                    BinaryOp::Sub => format!("({} - {})", l, r),
                    BinaryOp::Mul => format!("({} * {})", l, r),
                    BinaryOp::And => format!("({} & {})", l, r),
                    BinaryOp::Or => format!("({} | {})", l, r),
                    BinaryOp::Xor => format!("({} ^ {})", l, r),
                    // IR is responsible for masking shift amount per RISC-V spec
                    BinaryOp::Sll => format!("({} << {})", l, r),
                    BinaryOp::Srl => format!("({} >> {})", l, r),
                    BinaryOp::Sra => format!(
                        "(({})(({}){}  >> {}))",
                        self.reg_type, self.signed_type, l, r
                    ),
                    BinaryOp::Eq => format!("{} == {}", l, r),
                    BinaryOp::Ne => format!("{} != {}", l, r),
                    BinaryOp::Lt => {
                        format!("({}){} < ({}){}", self.signed_type, l, self.signed_type, r)
                    }
                    BinaryOp::Ge => {
                        format!("({}){} >= ({}){}", self.signed_type, l, self.signed_type, r)
                    }
                    BinaryOp::Ltu => format!("{} < {}", l, r),
                    BinaryOp::Geu => format!("{} >= {}", l, r),
                    BinaryOp::Div => {
                        if X::VALUE == 64 {
                            format!("rv_div64({}, {})", l, r)
                        } else {
                            format!("rv_div({}, {})", l, r)
                        }
                    }
                    BinaryOp::DivU => {
                        if X::VALUE == 64 {
                            format!("rv_divu64({}, {})", l, r)
                        } else {
                            format!("rv_divu({}, {})", l, r)
                        }
                    }
                    BinaryOp::Rem => {
                        if X::VALUE == 64 {
                            format!("rv_rem64({}, {})", l, r)
                        } else {
                            format!("rv_rem({}, {})", l, r)
                        }
                    }
                    BinaryOp::RemU => {
                        if X::VALUE == 64 {
                            format!("rv_remu64({}, {})", l, r)
                        } else {
                            format!("rv_remu({}, {})", l, r)
                        }
                    }
                    BinaryOp::AddW => format!(
                        "((uint64_t)(int64_t)(int32_t)((uint32_t){} + (uint32_t){}))",
                        l, r
                    ),
                    BinaryOp::SubW => format!(
                        "((uint64_t)(int64_t)(int32_t)((uint32_t){} - (uint32_t){}))",
                        l, r
                    ),
                    BinaryOp::MulW => format!(
                        "((uint64_t)(int64_t)(int32_t)((uint32_t){} * (uint32_t){}))",
                        l, r
                    ),
                    BinaryOp::DivW => format!("rv_divw((int32_t){}, (int32_t){})", l, r),
                    BinaryOp::DivUW => format!("rv_divuw((uint32_t){}, (uint32_t){})", l, r),
                    BinaryOp::RemW => format!("rv_remw((int32_t){}, (int32_t){})", l, r),
                    BinaryOp::RemUW => format!("rv_remuw((uint32_t){}, (uint32_t){})", l, r),
                    // IR is responsible for masking shift amount per RISC-V spec
                    BinaryOp::SllW => {
                        format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} << {}))", l, r)
                    }
                    BinaryOp::SrlW => {
                        format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} >> {}))", l, r)
                    }
                    BinaryOp::SraW => format!("((uint64_t)(int64_t)((int32_t){} >> {}))", l, r),
                    BinaryOp::MulH => {
                        if X::VALUE == 64 {
                            format!("rv_mulh64({}, {})", l, r)
                        } else {
                            format!("rv_mulh({}, {})", l, r)
                        }
                    }
                    BinaryOp::MulHSU => {
                        if X::VALUE == 64 {
                            format!("rv_mulhsu64({}, {})", l, r)
                        } else {
                            format!("rv_mulhsu({}, {})", l, r)
                        }
                    }
                    BinaryOp::MulHU => {
                        if X::VALUE == 64 {
                            format!("rv_mulhu64({}, {})", l, r)
                        } else {
                            format!("rv_mulhu({}, {})", l, r)
                        }
                    }
                    BinaryOp::Pack => {
                        if X::VALUE == 64 {
                            format!(
                                "(((uint64_t)(uint32_t){}) | ((uint64_t)(uint32_t){} << 32))",
                                l, r
                            )
                        } else {
                            format!(
                                "(((uint32_t)(uint16_t){}) | ((uint32_t)(uint16_t){} << 16))",
                                l, r
                            )
                        }
                    }
                    BinaryOp::Pack8 => format!(
                        "((({})(uint8_t){}) | (({})(uint8_t){} << 8))",
                        self.reg_type, l, self.reg_type, r
                    ),
                    BinaryOp::Pack16 => format!(
                        "((int64_t)(int32_t)(((uint32_t)(uint16_t){}) | ((uint32_t)(uint16_t){} << 16)))",
                        l, r
                    ),
                }
            }
            Expr::Ternary {
                op,
                first,
                second,
                third,
            } => match op {
                TernaryOp::Select => {
                    let c = self.render_expr(first);
                    let t = self.render_expr(second);
                    let e = self.render_expr(third);
                    format!("({} ? {} : {})", c, t, e)
                }
            },
            Expr::ExternCall { name, args, .. } => {
                let args: Vec<String> = args.iter().map(|a| self.render_expr(a)).collect();
                format!("{}({})", name, args.join(", "))
            }
        }
    }

    /// Render read expression.
    fn render_read(&self, expr: &ReadExpr<X>) -> String {
        match expr {
            ReadExpr::Reg(reg) => {
                if *reg == 0 {
                    // Use explicit ULL suffix for RV64 consistency
                    if X::VALUE == 64 {
                        "0x0ULL".to_string()
                    } else {
                        "0".to_string()
                    }
                } else if self.config.has_tracing() {
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    let state = self.state_ref();
                    if self.sig.is_hot_reg(*reg) {
                        let val = self.sig.reg_read(*reg);
                        format!(
                            "trd_regval(&{}->tracer, {}, {}, {}, {})",
                            state, pc_lit, op_lit, reg, val
                        )
                    } else {
                        format!(
                            "trd_reg(&{}->tracer, {}, {}, {}, {})",
                            state, pc_lit, op_lit, state, reg
                        )
                    }
                } else {
                    self.sig.reg_read(*reg)
                }
            }
            ReadExpr::Mem {
                base,
                offset,
                width,
                signed,
            } => {
                let base = self.render_expr(base);
                if self.config.has_tracing() {
                    let load_fn = match (*width, *signed) {
                        (1, true) => "trd_mem_i8",
                        (1, false) => "trd_mem_u8",
                        (2, true) => "trd_mem_i16",
                        (2, false) => "trd_mem_u16",
                        (4, true) if X::VALUE == 64 => "trd_mem_i32",
                        (4, _) => "trd_mem_u32",
                        (8, _) => "trd_mem_u64",
                        _ => "trd_mem_u32",
                    };
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    let state = self.state_ref();
                    format!(
                        "{}(&{}->tracer, {}, {}, memory, {}, {})",
                        load_fn, state, pc_lit, op_lit, base, offset
                    )
                } else if self.uses_fixed_addresses() {
                    // Fixed addresses: memory helpers don't take memory parameter
                    let load_fn = match (*width, *signed) {
                        (1, true) => "rd_mem_i8",
                        (1, false) => "rd_mem_u8",
                        (2, true) => "rd_mem_i16",
                        (2, false) => "rd_mem_u16",
                        (4, true) if X::VALUE == 64 => "rd_mem_i32",
                        (4, _) => "rd_mem_u32",
                        (8, _) => "rd_mem_u64",
                        _ => "rd_mem_u32",
                    };
                    format!("{}({}, {})", load_fn, base, offset)
                } else {
                    // Normal mode: pass memory parameter
                    let load_fn = match (*width, *signed) {
                        (1, true) => "rd_mem_i8",
                        (1, false) => "rd_mem_u8",
                        (2, true) => "rd_mem_i16",
                        (2, false) => "rd_mem_u16",
                        (4, true) if X::VALUE == 64 => "rd_mem_i32",
                        (4, _) => "rd_mem_u32",
                        (8, _) => "rd_mem_u64",
                        _ => "rd_mem_u32",
                    };
                    format!("{}(memory, {}, {})", load_fn, base, offset)
                }
            }
            ReadExpr::MemAddr {
                addr,
                width,
                signed,
            } => {
                let base = self.render_expr(addr);
                if self.config.has_tracing() {
                    let load_fn = match (*width, *signed) {
                        (1, true) => "trd_mem_i8",
                        (1, false) => "trd_mem_u8",
                        (2, true) => "trd_mem_i16",
                        (2, false) => "trd_mem_u16",
                        (4, true) if X::VALUE == 64 => "trd_mem_i32",
                        (4, _) => "trd_mem_u32",
                        (8, _) => "trd_mem_u64",
                        _ => "trd_mem_u32",
                    };
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    let state = self.state_ref();
                    format!(
                        "{}(&{}->tracer, {}, {}, memory, {}, 0)",
                        load_fn, state, pc_lit, op_lit, base
                    )
                } else if self.uses_fixed_addresses() {
                    // Fixed addresses: memory helpers don't take memory parameter
                    let load_fn = match (*width, *signed) {
                        (1, true) => "rd_mem_i8",
                        (1, false) => "rd_mem_u8",
                        (2, true) => "rd_mem_i16",
                        (2, false) => "rd_mem_u16",
                        (4, true) if X::VALUE == 64 => "rd_mem_i32",
                        (4, _) => "rd_mem_u32",
                        (8, _) => "rd_mem_u64",
                        _ => "rd_mem_u32",
                    };
                    format!("{}({}, 0)", load_fn, base)
                } else {
                    // Normal mode: pass memory parameter
                    let load_fn = match (*width, *signed) {
                        (1, true) => "rd_mem_i8",
                        (1, false) => "rd_mem_u8",
                        (2, true) => "rd_mem_i16",
                        (2, false) => "rd_mem_u16",
                        (4, true) if X::VALUE == 64 => "rd_mem_i32",
                        (4, _) => "rd_mem_u32",
                        (8, _) => "rd_mem_u64",
                        _ => "rd_mem_u32",
                    };
                    format!("{}(memory, {}, 0)", load_fn, base)
                }
            }
            ReadExpr::Csr(csr) => {
                if self.config.perf_mode {
                    return "0".to_string();
                }
                let state = self.state_ref();
                let state_arg = if self.uses_fixed_addresses() {
                    ""
                } else {
                    &format!("{}, ", state)
                };
                if self.config.has_tracing() {
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    if self.config.instret_mode.counts() {
                        format!(
                            "trd_csr(&{}->tracer, {}, {}, {}0x{:x}, instret)",
                            state, pc_lit, op_lit, state_arg, csr
                        )
                    } else {
                        format!(
                            "trd_csr(&{}->tracer, {}, {}, {}0x{:x})",
                            state, pc_lit, op_lit, state_arg, csr
                        )
                    }
                } else if self.config.instret_mode.counts() {
                    format!("rd_csr({}0x{:x}, instret)", state_arg, csr)
                } else {
                    format!("rd_csr({}0x{:x})", state_arg, csr)
                }
            }
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
            ReadExpr::Temp(idx) => format!("_t{}", idx),
            ReadExpr::TraceIdx => format!("{}->trace_idx", self.state_ref()),
            ReadExpr::PcIdx => format!("{}->pc_idx", self.state_ref()),
            ReadExpr::ResAddr => format!("{}->reservation_addr", self.state_ref()),
            ReadExpr::ResValid => format!("{}->reservation_valid", self.state_ref()),
            ReadExpr::Exited => format!("{}->exited", self.state_ref()),
            ReadExpr::ExitCode => format!("{}->exit_code", self.state_ref()),
        }
    }
    // ============= Statement rendering =============

    /// Render statement.
    pub(crate) fn render_stmt(&mut self, stmt: &Stmt<X>, indent: usize) {
        match stmt {
            Stmt::Write { target, value } => {
                let value_str = self.render_expr(value);
                let state = self.state_ref();
                match target {
                    WriteTarget::Reg(reg) => {
                        if *reg == 0 {
                            return;
                        }
                        if self.config.has_tracing() {
                            let pc_lit = self.fmt_addr(self.current_pc);
                            let op_lit = self.current_op;
                            if self.sig.is_hot_reg(*reg) {
                                let name = self.sig.reg_read(*reg);
                                self.writeln(
                                    indent,
                                    &format!(
                                        "{} = twr_regval(&{}->tracer, {}, {}, {}, {});",
                                        name, state, pc_lit, op_lit, reg, value_str
                                    ),
                                );
                            } else {
                                self.writeln(
                                    indent,
                                    &format!(
                                        "twr_reg(&{}->tracer, {}, {}, {}, {}, {});",
                                        state, pc_lit, op_lit, state, reg, value_str
                                    ),
                                );
                            }
                        } else {
                            let code = self.sig.reg_write(*reg, &value_str);
                            if !code.is_empty() {
                                self.writeln(indent, &code);
                            }
                        }
                    }
                    WriteTarget::Mem {
                        base,
                        offset,
                        width,
                    } => {
                        let base_str = self.render_expr(base);

                        // Check for tohost handling on 32/64-bit stores
                        if self.config.htif_enabled && (*width == 4 || *width == 8) {
                            self.render_mem_write_tohost(
                                &base_str, *offset, &value_str, *width, indent,
                            );
                        } else if self.config.has_tracing() {
                            let store_fn = match width {
                                1 => "twr_mem_u8",
                                2 => "twr_mem_u16",
                                4 => "twr_mem_u32",
                                8 => "twr_mem_u64",
                                _ => "twr_mem_u32",
                            };
                            let pc_lit = self.fmt_addr(self.current_pc);
                            let op_lit = self.current_op;
                            self.writeln(
                                indent,
                                &format!(
                                    "{}(&{}->tracer, {}, {}, memory, {}, {}, {});",
                                    store_fn, state, pc_lit, op_lit, base_str, offset, value_str
                                ),
                            );
                        } else if self.uses_fixed_addresses() {
                            // Fixed addresses: memory helpers don't take memory parameter
                            let store_fn = match width {
                                1 => "wr_mem_u8",
                                2 => "wr_mem_u16",
                                4 => "wr_mem_u32",
                                8 => "wr_mem_u64",
                                _ => "wr_mem_u32",
                            };
                            self.writeln(
                                indent,
                                &format!("{}({}, {}, {});", store_fn, base_str, offset, value_str),
                            );
                        } else {
                            // Normal mode: pass memory parameter
                            let store_fn = match width {
                                1 => "wr_mem_u8",
                                2 => "wr_mem_u16",
                                4 => "wr_mem_u32",
                                8 => "wr_mem_u64",
                                _ => "wr_mem_u32",
                            };
                            self.writeln(
                                indent,
                                &format!(
                                    "{}(memory, {}, {}, {});",
                                    store_fn, base_str, offset, value_str
                                ),
                            );
                        }
                    }
                    WriteTarget::Csr(csr) => {
                        let state_arg = if self.uses_fixed_addresses() {
                            "".to_string()
                        } else {
                            format!("{}, ", state)
                        };
                        if self.config.has_tracing() {
                            let pc_lit = self.fmt_addr(self.current_pc);
                            let op_lit = self.current_op;
                            self.writeln(
                                indent,
                                &format!(
                                    "twr_csr(&{}->tracer, {}, {}, {}0x{:x}, {});",
                                    state, pc_lit, op_lit, state_arg, csr, value_str
                                ),
                            );
                        } else {
                            self.writeln(
                                indent,
                                &format!("wr_csr({}0x{:x}, {});", state_arg, csr, value_str),
                            );
                        }
                    }
                    WriteTarget::Pc => {
                        self.writeln(indent, &format!("{}->pc = {};", state, value_str));
                    }
                    WriteTarget::Temp(idx) => {
                        self.writeln(
                            indent,
                            &format!("{} _t{} = {};", self.reg_type, idx, value_str),
                        );
                    }
                    WriteTarget::ResAddr => {
                        self.writeln(
                            indent,
                            &format!("{}->reservation_addr = {};", state, value_str),
                        );
                    }
                    WriteTarget::ResValid => {
                        self.writeln(
                            indent,
                            &format!("{}->reservation_valid = {};", state, value_str),
                        );
                    }
                    WriteTarget::Exited => {
                        self.writeln(indent, &format!("{}->has_exited = {};", state, value_str));
                    }
                    WriteTarget::ExitCode => {
                        self.writeln(indent, &format!("{}->exit_code = {};", state, value_str));
                    }
                }
            }
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => {
                let cond_str = self.render_expr(cond);
                self.writeln(indent, &format!("if ({}) {{", cond_str));
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
            Stmt::ExternCall { fn_name, args } => {
                let args_str: Vec<String> = args.iter().map(|a| self.render_expr(a)).collect();
                self.writeln(indent, &format!("{}({});", fn_name, args_str.join(", ")));
            }
        }
    }

    /// Render memory write with tohost check.
    ///
    /// When writing to TOHOST address, calls handle_tohost_write and checks for exit.
    fn render_mem_write_tohost(
        &mut self,
        base: &str,
        offset: i16,
        value: &str,
        width: u8,
        indent: usize,
    ) {
        let pc_lit = self.fmt_addr(self.current_pc);
        let save_to_state = self.sig.save_to_state.clone();

        // Build instret update if needed
        let instret_update = if self.config.instret_mode.counts() {
            format!("instret += {};", self.instr_idx + 1)
        } else {
            String::new()
        };

        // Build save_to_state call if needed
        let save_call = if !save_to_state.is_empty() {
            format!(" {}", save_to_state)
        } else {
            String::new()
        };

        let state = self.state_ref();

        // Generate the tohost check
        self.writeln(
            indent,
            &format!(
                "if (unlikely((uint32_t){} + {} == 0x{:x}u)) {{",
                base, offset, TOHOST_ADDR
            ),
        );
        self.writeln(
            indent + 1,
            &format!("handle_tohost_write({}, {});", state, value),
        );
        self.writeln(
            indent + 1,
            &format!("if (unlikely({}->has_exited)) {{", state),
        );
        self.writeln(indent + 2, &format!("{}->pc = {};", state, pc_lit));
        if !instret_update.is_empty() || !save_call.is_empty() {
            self.writeln(indent + 2, &format!("{}{}", instret_update, save_call));
        }
        self.writeln(indent + 2, "return;");
        self.writeln(indent + 1, "}");
        self.writeln(indent, "} else {");

        // Select correct store function based on width
        let (store_fn, tstore_fn) = match width {
            4 => ("wr_mem_u32", "twr_mem_u32"),
            8 => ("wr_mem_u64", "twr_mem_u64"),
            _ => ("wr_mem_u32", "twr_mem_u32"),
        };

        if self.config.has_tracing() {
            let pc_lit = self.fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            self.writeln(
                indent + 1,
                &format!(
                    "{}(&{}->tracer, {}, {}, memory, {}, {}, {});",
                    tstore_fn, state, pc_lit, op_lit, base, offset, value
                ),
            );
        } else if self.uses_fixed_addresses() {
            self.writeln(
                indent + 1,
                &format!("{}({}, {}, {});", store_fn, base, offset, value),
            );
        } else {
            self.writeln(
                indent + 1,
                &format!("{}(memory, {}, {}, {});", store_fn, base, offset, value),
            );
        }
        self.writeln(indent, "}");
    }
}
