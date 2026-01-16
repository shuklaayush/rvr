//! Base I extension (RV32I/RV64I) - decode, lift, disasm.

use rvr_ir::{Xlen, InstrIR, Expr, Stmt, Terminator};

use crate::{
    OpId, DecodedInstr, InstrArgs, EXT_I, reg_name,
    encode::{decode_opcode, decode_funct3, decode_funct7, decode_rd, decode_rs1, decode_rs2,
             decode_i_imm, decode_s_imm, decode_b_imm, decode_u_imm, decode_j_imm},
};
use super::InstructionExtension;

// ===== OpId Constants =====

pub const OP_LUI: OpId = OpId::new(EXT_I, 0);
pub const OP_AUIPC: OpId = OpId::new(EXT_I, 1);
pub const OP_JAL: OpId = OpId::new(EXT_I, 2);
pub const OP_JALR: OpId = OpId::new(EXT_I, 3);
pub const OP_BEQ: OpId = OpId::new(EXT_I, 4);
pub const OP_BNE: OpId = OpId::new(EXT_I, 5);
pub const OP_BLT: OpId = OpId::new(EXT_I, 6);
pub const OP_BGE: OpId = OpId::new(EXT_I, 7);
pub const OP_BLTU: OpId = OpId::new(EXT_I, 8);
pub const OP_BGEU: OpId = OpId::new(EXT_I, 9);
pub const OP_LB: OpId = OpId::new(EXT_I, 10);
pub const OP_LH: OpId = OpId::new(EXT_I, 11);
pub const OP_LW: OpId = OpId::new(EXT_I, 12);
pub const OP_LBU: OpId = OpId::new(EXT_I, 13);
pub const OP_LHU: OpId = OpId::new(EXT_I, 14);
pub const OP_SB: OpId = OpId::new(EXT_I, 15);
pub const OP_SH: OpId = OpId::new(EXT_I, 16);
pub const OP_SW: OpId = OpId::new(EXT_I, 17);
pub const OP_ADDI: OpId = OpId::new(EXT_I, 18);
pub const OP_SLTI: OpId = OpId::new(EXT_I, 19);
pub const OP_SLTIU: OpId = OpId::new(EXT_I, 20);
pub const OP_XORI: OpId = OpId::new(EXT_I, 21);
pub const OP_ORI: OpId = OpId::new(EXT_I, 22);
pub const OP_ANDI: OpId = OpId::new(EXT_I, 23);
pub const OP_SLLI: OpId = OpId::new(EXT_I, 24);
pub const OP_SRLI: OpId = OpId::new(EXT_I, 25);
pub const OP_SRAI: OpId = OpId::new(EXT_I, 26);
pub const OP_ADD: OpId = OpId::new(EXT_I, 27);
pub const OP_SUB: OpId = OpId::new(EXT_I, 28);
pub const OP_SLL: OpId = OpId::new(EXT_I, 29);
pub const OP_SLT: OpId = OpId::new(EXT_I, 30);
pub const OP_SLTU: OpId = OpId::new(EXT_I, 31);
pub const OP_XOR: OpId = OpId::new(EXT_I, 32);
pub const OP_SRL: OpId = OpId::new(EXT_I, 33);
pub const OP_SRA: OpId = OpId::new(EXT_I, 34);
pub const OP_OR: OpId = OpId::new(EXT_I, 35);
pub const OP_AND: OpId = OpId::new(EXT_I, 36);
pub const OP_FENCE: OpId = OpId::new(EXT_I, 37);
pub const OP_ECALL: OpId = OpId::new(EXT_I, 38);
pub const OP_EBREAK: OpId = OpId::new(EXT_I, 39);
// RV64I
pub const OP_LWU: OpId = OpId::new(EXT_I, 40);
pub const OP_LD: OpId = OpId::new(EXT_I, 41);
pub const OP_SD: OpId = OpId::new(EXT_I, 42);
pub const OP_ADDIW: OpId = OpId::new(EXT_I, 43);
pub const OP_SLLIW: OpId = OpId::new(EXT_I, 44);
pub const OP_SRLIW: OpId = OpId::new(EXT_I, 45);
pub const OP_SRAIW: OpId = OpId::new(EXT_I, 46);
pub const OP_ADDW: OpId = OpId::new(EXT_I, 47);
pub const OP_SUBW: OpId = OpId::new(EXT_I, 48);
pub const OP_SLLW: OpId = OpId::new(EXT_I, 49);
pub const OP_SRLW: OpId = OpId::new(EXT_I, 50);
pub const OP_SRAW: OpId = OpId::new(EXT_I, 51);

