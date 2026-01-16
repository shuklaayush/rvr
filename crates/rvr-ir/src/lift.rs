//! Instruction lifting (decode → IR).
//!
//! Converts decoded RISC-V instructions to their IR representation.

use rvr_isa::{
    DecodedInstr, InstrArgs, OpId, Xlen,
    // Base instructions
    OP_ADD, OP_SUB, OP_SLL, OP_SLT, OP_SLTU, OP_XOR, OP_SRL, OP_SRA, OP_OR, OP_AND,
    OP_ADDI, OP_SLTI, OP_SLTIU, OP_XORI, OP_ORI, OP_ANDI, OP_SLLI, OP_SRLI, OP_SRAI,
    OP_LUI, OP_AUIPC, OP_JAL, OP_JALR,
    OP_BEQ, OP_BNE, OP_BLT, OP_BGE, OP_BLTU, OP_BGEU,
    OP_LB, OP_LH, OP_LW, OP_LBU, OP_LHU, OP_SB, OP_SH, OP_SW,
    OP_LD, OP_LWU, OP_SD,
    OP_ADDW, OP_SUBW, OP_SLLW, OP_SRLW, OP_SRAW, OP_ADDIW, OP_SLLIW, OP_SRLIW, OP_SRAIW,
    OP_FENCE, OP_FENCE_I, OP_ECALL, OP_EBREAK,
    // M extension
    OP_MUL, OP_MULH, OP_MULHSU, OP_MULHU, OP_DIV, OP_DIVU, OP_REM, OP_REMU,
    OP_MULW, OP_DIVW, OP_DIVUW, OP_REMW, OP_REMUW,
    // A extension
    OP_LR_W, OP_SC_W, OP_AMOSWAP_W, OP_AMOADD_W, OP_AMOXOR_W, OP_AMOAND_W, OP_AMOOR_W,
    OP_AMOMIN_W, OP_AMOMAX_W, OP_AMOMINU_W, OP_AMOMAXU_W,
    OP_LR_D, OP_SC_D, OP_AMOSWAP_D, OP_AMOADD_D, OP_AMOXOR_D, OP_AMOAND_D, OP_AMOOR_D,
    OP_AMOMIN_D, OP_AMOMAX_D, OP_AMOMINU_D, OP_AMOMAXU_D,
    // C extension
    OP_C_ADD, OP_C_SUB, OP_C_XOR, OP_C_OR, OP_C_AND, OP_C_MV,
    OP_C_ADDW, OP_C_SUBW,
    OP_C_ADDI, OP_C_ADDI4SPN, OP_C_ADDI16SP, OP_C_ADDIW, OP_C_LI, OP_C_ANDI,
    OP_C_SLLI, OP_C_SRLI, OP_C_SRAI,
    OP_C_LUI,
    OP_C_LW, OP_C_LD, OP_C_LWSP, OP_C_LDSP,
    OP_C_SW, OP_C_SD, OP_C_SWSP, OP_C_SDSP,
    OP_C_J, OP_C_JAL, OP_C_JR, OP_C_JALR,
    OP_C_BEQZ, OP_C_BNEZ,
    OP_C_NOP, OP_C_EBREAK,
    // Zicsr extension
    OP_CSRRW, OP_CSRRS, OP_CSRRC, OP_CSRRWI, OP_CSRRSI, OP_CSRRCI,
};

use crate::{
    expr::Expr, stmt::Stmt, terminator::Terminator, instr::InstrIR,
};

/// Lift a decoded instruction to IR.
pub fn lift<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let pc = instr.pc;
    let size = instr.size;
    let next_pc = pc + X::from_u64(size as u64);

    // Get statements and terminator based on opcode
    let (stmts, term) = lift_opid::<X>(instr.opid, &instr.args, pc, size, next_pc);

    InstrIR::new(pc, size, instr.opid, stmts, term)
}

