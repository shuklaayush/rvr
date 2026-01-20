//! C code emitter for RISC-V recompiler.
//!
//! Generates C code from RISC-V IR blocks with:
//! - Explicit musttail calls for all control flow (including fall-through)
//! - Branch emits both taken and not-taken paths
//! - save_to_state on all exit paths
//! - Optional tracing hooks (trace_block, trace_pc, trace_branch_*)
//! - Optional tohost handling for riscv-tests

use rvr_ir::{BlockIR, BranchHint, Expr, ExprKind, InstrIR, Space, Stmt, Terminator, Xlen};

/// HTIF tohost address (matches riscv-tests expectation).
const TOHOST_ADDR: u64 = 0x80001000;

use crate::config::EmitConfig;
use crate::signature::FnSignature;

/// C code emitter.
pub struct CEmitter<X: Xlen> {
    pub config: EmitConfig<X>,
    /// Function signature for block functions.
    pub sig: FnSignature,
    /// Output buffer.
    pub out: String,
    /// Register type name ("uint32_t" or "uint64_t").
    reg_type: &'static str,
    /// Signed register type ("int32_t" or "int64_t").
    signed_type: &'static str,
    /// Current instruction PC.
    current_pc: u64,
    /// Instruction index within block (for instret).
    instr_idx: usize,
}

impl<X: Xlen> CEmitter<X> {
    /// Create a new emitter.
    pub fn new(config: EmitConfig<X>) -> Self {
        let (reg_type, signed_type) = if X::VALUE == 64 {
            ("uint64_t", "int64_t")
        } else {
            ("uint32_t", "int32_t")
        };
        let sig = FnSignature::new(&config);

        Self {
            config,
            sig,
            out: String::with_capacity(4096),
            reg_type,
            signed_type,
            current_pc: 0,
            instr_idx: 0,
        }
    }

    /// Reset output buffer.
    pub fn reset(&mut self) {
        self.out.clear();
        self.current_pc = 0;
        self.instr_idx = 0;
    }

    /// Get output string.
    pub fn output(&self) -> &str {
        &self.out
    }

    /// Take output string, consuming the emitter.
    pub fn take_output(self) -> String {
        self.out
    }

    /// Check if address is valid.
    fn is_valid_address(&self, addr: u64) -> bool {
        self.config.valid_addresses.contains(&addr)
    }

    /// Format address as hex.
    fn fmt_addr(&self, addr: u64) -> String {
        if X::VALUE == 64 {
            format!("0x{:016x}ULL", addr)
        } else {
            format!("0x{:08x}u", addr)
        }
    }