/// Get mnemonic for a base instruction.
pub fn base_mnemonic(opid: OpId) -> &'static str {
    match opid.idx {
        0 => "lui", 1 => "auipc", 2 => "jal", 3 => "jalr",
        4 => "beq", 5 => "bne", 6 => "blt", 7 => "bge", 8 => "bltu", 9 => "bgeu",
        10 => "lb", 11 => "lh", 12 => "lw", 13 => "lbu", 14 => "lhu",
        15 => "sb", 16 => "sh", 17 => "sw",
        18 => "addi", 19 => "slti", 20 => "sltiu", 21 => "xori", 22 => "ori", 23 => "andi",
        24 => "slli", 25 => "srli", 26 => "srai",
        27 => "add", 28 => "sub", 29 => "sll", 30 => "slt", 31 => "sltu",
        32 => "xor", 33 => "srl", 34 => "sra", 35 => "or", 36 => "and",
        37 => "fence", 38 => "ecall", 39 => "ebreak",
        40 => "lwu", 41 => "ld", 42 => "sd",
        43 => "addiw", 44 => "slliw", 45 => "srliw", 46 => "sraiw",
        47 => "addw", 48 => "subw", 49 => "sllw", 50 => "srlw", 51 => "sraw",
        _ => "???",
    }
}

// ===== Extension Implementation =====

/// Base I extension (RV32I/RV64I).
pub struct BaseExtension;

impl<X: Xlen> InstructionExtension<X> for BaseExtension {
    fn handled_extensions(&self) -> &[u8] {
        &[EXT_I]
    }

    fn decode(&self, bytes: &[u8], pc: X::Reg) -> Option<DecodedInstr<X>> {
        if bytes.len() < 4 || (bytes[0] & 0x03) != 0x03 {
            return None;
        }
        let instr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        decode_32bit(instr, pc)
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let (stmts, term) = lift_base(&instr.args, instr.opid, instr.pc, instr.size);
        InstrIR::new(instr.pc, instr.size, stmts, term)
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        format_instr(base_mnemonic(instr.opid), &instr.args)
    }
}

// ===== Decode =====

fn decode_32bit<X: Xlen>(instr: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
    let opcode = decode_opcode(instr);
    let funct3 = decode_funct3(instr);
    let funct7 = decode_funct7(instr);
    let rd = decode_rd(instr);
    let rs1 = decode_rs1(instr);
    let rs2 = decode_rs2(instr);

    let (opid, args) = match opcode {
        0x37 => (OP_LUI, InstrArgs::U { rd, imm: decode_u_imm(instr) }),
        0x17 => (OP_AUIPC, InstrArgs::U { rd, imm: decode_u_imm(instr) }),
        0x6F => (OP_JAL, InstrArgs::J { rd, imm: decode_j_imm(instr) }),
        0x67 if funct3 == 0 => (OP_JALR, InstrArgs::I { rd, rs1, imm: decode_i_imm(instr) }),
        0x63 => {
            let imm = decode_b_imm(instr);
            let op = match funct3 {
                0 => OP_BEQ, 1 => OP_BNE, 4 => OP_BLT, 5 => OP_BGE, 6 => OP_BLTU, 7 => OP_BGEU,
                _ => return None,
            };
            (op, InstrArgs::B { rs1, rs2, imm })
        }
        0x03 => {
            let imm = decode_i_imm(instr);
            let op = match funct3 {
                0 => OP_LB, 1 => OP_LH, 2 => OP_LW,
                3 if X::VALUE == 64 => OP_LD,
                4 => OP_LBU, 5 => OP_LHU,
                6 if X::VALUE == 64 => OP_LWU,
                _ => return None,
            };
            (op, InstrArgs::I { rd, rs1, imm })
        }
        0x23 => {
            let imm = decode_s_imm(instr);
            let op = match funct3 {
                0 => OP_SB, 1 => OP_SH, 2 => OP_SW,
                3 if X::VALUE == 64 => OP_SD,
                _ => return None,
            };
            (op, InstrArgs::S { rs1, rs2, imm })
        }
        0x13 => {
            let imm = decode_i_imm(instr);
            let shamt = (instr >> 20) & 0x3F;
            let op = match funct3 {
                0 => OP_ADDI,
                1 if (funct7 & 0xFE) == 0 => OP_SLLI,
                2 => OP_SLTI, 3 => OP_SLTIU, 4 => OP_XORI,
                5 if (funct7 & 0xFE) == 0 => OP_SRLI,
                5 if (funct7 & 0xFE) == 0x20 => OP_SRAI,
                6 => OP_ORI, 7 => OP_ANDI,
                _ => return None,
            };
            let imm = if funct3 == 1 || funct3 == 5 { shamt as i32 } else { imm };
            (op, InstrArgs::I { rd, rs1, imm })
        }
        0x1B if X::VALUE == 64 => {
            let imm = decode_i_imm(instr);
            let shamt = ((instr >> 20) & 0x1F) as i32;
            let op = match funct3 {
                0 => OP_ADDIW,
                1 if funct7 == 0 => OP_SLLIW,
                5 if funct7 == 0 => OP_SRLIW,
                5 if funct7 == 0x20 => OP_SRAIW,
                _ => return None,
            };
            let imm = if funct3 == 1 || funct3 == 5 { shamt } else { imm };
            (op, InstrArgs::I { rd, rs1, imm })
        }
        0x33 if funct7 != 0x01 => {
            let op = match (funct7, funct3) {
                (0x00, 0) => OP_ADD, (0x20, 0) => OP_SUB,
                (0x00, 1) => OP_SLL, (0x00, 2) => OP_SLT, (0x00, 3) => OP_SLTU,
                (0x00, 4) => OP_XOR, (0x00, 5) => OP_SRL, (0x20, 5) => OP_SRA,
                (0x00, 6) => OP_OR, (0x00, 7) => OP_AND,
                _ => return None,
            };
            (op, InstrArgs::R { rd, rs1, rs2 })
        }
        0x3B if X::VALUE == 64 && funct7 != 0x01 => {
            let op = match (funct7, funct3) {
                (0x00, 0) => OP_ADDW, (0x20, 0) => OP_SUBW,
                (0x00, 1) => OP_SLLW, (0x00, 5) => OP_SRLW, (0x20, 5) => OP_SRAW,
                _ => return None,
            };
            (op, InstrArgs::R { rd, rs1, rs2 })
        }
        0x0F if funct3 == 0 => (OP_FENCE, InstrArgs::None),
        0x73 if funct3 == 0 => {
            if instr == 0x00000073 { (OP_ECALL, InstrArgs::None) }
            else if instr == 0x00100073 { (OP_EBREAK, InstrArgs::None) }
            else { return None; }
        }
        _ => return None,
    };

    Some(DecodedInstr::new(opid, pc, 4, args))
}

