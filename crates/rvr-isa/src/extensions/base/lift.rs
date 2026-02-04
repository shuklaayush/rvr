use super::{
    Expr, InstrArgs, OP_ADD, OP_ADDI, OP_ADDIW, OP_ADDW, OP_AND, OP_ANDI, OP_AUIPC, OP_BEQ, OP_BGE,
    OP_BGEU, OP_BLT, OP_BLTU, OP_BNE, OP_EBREAK, OP_ECALL, OP_FENCE, OP_JAL, OP_JALR, OP_LB,
    OP_LBU, OP_LD, OP_LH, OP_LHU, OP_LUI, OP_LW, OP_LWU, OP_MRET, OP_OR, OP_ORI, OP_SB, OP_SD,
    OP_SH, OP_SLL, OP_SLLI, OP_SLLIW, OP_SLLW, OP_SLT, OP_SLTI, OP_SLTIU, OP_SLTU, OP_SRA, OP_SRAI,
    OP_SRAIW, OP_SRAW, OP_SRL, OP_SRLI, OP_SRLIW, OP_SRLW, OP_SUB, OP_SUBW, OP_SW, OP_XOR, OP_XORI,
    OpId, Stmt, Terminator, Xlen,
};

pub(super) fn lift_base<X: Xlen>(
    args: &InstrArgs,
    opid: OpId,
    pc: X::Reg,
    size: u8,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        OP_ADD => lift_r(args, Expr::add),
        OP_SUB => lift_r(args, Expr::sub),
        OP_SLL => lift_shift(args, Expr::sll),
        OP_SLT => lift_r(args, Expr::slt),
        OP_SLTU => lift_r(args, Expr::sltu),
        OP_XOR => lift_r(args, Expr::xor),
        OP_SRL => lift_shift(args, Expr::srl),
        OP_SRA => lift_shift(args, Expr::sra),
        OP_OR => lift_r(args, Expr::or),
        OP_AND => lift_r(args, Expr::and),

        OP_ADDI => lift_i(args, Expr::add),
        OP_SLTI => lift_i(args, Expr::slt),
        OP_SLTIU => lift_i(args, Expr::sltu),
        OP_XORI => lift_i(args, Expr::xor),
        OP_ORI => lift_i(args, Expr::or),
        OP_ANDI => lift_i(args, Expr::and),
        OP_SLLI => lift_shamt(args, Expr::sll),
        OP_SRLI => lift_shamt(args, Expr::srl),
        OP_SRAI => lift_shamt(args, Expr::sra),

        OP_ADDW => lift_r(args, Expr::addw),
        OP_SUBW => lift_r(args, Expr::subw),
        OP_SLLW => lift_shiftw(args, Expr::sllw),
        OP_SRLW => lift_shiftw(args, Expr::srlw),
        OP_SRAW => lift_shiftw(args, Expr::sraw),
        OP_ADDIW => lift_i(args, Expr::addw),
        OP_SLLIW => lift_shamt(args, Expr::sllw),
        OP_SRLIW => lift_shamt(args, Expr::srlw),
        OP_SRAIW => lift_shamt(args, Expr::sraw),

        OP_LUI => lift_lui(args),
        OP_AUIPC => lift_auipc(args, pc),

        OP_LB => lift_load(args, 1, true),
        OP_LH => lift_load(args, 2, true),
        OP_LW => lift_load(args, 4, true),
        OP_LD => lift_load(args, 8, false),
        OP_LBU => lift_load(args, 1, false),
        OP_LHU => lift_load(args, 2, false),
        OP_LWU => lift_load(args, 4, false),

        OP_SB => lift_store(args, 1),
        OP_SH => lift_store(args, 2),
        OP_SW => lift_store(args, 4),
        OP_SD => lift_store(args, 8),

        OP_JAL => lift_jal(args, pc, size),
        OP_JALR => lift_jalr(args, pc, size),

        OP_BEQ => lift_branch(args, pc, |a, b| Expr::eq(a, b)),
        OP_BNE => lift_branch(args, pc, |a, b| Expr::ne(a, b)),
        OP_BLT => lift_branch(args, pc, |a, b| Expr::lt(a, b)),
        OP_BGE => lift_branch(args, pc, |a, b| Expr::ge(a, b)),
        OP_BLTU => lift_branch(args, pc, |a, b| Expr::ltu(a, b)),
        OP_BGEU => lift_branch(args, pc, |a, b| Expr::geu(a, b)),

        // ECALL is handled by ExtensionRegistry's syscall_handler.
        // This fallback should not normally be reached.
        OP_ECALL => (Vec::new(), Terminator::trap("ecall: use ExtensionRegistry")),
        // ebreak = normal breakpoint/stop, exit_code = 0
        // For error exits, use ecall with exit syscall or unimp (illegal instruction)
        OP_EBREAK => (Vec::new(), Terminator::exit(Expr::imm(X::from_u64(0)))),
        OP_FENCE | OP_MRET => (Vec::new(), Terminator::Fall { target: None }),

        _ => (Vec::new(), Terminator::trap("unknown base instruction")),
    }
}