/// Lift based on OpId.
fn lift_opid<X: Xlen>(
    opid: OpId,
    args: &InstrArgs,
    pc: X::Reg,
    size: u8,
    next_pc: X::Reg,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    use rvr_isa::EXT_I;
    use rvr_isa::EXT_M;
    use rvr_isa::EXT_A;
    use rvr_isa::EXT_C;
    use rvr_isa::EXT_ZICSR;

    match opid.ext {
        EXT_I => lift_base(opid, args, pc, size, next_pc),
        EXT_M => lift_m(opid, args, pc, next_pc),
        EXT_A => lift_a(opid, args, next_pc),
        EXT_C => lift_c::<X>(opid, args, pc, size, next_pc),
        EXT_ZICSR => lift_zicsr(opid, args, next_pc),
        _ => (Vec::new(), Terminator::trap("unknown extension")),
    }
}

/// Lift base I extension instruction.
fn lift_base<X: Xlen>(
    opid: OpId,
    args: &InstrArgs,
    pc: X::Reg,
    size: u8,
    next_pc: X::Reg,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        // Arithmetic
        OP_ADD => lift_r_type(args, |rs1, rs2| Expr::add(rs1, rs2)),
        OP_SUB => lift_r_type(args, |rs1, rs2| Expr::sub(rs1, rs2)),
        OP_SLL => lift_r_type(args, |rs1, rs2| Expr::sll(rs1, rs2)),
        OP_SLT => lift_r_type(args, |rs1, rs2| Expr::slt(rs1, rs2)),
        OP_SLTU => lift_r_type(args, |rs1, rs2| Expr::sltu(rs1, rs2)),
        OP_XOR => lift_r_type(args, |rs1, rs2| Expr::xor(rs1, rs2)),
        OP_SRL => lift_r_type(args, |rs1, rs2| Expr::srl(rs1, rs2)),
        OP_SRA => lift_r_type(args, |rs1, rs2| Expr::sra(rs1, rs2)),
        OP_OR => lift_r_type(args, |rs1, rs2| Expr::or(rs1, rs2)),
        OP_AND => lift_r_type(args, |rs1, rs2| Expr::and(rs1, rs2)),

        // Immediate arithmetic
        OP_ADDI => lift_i_arith(args, |rs1, imm| Expr::add(rs1, imm)),
        OP_SLTI => lift_i_arith(args, |rs1, imm| Expr::slt(rs1, imm)),
        OP_SLTIU => lift_i_arith(args, |rs1, imm| Expr::sltu(rs1, imm)),
        OP_XORI => lift_i_arith(args, |rs1, imm| Expr::xor(rs1, imm)),
        OP_ORI => lift_i_arith(args, |rs1, imm| Expr::or(rs1, imm)),
        OP_ANDI => lift_i_arith(args, |rs1, imm| Expr::and(rs1, imm)),
        OP_SLLI => lift_shift_imm(args, |rs1, shamt| Expr::sll(rs1, shamt)),
        OP_SRLI => lift_shift_imm(args, |rs1, shamt| Expr::srl(rs1, shamt)),
        OP_SRAI => lift_shift_imm(args, |rs1, shamt| Expr::sra(rs1, shamt)),

        // RV64I W-suffix ops
        OP_ADDW => lift_r_type(args, |rs1, rs2| Expr::addw(rs1, rs2)),
        OP_SUBW => lift_r_type(args, |rs1, rs2| Expr::subw(rs1, rs2)),
        OP_SLLW => lift_r_type(args, |rs1, rs2| Expr::sllw(rs1, rs2)),
        OP_SRLW => lift_r_type(args, |rs1, rs2| Expr::srlw(rs1, rs2)),
        OP_SRAW => lift_r_type(args, |rs1, rs2| Expr::sraw(rs1, rs2)),
        OP_ADDIW => lift_i_arith(args, |rs1, imm| Expr::addw(rs1, imm)),
        OP_SLLIW => lift_shift_imm(args, |rs1, shamt| Expr::sllw(rs1, shamt)),
        OP_SRLIW => lift_shift_imm(args, |rs1, shamt| Expr::srlw(rs1, shamt)),
        OP_SRAIW => lift_shift_imm(args, |rs1, shamt| Expr::sraw(rs1, shamt)),

        // Upper immediate
        OP_LUI => lift_lui(args),
        OP_AUIPC => lift_auipc(args, pc),

        // Loads
        OP_LB => lift_load(args, 1, true),
        OP_LH => lift_load(args, 2, true),
        OP_LW => lift_load(args, 4, true),
        OP_LD => lift_load(args, 8, false),
        OP_LBU => lift_load(args, 1, false),
        OP_LHU => lift_load(args, 2, false),
        OP_LWU => lift_load(args, 4, false),

        // Stores
        OP_SB => lift_store(args, 1),
        OP_SH => lift_store(args, 2),
        OP_SW => lift_store(args, 4),
        OP_SD => lift_store(args, 8),

        // Jumps
        OP_JAL => lift_jal(args, pc, size, next_pc),
        OP_JALR => lift_jalr(args, pc, size),

        // Branches
        OP_BEQ => lift_branch(args, pc, |rs1, rs2| Expr::eq(rs1, rs2)),
        OP_BNE => lift_branch(args, pc, |rs1, rs2| Expr::ne(rs1, rs2)),
        OP_BLT => lift_branch(args, pc, |rs1, rs2| Expr::lt(rs1, rs2)),
        OP_BGE => lift_branch(args, pc, |rs1, rs2| Expr::ge(rs1, rs2)),
        OP_BLTU => lift_branch(args, pc, |rs1, rs2| Expr::ltu(rs1, rs2)),
        OP_BGEU => lift_branch(args, pc, |rs1, rs2| Expr::geu(rs1, rs2)),

        // System
        OP_ECALL => (Vec::new(), Terminator::trap("ecall")),
        OP_EBREAK => (Vec::new(), Terminator::trap("ebreak")),
        OP_FENCE => (Vec::new(), Terminator::Fall),
        OP_FENCE_I => (Vec::new(), Terminator::Fall),

        _ => (Vec::new(), Terminator::trap("unknown base instruction")),
    }
}

