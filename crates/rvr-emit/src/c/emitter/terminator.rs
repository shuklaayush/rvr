//! Terminator rendering for the C emitter.

use rvr_ir::{BlockIR, BranchHint, Expr, InstrIR, Stmt, Terminator, WriteTarget, Xlen};

use super::CEmitter;

impl<X: Xlen> CEmitter<X> {
    pub(super) fn render_terminator(&mut self, term: &Terminator<X>, fall_pc: u64) {
        match term {
            Terminator::Fall { target } => {
                // Explicit tail call for fall-through
                let target_pc = target.map_or(fall_pc, |t| X::to_u64(t));
                self.render_jump_static(target_pc);
            }
            Terminator::Jump { target } => {
                self.render_jump_static(X::to_u64(*target));
            }
            Terminator::JumpDyn { addr, resolved } => {
                if let Some(targets) = resolved {
                    let target_addrs: Vec<u64> = targets.iter().map(|t| X::to_u64(*t)).collect();
                    self.render_jump_resolved(&target_addrs, addr);
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
                let fall_target = fall.map_or(fall_pc, |f| X::to_u64(f));
                self.render_branch(&cond_str, X::to_u64(*target), *hint, fall_target);
            }
            Terminator::Exit { code } => {
                let code_str = self.render_expr(code);
                self.render_exit(&code_str);
            }
            Terminator::Trap { message } => {
                self.writeln(1, &format!("// TRAP: {message}"));
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
        // In suspend modes, check for suspension before the tail call
        if self.config.instret_mode.suspends() {
            self.render_instret_check_impl(target, indent);
        }

        if self.is_valid_address(target) {
            // Resolve absorbed addresses to their merged block
            let resolved = self.inputs.resolve_address(target);
            let pc_str = Self::fmt_pc(resolved);
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

    /// Render instret check with custom indent.
    fn render_instret_check_impl(&mut self, pc: u64, indent: usize) {
        if !self.config.instret_mode.suspends() {
            return;
        }
        let save_to_state = self.sig.save_to_state.clone();
        let pc_lit = Self::fmt_addr(pc);
        let state = self.state_ref();
        self.writeln(
            indent,
            &format!("if (unlikely({state}->target_instret <= instret)) {{"),
        );
        self.writeln(indent + 1, &format!("{state}->pc = {pc_lit};"));
        if !save_to_state.is_empty() {
            self.writeln(indent + 1, &save_to_state);
        }
        self.writeln(indent + 1, "return;");
        self.writeln(indent, "}");
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
        let target = pre_eval_var.map_or_else(
            || self.render_expr(target_expr),
            std::string::ToString::to_string,
        );

        // In suspend modes, check for suspension before the tail call
        if self.config.instret_mode.suspends() {
            self.render_instret_check_dynamic(&target, indent);
        }

        self.writeln(
            indent,
            &format!(
                "[[clang::musttail]] return dispatch_table[dispatch_index({})]({});",
                target, self.sig.args
            ),
        );
    }

    /// Render instret check for dynamic target.
    fn render_instret_check_dynamic(&mut self, target_var: &str, indent: usize) {
        if !self.config.instret_mode.suspends() {
            return;
        }
        let save_to_state = self.sig.save_to_state.clone();
        let state = self.state_ref();
        self.writeln(
            indent,
            &format!("if (unlikely({state}->target_instret <= instret)) {{"),
        );
        self.writeln(indent + 1, &format!("{state}->pc = {target_var};"));
        if !save_to_state.is_empty() {
            self.writeln(indent + 1, &save_to_state);
        }
        self.writeln(indent + 1, "return;");
        self.writeln(indent, "}");
    }

    /// Render jump with resolved targets.
    fn render_jump_resolved(&mut self, targets: &[u64], fallback: &Expr<X>) {
        self.render_jump_resolved_impl(targets, fallback, 1);
    }

    /// Render jump with resolved targets with custom indent.
    fn render_jump_resolved_impl(&mut self, targets: &[u64], fallback: &Expr<X>, indent: usize) {
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

        for target in targets {
            if self.is_valid_address(*target) {
                let pc_str = Self::fmt_pc(*target);
                let addr_lit = Self::fmt_addr(*target);
                self.writeln(indent, &format!("if ({var_name} == {addr_lit}) {{"));
                // In per-instruction mode, check for suspension before the tail call
                if self.config.instret_mode.per_instruction() {
                    self.render_instret_check_impl(*target, indent + 1);
                }
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
    /// Emits `trace_branch_taken` inside if-block, then `trace_branch_not_taken`
    /// after closing brace for fall-through.
    fn render_branch(&mut self, cond: &str, target: u64, hint: BranchHint, fall_pc: u64) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({cond})"),
            BranchHint::NotTaken => format!("unlikely({cond})"),
            BranchHint::None => cond.to_string(),
        };

        // Tracing hooks (if enabled)
        let trace_taken = if self.config.has_tracing() {
            let target_lit = Self::fmt_addr(target);
            let state = self.state_ref();
            format!(
                "trace_branch_taken(&{}->tracer, {}, {}, {});\n    ",
                state,
                Self::fmt_addr(self.current_pc),
                self.current_op,
                target_lit
            )
        } else {
            String::new()
        };

        let trace_not_taken = if self.config.has_tracing() {
            let fall_lit = Self::fmt_addr(fall_pc);
            let state = self.state_ref();
            format!(
                "trace_branch_not_taken(&{}->tracer, {}, {}, {});\n",
                state,
                Self::fmt_addr(self.current_pc),
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
            let pc_str = Self::fmt_pc(resolved);
            self.writeln(1, &format!("if ({cond_str}) {{"));
            if !trace_taken.is_empty() {
                self.writeln(2, trace_taken.trim_end());
            }
            // In suspend modes, check for suspension before the tail call
            if self.config.instret_mode.suspends() {
                self.render_instret_check_impl(target, 2);
            }
            self.writeln(
                2,
                &format!("[[clang::musttail]] return B_{pc_str}({args});"),
            );
        } else {
            let state = self.state_ref();
            self.writeln(1, &format!("if ({cond_str}) {{"));
            if !trace_taken.is_empty() {
                self.writeln(2, trace_taken.trim_end());
            }
            self.writeln(2, &format!("{state}->has_exited = true;"));
            self.writeln(2, &format!("{state}->exit_code = 1;"));
            let pc_lit = Self::fmt_addr(target);
            self.writeln(2, &format!("{state}->pc = {pc_lit};"));
            if !save_to_state.is_empty() {
                self.writeln(2, &save_to_state);
            }
            self.writeln(2, "return;");
        }
        self.writeln(1, "}");

        // Emit trace_branch_not_taken for fall-through path
        if !trace_not_taken.is_empty() {
            self.write(&trace_not_taken);
        }

        // Emit fall-through musttail return
        if self.is_valid_address(fall_pc) {
            let resolved = self.inputs.resolve_address(fall_pc);
            let pc_str = Self::fmt_pc(resolved);
            // In suspend modes, check for suspension before the tail call
            if self.config.instret_mode.suspends() {
                self.render_instret_check_impl(fall_pc, 1);
            }
            self.writeln(
                1,
                &format!("[[clang::musttail]] return B_{pc_str}({args});"),
            );
        } else {
            // Invalid fall address - exit
            let state = self.state_ref();
            self.writeln(1, &format!("{state}->has_exited = true;"));
            self.writeln(1, &format!("{state}->exit_code = 1;"));
            let pc_lit = Self::fmt_addr(fall_pc);
            self.writeln(1, &format!("{state}->pc = {pc_lit};"));
            if !save_to_state.is_empty() {
                self.writeln(1, &save_to_state);
            }
            self.writeln(1, "return;");
        }
    }

    /// Render superblock side exit (branch with instret update).
    pub(super) fn render_side_exit_impl(
        &mut self,
        cond: &str,
        target: u64,
        hint: BranchHint,
        indent: usize,
    ) {
        let cond_str = match hint {
            BranchHint::Taken => format!("likely({cond})"),
            BranchHint::NotTaken => format!("unlikely({cond})"),
            BranchHint::None => cond.to_string(),
        };

        let args = self.sig.args.clone();
        let save_to_state_no_instret = self.sig.save_to_state_no_instret.clone();

        if self.is_valid_address(target) {
            let resolved = self.inputs.resolve_address(target);
            let pc_str = Self::fmt_pc(resolved);
            self.writeln(indent, &format!("if ({cond_str}) {{"));
            if self.config.instret_mode.counts() {
                self.writeln(indent + 1, &format!("instret += {};", self.instr_idx));
            }
            self.writeln(
                indent + 1,
                &format!("[[clang::musttail]] return B_{pc_str}({args});"),
            );
        } else {
            let state = self.state_ref();
            self.writeln(indent, &format!("if ({cond_str}) {{"));
            self.writeln(indent + 1, &format!("{state}->has_exited = true;"));
            self.writeln(indent + 1, &format!("{state}->exit_code = 1;"));
            let pc_lit = Self::fmt_addr(target);
            self.writeln(indent + 1, &format!("{state}->pc = {pc_lit};"));
            if self.config.instret_mode.counts() {
                self.writeln(
                    indent + 1,
                    &format!("{}->instret = instret + {};", state, self.instr_idx),
                );
            }
            // Use save_to_state_no_instret since we already handled instret above
            if !save_to_state_no_instret.is_empty() {
                self.writeln(indent + 1, &save_to_state_no_instret);
            }
            self.writeln(indent + 1, "return;");
        }
        self.writeln(indent, "}");
    }

    /// Render exit with `save_to_state`.
    fn render_exit(&mut self, code: &str) {
        self.render_exit_impl(code, 1);
    }

    /// Render exit with custom indent.
    fn render_exit_impl(&mut self, code: &str, indent: usize) {
        let save_to_state = self.sig.save_to_state.clone();
        let state = self.state_ref();
        self.writeln(indent, &format!("{state}->has_exited = true;"));
        self.writeln(indent, &format!("{state}->exit_code = {code};"));
        let pc_lit = Self::fmt_addr(self.current_pc);
        self.writeln(indent, &format!("{state}->pc = {pc_lit};"));
        if !save_to_state.is_empty() {
            self.writeln(indent, &save_to_state);
        }
        self.writeln(indent, "return;");
    }

    pub(super) fn statements_write_exit(stmts: &[Stmt<X>]) -> bool {
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
                    if Self::statements_write_exit(then_stmts)
                        || Self::statements_write_exit(else_stmts)
                    {
                        return true;
                    }
                }
                Stmt::ExternCall { .. } => {}
            }
        }
        false
    }

    pub(super) fn render_exit_check(&mut self, indent: usize) {
        let save_to_state = self.sig.save_to_state.clone();
        let state = self.state_ref();
        let pc_lit = Self::fmt_addr(self.current_pc);
        self.writeln(indent, &format!("if (unlikely({state}->has_exited)) {{"));
        self.writeln(indent + 1, &format!("{state}->pc = {pc_lit};"));
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
    pub(super) fn render_instret_update_impl(&mut self, count: u64, indent: usize) {
        if self.config.instret_mode.counts() {
            self.writeln(indent, &format!("instret += {count};"));
        }
    }

    // ============= Block rendering =============

    /// Render a complete block.
    pub fn render_block(&mut self, block: &BlockIR<X>) {
        let start_pc = X::to_u64(block.start_pc);
        let end_pc = X::to_u64(block.end_pc);

        self.render_block_header_with_count(start_pc, end_pc, block.instructions.len());
        self.render_block_trace(start_pc);

        let num_instrs = block.instructions.len();
        for (i, instr) in block.instructions.iter().enumerate() {
            let is_last = i == num_instrs - 1;
            // For per-instruction mode, pass the next instruction's PC
            let next_instr_pc = if !is_last && i + 1 < num_instrs {
                Some(X::to_u64(block.instructions[i + 1].pc))
            } else {
                None
            };
            // For last instruction, fall_pc is end_pc (next block's start)
            // Note: instret update is now done inside render_instruction for is_last=true
            self.render_instruction(instr, is_last, end_pc, next_instr_pc);
        }

        self.render_block_footer();
    }

    /// Render `trace_block` call at block entry.
    pub fn render_block_trace(&mut self, pc: u64) {
        if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(pc);
            let state = self.state_ref();
            self.writeln(1, &format!("trace_block(&{state}->tracer, {pc_lit});"));
        }
    }

    /// Render `trace_pc` call for current instruction.
    pub fn emit_trace_pc(&mut self) {
        if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(self.current_pc);
            let state = self.state_ref();
            self.writeln(
                1,
                &format!(
                    "trace_pc(&{state}->tracer, {}, {});",
                    pc_lit, self.current_op
                ),
            );
            // Emit trace_opcode for Spike-compatible tracing
            self.writeln(
                1,
                &format!(
                    "trace_opcode(&{state}->tracer, {}, {}, 0x{:x});",
                    pc_lit, self.current_op, self.current_raw
                ),
            );
        }
    }

    /// Render `trace_pc` call for a specific instruction (used for taken-inline branches).
    pub fn emit_trace_pc_for(&mut self, pc: u64, op: u16, raw: u32) {
        if self.config.has_tracing() {
            let pc_lit = Self::fmt_addr(pc);
            let state = self.state_ref();
            self.writeln(1, &format!("trace_pc(&{state}->tracer, {pc_lit}, {op});"));
            self.writeln(
                1,
                &format!("trace_opcode(&{state}->tracer, {pc_lit}, {op}, 0x{raw:x});"),
            );
        }
    }

    /// Render instret check and early suspend if needed.
    pub(crate) fn render_instret_check(&mut self, pc: u64) {
        if !self.config.instret_mode.suspends() {
            return;
        }
        let save_to_state = self.sig.save_to_state.clone();
        let pc_lit = Self::fmt_addr(pc);
        let state = self.state_ref();
        self.writeln(
            1,
            &format!("if (unlikely({state}->target_instret <= instret)) {{"),
        );
        self.writeln(2, &format!("{state}->pc = {pc_lit};"));
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
            BranchHint::Taken => format!("likely({cond})"),
            BranchHint::NotTaken => format!("unlikely({cond})"),
            BranchHint::None => cond.to_string(),
        };
        self.writeln(1, &format!("if ({cond_str}) {{"));
    }

    /// Render branch close for taken-inline: `}`
    pub fn render_branch_close(&mut self) {
        self.writeln(1, "}");
    }

    /// Render instruction with custom indent (for inlined blocks).
    pub(crate) fn render_instruction_indented(
        &mut self,
        ir: &InstrIR<X>,
        is_last: bool,
        fall_pc: u64,
        next_instr_pc: Option<u64>,
        indent: usize,
    ) {
        self.render_instruction_impl(ir, is_last, fall_pc, next_instr_pc, indent, true);
    }

    /// Render terminator with custom indent (simplified, no tracing).
    ///
    /// Used for inlined blocks in superblocks where branches are side-exits.
    pub(super) fn render_terminator_simple(
        &mut self,
        term: &Terminator<X>,
        fall_pc: u64,
        indent: usize,
    ) {
        match term {
            Terminator::Fall { target } => {
                let target_pc = target.map_or(fall_pc, |t| X::to_u64(t));
                self.render_jump_static_impl(target_pc, indent);
            }
            Terminator::Jump { target } => {
                self.render_jump_static_impl(X::to_u64(*target), indent);
            }
            Terminator::JumpDyn { addr, resolved } => {
                if let Some(targets) = resolved {
                    let target_addrs: Vec<u64> = targets.iter().map(|t| X::to_u64(*t)).collect();
                    self.render_jump_resolved_impl(&target_addrs, addr, indent);
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
                let fall_target = fall.map_or(fall_pc, |f| X::to_u64(f));
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
                self.writeln(indent, &format!("// TRAP: {message}"));
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
            BranchHint::Taken => format!("likely({cond})"),
            BranchHint::NotTaken => format!("unlikely({cond})"),
            BranchHint::None => cond.to_string(),
        };

        let args = self.sig.args.clone();
        let save_to_state = self.sig.save_to_state.clone();

        if self.is_valid_address(target) {
            let resolved = self.inputs.resolve_address(target);
            let pc_str = Self::fmt_pc(resolved);
            self.writeln(indent, &format!("if ({cond_str}) {{"));
            self.writeln(
                indent + 1,
                &format!("[[clang::musttail]] return B_{pc_str}({args});"),
            );
        } else {
            let state = self.state_ref();
            self.writeln(indent, &format!("if ({cond_str}) {{"));
            self.writeln(indent + 1, &format!("{state}->has_exited = true;"));
            self.writeln(indent + 1, &format!("{state}->exit_code = 1;"));
            let pc_lit = Self::fmt_addr(target);
            self.writeln(indent + 1, &format!("{state}->pc = {pc_lit};"));
            if !save_to_state.is_empty() {
                self.writeln(indent + 1, &save_to_state);
            }
            self.writeln(indent + 1, "return;");
        }
        self.writeln(indent, "}");
    }

    /// Render instret update with custom indent.
    pub fn render_instret_update_indented(&mut self, count: u64, indent: usize) {
        self.render_instret_update_impl(count, indent);
    }
}
