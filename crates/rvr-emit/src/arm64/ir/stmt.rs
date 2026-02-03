use rvr_ir::{Stmt, WriteTarget, Xlen};

use crate::arm64::Arm64Emitter;
use crate::arm64::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
    /// Emit a statement.
    pub(super) fn emit_stmt(&mut self, stmt: &Stmt<X>) {
        let temp1 = self.temp1();
        let temp2 = self.temp2();

        match stmt {
            Stmt::Write { target, value } => match target {
                WriteTarget::Reg(reg) => {
                    if *reg == 0 {
                        return;
                    }
                    // Note: cold_cache_invalidate must be called AFTER emit_expr, not before.
                    // emit_expr may populate the cold cache when evaluating Read(Reg) in the
                    // expression. If we invalidate before, the cache gets re-populated with
                    // the old value being read, and remains stale after the write completes.
                    if let Some(arm_reg) = self.reg_map.get(*reg) {
                        let val_reg = self.emit_expr(value, arm_reg);
                        if val_reg != arm_reg {
                            self.emitf(format!("mov {arm_reg}, {val_reg}"));
                        }
                        self.emit_trace_reg_write(*reg, &val_reg);
                    } else {
                        let val_reg = self.emit_expr(value, temp1);
                        self.store_to_rv(*reg, &val_reg);
                        self.emit_trace_reg_write(*reg, &val_reg);
                    }
                    self.cold_cache_invalidate(*reg);
                }
                WriteTarget::Mem {
                    base,
                    offset,
                    width,
                } => {
                    // Check if HTIF handling might be needed for this store
                    let htif_possible = self.config.htif_enabled && (*width == 4 || *width == 8);

                    // First evaluate value to temp2 (x1)
                    let val_reg = self.emit_expr(value, temp2);
                    // Check if val_reg is temp2 (x1/w1) - use exact match
                    let is_temp2 = val_reg == "x1" || val_reg == "w1";
                    let is_temp1 = val_reg == "x0" || val_reg == "w0";
                    let store_reg: String;

                    if is_temp2 {
                        // Value is already in x1
                        store_reg = temp2.to_string();
                    } else if is_temp1 {
                        // Value is in x0, need to save it before address calc
                        self.emitf(format!("mov {temp2}, {val_reg}"));
                        store_reg = temp2.to_string();
                    } else {
                        // Value is in a hot register
                        if htif_possible {
                            // For HTIF check, we need the value in x1
                            let reg64 = self.reg_64(&val_reg);
                            self.emitf(format!("mov x1, {reg64}"));
                            store_reg = "x1".to_string();
                        } else {
                            // No HTIF, can store directly from hot register
                            store_reg = val_reg.clone();
                        }
                    }

                    // Then evaluate address (virtual)
                    let base_reg = self.emit_expr_as_addr(base);
                    if *offset != 0 {
                        self.emit_add_offset("x0", &base_reg, (*offset).into());
                    } else if base_reg != "x0" {
                        self.emitf(format!("mov x0, {base_reg}"));
                    }

                    // HTIF handling: check for tohost write
                    let htif_done_label = if htif_possible {
                        Some(self.emit_htif_check())
                    } else {
                        None
                    };

                    // Translate to physical address for the actual store
                    self.apply_address_mode("x0");

                    self.emit_trace_mem_access("x0", &store_reg, *width, true);

                    // Store
                    let val32 = self.reg_32(&store_reg);
                    let mem = format!("{}, x0", reserved::MEMORY_PTR);
                    match width {
                        1 => self.emitf(format!("strb {val32}, [{mem}]")),
                        2 => self.emitf(format!("strh {val32}, [{mem}]")),
                        4 => self.emitf(format!("str {val32}, [{mem}]")),
                        8 => {
                            let reg64 = self.reg_64(&store_reg);
                            self.emitf(format!("str {reg64}, [{mem}]"))
                        }
                        _ => self.emitf(format!("str {val32}, [{mem}]")),
                    }

                    // Emit the done label for HTIF syscall handling
                    if let Some(label) = htif_done_label {
                        self.emit_label(&label);
                    }
                }
                WriteTarget::Pc => {
                    let val_reg = self.emit_expr(value, temp1);
                    let pc_off = self.layout.offset_pc;
                    self.emitf(format!(
                        "str {val_reg}, [{}, #{}]",
                        reserved::STATE_PTR,
                        pc_off
                    ));
                }
                WriteTarget::Exited => {
                    let off = self.layout.offset_has_exited;
                    self.emit("mov w0, #1");
                    self.emitf(format!("strb w0, [{}, #{}]", reserved::STATE_PTR, off));
                }
                WriteTarget::ExitCode => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_exit_code;
                    self.emitf(format!(
                        "strb {}, [{}, #{}]",
                        self.reg_32(&val_reg),
                        reserved::STATE_PTR,
                        off
                    ));
                }
                WriteTarget::Temp(idx) => {
                    let val_reg = self.emit_expr(value, temp1);
                    if let Some(offset) = self.temp_slot_offset(*idx) {
                        if X::VALUE == 32 {
                            self.emitf(format!("str {}, [sp, #{}]", self.reg_32(&val_reg), offset));
                        } else {
                            self.emitf(format!("str {val_reg}, [sp, #{}]", offset));
                        }
                    } else {
                        self.emit_comment(&format!("temp {} out of range", idx));
                    }
                }
                WriteTarget::ResAddr => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_reservation_addr;
                    if X::VALUE == 32 {
                        self.emitf(format!(
                            "str {}, [{}, #{}]",
                            self.reg_32(&val_reg),
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
                WriteTarget::ResValid => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_reservation_valid;
                    self.emitf(format!(
                        "strb {}, [{}, #{}]",
                        self.reg_32(&val_reg),
                        reserved::STATE_PTR,
                        off
                    ));
                }
                _ => self.emit_comment(&format!("unsupported write: {:?}", target)),
            },
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => {
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
            Stmt::ExternCall { fn_name, args } => {
                self.emit_comment(&format!("extern call: {fn_name}"));
                self.cold_cache = None;
                let _ = self.emit_extern_call(fn_name, args);
            }
        }
    }
}