/// Lift M extension instruction.
fn lift_m<X: Xlen>(
    opid: OpId,
    args: &InstrArgs,
    _pc: X::Reg,
    _next_pc: X::Reg,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        OP_MUL => lift_r_type(args, |rs1, rs2| Expr::mul(rs1, rs2)),
        OP_MULH => lift_r_type(args, |rs1, rs2| Expr::mulh(rs1, rs2)),
        OP_MULHSU => lift_r_type(args, |rs1, rs2| Expr::mulhsu(rs1, rs2)),
        OP_MULHU => lift_r_type(args, |rs1, rs2| Expr::mulhu(rs1, rs2)),
        OP_DIV => lift_r_type(args, |rs1, rs2| Expr::div(rs1, rs2)),
        OP_DIVU => lift_r_type(args, |rs1, rs2| Expr::divu(rs1, rs2)),
        OP_REM => lift_r_type(args, |rs1, rs2| Expr::rem(rs1, rs2)),
        OP_REMU => lift_r_type(args, |rs1, rs2| Expr::remu(rs1, rs2)),

        // RV64M W-suffix ops
        OP_MULW => lift_r_type(args, |rs1, rs2| Expr::mulw(rs1, rs2)),
        OP_DIVW => lift_r_type(args, |rs1, rs2| Expr::divw(rs1, rs2)),
        OP_DIVUW => lift_r_type(args, |rs1, rs2| Expr::divuw(rs1, rs2)),
        OP_REMW => lift_r_type(args, |rs1, rs2| Expr::remw(rs1, rs2)),
        OP_REMUW => lift_r_type(args, |rs1, rs2| Expr::remuw(rs1, rs2)),

        _ => (Vec::new(), Terminator::trap("unknown M instruction")),
    }
}

