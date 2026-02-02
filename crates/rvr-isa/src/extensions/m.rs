//! M extension (multiply/divide) - decode, lift, disasm.

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use super::InstructionExtension;
use crate::{
    DecodedInstr, EXT_M, InstrArgs, OpClass, OpId, OpInfo,
    encode::{decode_funct3, decode_funct7, decode_opcode, decode_rd, decode_rs1, decode_rs2},
    reg_name,
};

// M extension OpId constants
pub const OP_MUL: OpId = OpId::new(EXT_M, 0);
pub const OP_MULH: OpId = OpId::new(EXT_M, 1);
pub const OP_MULHSU: OpId = OpId::new(EXT_M, 2);
pub const OP_MULHU: OpId = OpId::new(EXT_M, 3);
pub const OP_DIV: OpId = OpId::new(EXT_M, 4);
pub const OP_DIVU: OpId = OpId::new(EXT_M, 5);
pub const OP_REM: OpId = OpId::new(EXT_M, 6);
pub const OP_REMU: OpId = OpId::new(EXT_M, 7);

// RV64M W variants
pub const OP_MULW: OpId = OpId::new(EXT_M, 8);
pub const OP_DIVW: OpId = OpId::new(EXT_M, 9);
pub const OP_DIVUW: OpId = OpId::new(EXT_M, 10);
pub const OP_REMW: OpId = OpId::new(EXT_M, 11);
pub const OP_REMUW: OpId = OpId::new(EXT_M, 12);

/// Get the mnemonic for an M extension instruction.
pub fn m_mnemonic(opid: OpId) -> &'static str {
    match opid.idx {
        0 => "mul",
        1 => "mulh",
        2 => "mulhsu",
        3 => "mulhu",
        4 => "div",
        5 => "divu",
        6 => "rem",
        7 => "remu",
        8 => "mulw",
        9 => "divw",
        10 => "divuw",
        11 => "remw",
        12 => "remuw",
        _ => "???",
    }
}

/// M extension (multiply/divide).
pub struct MExtension;

impl<X: Xlen> InstructionExtension<X> for MExtension {
    fn name(&self) -> &'static str {
        "M"
    }

    fn ext_id(&self) -> u8 {
        EXT_M
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = decode_opcode(raw);
        let funct3 = decode_funct3(raw);
        let funct7 = decode_funct7(raw);
        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let rs2 = decode_rs2(raw);

        if funct7 != 0x01 {
            return None;
        }

        let opid = match opcode {
            0x33 => match funct3 {
                0 => OP_MUL,
                1 => OP_MULH,
                2 => OP_MULHSU,
                3 => OP_MULHU,
                4 => OP_DIV,
                5 => OP_DIVU,
                6 => OP_REM,
                7 => OP_REMU,
                _ => return None,
            },
            0x3B if X::VALUE == 64 => match funct3 {
                0 => OP_MULW,
                4 => OP_DIVW,
                5 => OP_DIVUW,
                6 => OP_REMW,
                7 => OP_REMUW,
                _ => return None,
            },
            _ => return None,
        };

        Some(DecodedInstr::new(
            opid,
            pc,
            4,
            raw,
            InstrArgs::R { rd, rs1, rs2 },
        ))
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let (stmts, term) = lift_m(&instr.args, instr.opid);
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            instr.raw,
            stmts,
            term,
        )
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        let mnemonic = m_mnemonic(instr.opid);
        match &instr.args {
            InstrArgs::R { rd, rs1, rs2 } => {
                format!(
                    "{} {}, {}, {}",
                    mnemonic,
                    reg_name(*rd),
                    reg_name(*rs1),
                    reg_name(*rs2)
                )
            }
            _ => format!("{} <?>", mnemonic),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_M.iter().find(|info| info.opid == opid).copied()
    }
}

/// Table-driven OpInfo for M extension.
const OP_INFO_M: &[OpInfo] = &[
    OpInfo {
        opid: OP_MUL,
        name: "mul",
        class: OpClass::Mul,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MULH,
        name: "mulh",
        class: OpClass::Mul,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MULHSU,
        name: "mulhsu",
        class: OpClass::Mul,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MULHU,
        name: "mulhu",
        class: OpClass::Mul,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_DIV,
        name: "div",
        class: OpClass::Div,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_DIVU,
        name: "divu",
        class: OpClass::Div,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_REM,
        name: "rem",
        class: OpClass::Div,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_REMU,
        name: "remu",
        class: OpClass::Div,
        size_hint: 4,
    },
    // RV64M
    OpInfo {
        opid: OP_MULW,
        name: "mulw",
        class: OpClass::Mul,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_DIVW,
        name: "divw",
        class: OpClass::Div,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_DIVUW,
        name: "divuw",
        class: OpClass::Div,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_REMW,
        name: "remw",
        class: OpClass::Div,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_REMUW,
        name: "remuw",
        class: OpClass::Div,
        size_hint: 4,
    },
];

fn lift_m<X: Xlen>(args: &InstrArgs, opid: crate::OpId) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        OP_MUL => lift_r(args, |a, b| Expr::mul(a, b)),
        OP_MULH => lift_r(args, |a, b| Expr::mulh(a, b)),
        OP_MULHSU => lift_r(args, |a, b| Expr::mulhsu(a, b)),
        OP_MULHU => lift_r(args, |a, b| Expr::mulhu(a, b)),
        OP_DIV => lift_r(args, |a, b| Expr::div(a, b)),
        OP_DIVU => lift_r(args, |a, b| Expr::divu(a, b)),
        OP_REM => lift_r(args, |a, b| Expr::rem(a, b)),
        OP_REMU => lift_r(args, |a, b| Expr::remu(a, b)),
        OP_MULW => lift_r(args, |a, b| Expr::mulw(a, b)),
        OP_DIVW => lift_r(args, |a, b| Expr::divw(a, b)),
        OP_DIVUW => lift_r(args, |a, b| Expr::divuw(a, b)),
        OP_REMW => lift_r(args, |a, b| Expr::remw(a, b)),
        OP_REMUW => lift_r(args, |a, b| Expr::remuw(a, b)),
        _ => (Vec::new(), Terminator::trap("unknown M instruction")),
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
