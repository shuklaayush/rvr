use rvr_ir::{Stmt, WriteTarget, Xlen};

use super::X86Emitter;
use crate::x86::registers::reserved;

impl<X: Xlen> X86Emitter<X> {
    /// Emit a statement.
    pub(super) fn emit_stmt(&mut self, stmt: &Stmt<X>) {
        let temp1 = self.temp1();
        let temp2 = self.temp2();
        let suffix = self.suffix();

        match stmt {
            Stmt::Write { target, value } => match target {
                WriteTarget::Reg(reg) => {
                    if *reg == 0 {
                        return;
                    }
                    self.cold_cache_invalidate(*reg);
                    if let Some(x86_reg) = self.reg_map.get(*reg) {
                        let val_reg = self.emit_expr(value, x86_reg);
                        if val_reg != x86_reg {
                            if X::VALUE == 32 {
                                self.emitf(format!(
                                    "movl %{}, %{}",
                                    self.reg_dword(&val_reg),
                                    self.reg_dword(x86_reg)
                                ));
                            } else {
                                self.emitf(format!("movq %{val_reg}, %{x86_reg}"));
                            }
                        }
                        self.emit_trace_reg_write(*reg, &val_reg);
                    } else {
                        let val_reg = self.emit_expr(value, temp1);
                        self.store_to_rv(*reg, &val_reg);
                        self.emit_trace_reg_write(*reg, &val_reg);
                    }
                }
                WriteTarget::Mem {
                    base,
                    offset,
                    width,
                } => {
                    let val_reg = self.emit_expr(value, temp2);
                    if val_reg != temp2 && val_reg != "rcx" && val_reg != "ecx" {
                        self.emitf(format!("mov{suffix} %{val_reg}, %{temp2}"));
                    }
                    self.emit_expr_as_addr(base);
                    if *offset != 0 {
                        self.emitf(format!("leaq {offset}(%rax), %rax"));
                    }
                    self.apply_address_mode("rax");
                    self.emit_trace_mem_access("rax", temp2, *width, true);
                    let mem = format!("(%{}, %rax)", reserved::MEMORY_PTR);
                    let (sfx, reg) = match width {
                        1 => ("b", "cl"),
                        2 => ("w", "cx"),
                        4 => ("l", "ecx"),
                        8 => ("q", "rcx"),
                        _ => ("l", "ecx"),
                    };
                    self.emitf(format!("mov{sfx} %{reg}, {mem}"));
                }
                WriteTarget::Pc => {
                    let val_reg = self.emit_expr(value, temp1);
                    let pc_off = self.layout.offset_pc;
                    self.emitf(format!(
                        "mov{suffix} %{val_reg}, {}(%{})",
                        pc_off,
                        reserved::STATE_PTR
                    ));
                }
                WriteTarget::Exited => {
                    let off = self.layout.offset_has_exited;
                    self.emitf(format!("movb $1, {}(%{})", off, reserved::STATE_PTR));
                }
                WriteTarget::ExitCode => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_exit_code;
                    self.emitf(format!(
                        "movb %{}, {}(%{})",
                        self.reg_byte(&val_reg),
                        off,
                        reserved::STATE_PTR
                    ));
                }
                WriteTarget::Temp(idx) => {
                    let val_reg = self.emit_expr(value, temp1);
                    if let Some(offset) = self.temp_slot_offset(*idx) {
                        if X::VALUE == 32 {
                            self.emitf(format!(
                                "movl %{}, {}(%rsp)",
                                self.reg_dword(&val_reg),
                                offset
                            ));
                        } else {
                            self.emitf(format!("movq %{val_reg}, {}(%rsp)", offset));
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
                            "movl %{}, {}(%{})",
                            self.reg_dword(&val_reg),
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
                WriteTarget::ResValid => {
                    let val_reg = self.emit_expr(value, temp1);
                    let off = self.layout.offset_reservation_valid;
                    self.emitf(format!(
                        "movb %{}, {}(%{})",
                        self.reg_byte(&val_reg),
                        off,
                        reserved::STATE_PTR
                    ));
                }
                _ => self.emit_comment(&format!("unsupported write: {:?}", target)),
            },
            Stmt::If {
                cond,
                then_stmts,
                else_stmts,
            } => {
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
            Stmt::ExternCall { fn_name, args } => {
                self.emit_comment(&format!("extern call: {fn_name}"));
                self.cold_cache = None;
                let _ = self.emit_extern_call(fn_name, args);
            }
        }
    }
}