/// Lift A extension instruction.
fn lift_a<X: Xlen>(
    opid: OpId,
    args: &InstrArgs,
    _next_pc: X::Reg,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::Amo { rd, rs1, rs2, aq: _, rl: _ } => {
            let rd = *rd;
            let rs1 = *rs1;
            let rs2 = *rs2;

            let is_64 = opid == OP_LR_D || opid == OP_SC_D || opid == OP_AMOSWAP_D ||
                opid == OP_AMOADD_D || opid == OP_AMOXOR_D || opid == OP_AMOAND_D ||
                opid == OP_AMOOR_D || opid == OP_AMOMIN_D || opid == OP_AMOMAX_D ||
                opid == OP_AMOMINU_D || opid == OP_AMOMAXU_D;
            let width: u8 = if is_64 { 8 } else { 4 };

            // LR.W / LR.D
            if opid == OP_LR_W || opid == OP_LR_D {
                let addr = Expr::read(rs1);
                let value = Expr::mem_u(addr.clone(), width);
                let stmts = vec![Stmt::write_reg(rd, value)];
                return (stmts, Terminator::Fall);
            }

            // SC.W / SC.D
            if opid == OP_SC_W || opid == OP_SC_D {
                let addr = Expr::read(rs1);
                let value = Expr::read(rs2);
                let stmts = vec![
                    Stmt::write_mem(addr, value, width),
                    Stmt::write_reg(rd, Expr::imm(X::from_u64(0))),
                ];
                return (stmts, Terminator::Fall);
            }

            // AMO operations
            if opid == OP_AMOSWAP_W || opid == OP_AMOSWAP_D {
                return lift_amo(rd, rs1, rs2, width, |_old, new| new);
            }
            if opid == OP_AMOADD_W || opid == OP_AMOADD_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::add(old, new));
            }
            if opid == OP_AMOXOR_W || opid == OP_AMOXOR_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::xor(old, new));
            }
            if opid == OP_AMOAND_W || opid == OP_AMOAND_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::and(old, new));
            }
            if opid == OP_AMOOR_W || opid == OP_AMOOR_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::or(old, new));
            }
            if opid == OP_AMOMIN_W || opid == OP_AMOMIN_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::min(old, new));
            }
            if opid == OP_AMOMAX_W || opid == OP_AMOMAX_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::max(old, new));
            }
            if opid == OP_AMOMINU_W || opid == OP_AMOMINU_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::minu(old, new));
            }
            if opid == OP_AMOMAXU_W || opid == OP_AMOMAXU_D {
                return lift_amo(rd, rs1, rs2, width, |old, new| Expr::maxu(old, new));
            }

            (Vec::new(), Terminator::trap("unknown A instruction"))
        }
        _ => (Vec::new(), Terminator::trap("invalid A instruction args")),
    }
}

/// Lift AMO instruction: rd = old value, mem[rs1] = op(old, rs2)
fn lift_amo<X: Xlen, F>(rd: u8, rs1: u8, rs2: u8, width: u8, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    let addr = Expr::read(rs1);
    let old_value = Expr::mem_u(addr.clone(), width);
    let new_value = op(old_value.clone(), Expr::read(rs2));

    let stmts = vec![
        Stmt::write_reg(rd, old_value),
        Stmt::write_mem(addr, new_value, width),
    ];
    (stmts, Terminator::Fall)
}

/// Lift C extension instruction.
fn lift_c<X: Xlen>(
    opid: OpId,
    args: &InstrArgs,
    pc: X::Reg,
    size: u8,
    _next_pc: X::Reg,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        // Arithmetic
        OP_C_ADD => lift_r_type(args, |rs1, rs2| Expr::add(rs1, rs2)),
        OP_C_SUB => lift_r_type(args, |rs1, rs2| Expr::sub(rs1, rs2)),
        OP_C_XOR => lift_r_type(args, |rs1, rs2| Expr::xor(rs1, rs2)),
        OP_C_OR => lift_r_type(args, |rs1, rs2| Expr::or(rs1, rs2)),
        OP_C_AND => lift_r_type(args, |rs1, rs2| Expr::and(rs1, rs2)),
        OP_C_MV => lift_mv(args),

        // RV64C W-suffix
        OP_C_ADDW => lift_r_type(args, |rs1, rs2| Expr::addw(rs1, rs2)),
        OP_C_SUBW => lift_r_type(args, |rs1, rs2| Expr::subw(rs1, rs2)),

        // Immediate arithmetic
        OP_C_ADDI => lift_i_arith(args, |rs1, imm| Expr::add(rs1, imm)),
        OP_C_ADDI4SPN => lift_i_arith(args, |rs1, imm| Expr::add(rs1, imm)),
        OP_C_ADDI16SP => lift_i_arith(args, |rs1, imm| Expr::add(rs1, imm)),
        OP_C_ADDIW => lift_i_arith(args, |rs1, imm| Expr::addw(rs1, imm)),
        OP_C_LI => lift_i_arith(args, |_rs1, imm| imm),
        OP_C_ANDI => lift_i_arith(args, |rs1, imm| Expr::and(rs1, imm)),

        // Shifts
        OP_C_SLLI => lift_shift_imm(args, |rs1, shamt| Expr::sll(rs1, shamt)),
        OP_C_SRLI => lift_shift_imm(args, |rs1, shamt| Expr::srl(rs1, shamt)),
        OP_C_SRAI => lift_shift_imm(args, |rs1, shamt| Expr::sra(rs1, shamt)),

        // Upper immediate
        OP_C_LUI => lift_lui(args),

        // Loads
        OP_C_LW => lift_load(args, 4, true),
        OP_C_LD => lift_load(args, 8, false),
        OP_C_LWSP => lift_load(args, 4, true),
        OP_C_LDSP => lift_load(args, 8, false),

        // Stores
        OP_C_SW => lift_store(args, 4),
        OP_C_SD => lift_store(args, 8),
        OP_C_SWSP => lift_store(args, 4),
        OP_C_SDSP => lift_store(args, 8),

        // Jumps
        OP_C_J => lift_c_j(args, pc),
        OP_C_JAL => lift_c_jal(args, pc, size),
        OP_C_JR => lift_c_jr(args),
        OP_C_JALR => lift_c_jalr(args, pc, size),

        // Branches
        OP_C_BEQZ => lift_c_branch(args, pc, |rs1| Expr::eq(rs1, Expr::imm(X::from_u64(0)))),
        OP_C_BNEZ => lift_c_branch(args, pc, |rs1| Expr::ne(rs1, Expr::imm(X::from_u64(0)))),

        // System
        OP_C_NOP => (Vec::new(), Terminator::Fall),
        OP_C_EBREAK => (Vec::new(), Terminator::trap("ebreak")),

        _ => (Vec::new(), Terminator::trap("unknown C instruction")),
    }
}