// ===== Lift =====

fn lift_base<X: Xlen>(
    args: &InstrArgs, opid: OpId, pc: X::Reg, size: u8,
) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        OP_ADD => lift_r(args, |a, b| Expr::add(a, b)),
        OP_SUB => lift_r(args, |a, b| Expr::sub(a, b)),
        OP_SLL => lift_r(args, |a, b| Expr::sll(a, b)),
        OP_SLT => lift_r(args, |a, b| Expr::slt(a, b)),
        OP_SLTU => lift_r(args, |a, b| Expr::sltu(a, b)),
        OP_XOR => lift_r(args, |a, b| Expr::xor(a, b)),
        OP_SRL => lift_r(args, |a, b| Expr::srl(a, b)),
        OP_SRA => lift_r(args, |a, b| Expr::sra(a, b)),
        OP_OR => lift_r(args, |a, b| Expr::or(a, b)),
        OP_AND => lift_r(args, |a, b| Expr::and(a, b)),

        OP_ADDI => lift_i(args, |a, b| Expr::add(a, b)),
        OP_SLTI => lift_i(args, |a, b| Expr::slt(a, b)),
        OP_SLTIU => lift_i(args, |a, b| Expr::sltu(a, b)),
        OP_XORI => lift_i(args, |a, b| Expr::xor(a, b)),
        OP_ORI => lift_i(args, |a, b| Expr::or(a, b)),
        OP_ANDI => lift_i(args, |a, b| Expr::and(a, b)),
        OP_SLLI => lift_shamt(args, |a, b| Expr::sll(a, b)),
        OP_SRLI => lift_shamt(args, |a, b| Expr::srl(a, b)),
        OP_SRAI => lift_shamt(args, |a, b| Expr::sra(a, b)),

        OP_ADDW => lift_r(args, |a, b| Expr::addw(a, b)),
        OP_SUBW => lift_r(args, |a, b| Expr::subw(a, b)),
        OP_SLLW => lift_r(args, |a, b| Expr::sllw(a, b)),
        OP_SRLW => lift_r(args, |a, b| Expr::srlw(a, b)),
        OP_SRAW => lift_r(args, |a, b| Expr::sraw(a, b)),
        OP_ADDIW => lift_i(args, |a, b| Expr::addw(a, b)),
        OP_SLLIW => lift_shamt(args, |a, b| Expr::sllw(a, b)),
        OP_SRLIW => lift_shamt(args, |a, b| Expr::srlw(a, b)),
        OP_SRAIW => lift_shamt(args, |a, b| Expr::sraw(a, b)),

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

        OP_ECALL => (Vec::new(), Terminator::trap("ecall")),
        OP_EBREAK => (Vec::new(), Terminator::trap("ebreak")),
        OP_FENCE => (Vec::new(), Terminator::Fall),

        _ => (Vec::new(), Terminator::trap("unknown base instruction")),
    }
}

