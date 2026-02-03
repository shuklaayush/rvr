use super::*;

pub(super) fn lift_c<X: Xlen>(
    args: &InstrArgs,
    opid: crate::OpId,
    pc: X::Reg,
    size: u8,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        // R-type
        OP_C_ADD => lift_r(args, Expr::add),
        OP_C_SUB => lift_r(args, Expr::sub),
        OP_C_XOR => lift_r(args, Expr::xor),
        OP_C_OR => lift_r(args, Expr::or),
        OP_C_AND => lift_r(args, Expr::and),
        OP_C_MV => lift_mv(args),
        OP_C_ADDW => lift_r(args, Expr::addw),
        OP_C_SUBW => lift_r(args, Expr::subw),

        // I-type
        OP_C_ADDI | OP_C_ADDI4SPN | OP_C_ADDI16SP => lift_i(args, Expr::add),
        OP_C_ADDIW => lift_i(args, Expr::addw),
        OP_C_LI => lift_i(args, |_, b| b),
        OP_C_ANDI => lift_i(args, Expr::and),
        OP_C_SLLI => lift_shamt(args, Expr::sll),
        OP_C_SRLI => lift_shamt(args, Expr::srl),
        OP_C_SRAI => lift_shamt(args, Expr::sra),

        // U-type
        OP_C_LUI => lift_lui(args),

        // Loads
        OP_C_LW | OP_C_LWSP => lift_load(args, 4, true),
        OP_C_LD | OP_C_LDSP => lift_load(args, 8, false),

        // Stores
        OP_C_SW | OP_C_SWSP => lift_store(args, 4),
        OP_C_SD | OP_C_SDSP => lift_store(args, 8),

        // Jumps
        OP_C_J => lift_j(args, pc),
        OP_C_JAL => lift_jal(args, pc, size),
        OP_C_JR => lift_jr(args),
        OP_C_JALR => lift_jalr(args, pc, size),

        // Branches
        OP_C_BEQZ => lift_beqz(args, pc),
        OP_C_BNEZ => lift_bnez(args, pc),

        // System
        OP_C_NOP => (Vec::new(), Terminator::Fall { target: None }),
        // c.ebreak = normal breakpoint/stop, exit_code = 0
        // For error exits, use ecall with exit syscall or unimp (illegal instruction)
        OP_C_EBREAK => (Vec::new(), Terminator::exit(Expr::imm(X::from_u64(0)))),

        // Invalid instruction - trap with exit
        OP_C_INVALID => (Vec::new(), Terminator::trap("invalid")),

        _ => (Vec::new(), Terminator::trap("unknown C instruction")),
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

fn lift_mv<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::R { rd, rs2, .. } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, Expr::read(*rs2))]
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
                    op(Expr::read(*rs1), Expr::imm(X::from_u64(*imm as i64 as u64))),
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
                    op(Expr::read(*rs1), Expr::imm(X::from_u64(*imm as u64 & 0x3F))),
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
                    Expr::imm(X::from_u64(*imm as i64 as u64)),
                )]
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
                let val = Expr::mem(Expr::read(*rs1), *imm as i16, width, signed);
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
            let offset = *imm as i16;
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

fn lift_j<X: Xlen>(args: &InstrArgs, pc: X::Reg) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::J { imm, .. } => {
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            (Vec::new(), Terminator::jump(X::from_u64(target)))
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
                    Expr::imm(pc + X::from_u64(size as u64)),
                ));
            }
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            (stmts, Terminator::jump(X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_jr<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rs1, .. } => {
            // Don't mask - return addresses from JAL/JALR are always aligned
            (Vec::new(), Terminator::jump_dyn(Expr::read(*rs1)))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_jalr<X: Xlen>(args: &InstrArgs, pc: X::Reg, size: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, .. } => {
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
                    Expr::imm(pc + X::from_u64(size as u64)),
                ));
            }
            // Don't mask - C.JALR has imm=0 and targets are always aligned
            (stmts, Terminator::jump_dyn(base))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_beqz<X: Xlen>(args: &InstrArgs, pc: X::Reg) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::B { rs1, imm, .. } => {
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            let cond = Expr::eq(Expr::read(*rs1), Expr::imm(X::from_u64(0)));
            (Vec::new(), Terminator::branch(cond, X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_bnez<X: Xlen>(args: &InstrArgs, pc: X::Reg) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::B { rs1, imm, .. } => {
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            let cond = Expr::ne(Expr::read(*rs1), Expr::imm(X::from_u64(0)));
            (Vec::new(), Terminator::branch(cond, X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}