/// Lift Zicsr instruction.
fn lift_zicsr<X: Xlen>(
    opid: OpId,
    args: &InstrArgs,
    _next_pc: X::Reg,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::Csr { rd, rs1, csr } => {
            let rd = *rd;
            let rs1 = *rs1;
            let csr = *csr;

            match opid {
                OP_CSRRW => {
                    let old = Expr::csr(csr);
                    let new_val = Expr::read(rs1);
                    let stmts = vec![
                        Stmt::write_reg(rd, old),
                        Stmt::write_csr(csr, new_val),
                    ];
                    (stmts, Terminator::Fall)
                }
                OP_CSRRS => {
                    let old = Expr::csr(csr);
                    let mask = Expr::read(rs1);
                    let new_val = Expr::or(old.clone(), mask);
                    let mut stmts = vec![Stmt::write_reg(rd, old)];
                    if rs1 != 0 {
                        stmts.push(Stmt::write_csr(csr, new_val));
                    }
                    (stmts, Terminator::Fall)
                }
                OP_CSRRC => {
                    let old = Expr::csr(csr);
                    let mask = Expr::read(rs1);
                    let new_val = Expr::and(old.clone(), Expr::not(mask));
                    let mut stmts = vec![Stmt::write_reg(rd, old)];
                    if rs1 != 0 {
                        stmts.push(Stmt::write_csr(csr, new_val));
                    }
                    (stmts, Terminator::Fall)
                }
                _ => (Vec::new(), Terminator::trap("unknown CSR instruction")),
            }
        }
        InstrArgs::CsrI { rd, imm, csr } => {
            let rd = *rd;
            let imm = *imm;
            let csr = *csr;

            match opid {
                OP_CSRRWI => {
                    let old = Expr::csr(csr);
                    let new_val = Expr::imm(X::from_u64(imm as u64));
                    let stmts = vec![
                        Stmt::write_reg(rd, old),
                        Stmt::write_csr(csr, new_val),
                    ];
                    (stmts, Terminator::Fall)
                }
                OP_CSRRSI => {
                    let old = Expr::csr(csr);
                    let mask = Expr::imm(X::from_u64(imm as u64));
                    let new_val = Expr::or(old.clone(), mask);
                    let mut stmts = vec![Stmt::write_reg(rd, old)];
                    if imm != 0 {
                        stmts.push(Stmt::write_csr(csr, new_val));
                    }
                    (stmts, Terminator::Fall)
                }
                OP_CSRRCI => {
                    let old = Expr::csr(csr);
                    let mask = Expr::imm(X::from_u64(imm as u64));
                    let new_val = Expr::and(old.clone(), Expr::not(mask));
                    let mut stmts = vec![Stmt::write_reg(rd, old)];
                    if imm != 0 {
                        stmts.push(Stmt::write_csr(csr, new_val));
                    }
                    (stmts, Terminator::Fall)
                }
                _ => (Vec::new(), Terminator::trap("unknown CSRI instruction")),
            }
        }
        _ => (Vec::new(), Terminator::trap("invalid CSR instruction args")),
    }
}

