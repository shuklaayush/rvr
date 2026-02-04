use rvr_ir::{Stmt, WriteTarget, Xlen};

use crate::arm64::Arm64Emitter;
use crate::arm64::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
    /// Emit a statement.
    pub(super) fn emit_stmt(&mut self, stmt: &Stmt<X>) {
        match stmt {
            Stmt::Write { target, value } => self.emit_write_stmt(target, value),
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => self.emit_if_stmt(cond, then_stmts, else_stmts),
            Stmt::ExternCall { fn_name, args } => self.emit_extern_stmt(fn_name, args),
        }
    }

    fn emit_write_stmt(&mut self, target: &WriteTarget<X>, value: &rvr_ir::Expr<X>) {
        match target {
            WriteTarget::Reg(reg) => self.emit_write_reg(*reg, value),
            WriteTarget::Mem {
                base,
                offset,
                width,
            } => self.emit_write_mem(base, *offset, *width, value),
            WriteTarget::Pc => self.emit_write_pc(value),
            WriteTarget::Exited => self.emit_write_exited(),
            WriteTarget::ExitCode => self.emit_write_exit_code(value),
            WriteTarget::Temp(idx) => self.emit_write_temp(*idx, value),
            WriteTarget::ResAddr => self.emit_write_res_addr(value),
            WriteTarget::ResValid => self.emit_write_res_valid(value),
            WriteTarget::Csr(_) => self.emit_comment(&format!("unsupported write: {target:?}")),
        }
    }

    fn emit_write_reg(&mut self, reg: u8, value: &rvr_ir::Expr<X>) {
        if reg == 0 {
            return;
        }
        let temp1 = Self::temp1();
        if let Some(arm_reg) = self.reg_map.get(reg) {
            let val_reg = self.emit_expr(value, arm_reg);
            if val_reg != arm_reg {
                self.emitf(format!("mov {arm_reg}, {val_reg}"));
            }
            self.emit_trace_reg_write(reg, &val_reg);
        } else {
            let val_reg = self.emit_expr(value, temp1);
            self.store_to_rv(reg, &val_reg);
            self.emit_trace_reg_write(reg, &val_reg);
        }
        self.cold_cache_invalidate(reg);
    }

    fn emit_write_mem(
        &mut self,
        base: &rvr_ir::Expr<X>,
        offset: i16,
        width: u8,
        value: &rvr_ir::Expr<X>,
    ) {
        let temp2 = Self::temp2();
        let htif_possible = self.config.htif_enabled() && (width == 4 || width == 8);

        let val_reg = self.emit_expr(value, temp2);
        let is_temp2 = val_reg == "x1" || val_reg == "w1";
        let is_temp1 = val_reg == "x0" || val_reg == "w0";
        let store_reg = if is_temp2 {
            temp2.to_string()
        } else if is_temp1 {
            self.emitf(format!("mov {temp2}, {val_reg}"));
            temp2.to_string()
        } else if htif_possible {
            let reg64 = Self::reg_64(&val_reg);
            self.emitf(format!("mov x1, {reg64}"));
            "x1".to_string()
        } else {
            val_reg
        };

        let base_reg = self.emit_expr_as_addr(base);
        if offset != 0 {
            self.emit_add_offset("x0", &base_reg, offset.into());
        } else if base_reg != "x0" {
            self.emitf(format!("mov x0, {base_reg}"));
        }

        let htif_done_label = if htif_possible {
            Some(self.emit_htif_check())
        } else {
            None
        };

        self.apply_address_mode("x0");
        self.emit_trace_mem_access("x0", &store_reg, width, true);

        let val32 = Self::reg_32(&store_reg);
        let mem = format!("{}, x0", reserved::MEMORY_PTR);
        match width {
            1 => self.emitf(format!("strb {val32}, [{mem}]")),
            2 => self.emitf(format!("strh {val32}, [{mem}]")),
            8 => {
                let reg64 = Self::reg_64(&store_reg);
                self.emitf(format!("str {reg64}, [{mem}]"));
            }
            _ => self.emitf(format!("str {val32}, [{mem}]")),
        }

        if let Some(label) = htif_done_label {
            self.emit_label(&label);
        }
    }

    fn emit_write_pc(&mut self, value: &rvr_ir::Expr<X>) {
        let temp1 = Self::temp1();
        let val_reg = self.emit_expr(value, temp1);
        let pc_off = self.layout.offset_pc;
        self.emitf(format!(
            "str {val_reg}, [{}, #{}]",
            reserved::STATE_PTR,
            pc_off
        ));
    }

    fn emit_write_exited(&mut self) {
        let off = self.layout.offset_has_exited;
        self.emit("mov w0, #1");
        self.emitf(format!("strb w0, [{}, #{}]", reserved::STATE_PTR, off));
    }

    fn emit_write_exit_code(&mut self, value: &rvr_ir::Expr<X>) {
        let temp1 = Self::temp1();
        let val_reg = self.emit_expr(value, temp1);
        let off = self.layout.offset_exit_code;
        self.emitf(format!(
            "strb {}, [{}, #{}]",
            Self::reg_32(&val_reg),
            reserved::STATE_PTR,
            off
        ));
    }

    fn emit_write_temp(&mut self, idx: u8, value: &rvr_ir::Expr<X>) {
        let temp1 = Self::temp1();
        let val_reg = self.emit_expr(value, temp1);
        if let Some(offset) = Self::temp_slot_offset(idx) {
            if X::VALUE == 32 {
                self.emitf(format!("str {}, [sp, #{}]", Self::reg_32(&val_reg), offset));
            } else {
                self.emitf(format!("str {val_reg}, [sp, #{offset}]"));
            }
        } else {
            self.emit_comment(&format!("temp {idx} out of range"));
        }
    }

    fn emit_write_res_addr(&mut self, value: &rvr_ir::Expr<X>) {
        let temp1 = Self::temp1();
        let val_reg = self.emit_expr(value, temp1);
        let off = self.layout.offset_reservation_addr;
        if X::VALUE == 32 {
            self.emitf(format!(
                "str {}, [{}, #{}]",
                Self::reg_32(&val_reg),
                reserved::STATE_PTR,
                off
            ));
        } else {
            self.emitf(format!(
                "str {val_reg}, [{}, #{}]",
                reserved::STATE_PTR,
                off
            ));
        }
    }

    fn emit_write_res_valid(&mut self, value: &rvr_ir::Expr<X>) {
        let temp1 = Self::temp1();
        let val_reg = self.emit_expr(value, temp1);
        let off = self.layout.offset_reservation_valid;
        self.emitf(format!(
            "strb {}, [{}, #{}]",
            Self::reg_32(&val_reg),
            reserved::STATE_PTR,
            off
        ));
    }

    fn emit_if_stmt(
        &mut self,
        cond: &rvr_ir::Expr<X>,
        then_stmts: &[Stmt<X>],
        else_stmts: &[Stmt<X>],
    ) {
        let temp1 = Self::temp1();
        let else_label = self.next_label("if_else");
        let end_label = self.next_label("if_end");
        if !self.try_emit_compare_branch(cond, &else_label, true) {
            let cond_reg = self.emit_expr(cond, temp1);
            self.emitf(format!("cbz {cond_reg}, {else_label}"));
        }
        for s in then_stmts {
            self.emit_stmt(s);
        }
        if !else_stmts.is_empty() {
            self.emitf(format!("b {end_label}"));
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
