//! Block and instruction rendering for the C emitter.

use rvr_ir::{InstrIR, Terminator, Xlen};
use rvr_isa::op_mnemonic;

use super::CEmitter;

impl<X: Xlen> CEmitter<X> {
    // ============= Block rendering =============

    /// Render block header with optional block comment.
    pub fn render_block_header(&mut self, start_pc: u64, end_pc: u64) {
        self.render_block_header_with_count(start_pc, end_pc, 0);
    }

    /// Render block header with instruction count in comment.
    pub fn render_block_header_with_count(
        &mut self,
        start_pc: u64,
        end_pc: u64,
        instr_count: usize,
    ) {
        let pc_str = self.fmt_pc(start_pc);
        if self.config.emit_comments && instr_count > 0 {
            let start_comment = self.fmt_pc_comment(start_pc);
            let end_comment = self.fmt_pc_comment(end_pc.saturating_sub(1));
            self.write(&format!(
                "// Block: {}-{} ({} instrs)\n",
                start_comment, end_comment, instr_count
            ));
        }

        // Function attributes differ based on whether fixed addresses are used
        let attrs = if self.sig.fixed_addresses {
            // No nonnull since state/memory aren't pointer arguments
            "__attribute__((preserve_none))"
        } else {
            // nonnull(1) for state pointer (first argument)
            "__attribute__((preserve_none, nonnull(1)))"
        };

        self.write(&format!(
            "{} void B_{}({}) {{\n",
            attrs, pc_str, self.sig.params
        ));
    }

    /// Format PC for block names (hex without 0x prefix).
    pub(super) fn fmt_pc(&self, pc: u64) -> String {
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
    /// `next_instr_pc` is the PC of the next instruction in this block (for per-instruction checks).
    pub fn render_instruction(
        &mut self,
        ir: &InstrIR<X>,
        is_last: bool,
        fall_pc: u64,
        next_instr_pc: Option<u64>,
    ) {
        self.render_instruction_impl(ir, is_last, fall_pc, next_instr_pc, 1, false);
    }

    /// Render instruction with custom indent (for inlined blocks).
    ///
    /// When `use_simple_branch` is true, uses simplified branch rendering for superblock side-exits.
    /// `next_instr_pc` is the actual PC of the next instruction (for per-instruction checks in superblocks).
    pub(super) fn render_instruction_impl(
        &mut self,
        ir: &InstrIR<X>,
        is_last: bool,
        fall_pc: u64,
        next_instr_pc: Option<u64>,
        indent: usize,
        use_simple_branch: bool,
    ) {
        self.current_pc = X::to_u64(ir.pc);
        self.current_op = ir.op;
        self.current_raw = ir.raw;

        // Optional: emit comment with PC and instruction mnemonic
        if self.config.emit_comments {
            let pc_hex = self.fmt_pc_comment(self.current_pc);
            let mnemonic = op_mnemonic(ir.op).to_uppercase();

            // Include function name if available
            let comment = if let Some(ref loc) = ir.source_loc {
                if !loc.function.is_empty() {
                    format!("// PC: {} {}  @ {}", pc_hex, mnemonic, loc.function)
                } else {
                    format!("// PC: {} {}", pc_hex, mnemonic)
                }
            } else {
                format!("// PC: {} {}", pc_hex, mnemonic)
            };
            self.writeln(indent, &comment);
        }

        // Emit #line directive for source-level debugging
        if self.config.emit_line_info
            && let Some(ref loc) = ir.source_loc
            && loc.is_valid()
        {
            self.writeln(indent, &format!("#line {} \"{}\"", loc.line, loc.file));
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

        // Per-instruction instret check: update and potentially suspend after every instruction
        if self.config.instret_mode.per_instruction() {
            // Update instret by 1 for this instruction
            self.writeln(indent, "instret += 1;");
            // For non-last instructions, check suspension inline
            // For last instruction, the dispatch loop will check after the tail call returns
            if !is_last {
                // Use the actual next instruction PC if provided (for superblocks),
                // otherwise fall back to sequential PC
                let next_pc = next_instr_pc.unwrap_or_else(|| X::to_u64(ir.pc) + ir.size as u64);
                self.render_instret_check(next_pc);
            }
        }

        // Render terminator
        if is_last {
            // Update instret BEFORE the terminator (tail call) so the incremented value is passed
            // Skip bulk update if per-instruction mode (already updated above)
            if self.config.instret_mode.counts() && !self.config.instret_mode.per_instruction() {
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

}
