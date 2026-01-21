//! C code emitter for RISC-V recompiler.
//!
//! Generates C code from RISC-V IR blocks with:
//! - Explicit musttail calls for all control flow (including fall-through)
//! - Branch emits both taken and not-taken paths
//! - save_to_state on all exit paths
//! - Optional tracing hooks (trace_block, trace_pc, trace_branch_*)
//! - Optional tohost handling for riscv-tests

use rvr_ir::{
    BinaryOp, BlockIR, BranchHint, Expr, InstrIR, ReadExpr, Stmt, Terminator, TernaryOp, UnaryOp,
    WriteTarget, Xlen,
};

/// HTIF tohost address (matches riscv-tests expectation).
const TOHOST_ADDR: u64 = 0x80001000;

use crate::config::EmitConfig;
use crate::inputs::EmitInputs;
use crate::signature::FnSignature;

/// C code emitter.
pub struct CEmitter<X: Xlen> {
    pub config: EmitConfig<X>,
    pub inputs: EmitInputs,
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
    /// Current instruction op (packed OpId for tracing).
    current_op: u16,
    /// Instruction index within block (for instret).
    instr_idx: usize,
}

impl<X: Xlen> CEmitter<X> {
    /// Create a new emitter.
    pub fn new(config: EmitConfig<X>, inputs: EmitInputs) -> Self {
        let (reg_type, signed_type) = if X::VALUE == 64 {
            ("uint64_t", "int64_t")
        } else {
            ("uint32_t", "int32_t")
        };
        let sig = FnSignature::new(&config);

        Self {
            config,
            inputs,
            sig,
            out: String::with_capacity(4096),
            reg_type,
            signed_type,
            current_pc: 0,
            current_op: 0,
            instr_idx: 0,
        }
    }