fn lift_r<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where F: FnOnce(Expr<X>, Expr<X>) -> Expr<X> {
    match args {
        InstrArgs::R { rd, rs1, rs2 } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), Expr::read(*rs2)))]
            } else { Vec::new() };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_i<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where F: FnOnce(Expr<X>, Expr<X>) -> Expr<X> {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), Expr::imm(X::sign_extend_32(*imm as u32))))]
            } else { Vec::new() };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_shamt<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where F: FnOnce(Expr<X>, Expr<X>) -> Expr<X> {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), Expr::imm(X::from_u64(*imm as u64 & 0x3F))))]
            } else { Vec::new() };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_lui<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::U { rd, imm } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, Expr::imm(X::sign_extend_32(*imm as u32)))]
            } else { Vec::new() };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_auipc<X: Xlen>(args: &InstrArgs, pc: X::Reg) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::U { rd, imm } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, Expr::add(Expr::imm(pc), Expr::imm(X::sign_extend_32(*imm as u32))))]
            } else { Vec::new() };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_load<X: Xlen>(args: &InstrArgs, width: u8, signed: bool) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let stmts = if *rd != 0 {
                let addr = Expr::add(Expr::read(*rs1), Expr::imm(X::sign_extend_32(*imm as u32)));
                let val = if signed { Expr::mem_s(addr, width) } else { Expr::mem_u(addr, width) };
                vec![Stmt::write_reg(*rd, val)]
            } else { Vec::new() };
            (stmts, Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_store<X: Xlen>(args: &InstrArgs, width: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::S { rs1, rs2, imm } => {
            let addr = Expr::add(Expr::read(*rs1), Expr::imm(X::sign_extend_32(*imm as u32)));
            (vec![Stmt::write_mem(addr, Expr::read(*rs2), width)], Terminator::Fall)
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_jal<X: Xlen>(args: &InstrArgs, pc: X::Reg, size: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::J { rd, imm } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, Expr::imm(pc + X::from_u64(size as u64))));
            }
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            (stmts, Terminator::jump(X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_jalr<X: Xlen>(args: &InstrArgs, pc: X::Reg, size: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::I { rd, rs1, imm } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, Expr::imm(pc + X::from_u64(size as u64))));
            }
            let target = Expr::and(
                Expr::add(Expr::read(*rs1), Expr::imm(X::sign_extend_32(*imm as u32))),
                Expr::not(Expr::imm(X::from_u64(1))),
            );
            (stmts, Terminator::jump_dyn(target))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_branch<X: Xlen, F>(args: &InstrArgs, pc: X::Reg, cond: F) -> (Vec<Stmt<X>>, Terminator<X>)
where F: FnOnce(Expr<X>, Expr<X>) -> Expr<X> {
    match args {
        InstrArgs::B { rs1, rs2, imm } => {
            let offset = X::to_u64(X::sign_extend_32(*imm as u32)) as i64;
            let target = (X::to_u64(pc) as i64 + offset) as u64;
            (Vec::new(), Terminator::branch(cond(Expr::read(*rs1), Expr::read(*rs2)), X::from_u64(target)))
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

// ===== Disasm =====

fn format_instr(mnemonic: &str, args: &InstrArgs) -> String {
    match args {
        InstrArgs::R { rd, rs1, rs2 } => {
            format!("{} {}, {}, {}", mnemonic, reg_name(*rd), reg_name(*rs1), reg_name(*rs2))
        }
        InstrArgs::I { rd, rs1, imm } => {
            if mnemonic.starts_with('l') && mnemonic != "lui" {
                format!("{} {}, {}({})", mnemonic, reg_name(*rd), imm, reg_name(*rs1))
            } else {
                format!("{} {}, {}, {}", mnemonic, reg_name(*rd), reg_name(*rs1), imm)
            }
        }
        InstrArgs::S { rs1, rs2, imm } => {
            format!("{} {}, {}({})", mnemonic, reg_name(*rs2), imm, reg_name(*rs1))
        }
        InstrArgs::B { rs1, rs2, imm } => {
            format!("{} {}, {}, {}", mnemonic, reg_name(*rs1), reg_name(*rs2), imm)
        }
        InstrArgs::U { rd, imm } => {
            format!("{} {}, {:#x}", mnemonic, reg_name(*rd), (*imm as u32) >> 12)
        }
        InstrArgs::J { rd, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rd), imm)
        }
        InstrArgs::None => mnemonic.to_string(),
        _ => format!("{} <?>", mnemonic),
    }
}