// ===== Helper functions =====

/// Lift R-type instruction (rd = op(rs1, rs2)).
fn lift_r_type<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
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
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid R-type args")),
    }
}

/// Lift I-type arithmetic instruction (rd = op(rs1, imm)).
fn lift_i_arith<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                // Sign-extend immediate
                let imm_val = X::sign_extend_32(*imm as u32);
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), Expr::imm(imm_val)))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid I-type args")),
    }
}

/// Lift shift immediate instruction (rd = op(rs1, shamt)).
fn lift_shift_imm<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                // Shift amount is always positive
                let shamt = X::from_u64(*imm as u64 & 0x3F);
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), Expr::imm(shamt)))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid shift args")),
    }
}

/// Lift LUI instruction.
fn lift_lui<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::U { rd, imm } => {
            let stmts = if *rd != 0 {
                // imm is already shifted by 12
                let imm_val = X::sign_extend_32(*imm as u32);
                vec![Stmt::write_reg(*rd, Expr::imm(imm_val))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid LUI args")),
    }
}

/// Lift AUIPC instruction.
fn lift_auipc<X: Xlen>(args: &InstrArgs, pc: X::Reg) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::U { rd, imm } => {
            let stmts = if *rd != 0 {
                let imm_val = X::sign_extend_32(*imm as u32);
                vec![Stmt::write_reg(*rd, Expr::add(Expr::imm(pc), Expr::imm(imm_val)))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid AUIPC args")),
    }
}

/// Lift load instruction.
fn lift_load<X: Xlen>(args: &InstrArgs, width: u8, signed: bool) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                let imm_val = X::sign_extend_32(*imm as u32);
                let addr = Expr::add(Expr::read(*rs1), Expr::imm(imm_val));
                let value = if signed {
                    Expr::mem_s(addr, width)
                } else {
                    Expr::mem_u(addr, width)
                };
                vec![Stmt::write_reg(*rd, value)]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid load args")),
    }
}

/// Lift store instruction.
fn lift_store<X: Xlen>(args: &InstrArgs, width: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::S { rs1, rs2, imm } => {
            let imm_val = X::sign_extend_32(*imm as u32);
            let addr = Expr::add(Expr::read(*rs1), Expr::imm(imm_val));
            let stmts = vec![Stmt::write_mem(addr, Expr::read(*rs2), width)];
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid store args")),
    }
}

/// Lift JAL instruction.
fn lift_jal<X: Xlen>(
    args: &InstrArgs,
    pc: X::Reg,
    size: u8,
    _next_pc: X::Reg,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::J { rd, imm } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                let return_addr = pc + X::from_u64(size as u64);
                stmts.push(Stmt::write_reg(*rd, Expr::imm(return_addr)));
            }
            // Calculate target (pc + sign-extended offset)
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            (stmts, Terminator::jump(X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid JAL args")),
    }
}

/// Lift JALR instruction.
fn lift_jalr<X: Xlen>(
    args: &InstrArgs,
    pc: X::Reg,
    size: u8,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                let return_addr = pc + X::from_u64(size as u64);
                stmts.push(Stmt::write_reg(*rd, Expr::imm(return_addr)));
            }
            // Target = (rs1 + imm) & ~1
            let imm_val = X::sign_extend_32(*imm as u32);
            let target = Expr::and(
                Expr::add(Expr::read(*rs1), Expr::imm(imm_val)),
                Expr::not(Expr::imm(X::from_u64(1))),
            );
            (stmts, Terminator::jump_dyn(target))
        }
        _ => (Vec::new(), Terminator::trap("invalid JALR args")),
    }
}

