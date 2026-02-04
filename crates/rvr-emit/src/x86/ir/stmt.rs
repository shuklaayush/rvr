use rvr_ir::{Stmt, WriteTarget, Xlen};

use crate::x86::X86Emitter;
use crate::x86::registers::reserved;

impl<X: Xlen> X86Emitter<X> {
    /// Emit a statement.
    pub(super) fn emit_stmt(&mut self, stmt: &Stmt<X>) {
        let temp1 = Self::temp1();
        let temp2 = Self::temp2();
        let suffix = Self::suffix();

        match stmt {
            Stmt::Write { target, value } => {
                self.emit_write_stmt(target, value, temp1, temp2, suffix);
            }
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => self.emit_if_stmt(cond, then_stmts, else_stmts, temp1, suffix),
            Stmt::ExternCall { fn_name, args } => self.emit_extern_stmt(fn_name, args),
        }
    }

    fn emit_write_stmt(
        &mut self,
        target: &WriteTarget<X>,
        value: &rvr_ir::Expr<X>,
        temp1: &str,
        temp2: &str,
        suffix: &str,
    ) {
        match target {
            WriteTarget::Reg(reg) => self.emit_write_reg(*reg, value, temp1),
            WriteTarget::Mem {
                base,
                offset,
                width,
            } => self.emit_write_mem(base, *offset, *width, value, temp2, suffix),
            WriteTarget::Pc => self.emit_write_pc(value, temp1, suffix),
            WriteTarget::Exited => self.emit_write_exited(),
            WriteTarget::ExitCode => self.emit_write_exit_code(value, temp1),
            WriteTarget::Temp(idx) => self.emit_write_temp(*idx, value, temp1),
            WriteTarget::ResAddr => self.emit_write_reservation_addr(value, temp1),
            WriteTarget::ResValid => self.emit_write_reservation_valid(value, temp1),
            WriteTarget::Csr(_) => self.emit_comment(&format!("unsupported write: {target:?}")),
        }
    }

    fn emit_write_reg(&mut self, reg: u8, value: &rvr_ir::Expr<X>, temp1: &str) {
        if reg == 0 {
            return;
        }
        self.cold_cache_invalidate(reg);
        if let Some(x86_reg) = self.reg_map.get(reg) {
            let val_reg = self.emit_expr(value, x86_reg);
            if val_reg != x86_reg {
                if X::VALUE == 32 {
                    self.emitf(format!(
                        "movl %{}, %{}",
                        Self::reg_dword(&val_reg),
                        Self::reg_dword(x86_reg)
                    ));
                } else {
                    self.emitf(format!("movq %{val_reg}, %{x86_reg}"));
                }
            }
            self.emit_trace_reg_write(reg, &val_reg);
        } else {
            let val_reg = self.emit_expr(value, temp1);
            self.store_to_rv(reg, &val_reg);
            self.emit_trace_reg_write(reg, &val_reg);
        }
    }

    fn emit_write_mem(
        &mut self,
        base: &rvr_ir::Expr<X>,
        offset: i16,
        width: u8,
        value: &rvr_ir::Expr<X>,
        temp2: &str,
        suffix: &str,
    ) {
        let val_reg = self.emit_expr(value, temp2);
        if val_reg != temp2 && val_reg != "rcx" && val_reg != "ecx" {
            self.emitf(format!("mov{suffix} %{val_reg}, %{temp2}"));
        }
        self.emit_expr_as_addr(base);
        if offset != 0 {
            self.emitf(format!("leaq {offset}(%rax), %rax"));
        }
        self.apply_address_mode("rax");
        self.emit_trace_mem_access("rax", temp2, width, true);
        let mem = format!("(%{}, %rax)", reserved::MEMORY_PTR);
        let (sfx, reg) = match width {
            1 => ("b", "cl"),
            2 => ("w", "cx"),
            8 => ("q", "rcx"),
            _ => ("l", "ecx"),
        };
        self.emitf(format!("mov{sfx} %{reg}, {mem}"));
    }

    fn emit_write_pc(&mut self, value: &rvr_ir::Expr<X>, temp1: &str, suffix: &str) {
        let val_reg = self.emit_expr(value, temp1);
        let pc_off = self.layout.offset_pc;
        self.emitf(format!(
            "mov{suffix} %{val_reg}, {}(%{})",
            pc_off,
            reserved::STATE_PTR
        ));
    }

    fn emit_write_exited(&mut self) {
        let off = self.layout.offset_has_exited;
        self.emitf(format!("movb $1, {}(%{})", off, reserved::STATE_PTR));
    }

    fn emit_write_exit_code(&mut self, value: &rvr_ir::Expr<X>, temp1: &str) {
        let val_reg = self.emit_expr(value, temp1);
        let off = self.layout.offset_exit_code;
        self.emitf(format!(
            "movb %{}, {}(%{})",
            Self::reg_byte(&val_reg),
            off,
            reserved::STATE_PTR
        ));
    }

    fn emit_write_temp(&mut self, idx: u8, value: &rvr_ir::Expr<X>, temp1: &str) {
        let val_reg = self.emit_expr(value, temp1);
        if let Some(offset) = Self::temp_slot_offset(idx) {
            if X::VALUE == 32 {
                self.emitf(format!(
                    "movl %{}, {}(%rsp)",
                    Self::reg_dword(&val_reg),
                    offset
                ));
            } else {
                self.emitf(format!("movq %{val_reg}, {offset}(%rsp)"));
            }
        } else {
            self.emit_comment(&format!("temp {idx} out of range"));
        }
    }

    fn emit_write_reservation_addr(&mut self, value: &rvr_ir::Expr<X>, temp1: &str) {
        let val_reg = self.emit_expr(value, temp1);
        let off = self.layout.offset_reservation_addr;
        if X::VALUE == 32 {
            self.emitf(format!(
                "movl %{}, {}(%{})",
                Self::reg_dword(&val_reg),
                off,
                reserved::STATE_PTR
            ));
        } else {
            self.emitf(format!(
                "movq %{val_reg}, {}(%{})",
                off,
                reserved::STATE_PTR
            ));
        }
    }

    fn emit_write_reservation_valid(&mut self, value: &rvr_ir::Expr<X>, temp1: &str) {
        let val_reg = self.emit_expr(value, temp1);
        let off = self.layout.offset_reservation_valid;
        self.emitf(format!(
            "movb %{}, {}(%{})",
            Self::reg_byte(&val_reg),
            off,
            reserved::STATE_PTR
        ));
    }

    fn emit_if_stmt(
        &mut self,
        cond: &rvr_ir::Expr<X>,
        then_stmts: &[Stmt<X>],
        else_stmts: &[Stmt<X>],
        temp1: &str,
        suffix: &str,
    ) {
        let cond_reg = self.emit_expr(cond, temp1);
        self.emitf(format!("test{suffix} %{cond_reg}, %{cond_reg}"));
        let else_label = self.next_label("if_else");
        let end_label = self.next_label("if_end");
        self.emitf(format!("jz {else_label}"));
        for s in then_stmts {
            self.emit_stmt(s);
        }
        if !else_stmts.is_empty() {
            self.emitf(format!("jmp {end_label}"));
        }
        self.emit_label(&else_label);
        for s in else_stmts {
            self.emit_stmt(s);
        }
        if !else_stmts.is_empty() {
            self.emit_label(&end_label);
        }
    }

    fn emit_extern_stmt(&mut self, fn_name: &str, args: &[rvr_ir::Expr<X>]) {
        self.emit_comment(&format!("extern call: {fn_name}"));
        self.cold_cache = None;
        let _ = self.emit_extern_call(fn_name, args);
    }
}
