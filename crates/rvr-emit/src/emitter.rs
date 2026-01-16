//! C code emitter for RISC-V recompiler.

use rvr_ir::{BlockIR, BranchHint, Expr, ExprKind, InstrIR, Space, Stmt, Terminator};
use rvr_isa::{OpId, Xlen};

use crate::config::EmitConfig;

/// C code emitter.
pub struct CEmitter<X: Xlen> {
    pub config: EmitConfig<X>,
    /// Output buffer.
    pub out: String,
    /// Register type name ("uint32_t" or "uint64_t").
    reg_type: &'static str,
    /// Signed register type ("int32_t" or "int64_t").
    signed_type: &'static str,
    /// Current instruction PC.
    current_pc: u64,
    /// Current instruction OpId.
    current_op: OpId,
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

        Self {
            config,
            out: String::with_capacity(4096),
            reg_type,
            signed_type,
            current_pc: 0,
            current_op: OpId::new(0, 0),
            instr_idx: 0,
        }
    }

    /// Reset output buffer.
    pub fn reset(&mut self) {
        self.out.clear();
        self.current_pc = 0;
        self.current_op = OpId::new(0, 0);
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
        }
    }

    /// Render read expression.
    fn render_read(&self, expr: &Expr<X>) -> String {
        match expr.space {
            Space::Reg => {
                let reg = X::to_u64(expr.imm) as u8;
                if reg == 0 {
                    "0".to_string()
                } else {
                    format!("regs[{}]", reg)
                }
            }
            Space::Mem => {
                let base = self.render_expr(expr.left.as_ref().unwrap());
                let offset = expr.mem_offset;
                let load_fn = match (expr.width, expr.signed) {
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
            Space::Csr => {
                let csr = X::to_u64(expr.imm) as u16;
                format!("rd_csr(state, 0x{:x})", csr)
            }
            Space::Pc => "state->pc".to_string(),
            Space::Cycle => "state->cycle".to_string(),
            Space::Instret => "state->instret".to_string(),
            Space::Temp => {
                let idx = X::to_u64(expr.imm);
                format!("_t{}", idx)
            }
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
                        if reg != 0 {
                            self.writeln(indent, &format!("regs[{}] = {};", reg, value_str));
                        }
                    }
                    Space::Mem => {
                        let base = self.render_expr(addr);
                        let store_fn = match width {
                            1 => "wr_mem_u8",
                            2 => "wr_mem_u16",
                            4 => "wr_mem_u32",
                            8 => "wr_mem_u64",
                            _ => "wr_mem_u32",
                        };
                        self.writeln(indent, &format!("{}(memory, {}, {});", store_fn, base, value_str));
                    }
                    Space::Csr => {
                        let csr = X::to_u64(addr.imm) as u16;
                        self.writeln(indent, &format!("wr_csr(state, 0x{:x}, {});", csr, value_str));
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

    // ============= Block rendering =============

    /// Render block header.
    pub fn render_block_header(&mut self, start_pc: u64, _end_pc: u64) {
        let pc_str = if X::VALUE == 64 {
            format!("{:016x}", start_pc)
        } else {
            format!("{:08x}", start_pc)
        };
        self.write(&format!(
            "__attribute__((preserve_none, nonnull(1))) void B_0x{}(RvState* state, uint8_t* memory, {} instret, {}* regs) {{\n",
            pc_str, self.reg_type, self.reg_type
        ));
    }

    /// Render block footer.
    pub fn render_block_footer(&mut self) {
        self.write("}\n\n");
    }

    /// Render instruction.
    pub fn render_instruction(&mut self, ir: &InstrIR<X>, is_last: bool) {
        self.current_pc = X::to_u64(ir.pc);
        self.current_op = ir.opid;

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
            self.render_terminator(&ir.terminator);
        }
    }

    /// Render terminator.
    fn render_terminator(&mut self, term: &Terminator<X>) {
        match term {
            Terminator::Fall => {
                // Fall through to next instruction
            }
            Terminator::Jump { target } => {
                self.render_jump_static(X::to_u64(*target));
            }
            Terminator::JumpDyn { addr, resolved } => {
                if let Some(targets) = resolved {
                    self.render_jump_resolved(targets.iter().map(|t| X::to_u64(*t)).collect(), addr);
                } else {
                    self.render_jump_dynamic(addr);
                }
            }
            Terminator::Branch { cond, target, hint } => {
                let cond_str = self.render_expr(cond);
                self.render_branch(&cond_str, X::to_u64(*target), *hint);
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
    fn render_jump_static(&mut self, target: u64) {
        if self.is_valid_address(target) {
            let pc_str = if X::VALUE == 64 {
                format!("{:016x}", target)
            } else {
                format!("{:08x}", target)
            };
            self.writeln(1, &format!("[[clang::musttail]] return B_0x{}(state, memory, instret, regs);", pc_str));
        } else {
            self.render_exit("1");
        }
    }

    /// Render dynamic jump.
    fn render_jump_dynamic(&mut self, target_expr: &Expr<X>) {
        let target = self.render_expr(target_expr);
        self.writeln(1, &format!("[[clang::musttail]] return dispatch_table[dispatch_index({})](state, memory, instret, regs);", target));
    }

    /// Render jump with resolved targets.
    fn render_jump_resolved(&mut self, targets: Vec<u64>, fallback: &Expr<X>) {
        if targets.is_empty() {
            self.render_jump_dynamic(fallback);
            return;
        }

        let target_var = self.render_expr(fallback);
        if targets.len() > 1 {
            self.writeln(1, &format!("{} target = {};", self.reg_type, target_var));
        }

        let var_name = if targets.len() > 1 { "target" } else { &target_var };

        for target in &targets {
            if self.is_valid_address(*target) {
                let pc_str = if X::VALUE == 64 {
                    format!("{:016x}", target)
                } else {
                    format!("{:08x}", target)
                };
                let addr_lit = self.fmt_addr(*target);
                self.writeln(1, &format!("if ({} == {}) {{", var_name, addr_lit));
                self.writeln(2, &format!("[[clang::musttail]] return B_0x{}(state, memory, instret, regs);", pc_str));
                self.writeln(1, "}");
            }
        }

        self.render_jump_dynamic(fallback);
    }

    /// Render branch.
    fn render_branch(&mut self, cond: &str, target: u64, hint: BranchHint) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({})", cond),
            BranchHint::NotTaken => format!("unlikely({})", cond),
            BranchHint::None => cond.to_string(),
        };

        if self.is_valid_address(target) {
            let pc_str = if X::VALUE == 64 {
                format!("{:016x}", target)
            } else {
                format!("{:08x}", target)
            };
            self.writeln(1, &format!("if ({}) {{", cond_str));
            self.writeln(2, &format!("[[clang::musttail]] return B_0x{}(state, memory, instret, regs);", pc_str));
            self.writeln(1, "}");
        } else {
            self.writeln(1, &format!("if ({}) {{", cond_str));
            self.writeln(2, "state->has_exited = true;");
            self.writeln(2, "state->exit_code = 1;");
            let pc_lit = self.fmt_addr(target);
            self.writeln(2, &format!("state->pc = {};", pc_lit));
            self.writeln(2, "return;");
            self.writeln(1, "}");
        }
    }

    /// Render exit.
    fn render_exit(&mut self, code: &str) {
        self.writeln(1, "state->has_exited = true;");
        self.writeln(1, &format!("state->exit_code = (uint8_t)({});", code));
        let pc_lit = self.fmt_addr(self.current_pc);
        self.writeln(1, &format!("state->pc = {};", pc_lit));
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
            self.render_instruction(instr, is_last);
        }

        // Update instret before terminator
        if self.config.instret_mode.counts() && num_instrs > 0 {
            self.render_instret_update(num_instrs as u64);
        }

        self.render_block_footer();
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

        let expr = Expr::reg(5);
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "regs[5]");
    }

    #[test]
    fn test_render_add() {
        let config = EmitConfig::<Rv64>::default();
        let emitter = CEmitter::new(config);

        let expr = Expr::add(Expr::reg(1), Expr::imm(10));
        let result = emitter.render_expr(&expr);
        assert_eq!(result, "(regs[1] + 0xaULL)");
    }
}