    /// Write indented line.
    fn writeln(&mut self, indent: usize, s: &str) {
        for _ in 0..indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    /// Write without indent.
    fn write(&mut self, s: &str) {
        self.out.push_str(s);
    }

    // ============= Expression rendering =============

    /// Render expression to C code.
    pub fn render_expr(&self, expr: &Expr<X>) -> String {
        match expr.kind {
            ExprKind::Imm => {
                let val = X::to_u64(expr.imm);
                if X::VALUE == 64 {
                    format!("0x{:x}ULL", val)
                } else {
                    format!("0x{:x}u", val)
                }
            }
            ExprKind::Read => self.render_read(expr),
            ExprKind::PcConst => self.fmt_addr(X::to_u64(expr.imm)),
            ExprKind::Var => {
                expr.var_name.clone().unwrap_or_else(|| "/*unknown*/".to_string())
            }
            ExprKind::Add => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} + {})", l, r)
            }
            ExprKind::Sub => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} - {})", l, r)
            }
            ExprKind::Mul => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} * {})", l, r)
            }
            ExprKind::And => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} & {})", l, r)
            }
            ExprKind::Or => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} | {})", l, r)
            }
            ExprKind::Xor => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} ^ {})", l, r)
            }
            ExprKind::Sll => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} << {})", l, r)
            }
            ExprKind::Srl => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({} >> {})", l, r)
            }
            ExprKind::Sra => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("(({})(({}){}  >> {}))", self.reg_type, self.signed_type, l, r)
            }
            ExprKind::Not => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("(~{})", o)
            }
            ExprKind::Eq => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("{} == {}", l, r)
            }
            ExprKind::Ne => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("{} != {}", l, r)
            }
            ExprKind::Lt => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({}){} < ({}){}", self.signed_type, l, self.signed_type, r)
            }
            ExprKind::Ge => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("({}){} >= ({}){}", self.signed_type, l, self.signed_type, r)
            }
            ExprKind::Ltu => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("{} < {}", l, r)
            }
            ExprKind::Geu => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("{} >= {}", l, r)
            }
            ExprKind::Div => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_div64({}, {})", l, r)
                } else {
                    format!("rv_div({}, {})", l, r)
                }
            }
            ExprKind::DivU => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_divu64({}, {})", l, r)
                } else {
                    format!("rv_divu({}, {})", l, r)
                }
            }
            ExprKind::Rem => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_rem64({}, {})", l, r)
                } else {
                    format!("rv_rem({}, {})", l, r)
                }
            }
            ExprKind::RemU => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_remu64({}, {})", l, r)
                } else {
                    format!("rv_remu({}, {})", l, r)
                }
            }
            // RV64 32-bit operations
            ExprKind::AddW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} + (uint32_t){}))", l, r)
            }
            ExprKind::SubW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} - (uint32_t){}))", l, r)
            }
            ExprKind::MulW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} * (uint32_t){}))", l, r)
            }
            ExprKind::DivW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("rv_divw((int32_t){}, (int32_t){})", l, r)
            }
            ExprKind::DivUW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("rv_divuw((uint32_t){}, (uint32_t){})", l, r)
            }
            ExprKind::RemW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("rv_remw((int32_t){}, (int32_t){})", l, r)
            }
            ExprKind::RemUW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("rv_remuw((uint32_t){}, (uint32_t){})", l, r)
            }
            ExprKind::SllW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} << ({} & 0x1f)))", l, r)
            }
            ExprKind::SrlW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} >> ({} & 0x1f)))", l, r)
            }
            ExprKind::SraW => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((uint64_t)(int64_t)((int32_t){} >> ({} & 0x1f)))", l, r)
            }
            // Sign/zero extension
            ExprKind::Sext8 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("(({})(int8_t){})", self.reg_type, o)
            }
            ExprKind::Sext16 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("(({})(int16_t){})", self.reg_type, o)
            }
            ExprKind::Sext32 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("((uint64_t)(int64_t)(int32_t){})", o)
                } else {
                    o
                }
            }
            ExprKind::Zext8 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("(({})(uint8_t){})", self.reg_type, o)
            }
            ExprKind::Zext16 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("(({})(uint16_t){})", self.reg_type, o)
            }
            ExprKind::Zext32 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("((uint64_t)(uint32_t){})", o)
                } else {
                    o
                }
            }
            ExprKind::Select => {
                let c = self.render_expr(expr.left.as_ref().unwrap());
                let t = self.render_expr(expr.right.as_ref().unwrap());
                let e = self.render_expr(expr.third.as_ref().unwrap());
                format!("({} ? {} : {})", c, t, e)
            }
            ExprKind::ExternCall => {
                let fn_name = expr.extern_fn.as_ref().map(|s| s.as_str()).unwrap_or("unknown");
                let args: Vec<String> = expr.extern_args.iter().map(|a| self.render_expr(a)).collect();
                format!("{}({})", fn_name, args.join(", "))
            }
            // M extension high bits
            ExprKind::MulH => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_mulh64({}, {})", l, r)
                } else {
                    format!("rv_mulh({}, {})", l, r)
                }
            }
            ExprKind::MulHSU => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_mulhsu64({}, {})", l, r)
                } else {
                    format!("rv_mulhsu({}, {})", l, r)
                }
            }
            ExprKind::MulHU => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_mulhu64({}, {})", l, r)
                } else {
                    format!("rv_mulhu({}, {})", l, r)
                }
            }
            ExprKind::Neg => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("(-{})", o)
            }
            // Zbb bit manipulation
            ExprKind::Clz => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("__builtin_clzll({} | 1) - ({} == 0 ? 0 : 0)", o, o)
                } else {
                    format!("__builtin_clz({} | 1) - ({} == 0 ? 0 : 0)", o, o)
                }
            }
            ExprKind::Ctz => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("({} ? __builtin_ctzll({}) : 64)", o, o)
                } else {
                    format!("({} ? __builtin_ctz({}) : 32)", o, o)
                }
            }
            ExprKind::Cpop => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("__builtin_popcountll({})", o)
                } else {
                    format!("__builtin_popcount({})", o)
                }
            }
            ExprKind::Clz32 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("((uint64_t)__builtin_clz((uint32_t){} | 1))", o)
            }
            ExprKind::Ctz32 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("((uint64_t)((uint32_t){} ? __builtin_ctz((uint32_t){}) : 32))", o, o)
            }
            ExprKind::Cpop32 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("((uint64_t)__builtin_popcount((uint32_t){}))", o)
            }
            ExprKind::Orc8 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_orc_b64({})", o)
                } else {
                    format!("rv_orc_b32({})", o)
                }
            }
            ExprKind::Rev8 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("__builtin_bswap64({})", o)
                } else {
                    format!("__builtin_bswap32({})", o)
                }
            }
            // Zbkb bit manipulation
            ExprKind::Pack => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("(((uint64_t)(uint32_t){}) | ((uint64_t)(uint32_t){} << 32))", l, r)
                } else {
                    format!("(((uint32_t)(uint16_t){}) | ((uint32_t)(uint16_t){} << 16))", l, r)
                }
            }
            ExprKind::Pack8 => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((({})(uint8_t){}) | (({})(uint8_t){} << 8))", self.reg_type, l, self.reg_type, r)
            }
            ExprKind::Pack16 => {
                let l = self.render_expr(expr.left.as_ref().unwrap());
                let r = self.render_expr(expr.right.as_ref().unwrap());
                format!("((int64_t)(int32_t)(((uint32_t)(uint16_t){}) | ((uint32_t)(uint16_t){} << 16)))", l, r)
            }
            ExprKind::Brev8 => {
                let o = self.render_expr(expr.left.as_ref().unwrap());
                if X::VALUE == 64 {
                    format!("rv_brev8_64({})", o)
                } else {
                    format!("rv_brev8_32({})", o)
                }
            }
            ExprKind::Zip => {
                // ZIP is RV32-only
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("rv_zip32({})", o)
            }
            ExprKind::Unzip => {
                // UNZIP is RV32-only
                let o = self.render_expr(expr.left.as_ref().unwrap());
                format!("rv_unzip32({})", o)
            }
        }
    }

    /// Render read expression.
    fn render_read(&self, expr: &Expr<X>) -> String {
        match expr.space {
            Space::Reg => {
                let reg = X::to_u64(expr.imm) as u8;
                self.sig.reg_read(reg)
            }
            Space::Mem => {
                let base = self.render_expr(expr.left.as_ref().unwrap());
                let offset = expr.mem_offset;
                // Use traced helpers (trd_*) - these are passthroughs when tracing is disabled
                let load_fn = match (expr.width, expr.signed) {
                    (1, true) => "trd_mem_i8",
                    (1, false) => "trd_mem_u8",
                    (2, true) => "trd_mem_i16",
                    (2, false) => "trd_mem_u16",
                    (4, true) if X::VALUE == 64 => "trd_mem_i32",
                    (4, _) => "trd_mem_u32",
                    (8, _) => "trd_mem_u64",
                    _ => "trd_mem_u32",
                };
                format!("{}(memory, {}, {})", load_fn, base, offset)
            }
            Space::Csr => {
                let csr = X::to_u64(expr.imm) as u16;
                // Use traced helper (trd_csr)
                if self.config.instret_mode.counts() {
                    format!("trd_csr(state, instret, 0x{:x})", csr)
                } else {
                    format!("trd_csr(state, 0x{:x})", csr)
                }
            }
            Space::Pc => "state->pc".to_string(),
            Space::Cycle => "state->cycle".to_string(),
            Space::Instret => "state->instret".to_string(),
            Space::Temp => {
                let idx = X::to_u64(expr.imm);
                format!("_t{}", idx)
            }
            Space::TraceIdx => "state->trace_idx".to_string(),
            Space::PcIdx => "state->pc_idx".to_string(),
            Space::ResAddr => "state->res_addr".to_string(),
            Space::ResValid => "state->res_valid".to_string(),
            Space::Exited => "state->exited".to_string(),
            Space::ExitCode => "state->exit_code".to_string(),
        }
    }

    // ============= Statement rendering =============

    /// Render statement.
    pub fn render_stmt(&mut self, stmt: &Stmt<X>, indent: usize) {
        match stmt {
            Stmt::Write { space, addr, value, width } => {
                let value_str = self.render_expr(value);
                match space {
                    Space::Reg => {
                        let reg = X::to_u64(addr.imm) as u8;
                        let code = self.sig.reg_write(reg, &value_str);
                        if !code.is_empty() {
                            self.writeln(indent, &code);
                        }
                    }
                    Space::Mem => {
                        // Extract base and offset from address expression.
                        // Store addresses come from ISA as Expr::add(rs1, imm).
                        let (base, offset) = if addr.kind == ExprKind::Add {
                            // Add(base, offset) - common pattern for stores
                            let base_str = self.render_expr(addr.left.as_ref().unwrap());
                            let offset_val = if let Some(right) = &addr.right {
                                if right.kind == ExprKind::Imm {
                                    X::to_u64(right.imm) as i64 as i16
                                } else {
                                    // Right is not immediate, render full address
                                    0i16
                                }
                            } else {
                                0i16
                            };
                            // If right operand wasn't a simple immediate, we need to include it
                            if offset_val == 0 && addr.right.is_some() && addr.right.as_ref().unwrap().kind != ExprKind::Imm {
                                // Complex right operand - render full expression as base with 0 offset
                                (self.render_expr(addr), 0i16)
                            } else {
                                (base_str, offset_val)
                            }
                        } else if addr.mem_offset != 0 || addr.left.is_some() {
                            // Memory read expression with mem_offset
                            let base_str = self.render_expr(addr.left.as_ref().unwrap());
                            (base_str, addr.mem_offset)
                        } else {
                            // Plain address expression (fallback)
                            (self.render_expr(addr), 0i16)
                        };

                        // Check for tohost handling on 32-bit stores
                        if self.config.tohost_enabled && *width == 4 {
                            self.render_mem_write_tohost(&base, offset, &value_str, indent);
                        } else {
                            // Use traced helpers (twr_*) - passthroughs when tracing is disabled
                            let store_fn = match width {
                                1 => "twr_mem_u8",
                                2 => "twr_mem_u16",
                                4 => "twr_mem_u32",
                                8 => "twr_mem_u64",
                                _ => "twr_mem_u32",
                            };
                            self.writeln(indent, &format!("{}(memory, {}, {}, {});", store_fn, base, offset, value_str));
                        }
                    }
                    Space::Csr => {
                        let csr = X::to_u64(addr.imm) as u16;
                        // Use traced helper (twr_csr)
                        self.writeln(indent, &format!("twr_csr(state, 0x{:x}, {});", csr, value_str));
                    }
                    Space::Pc => {
                        self.writeln(indent, &format!("state->pc = {};", value_str));
                    }
                    Space::Temp => {
                        let idx = X::to_u64(addr.imm);
                        self.writeln(indent, &format!("{} _t{} = {};", self.reg_type, idx, value_str));
                    }
                    _ => {}
                }
            }
            Stmt::If { cond, then_stmts, else_stmts } => {
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
    fn render_mem_write_tohost(&mut self, base: &str, offset: i16, value: &str, indent: usize) {
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

        // Generate the tohost check
        self.writeln(indent, &format!(
            "if (unlikely((uint32_t){} + {} == 0x{:x}u)) {{",
            base, offset, TOHOST_ADDR
        ));
        self.writeln(indent + 1, &format!("handle_tohost_write(state, {});", value));
        self.writeln(indent + 1, "if (unlikely(state->has_exited)) {");
        self.writeln(indent + 2, &format!("state->pc = {};", pc_lit));
        if !instret_update.is_empty() || !save_call.is_empty() {
            self.writeln(indent + 2, &format!("{}{}", instret_update, save_call));
        }
        self.writeln(indent + 2, "return;");
        self.writeln(indent + 1, "}");
        self.writeln(indent, "} else {");
        // Use traced helper for non-tohost writes
        self.writeln(indent + 1, &format!("twr_mem_u32(memory, {}, {}, {});", base, offset, value));
        self.writeln(indent, "}");
    }

    // ============= Block rendering =============

    /// Render block header.
    pub fn render_block_header(&mut self, start_pc: u64, _end_pc: u64) {
        let pc_str = self.fmt_pc(start_pc);
        self.write(&format!(
            "__attribute__((preserve_none, nonnull(1))) void B_{}({}) {{\n",
            pc_str, self.sig.params
        ));
    }

    /// Format PC for block names (hex without 0x prefix).
    fn fmt_pc(&self, pc: u64) -> String {
        if X::VALUE == 64 {
            format!("{:016x}", pc)
        } else {
            format!("{:08x}", pc)
        }
    }

    /// Render block footer.
    pub fn render_block_footer(&mut self) {
        self.write("}\n\n");
    }

    /// Render instruction.
    ///
    /// `fall_pc` is the address to fall through to (typically end_pc of the block).
    pub fn render_instruction(&mut self, ir: &InstrIR<X>, is_last: bool, fall_pc: u64) {
        self.current_pc = X::to_u64(ir.pc);

        // Optional: emit comment
        if self.config.emit_comments {
            let pc_hex = self.fmt_addr(self.current_pc);
            self.writeln(1, &format!("// PC: {}", pc_hex));
        }

        // Render statements
        for stmt in &ir.statements {
            self.render_stmt(stmt, 1);
        }

        self.instr_idx += 1;

        // Render terminator if last instruction
        if is_last {
            self.render_terminator(&ir.terminator, fall_pc);
        }
    }

    /// Render terminator with explicit fall-through target.
    ///
    /// `fall_pc` is used for:
    /// - Fall terminator: explicit tail call to fall-through block (if target is None)
    /// - Branch terminator: trace_branch_not_taken call
    fn render_terminator(&mut self, term: &Terminator<X>, fall_pc: u64) {
        match term {
            Terminator::Fall { target } => {
                // Explicit tail call for fall-through (matches Mojo)
                let target_pc = target.map(|t| X::to_u64(t)).unwrap_or(fall_pc);
                self.render_jump_static(target_pc);
            }
            Terminator::Jump { target } => {
                self.render_jump_static(X::to_u64(*target));
            }
            Terminator::JumpDyn { addr, resolved } => {
                if let Some(targets) = resolved {
                    self.render_jump_resolved(targets.iter().map(|t| X::to_u64(*t)).collect(), addr);
                } else {
                    self.render_jump_dynamic(addr, None);
                }
            }
            Terminator::Branch { cond, target, fall, hint } => {
                let cond_str = self.render_expr(cond);
                let fall_target = fall.map(|f| X::to_u64(f)).unwrap_or(fall_pc);
                self.render_branch(&cond_str, X::to_u64(*target), *hint, fall_target);
            }
            Terminator::Exit { code } => {
                let code_str = self.render_expr(code);
                self.render_exit(&code_str);
            }
            Terminator::Trap { message } => {
                self.writeln(1, &format!("// TRAP: {}", message));
                self.render_exit("1");
            }
        }
    }

    /// Render static jump.
    pub fn render_jump_static(&mut self, target: u64) {
        if self.is_valid_address(target) {
            // Resolve absorbed addresses to their merged block
            let resolved = self.config.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(1, &format!("[[clang::musttail]] return B_{}({});", pc_str, self.sig.args));
        } else {
            self.render_exit("1");
        }
    }

    /// Render dynamic jump.
    ///
    /// If `pre_eval_var` is set, use that variable name instead of rendering the expression.
    fn render_jump_dynamic(&mut self, target_expr: &Expr<X>, pre_eval_var: Option<&str>) {
        let target = pre_eval_var
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.render_expr(target_expr));
        self.writeln(1, &format!(
            "[[clang::musttail]] return dispatch_table[dispatch_index({})]({});",
            target, self.sig.args
        ));
    }

    /// Render jump with resolved targets.
    fn render_jump_resolved(&mut self, targets: Vec<u64>, fallback: &Expr<X>) {
        if targets.is_empty() {
            self.render_jump_dynamic(fallback, None);
            return;
        }

        let target_var = self.render_expr(fallback);
        if targets.len() > 1 {
            self.writeln(1, &format!("{} target = {};", self.reg_type, target_var));
        }

        let var_name = if targets.len() > 1 { "target" } else { &target_var };

        for target in &targets {
            if self.is_valid_address(*target) {
                let pc_str = self.fmt_pc(*target);
                let addr_lit = self.fmt_addr(*target);
                self.writeln(1, &format!("if ({} == {}) {{", var_name, addr_lit));
                self.writeln(2, &format!("[[clang::musttail]] return B_{}({});", pc_str, self.sig.args));
                self.writeln(1, "}");
            }
        }

        // Fallback to dispatch table
        let pre_eval = if targets.len() > 1 { Some("target") } else { None };
        self.render_jump_dynamic(fallback, pre_eval);
    }

    /// Render branch with both taken and not-taken paths.
    ///
    /// Matches Mojo pattern: emits trace_branch_taken inside if-block,
    /// then trace_branch_not_taken after closing brace for fall-through.
    fn render_branch(&mut self, cond: &str, target: u64, hint: BranchHint, fall_pc: u64) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({})", cond),
            BranchHint::NotTaken => format!("unlikely({})", cond),
            BranchHint::None => cond.to_string(),
        };

        // Tracing hooks (if enabled)
        let trace_taken = if self.config.has_tracing() {
            let target_lit = self.fmt_addr(target);
            format!("trace_branch_taken(&state->tracer, {}, {});\n    ",
                self.fmt_addr(self.current_pc), target_lit)
        } else {
            String::new()
        };

        let trace_not_taken = if self.config.has_tracing() {
            let fall_lit = self.fmt_addr(fall_pc);
            format!("trace_branch_not_taken(&state->tracer, {}, {});\n",
                self.fmt_addr(self.current_pc), fall_lit)
        } else {
            String::new()
        };

        // Pre-clone values that need to outlive the mutable borrows
        let args = self.sig.args.clone();
        let save_to_state = self.sig.save_to_state.clone();

        if self.is_valid_address(target) {
            // Resolve absorbed addresses to their merged block
            let resolved = self.config.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(1, &format!("if ({}) {{", cond_str));
            if !trace_taken.is_empty() {
                self.writeln(2, trace_taken.trim_end());
            }
            self.writeln(2, &format!("[[clang::musttail]] return B_{}({});", pc_str, args));
            self.writeln(1, "}");
        } else {
            self.writeln(1, &format!("if ({}) {{", cond_str));
            if !trace_taken.is_empty() {
                self.writeln(2, trace_taken.trim_end());
            }
            self.writeln(2, "state->has_exited = true;");
            self.writeln(2, "state->exit_code = 1;");
            let pc_lit = self.fmt_addr(target);
            self.writeln(2, &format!("state->pc = {};", pc_lit));
            if !save_to_state.is_empty() {
                self.writeln(2, &save_to_state);
            }
            self.writeln(2, "return;");
            self.writeln(1, "}");
        }

        // Emit trace_branch_not_taken for fall-through path
        if !trace_not_taken.is_empty() {
            self.write(&trace_not_taken);
        }
    }

    /// Render exit with save_to_state.
    fn render_exit(&mut self, code: &str) {
        let save_to_state = self.sig.save_to_state.clone();
        self.writeln(1, "state->has_exited = true;");
        self.writeln(1, &format!("state->exit_code = (uint8_t)({});", code));
        let pc_lit = self.fmt_addr(self.current_pc);
        self.writeln(1, &format!("state->pc = {};", pc_lit));
        if !save_to_state.is_empty() {
            self.writeln(1, &save_to_state);
        }
        self.writeln(1, "return;");
    }

    /// Render instret update.
    pub fn render_instret_update(&mut self, count: u64) {
        if self.config.instret_mode.counts() {
            self.writeln(1, &format!("instret += {};", count));
        }
    }

    // ============= Block rendering =============

    /// Render a complete block.
    pub fn render_block(&mut self, block: &BlockIR<X>) {
        let start_pc = X::to_u64(block.start_pc);
        let end_pc = X::to_u64(block.end_pc);

        self.render_block_header(start_pc, end_pc);

        let num_instrs = block.instructions.len();
        for (i, instr) in block.instructions.iter().enumerate() {
            let is_last = i == num_instrs - 1;
            // For last instruction, fall_pc is end_pc (next block's start)
            self.render_instruction(instr, is_last, end_pc);
        }

        // Update instret before terminator
        if self.config.instret_mode.counts() && num_instrs > 0 {
            self.render_instret_update(num_instrs as u64);
        }

        self.render_block_footer();
    }

    /// Render trace_block call at block entry.
    pub fn render_block_trace(&mut self, pc: u64) {
        if self.config.has_tracing() {
            let pc_lit = self.fmt_addr(pc);
            self.writeln(1, &format!("trace_block(&state->tracer, {});", pc_lit));
        }
    }

    /// Render trace_pc call for current instruction.
    pub fn emit_trace_pc(&mut self) {
        if self.config.has_tracing() {
            let pc_lit = self.fmt_addr(self.current_pc);
            self.writeln(1, &format!("trace_pc(&state->tracer, {});", pc_lit));
        }
    }

    /// Render instret check and early suspend if needed.
    pub fn render_instret_check(&mut self, pc: u64) {
        if !self.config.instret_mode.suspends() {
            return;
        }
        let save_to_state = self.sig.save_to_state.clone();
        let pc_lit = self.fmt_addr(pc);
        self.writeln(1, "if (unlikely(state->target_instret <= instret)) {");
        self.writeln(2, &format!("state->pc = {};", pc_lit));
        if !save_to_state.is_empty() {
            self.writeln(2, &save_to_state);
        }
        self.writeln(2, "return;");
        self.writeln(1, "}");
    }

    // ============= Taken-inline support =============

    /// Render branch open for taken-inline: `if (cond) {`
    pub fn render_branch_open(&mut self, cond: &str, hint: BranchHint) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({})", cond),
            BranchHint::NotTaken => format!("unlikely({})", cond),
            BranchHint::None => cond.to_string(),
        };
        self.writeln(1, &format!("if ({}) {{", cond_str));
    }

    /// Render branch close for taken-inline: `}`
    pub fn render_branch_close(&mut self) {
        self.writeln(1, "}");
    }

    /// Render instruction with custom indent (for inlined blocks).
    pub fn render_instruction_indented(&mut self, ir: &InstrIR<X>, is_last: bool, fall_pc: u64, indent: usize) {
        self.current_pc = X::to_u64(ir.pc);

        // Optional: emit comment
        if self.config.emit_comments {
            let pc_hex = self.fmt_addr(self.current_pc);
            self.writeln(indent, &format!("// PC: {}", pc_hex));
        }

        // Render statements
        for stmt in &ir.statements {
            self.render_stmt_indented(stmt, indent);
        }

        self.instr_idx += 1;

        // Render terminator if last instruction
        if is_last {
            self.render_terminator_indented(&ir.terminator, fall_pc, indent);
        }
    }

    /// Render statement with custom indent.
    fn render_stmt_indented(&mut self, stmt: &Stmt<X>, indent: usize) {
        match stmt {
            Stmt::Write { space, addr, value, width } => {
                let value_str = self.render_expr(value);
                match space {
                    Space::Reg => {
                        let reg = X::to_u64(addr.imm) as u8;
                        let code = self.sig.reg_write(reg, &value_str);
                        if !code.is_empty() {
                            self.writeln(indent, &code);
                        }
                    }
                    Space::Mem => {
                        let (base, offset) = if addr.kind == ExprKind::Add {
                            let base_str = self.render_expr(addr.left.as_ref().unwrap());
                            let offset_val = if let Some(right) = &addr.right {
                                if right.kind == ExprKind::Imm {
                                    X::to_u64(right.imm) as i64 as i16
                                } else {
                                    0i16
                                }
                            } else {
                                0i16
                            };
                            if offset_val == 0 && addr.right.is_some() && addr.right.as_ref().unwrap().kind != ExprKind::Imm {
                                (self.render_expr(addr), 0i16)
                            } else {
                                (base_str, offset_val)
                            }
                        } else if addr.mem_offset != 0 || addr.left.is_some() {
                            let base_str = self.render_expr(addr.left.as_ref().unwrap());
                            (base_str, addr.mem_offset)
                        } else {
                            (self.render_expr(addr), 0i16)
                        };

                        let store_fn = match width {
                            1 => "twr_mem_u8",
                            2 => "twr_mem_u16",
                            4 => "twr_mem_u32",
                            8 => "twr_mem_u64",
                            _ => "twr_mem_u32",
                        };
                        self.writeln(indent, &format!("{}(memory, {}, {}, {});", store_fn, base, offset, value_str));
                    }
                    Space::Csr => {
                        let csr = X::to_u64(addr.imm) as u16;
                        self.writeln(indent, &format!("twr_csr(state, 0x{:x}, {});", csr, value_str));
                    }
                    Space::Pc => {
                        self.writeln(indent, &format!("state->pc = {};", value_str));
                    }
                    Space::Temp => {
                        let idx = X::to_u64(addr.imm);
                        self.writeln(indent, &format!("{} _t{} = {};", self.reg_type, idx, value_str));
                    }
                    _ => {}
                }
            }
            Stmt::If { cond, then_stmts, else_stmts } => {
                let cond_str = self.render_expr(cond);
                self.writeln(indent, &format!("if ({}) {{", cond_str));
                for s in then_stmts {
                    self.render_stmt_indented(s, indent + 1);
                }
                if !else_stmts.is_empty() {
                    self.writeln(indent, "} else {");
                    for s in else_stmts {
                        self.render_stmt_indented(s, indent + 1);
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

    /// Render terminator with custom indent.
    fn render_terminator_indented(&mut self, term: &Terminator<X>, fall_pc: u64, indent: usize) {
        match term {
            Terminator::Fall { target } => {
                let target_pc = target.map(|t| X::to_u64(t)).unwrap_or(fall_pc);
                self.render_jump_static_indented(target_pc, indent);
            }
            Terminator::Jump { target } => {
                self.render_jump_static_indented(X::to_u64(*target), indent);
            }
            Terminator::JumpDyn { addr, resolved } => {
                if let Some(targets) = resolved {
                    self.render_jump_resolved_indented(targets.iter().map(|t| X::to_u64(*t)).collect(), addr, indent);
                } else {
                    self.render_jump_dynamic_indented(addr, None, indent);
                }
            }
            Terminator::Branch { cond, target, fall, hint } => {
                let cond_str = self.render_expr(cond);
                let fall_target = fall.map(|f| X::to_u64(f)).unwrap_or(fall_pc);
                self.render_branch_indented(&cond_str, X::to_u64(*target), *hint, fall_target, indent);
            }
            Terminator::Exit { code } => {
                let code_str = self.render_expr(code);
                self.render_exit_indented(&code_str, indent);
            }
            Terminator::Trap { message } => {
                self.writeln(indent, &format!("// TRAP: {}", message));
                self.render_exit_indented("1", indent);
            }
        }
    }

    /// Render static jump with custom indent.
    fn render_jump_static_indented(&mut self, target: u64, indent: usize) {
        if self.is_valid_address(target) {
            let resolved = self.config.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(indent, &format!("[[clang::musttail]] return B_{}({});", pc_str, self.sig.args));
        } else {
            self.render_exit_indented("1", indent);
        }
    }

    /// Render dynamic jump with custom indent.
    fn render_jump_dynamic_indented(&mut self, target_expr: &Expr<X>, pre_eval_var: Option<&str>, indent: usize) {
        let target = pre_eval_var
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.render_expr(target_expr));
        self.writeln(indent, &format!(
            "[[clang::musttail]] return dispatch_table[dispatch_index({})]({});",
            target, self.sig.args
        ));
    }

    /// Render jump with resolved targets with custom indent.
    fn render_jump_resolved_indented(&mut self, targets: Vec<u64>, fallback: &Expr<X>, indent: usize) {
        if targets.is_empty() {
            self.render_jump_dynamic_indented(fallback, None, indent);
            return;
        }

        let target_var = self.render_expr(fallback);
        if targets.len() > 1 {
            self.writeln(indent, &format!("{} target = {};", self.reg_type, target_var));
        }

        let var_name = if targets.len() > 1 { "target" } else { &target_var };

        for target in &targets {
            if self.is_valid_address(*target) {
                let pc_str = self.fmt_pc(*target);
                let addr_lit = self.fmt_addr(*target);
                self.writeln(indent, &format!("if ({} == {}) {{", var_name, addr_lit));
                self.writeln(indent + 1, &format!("[[clang::musttail]] return B_{}({});", pc_str, self.sig.args));
                self.writeln(indent, "}");
            }
        }

        let pre_eval = if targets.len() > 1 { Some("target") } else { None };
        self.render_jump_dynamic_indented(fallback, pre_eval, indent);
    }

    /// Render branch with custom indent.
    fn render_branch_indented(&mut self, cond: &str, target: u64, hint: BranchHint, _fall_pc: u64, indent: usize) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({})", cond),
            BranchHint::NotTaken => format!("unlikely({})", cond),
            BranchHint::None => cond.to_string(),
        };

        let args = self.sig.args.clone();
        let save_to_state = self.sig.save_to_state.clone();

        if self.is_valid_address(target) {
            let resolved = self.config.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(indent, &format!("if ({}) {{", cond_str));
            self.writeln(indent + 1, &format!("[[clang::musttail]] return B_{}({});", pc_str, args));
            self.writeln(indent, "}");
        } else {
            self.writeln(indent, &format!("if ({}) {{", cond_str));
            self.writeln(indent + 1, "state->has_exited = true;");
            self.writeln(indent + 1, "state->exit_code = 1;");
            let pc_lit = self.fmt_addr(target);
            self.writeln(indent + 1, &format!("state->pc = {};", pc_lit));
            if !save_to_state.is_empty() {
                self.writeln(indent + 1, &save_to_state);
            }
            self.writeln(indent + 1, "return;");
            self.writeln(indent, "}");
        }
    }

    /// Render exit with custom indent.
    fn render_exit_indented(&mut self, code: &str, indent: usize) {
        let save_to_state = self.sig.save_to_state.clone();
        self.writeln(indent, "state->has_exited = true;");
        self.writeln(indent, &format!("state->exit_code = (uint8_t)({});", code));
        let pc_lit = self.fmt_addr(self.current_pc);
        self.writeln(indent, &format!("state->pc = {};", pc_lit));
        if !save_to_state.is_empty() {
            self.writeln(indent, &save_to_state);
        }
        self.writeln(indent, "return;");
    }

    /// Render instret update with custom indent.
    pub fn render_instret_update_indented(&mut self, count: u64, indent: usize) {
        if self.config.instret_mode.counts() {
            self.writeln(indent, &format!("instret += {};", count));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_isa::Rv64;

    #[test]
    fn test_render_imm() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = CEmitter::new(config);

        let expr = Expr::imm(42);
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "0x2aULL");
    }

    #[test]
    fn test_render_reg_read() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = CEmitter::new(config);

        // Default config has no hot regs, so uses state->regs[]
        let expr = Expr::reg(5);
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "state->regs[5]");
    }

    #[test]
    fn test_render_reg_read_hot() {
        let mut config = EmitConfig::<Rv64>::default();
        config.hot_regs = vec![5]; // Make t0 hot
        let emitter = CEmitter::new(config);

        // Hot reg uses ABI name directly
        let expr = Expr::reg(5);
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "t0");
    }

    #[test]
    fn test_render_add() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = CEmitter::new(config);

        let expr = Expr::add(Expr::reg(1), Expr::imm(10));
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "(state->regs[1] + 0xaULL)");
    }

    #[test]
    fn test_render_add_hot() {
        let mut config = EmitConfig::<Rv64>::default();
        config.hot_regs = vec![1]; // Make ra hot
        let emitter = CEmitter::new(config);

        let expr = Expr::add(Expr::reg(1), Expr::imm(10));
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "(ra + 0xaULL)");
    }
}