fn lift_r<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::R { rd, rs1, rs2 } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), Expr::read(*rs2)))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

/// Lift R-type shift: masks rs2 by XLEN-1 per RISC-V spec.
fn lift_shift<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::R { rd, rs1, rs2 } => {
            let mask = X::from_u64(u64::from(X::VALUE) - 1);
            let masked_shamt = Expr::and(Expr::read(*rs2), Expr::imm(mask));
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), masked_shamt))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

/// Lift R-type 32-bit shift (RV64 only): masks rs2 by 0x1f per RISC-V spec.
fn lift_shiftw<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::R { rd, rs1, rs2 } => {
            let mask = X::from_u64(0x1f);
            let masked_shamt = Expr::and(Expr::read(*rs2), Expr::imm(mask));
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), masked_shamt))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_i<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(
                    *rd,
                    op(
                        Expr::read(*rs1),
                        Expr::imm(X::from_u64(i64::from(*imm).cast_unsigned())),
                    ),
                )]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_shamt<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(
                    *rd,
                    op(
                        Expr::read(*rs1),
                        Expr::imm(X::from_u64(u64::from(imm.cast_unsigned() & 0x3F))),
                    ),
                )]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_lui<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::U { rd, imm } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(
                    *rd,
                    Expr::imm(X::from_u64(i64::from(*imm).cast_unsigned())),
                )]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_auipc<X: Xlen>(args: &InstrArgs, pc: X::Reg) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::U { rd, imm } => {
            let stmts = if *rd != 0 {
                // Pre-fold pc + imm at lift time since both are constants
                let pc_val = X::to_u64(pc).cast_signed();
                let imm_val = i64::from(*imm);
                let result = X::from_u64((pc_val + imm_val).cast_unsigned());
                vec![Stmt::write_reg(*rd, Expr::imm(result))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_load<X: Xlen>(args: &InstrArgs, width: u8, signed: bool) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                // Keep base and offset separate for better codegen
                let offset = i16::try_from(*imm).expect("load offset fits i16");
                let val = Expr::mem(Expr::read(*rs1), offset, width, signed);
                vec![Stmt::write_reg(*rd, val)]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_store<X: Xlen>(args: &InstrArgs, width: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::S { rs1, rs2, imm } => {
            let base = Expr::read(*rs1);
            let offset = i16::try_from(*imm).expect("store offset fits i16");
            (
                vec![
                    Stmt::write_mem(base, offset, Expr::read(*rs2), width),
                    // Clear reservation on any store (spurious failure is allowed).
                    Stmt::write_res_valid(Expr::imm(X::from_u64(0))),
                ],
                Terminator::Fall { target: None },
            )
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_jal<X: Xlen>(args: &InstrArgs, pc: X::Reg, size: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::J { rd, imm } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                stmts.push(Stmt::write_reg(
                    *rd,
                    Expr::imm(pc + X::from_u64(u64::from(size))),
                ));
            }
            let offset = X::to_u64(X::sign_extend_32(imm.cast_unsigned())).cast_signed();
            let target = (X::to_u64(pc).cast_signed() + offset).cast_unsigned();
            (stmts, Terminator::jump(X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_jalr<X: Xlen>(args: &InstrArgs, pc: X::Reg, size: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let mut stmts = Vec::new();
            let base = if rd == rs1 {
                stmts.push(Stmt::write_temp(0, Expr::read(*rs1)));
                Expr::temp(0)
            } else {
                Expr::read(*rs1)
            };
            if *rd != 0 {
                stmts.push(Stmt::write_reg(
                    *rd,
                    Expr::imm(pc + X::from_u64(u64::from(size))),
                ));
            }
            // Clear low bit for 2-byte alignment (use !1 to get correct mask for XLEN)
            // Note: add(base, imm(0)) is optimized to just base by Expr::add
            let target = Expr::and(
                Expr::add(
                    base,
                    Expr::imm(X::from_u64(i64::from(*imm).cast_unsigned())),
                ),
                Expr::imm(X::from_u64(!1u64)),
            );
            (stmts, Terminator::jump_dyn(target))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_branch<X: Xlen, F>(args: &InstrArgs, pc: X::Reg, cond: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::B { rs1, rs2, imm } => {
            let offset = X::to_u64(X::sign_extend_32(imm.cast_unsigned())).cast_signed();
            let target = (X::to_u64(pc).cast_signed() + offset).cast_unsigned();
            (
                Vec::new(),
                Terminator::branch(
                    cond(Expr::read(*rs1), Expr::read(*rs2)),
                    X::from_u64(target),
                ),
            )
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

// ===== Disasm =====