    /// Reset output buffer.
    pub fn reset(&mut self) {
        self.out.clear();
        self.current_pc = 0;
        self.current_op = 0;
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
        self.inputs.is_valid_address(addr)
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
            Expr::Var(name) => name.clone(),
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
                    // Mask shift amount per RISC-V spec: lower 5 bits (RV32) or 6 bits (RV64)
                    BinaryOp::Sll => format!("({} << ({} & 0x{:x}ULL))", l, r, X::VALUE - 1),
                    BinaryOp::Srl => format!("({} >> ({} & 0x{:x}ULL))", l, r, X::VALUE - 1),
                    BinaryOp::Sra => format!("(({})(({}){}  >> ({} & 0x{:x}ULL)))", self.reg_type, self.signed_type, l, r, X::VALUE - 1),
                    BinaryOp::Eq => format!("{} == {}", l, r),
                    BinaryOp::Ne => format!("{} != {}", l, r),
                    BinaryOp::Lt => format!("({}){} < ({}){}", self.signed_type, l, self.signed_type, r),
                    BinaryOp::Ge => format!("({}){} >= ({}){}", self.signed_type, l, self.signed_type, r),
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
                    BinaryOp::AddW => format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} + (uint32_t){}))", l, r),
                    BinaryOp::SubW => format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} - (uint32_t){}))", l, r),
                    BinaryOp::MulW => format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} * (uint32_t){}))", l, r),
                    BinaryOp::DivW => format!("rv_divw((int32_t){}, (int32_t){})", l, r),
                    BinaryOp::DivUW => format!("rv_divuw((uint32_t){}, (uint32_t){})", l, r),
                    BinaryOp::RemW => format!("rv_remw((int32_t){}, (int32_t){})", l, r),
                    BinaryOp::RemUW => format!("rv_remuw((uint32_t){}, (uint32_t){})", l, r),
                    BinaryOp::SllW => format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} << ({} & 0x1f)))", l, r),
                    BinaryOp::SrlW => format!("((uint64_t)(int64_t)(int32_t)((uint32_t){} >> ({} & 0x1f)))", l, r),
                    BinaryOp::SraW => format!("((uint64_t)(int64_t)((int32_t){} >> ({} & 0x1f)))", l, r),
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
                            format!("(((uint64_t)(uint32_t){}) | ((uint64_t)(uint32_t){} << 32))", l, r)
                        } else {
                            format!("(((uint32_t)(uint16_t){}) | ((uint32_t)(uint16_t){} << 16))", l, r)
                        }
                    }
                    BinaryOp::Pack8 => format!("((({})(uint8_t){}) | (({})(uint8_t){} << 8))", self.reg_type, l, self.reg_type, r),
                    BinaryOp::Pack16 => format!("((int64_t)(int32_t)(((uint32_t)(uint16_t){}) | ((uint32_t)(uint16_t){} << 16)))", l, r),
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
                    "0".to_string()
                } else if self.config.has_tracing() {
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    if self.sig.is_hot_reg(*reg) {
                        let val = self.sig.reg_read(*reg);
                        format!(
                            "trd_regval(&state->tracer, {}, {}, {}, {})",
                            pc_lit, op_lit, reg, val
                        )
                    } else {
                        format!(
                            "trd_reg(&state->tracer, {}, {}, state, {})",
                            pc_lit, op_lit, reg
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
                if self.config.has_tracing() {
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    format!(
                        "{}(&state->tracer, {}, {}, memory, {}, {})",
                        load_fn, pc_lit, op_lit, base, offset
                    )
                } else {
                    format!("{}(memory, {}, {})", load_fn, base, offset)
                }
            }
            ReadExpr::MemAddr {
                addr,
                width,
                signed,
            } => {
                let base = self.render_expr(addr);
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
                if self.config.has_tracing() {
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    format!(
                        "{}(&state->tracer, {}, {}, memory, {}, 0)",
                        load_fn, pc_lit, op_lit, base
                    )
                } else {
                    format!("{}(memory, {}, 0)", load_fn, base)
                }
            }
            ReadExpr::Csr(csr) => {
                if self.config.has_tracing() {
                    let pc_lit = self.fmt_addr(self.current_pc);
                    let op_lit = self.current_op;
                    if self.config.instret_mode.counts() {
                        format!(
                            "trd_csr(&state->tracer, {}, {}, state, instret, 0x{:x})",
                            pc_lit, op_lit, csr
                        )
                    } else {
                        format!(
                            "trd_csr(&state->tracer, {}, {}, state, 0x{:x})",
                            pc_lit, op_lit, csr
                        )
                    }
                } else if self.config.instret_mode.counts() {
                    format!("trd_csr(state, instret, 0x{:x})", csr)
                } else {
                    format!("trd_csr(state, 0x{:x})", csr)
                }
            }
            ReadExpr::Pc => "state->pc".to_string(),
            ReadExpr::Cycle => "state->cycle".to_string(),
            ReadExpr::Instret => "state->instret".to_string(),
            ReadExpr::Temp(idx) => format!("_t{}", idx),
            ReadExpr::TraceIdx => "state->trace_idx".to_string(),
            ReadExpr::PcIdx => "state->pc_idx".to_string(),
            ReadExpr::ResAddr => "state->reservation_addr".to_string(),
            ReadExpr::ResValid => "state->reservation_valid".to_string(),
            ReadExpr::Exited => "state->exited".to_string(),
            ReadExpr::ExitCode => "state->exit_code".to_string(),
        }
    }
    // ============= Statement rendering =============

    /// Render statement.
    pub fn render_stmt(&mut self, stmt: &Stmt<X>, indent: usize) {
        match stmt {
            Stmt::Write { target, value } => {
                let value_str = self.render_expr(value);
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
                                        "{} = twr_regval(&state->tracer, {}, {}, {}, {});",
                                        name, pc_lit, op_lit, reg, value_str
                                    ),
                                );
                            } else {
                                self.writeln(
                                    indent,
                                    &format!(
                                        "twr_reg(&state->tracer, {}, {}, state, {}, {});",
                                        pc_lit, op_lit, reg, value_str
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
                    WriteTarget::Mem { addr, width } => {
                        // Extract base and offset from address expression.
                        // Store addresses come from ISA as Expr::add(rs1, imm).
                        let (base, offset) = match addr {
                            Expr::Binary {
                                op: BinaryOp::Add,
                                left,
                                right,
                            } => {
                                let base_str = self.render_expr(left);
                                if let Expr::Imm(imm) = right.as_ref() {
                                    (base_str, X::to_u64(*imm) as i64 as i16)
                                } else {
                                    (self.render_expr(addr), 0i16)
                                }
                            }
                            _ => (self.render_expr(addr), 0i16),
                        };

                        // Check for tohost handling on 32-bit stores
                        if self.config.tohost_enabled && *width == 4 {
                            self.render_mem_write_tohost(&base, offset, &value_str, indent);
                        } else {
                            let store_fn = match width {
                                1 => "twr_mem_u8",
                                2 => "twr_mem_u16",
                                4 => "twr_mem_u32",
                                8 => "twr_mem_u64",
                                _ => "twr_mem_u32",
                            };
                            if self.config.has_tracing() {
                                let pc_lit = self.fmt_addr(self.current_pc);
                                let op_lit = self.current_op;
                                self.writeln(
                                    indent,
                                    &format!(
                                        "{}(&state->tracer, {}, {}, memory, {}, {}, {});",
                                        store_fn, pc_lit, op_lit, base, offset, value_str
                                    ),
                                );
                            } else {
                                self.writeln(
                                    indent,
                                    &format!(
                                        "{}(memory, {}, {}, {});",
                                        store_fn, base, offset, value_str
                                    ),
                                );
                            }
                        }
                    }
                    WriteTarget::Csr(csr) => {
                        if self.config.has_tracing() {
                            let pc_lit = self.fmt_addr(self.current_pc);
                            let op_lit = self.current_op;
                            self.writeln(
                                indent,
                                &format!(
                                    "twr_csr(&state->tracer, {}, {}, state, 0x{:x}, {});",
                                    pc_lit, op_lit, csr, value_str
                                ),
                            );
                        } else {
                            self.writeln(
                                indent,
                                &format!("twr_csr(state, 0x{:x}, {});", csr, value_str),
                            );
                        }
                    }
                    WriteTarget::Pc => {
                        self.writeln(indent, &format!("state->pc = {};", value_str));
                    }
                    WriteTarget::Temp(idx) => {
                        self.writeln(
                            indent,
                            &format!("{} _t{} = {};", self.reg_type, idx, value_str),
                        );
                    }
                    WriteTarget::ResAddr => {
                        self.writeln(indent, &format!("state->reservation_addr = {};", value_str));
                    }
                    WriteTarget::ResValid => {
                        self.writeln(
                            indent,
                            &format!("state->reservation_valid = {};", value_str),
                        );
                    }
                    WriteTarget::Exited => {
                        self.writeln(indent, &format!("state->has_exited = {};", value_str));
                    }
                    WriteTarget::ExitCode => {
                        self.writeln(indent, &format!("state->exit_code = {};", value_str));
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
        self.writeln(
            indent,
            &format!(
                "if (unlikely((uint32_t){} + {} == 0x{:x}u)) {{",
                base, offset, TOHOST_ADDR
            ),
        );
        self.writeln(
            indent + 1,
            &format!("handle_tohost_write(state, {});", value),
        );
        self.writeln(indent + 1, "if (unlikely(state->has_exited)) {");
        self.writeln(indent + 2, &format!("state->pc = {};", pc_lit));
        if !instret_update.is_empty() || !save_call.is_empty() {
            self.writeln(indent + 2, &format!("{}{}", instret_update, save_call));
        }
        self.writeln(indent + 2, "return;");
        self.writeln(indent + 1, "}");
        self.writeln(indent, "} else {");
        if self.config.has_tracing() {
            let pc_lit = self.fmt_addr(self.current_pc);
            let op_lit = self.current_op;
            self.writeln(
                indent + 1,
                &format!(
                    "twr_mem_u32(&state->tracer, {}, {}, memory, {}, {}, {});",
                    pc_lit, op_lit, base, offset, value
                ),
            );
        } else {
            self.writeln(
                indent + 1,
                &format!("twr_mem_u32(memory, {}, {}, {});", base, offset, value),
            );
        }
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
        self.render_instruction_impl(ir, is_last, fall_pc, 1, false);
    }

    /// Render instruction with custom indent (for inlined blocks).
    ///
    /// When `use_simple_branch` is true, uses simplified branch rendering for superblock side-exits.
    fn render_instruction_impl(
        &mut self,
        ir: &InstrIR<X>,
        is_last: bool,
        fall_pc: u64,
        indent: usize,
        use_simple_branch: bool,
    ) {
        self.current_pc = X::to_u64(ir.pc);
        self.current_op = ir.op;

        // Optional: emit comment
        if self.config.emit_comments {
            let pc_hex = self.fmt_addr(self.current_pc);
            self.writeln(indent, &format!("// PC: {}", pc_hex));
        }

        self.emit_trace_pc();

        // Render statements
        for stmt in &ir.statements {
            self.render_stmt(stmt, indent);
        }

        if self.statements_write_exit(&ir.statements) {
            self.render_exit_check(indent);
        }

        self.instr_idx += 1;

        // Render terminator
        if is_last {
            // Update instret BEFORE the terminator (tail call) so the incremented value is passed
            if self.config.instret_mode.counts() {
                self.render_instret_update_impl(self.instr_idx as u64, indent);
            }
            if use_simple_branch {
                self.render_terminator_simple(&ir.terminator, fall_pc, indent);
            } else {
                self.render_terminator(&ir.terminator, fall_pc);
            }
        } else {
            // For superblocks: render BRANCH terminators as side exits even if not last
            // If branch is taken, jump to target. If not, fall through to next inlined instr.
            if let Terminator::Branch {
                cond, target, hint, ..
            } = &ir.terminator
            {
                let cond_str = self.render_expr(cond);
                self.render_side_exit_impl(&cond_str, X::to_u64(*target), *hint, indent);
            }
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
                    self.render_jump_resolved(
                        targets.iter().map(|t| X::to_u64(*t)).collect(),
                        addr,
                    );
                } else {
                    self.render_jump_dynamic(addr, None);
                }
            }
            Terminator::Branch {
                cond,
                target,
                fall,
                hint,
            } => {
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
        self.render_jump_static_impl(target, 1);
    }

    /// Render static jump with custom indent.
    fn render_jump_static_impl(&mut self, target: u64, indent: usize) {
        if self.is_valid_address(target) {
            // Resolve absorbed addresses to their merged block
            let resolved = self.inputs.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(
                indent,
                &format!(
                    "[[clang::musttail]] return B_{}({});",
                    pc_str, self.sig.args
                ),
            );
        } else {
            self.render_exit_impl("1", indent);
        }
    }

    /// Render dynamic jump.
    ///
    /// If `pre_eval_var` is set, use that variable name instead of rendering the expression.
    fn render_jump_dynamic(&mut self, target_expr: &Expr<X>, pre_eval_var: Option<&str>) {
        self.render_jump_dynamic_impl(target_expr, pre_eval_var, 1);
    }

    /// Render dynamic jump with custom indent.
    fn render_jump_dynamic_impl(
        &mut self,
        target_expr: &Expr<X>,
        pre_eval_var: Option<&str>,
        indent: usize,
    ) {
        let target = pre_eval_var
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.render_expr(target_expr));
        self.writeln(
            indent,
            &format!(
                "[[clang::musttail]] return dispatch_table[dispatch_index({})]({});",
                target, self.sig.args
            ),
        );
    }

    /// Render jump with resolved targets.
    fn render_jump_resolved(&mut self, targets: Vec<u64>, fallback: &Expr<X>) {
        self.render_jump_resolved_impl(targets, fallback, 1);
    }

    /// Render jump with resolved targets with custom indent.
    fn render_jump_resolved_impl(&mut self, targets: Vec<u64>, fallback: &Expr<X>, indent: usize) {
        if targets.is_empty() {
            self.render_jump_dynamic_impl(fallback, None, indent);
            return;
        }

        let target_var = self.render_expr(fallback);
        if targets.len() > 1 {
            self.writeln(
                indent,
                &format!("{} target = {};", self.reg_type, target_var),
            );
        }

        let var_name = if targets.len() > 1 {
            "target"
        } else {
            &target_var
        };

        for target in &targets {
            if self.is_valid_address(*target) {
                let pc_str = self.fmt_pc(*target);
                let addr_lit = self.fmt_addr(*target);
                self.writeln(indent, &format!("if ({} == {}) {{", var_name, addr_lit));
                self.writeln(
                    indent + 1,
                    &format!(
                        "[[clang::musttail]] return B_{}({});",
                        pc_str, self.sig.args
                    ),
                );
                self.writeln(indent, "}");
            }
        }

        // Fallback to dispatch table
        let pre_eval = if targets.len() > 1 {
            Some("target")
        } else {
            None
        };
        self.render_jump_dynamic_impl(fallback, pre_eval, indent);
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
            format!(
                "trace_branch_taken(&state->tracer, {}, {}, {});\n    ",
                self.fmt_addr(self.current_pc),
                self.current_op,
                target_lit
            )
        } else {
            String::new()
        };

        let trace_not_taken = if self.config.has_tracing() {
            let fall_lit = self.fmt_addr(fall_pc);
            format!(
                "trace_branch_not_taken(&state->tracer, {}, {}, {});\n",
                self.fmt_addr(self.current_pc),
                self.current_op,
                fall_lit
            )
        } else {
            String::new()
        };

        // Pre-clone values that need to outlive the mutable borrows
        let args = self.sig.args.clone();
        let save_to_state = self.sig.save_to_state.clone();

        if self.is_valid_address(target) {
            // Resolve absorbed addresses to their merged block
            let resolved = self.inputs.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(1, &format!("if ({}) {{", cond_str));
            if !trace_taken.is_empty() {
                self.writeln(2, trace_taken.trim_end());
            }
            self.writeln(
                2,
                &format!("[[clang::musttail]] return B_{}({});", pc_str, args),
            );
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

        // Emit fall-through musttail return
        if self.is_valid_address(fall_pc) {
            let resolved = self.inputs.resolve_address(fall_pc);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(
                1,
                &format!("[[clang::musttail]] return B_{}({});", pc_str, args),
            );
        } else {
            // Invalid fall address - exit
            self.writeln(1, "state->has_exited = true;");
            self.writeln(1, "state->exit_code = 1;");
            let pc_lit = self.fmt_addr(fall_pc);
            self.writeln(1, &format!("state->pc = {};", pc_lit));
            if !save_to_state.is_empty() {
                self.writeln(1, &save_to_state);
            }
            self.writeln(1, "return;");
        }
    }

    /// Render superblock side exit (branch with instret update).
    fn render_side_exit_impl(&mut self, cond: &str, target: u64, hint: BranchHint, indent: usize) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({})", cond),
            BranchHint::NotTaken => format!("unlikely({})", cond),
            BranchHint::None => cond.to_string(),
        };

        let args = self.sig.args.clone();
        let save_to_state_no_instret = self.sig.save_to_state_no_instret.clone();

        if self.is_valid_address(target) {
            let resolved = self.inputs.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(indent, &format!("if ({}) {{", cond_str));
            if self.config.instret_mode.counts() {
                self.writeln(indent + 1, &format!("instret += {};", self.instr_idx));
            }
            self.writeln(
                indent + 1,
                &format!("[[clang::musttail]] return B_{}({});", pc_str, args),
            );
            self.writeln(indent, "}");
        } else {
            self.writeln(indent, &format!("if ({}) {{", cond_str));
            self.writeln(indent + 1, "state->has_exited = true;");
            self.writeln(indent + 1, "state->exit_code = 1;");
            let pc_lit = self.fmt_addr(target);
            self.writeln(indent + 1, &format!("state->pc = {};", pc_lit));
            if self.config.instret_mode.counts() {
                self.writeln(
                    indent + 1,
                    &format!("state->instret = instret + {};", self.instr_idx),
                );
            }
            // Use save_to_state_no_instret since we already handled instret above
            if !save_to_state_no_instret.is_empty() {
                self.writeln(indent + 1, &save_to_state_no_instret);
            }
            self.writeln(indent + 1, "return;");
            self.writeln(indent, "}");
        }
    }

    /// Render exit with save_to_state.
    fn render_exit(&mut self, code: &str) {
        self.render_exit_impl(code, 1);
    }

    /// Render exit with custom indent.
    fn render_exit_impl(&mut self, code: &str, indent: usize) {
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

    fn statements_write_exit(&self, stmts: &[Stmt<X>]) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Write { target, .. } => match target {
                    WriteTarget::Exited | WriteTarget::ExitCode => return true,
                    _ => {}
                },
                Stmt::If {
                    then_stmts,
                    else_stmts,
                    ..
                } => {
                    if self.statements_write_exit(then_stmts)
                        || self.statements_write_exit(else_stmts)
                    {
                        return true;
                    }
                }
                Stmt::ExternCall { .. } => {}
            }
        }
        false
    }

    fn render_exit_check(&mut self, indent: usize) {
        let save_to_state = self.sig.save_to_state.clone();
        let pc_lit = self.fmt_addr(self.current_pc);
        self.writeln(indent, "if (unlikely(state->has_exited)) {");
        self.writeln(indent + 1, &format!("state->pc = {};", pc_lit));
        if !save_to_state.is_empty() {
            self.writeln(indent + 1, &save_to_state);
        }
        self.writeln(indent + 1, "return;");
        self.writeln(indent, "}");
    }

    /// Render instret update.
    pub fn render_instret_update(&mut self, count: u64) {
        self.render_instret_update_impl(count, 1);
    }

    /// Render instret update with custom indent.
    fn render_instret_update_impl(&mut self, count: u64, indent: usize) {
        if self.config.instret_mode.counts() {
            self.writeln(indent, &format!("instret += {};", count));
        }
    }

    // ============= Block rendering =============

    /// Render a complete block.
    pub fn render_block(&mut self, block: &BlockIR<X>) {
        let start_pc = X::to_u64(block.start_pc);
        let end_pc = X::to_u64(block.end_pc);

        self.render_block_header(start_pc, end_pc);
        self.render_block_trace(start_pc);

        let num_instrs = block.instructions.len();
        for (i, instr) in block.instructions.iter().enumerate() {
            let is_last = i == num_instrs - 1;
            // For last instruction, fall_pc is end_pc (next block's start)
            // Note: instret update is now done inside render_instruction for is_last=true
            self.render_instruction(instr, is_last, end_pc);
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
            self.writeln(
                1,
                &format!("trace_pc(&state->tracer, {}, {});", pc_lit, self.current_op),
            );
        }
    }

    /// Render trace_pc call for a specific instruction (used for taken-inline branches).
    pub fn emit_trace_pc_for(&mut self, pc: u64, op: u16) {
        if self.config.has_tracing() {
            let pc_lit = self.fmt_addr(pc);
            self.writeln(1, &format!("trace_pc(&state->tracer, {}, {});", pc_lit, op));
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
    pub fn render_instruction_indented(
        &mut self,
        ir: &InstrIR<X>,
        is_last: bool,
        fall_pc: u64,
        indent: usize,
    ) {
        self.render_instruction_impl(ir, is_last, fall_pc, indent, true);
    }

    /// Render terminator with custom indent (simplified, no tracing).
    ///
    /// Used for inlined blocks in superblocks where branches are side-exits.
    fn render_terminator_simple(&mut self, term: &Terminator<X>, fall_pc: u64, indent: usize) {
        match term {
            Terminator::Fall { target } => {
                let target_pc = target.map(|t| X::to_u64(t)).unwrap_or(fall_pc);
                self.render_jump_static_impl(target_pc, indent);
            }
            Terminator::Jump { target } => {
                self.render_jump_static_impl(X::to_u64(*target), indent);
            }
            Terminator::JumpDyn { addr, resolved } => {
                if let Some(targets) = resolved {
                    self.render_jump_resolved_impl(
                        targets.iter().map(|t| X::to_u64(*t)).collect(),
                        addr,
                        indent,
                    );
                } else {
                    self.render_jump_dynamic_impl(addr, None, indent);
                }
            }
            Terminator::Branch {
                cond,
                target,
                fall,
                hint,
            } => {
                let cond_str = self.render_expr(cond);
                let fall_target = fall.map(|f| X::to_u64(f)).unwrap_or(fall_pc);
                self.render_branch_simple(
                    &cond_str,
                    X::to_u64(*target),
                    *hint,
                    fall_target,
                    indent,
                );
            }
            Terminator::Exit { code } => {
                let code_str = self.render_expr(code);
                self.render_exit_impl(&code_str, indent);
            }
            Terminator::Trap { message } => {
                self.writeln(indent, &format!("// TRAP: {}", message));
                self.render_exit_impl("1", indent);
            }
        }
    }

    /// Render branch (simplified, no tracing, no fall-through).
    ///
    /// Used for inlined blocks where fall-through continues to next instruction.
    fn render_branch_simple(
        &mut self,
        cond: &str,
        target: u64,
        hint: BranchHint,
        _fall_pc: u64,
        indent: usize,
    ) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({})", cond),
            BranchHint::NotTaken => format!("unlikely({})", cond),
            BranchHint::None => cond.to_string(),
        };

        let args = self.sig.args.clone();
        let save_to_state = self.sig.save_to_state.clone();

        if self.is_valid_address(target) {
            let resolved = self.inputs.resolve_address(target);
            let pc_str = self.fmt_pc(resolved);
            self.writeln(indent, &format!("if ({}) {{", cond_str));
            self.writeln(
                indent + 1,
                &format!("[[clang::musttail]] return B_{}({});", pc_str, args),
            );
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

    /// Render instret update with custom indent.
    pub fn render_instret_update_indented(&mut self, count: u64, indent: usize) {
        self.render_instret_update_impl(count, indent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_isa::Rv64;

    #[test]
    fn test_render_imm() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = CEmitter::new(config, EmitInputs::default());

        let expr = Expr::imm(42);
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "0x2aULL");
    }

    #[test]
    fn test_render_reg_read() {
        let mut config = EmitConfig::<Rv64>::default();
        config.hot_regs.clear(); // Test non-hot path
        let emitter = CEmitter::new(config, EmitInputs::default());

        // Non-hot reg uses state->regs[]
        let expr = Expr::reg(5);
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "state->regs[5]");
    }

    #[test]
    fn test_render_reg_read_hot() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = CEmitter::new(config, EmitInputs::default());

        // Default config has hot regs, t0 (reg 5) should be hot
        let expr = Expr::reg(5);
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "t0");
    }

    #[test]
    fn test_render_add() {
        let mut config = EmitConfig::<Rv64>::default();
        config.hot_regs.clear(); // Test non-hot path
        let emitter = CEmitter::new(config, EmitInputs::default());

        let expr = Expr::add(Expr::reg(1), Expr::imm(10));
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "(state->regs[1] + 0xaULL)");
    }

    #[test]
    fn test_render_add_hot() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = CEmitter::new(config, EmitInputs::default());

        // Default config has hot regs, ra (reg 1) should be hot
        let expr = Expr::add(Expr::reg(1), Expr::imm(10));
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "(ra + 0xaULL)");
    }
}