/// Lift branch instruction.
fn lift_branch<X: Xlen, F>(
    args: &InstrArgs,
    pc: X::Reg,
    cond: F,
) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::B { rs1, rs2, imm } => {
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            let cond_expr = cond(Expr::read(*rs1), Expr::read(*rs2));
            (Vec::new(), Terminator::branch(cond_expr, X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid branch args")),
    }
}

/// Lift C.MV instruction.
fn lift_mv<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::R { rd, rs1: _, rs2 } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, Expr::read(*rs2))]
            } else {
                Vec::new()
            };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid MV args")),
    }
}

/// Lift C.J instruction.
fn lift_c_j<X: Xlen>(args: &InstrArgs, pc: X::Reg) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::J { rd: _, imm } => {
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            (Vec::new(), Terminator::jump(X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid C.J args")),
    }
}

/// Lift C.JAL instruction (RV32 only).
fn lift_c_jal<X: Xlen>(
    args: &InstrArgs,
    pc: X::Reg,
    size: u8,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::J { rd, imm } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                let return_addr = pc + X::from_u64(size as u64);
                stmts.push(Stmt::write_reg(*rd, Expr::imm(return_addr)));
            }
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            (stmts, Terminator::jump(X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid C.JAL args")),
    }
}

/// Lift C.JR instruction.
fn lift_c_jr<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd: _, rs1, imm: _ } => {
            let target = Expr::read(*rs1);
            (Vec::new(), Terminator::jump_dyn(target))
        }
        _ => (Vec::new(), Terminator::trap("invalid C.JR args")),
    }
}

/// Lift C.JALR instruction.
fn lift_c_jalr<X: Xlen>(
    args: &InstrArgs,
    pc: X::Reg,
    size: u8,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, imm: _ } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                let return_addr = pc + X::from_u64(size as u64);
                stmts.push(Stmt::write_reg(*rd, Expr::imm(return_addr)));
            }
            let target = Expr::read(*rs1);
            (stmts, Terminator::jump_dyn(target))
        }
        _ => (Vec::new(), Terminator::trap("invalid C.JALR args")),
    }
}

/// Lift C.BEQZ/C.BNEZ instruction.
fn lift_c_branch<X: Xlen, F>(
    args: &InstrArgs,
    pc: X::Reg,
    cond: F,
) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>) -> Expr<X>,
{
    match args {
        InstrArgs::B { rs1, rs2: _, imm } => {
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            let cond_expr = cond(Expr::read(*rs1));
            (Vec::new(), Terminator::branch(cond_expr, X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid C branch args")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_isa::{Rv64, decode};

    #[test]
    fn test_lift_addi() {
        // ADDI x1, x0, 42
        let bytes = [0x93, 0x00, 0xa0, 0x02]; // addi x1, x0, 42
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        let ir = lift(&decoded);

        assert_eq!(ir.pc, 0);
        assert_eq!(ir.size, 4);
        assert_eq!(ir.statements.len(), 1);
        assert!(ir.terminator.is_fall());
    }

    #[test]
    fn test_lift_add() {
        // ADD x1, x2, x3
        let bytes = [0xB3, 0x00, 0x31, 0x00];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        let ir = lift(&decoded);

        assert_eq!(ir.statements.len(), 1);
        assert!(ir.terminator.is_fall());
    }

    #[test]
    fn test_lift_jal() {
        // JAL x1, 8 (offset to 0x08)
        let bytes = [0xEF, 0x00, 0x80, 0x00];
        let decoded = decode::<Rv64>(&bytes, 0x100u64).unwrap();
        let ir = lift(&decoded);

        // Should write return address to x1
        assert_eq!(ir.statements.len(), 1);
        // Should jump to target
        assert!(ir.terminator.is_jump());
    }

    #[test]
    fn test_lift_beq() {
        // BEQ x0, x0, 8
        let bytes = [0x63, 0x04, 0x00, 0x00];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        let ir = lift(&decoded);

        assert_eq!(ir.statements.len(), 0);
        assert!(ir.terminator.is_branch());
    }

    #[test]
    fn test_lift_lw() {
        // LW x1, 0(x2)
        let bytes = [0x83, 0x20, 0x01, 0x00];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        let ir = lift(&decoded);

        assert_eq!(ir.statements.len(), 1);
        assert!(ir.terminator.is_fall());
    }
}
