//! Zbb extension (basic bit manipulation) - decode, lift, disasm.
//!
//! Instructions: andn, orn, xnor, clz, clzw, ctz, ctzw, cpop, cpopw,
//!               max, maxu, min, minu, sext.b, sext.h, zext.h,
//!               rol, rolw, ror, rorw, rori, roriw, orc.b, rev8

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use super::InstructionExtension;
use crate::{
    encode::{decode_funct3, decode_funct7, decode_rd, decode_rs1, decode_rs2},
    reg_name, DecodedInstr, InstrArgs, OpClass, OpId, OpInfo, EXT_ZBB,
};

// Instruction OpIds
pub const OP_ANDN: OpId = OpId::new(EXT_ZBB, 0);
pub const OP_ORN: OpId = OpId::new(EXT_ZBB, 1);
pub const OP_XNOR: OpId = OpId::new(EXT_ZBB, 2);
pub const OP_CLZ: OpId = OpId::new(EXT_ZBB, 3);
pub const OP_CTZ: OpId = OpId::new(EXT_ZBB, 4);
pub const OP_CPOP: OpId = OpId::new(EXT_ZBB, 5);
pub const OP_CLZW: OpId = OpId::new(EXT_ZBB, 6);
pub const OP_CTZW: OpId = OpId::new(EXT_ZBB, 7);
pub const OP_CPOPW: OpId = OpId::new(EXT_ZBB, 8);
pub const OP_MAX: OpId = OpId::new(EXT_ZBB, 9);
pub const OP_MAXU: OpId = OpId::new(EXT_ZBB, 10);
pub const OP_MIN: OpId = OpId::new(EXT_ZBB, 11);
pub const OP_MINU: OpId = OpId::new(EXT_ZBB, 12);
pub const OP_SEXT_B: OpId = OpId::new(EXT_ZBB, 13);
pub const OP_SEXT_H: OpId = OpId::new(EXT_ZBB, 14);
pub const OP_ZEXT_H: OpId = OpId::new(EXT_ZBB, 15);
pub const OP_ROL: OpId = OpId::new(EXT_ZBB, 16);
pub const OP_ROR: OpId = OpId::new(EXT_ZBB, 17);
pub const OP_RORI: OpId = OpId::new(EXT_ZBB, 18);
pub const OP_ROLW: OpId = OpId::new(EXT_ZBB, 19);
pub const OP_RORW: OpId = OpId::new(EXT_ZBB, 20);
pub const OP_RORIW: OpId = OpId::new(EXT_ZBB, 21);
pub const OP_ORC_B: OpId = OpId::new(EXT_ZBB, 22);
pub const OP_REV8: OpId = OpId::new(EXT_ZBB, 23);

// Opcodes
const OPCODE_OP: u32 = 0b0110011;
const OPCODE_OP_IMM: u32 = 0b0010011;
const OPCODE_OP_32: u32 = 0b0111011;
const OPCODE_OP_IMM_32: u32 = 0b0011011;

// Encoding constants
const FUNCT7_ANDN_ORN_XNOR: u8 = 0b0100000;
const FUNCT7_MINMAX: u8 = 0b0000101;
const FUNCT7_ROL_ROR: u8 = 0b0110000;
const FUNCT7_ZEXT_H: u8 = 0b0000100;

// imm[11:0] for unary ops
const IMM_CLZ: u32 = 0b011000000000;
const IMM_CTZ: u32 = 0b011000000001;
const IMM_CPOP: u32 = 0b011000000010;
const IMM_SEXT_B: u32 = 0b011000000100;
const IMM_SEXT_H: u32 = 0b011000000101;
const IMM_ORC_B: u32 = 0b001010000111;
const IMM_REV8_32: u32 = 0b011010011000;
const IMM_REV8_64: u32 = 0b011010111000;

/// Zbb extension (basic bit manipulation).
pub struct ZbbExtension;

impl<X: Xlen> InstructionExtension<X> for ZbbExtension {
    fn name(&self) -> &'static str {
        "Zbb"
    }

    fn ext_id(&self) -> u8 {
        EXT_ZBB
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = decode_funct3(raw);
        let funct7 = decode_funct7(raw);
        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let rs2 = decode_rs2(raw);
        let imm12 = (raw >> 20) & 0xFFF;

        // R-type on OPCODE_OP
        if opcode == OPCODE_OP {
            // andn, orn, xnor (funct7=0100000)
            if funct7 == FUNCT7_ANDN_ORN_XNOR {
                let opid = match funct3 {
                    0b111 => OP_ANDN,
                    0b110 => OP_ORN,
                    0b100 => OP_XNOR,
                    _ => return None,
                };
                return Some(DecodedInstr::new(
                    opid,
                    pc,
                    4,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
            // min, max, minu, maxu (funct7=0000101)
            if funct7 == FUNCT7_MINMAX {
                let opid = match funct3 {
                    0b100 => OP_MIN,
                    0b101 => OP_MINU,
                    0b110 => OP_MAX,
                    0b111 => OP_MAXU,
                    _ => return None,
                };
                return Some(DecodedInstr::new(
                    opid,
                    pc,
                    4,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
            // rol, ror (funct7=0110000)
            if funct7 == FUNCT7_ROL_ROR {
                let opid = match funct3 {
                    0b001 => OP_ROL,
                    0b101 => OP_ROR,
                    _ => return None,
                };
                return Some(DecodedInstr::new(
                    opid,
                    pc,
                    4,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
            // zext.h on RV32 (funct7=0000100, funct3=100, rs2=0)
            if funct7 == FUNCT7_ZEXT_H && funct3 == 0b100 && rs2 == 0 && X::VALUE == 32 {
                return Some(DecodedInstr::new(
                    OP_ZEXT_H,
                    pc,
                    4,
                    InstrArgs::I { rd, rs1, imm: 0 },
                ));
            }
        }

        // I-type on OPCODE_OP_IMM
        if opcode == OPCODE_OP_IMM {
            // Unary operations (funct3=001)
            if funct3 == 0b001 {
                let opid = match imm12 {
                    IMM_CLZ => OP_CLZ,
                    IMM_CTZ => OP_CTZ,
                    IMM_CPOP => OP_CPOP,
                    IMM_SEXT_B => OP_SEXT_B,
                    IMM_SEXT_H => OP_SEXT_H,
                    _ => return None,
                };
                return Some(DecodedInstr::new(
                    opid,
                    pc,
                    4,
                    InstrArgs::I { rd, rs1, imm: 0 },
                ));
            }
            // rori, orc.b, rev8 (funct3=101)
            if funct3 == 0b101 {
                if imm12 == IMM_ORC_B {
                    return Some(DecodedInstr::new(
                        OP_ORC_B,
                        pc,
                        4,
                        InstrArgs::I { rd, rs1, imm: 0 },
                    ));
                }
                let rev8_imm = if X::VALUE == 32 {
                    IMM_REV8_32
                } else {
                    IMM_REV8_64
                };
                if imm12 == rev8_imm {
                    return Some(DecodedInstr::new(
                        OP_REV8,
                        pc,
                        4,
                        InstrArgs::I { rd, rs1, imm: 0 },
                    ));
                }
                // rori: check funct6
                let funct6 = (raw >> 26) & 0x3F;
                if funct6 == 0b011000 {
                    // RV32: bit 25 must be 0
                    if X::VALUE == 32 && ((raw >> 25) & 1) != 0 {
                        return None;
                    }
                    let shamt = if X::VALUE == 32 {
                        ((raw >> 20) & 0x1F) as i32
                    } else {
                        ((raw >> 20) & 0x3F) as i32
                    };
                    return Some(DecodedInstr::new(
                        OP_RORI,
                        pc,
                        4,
                        InstrArgs::I {
                            rd,
                            rs1,
                            imm: shamt,
                        },
                    ));
                }
            }
        }

        // RV64-only: OPCODE_OP_32 (zext.h, rolw, rorw)
        if opcode == OPCODE_OP_32 && X::VALUE == 64 {
            // zext.h on RV64 (funct7=0000100, funct3=100, rs2=0)
            if funct7 == FUNCT7_ZEXT_H && funct3 == 0b100 && rs2 == 0 {
                return Some(DecodedInstr::new(
                    OP_ZEXT_H,
                    pc,
                    4,
                    InstrArgs::I { rd, rs1, imm: 0 },
                ));
            }
            // rolw, rorw (funct7=0110000)
            if funct7 == FUNCT7_ROL_ROR {
                let opid = match funct3 {
                    0b001 => OP_ROLW,
                    0b101 => OP_RORW,
                    _ => return None,
                };
                return Some(DecodedInstr::new(
                    opid,
                    pc,
                    4,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
        }

        // RV64-only: OPCODE_OP_IMM_32 (clzw, ctzw, cpopw, roriw)
        if opcode == OPCODE_OP_IMM_32 && X::VALUE == 64 {
            if funct3 == 0b001 && funct7 == FUNCT7_ROL_ROR {
                let rs2_val = (raw >> 20) & 0x1F;
                let opid = match rs2_val {
                    0b00000 => OP_CLZW,
                    0b00001 => OP_CTZW,
                    0b00010 => OP_CPOPW,
                    _ => return None,
                };
                return Some(DecodedInstr::new(
                    opid,
                    pc,
                    4,
                    InstrArgs::I { rd, rs1, imm: 0 },
                ));
            }
            if funct3 == 0b101 && funct7 == FUNCT7_ROL_ROR {
                let shamt = ((raw >> 20) & 0x1F) as i32;
                return Some(DecodedInstr::new(
                    OP_RORIW,
                    pc,
                    4,
                    InstrArgs::I {
                        rd,
                        rs1,
                        imm: shamt,
                    },
                ));
            }
        }

        None
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        match instr.opid {
            OP_ANDN => lift_andn::<X>(instr),
            OP_ORN => lift_orn::<X>(instr),
            OP_XNOR => lift_xnor::<X>(instr),
            OP_CLZ => lift_unary::<X, _>(instr, |v| Expr::clz(v)),
            OP_CTZ => lift_unary::<X, _>(instr, |v| Expr::ctz(v)),
            OP_CPOP => lift_unary::<X, _>(instr, |v| Expr::cpop(v)),
            OP_CLZW => lift_unary::<X, _>(instr, |v| Expr::clz32(v)),
            OP_CTZW => lift_unary::<X, _>(instr, |v| Expr::ctz32(v)),
            OP_CPOPW => lift_unary::<X, _>(instr, |v| Expr::cpop32(v)),
            OP_MAX => lift_binary::<X, _>(instr, Expr::max),
            OP_MAXU => lift_binary::<X, _>(instr, Expr::maxu),
            OP_MIN => lift_binary::<X, _>(instr, Expr::min),
            OP_MINU => lift_binary::<X, _>(instr, Expr::minu),
            OP_SEXT_B => lift_unary::<X, _>(instr, |v| Expr::sext8(v)),
            OP_SEXT_H => lift_unary::<X, _>(instr, |v| Expr::sext16(v)),
            OP_ZEXT_H => lift_unary::<X, _>(instr, |v| Expr::zext16(v)),
            OP_ROL => lift_rol::<X>(instr),
            OP_ROR => lift_ror::<X>(instr),
            OP_RORI => lift_rori::<X>(instr),
            OP_ROLW => lift_rolw::<X>(instr),
            OP_RORW => lift_rorw::<X>(instr),
            OP_RORIW => lift_roriw::<X>(instr),
            OP_ORC_B => lift_unary::<X, _>(instr, |v| Expr::orc8(v)),
            OP_REV8 => lift_unary::<X, _>(instr, |v| Expr::rev8(v)),
            _ => InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("unknown Zbb opid"),
            ),
        }
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        match instr.opid {
            OP_ANDN => disasm_r(instr, "andn"),
            OP_ORN => disasm_r(instr, "orn"),
            OP_XNOR => disasm_r(instr, "xnor"),
            OP_CLZ => disasm_unary(instr, "clz"),
            OP_CTZ => disasm_unary(instr, "ctz"),
            OP_CPOP => disasm_unary(instr, "cpop"),
            OP_CLZW => disasm_unary(instr, "clzw"),
            OP_CTZW => disasm_unary(instr, "ctzw"),
            OP_CPOPW => disasm_unary(instr, "cpopw"),
            OP_MAX => disasm_r(instr, "max"),
            OP_MAXU => disasm_r(instr, "maxu"),
            OP_MIN => disasm_r(instr, "min"),
            OP_MINU => disasm_r(instr, "minu"),
            OP_SEXT_B => disasm_unary(instr, "sext.b"),
            OP_SEXT_H => disasm_unary(instr, "sext.h"),
            OP_ZEXT_H => disasm_unary(instr, "zext.h"),
            OP_ROL => disasm_r(instr, "rol"),
            OP_ROR => disasm_r(instr, "ror"),
            OP_RORI => disasm_shift(instr, "rori"),
            OP_ROLW => disasm_r(instr, "rolw"),
            OP_RORW => disasm_r(instr, "rorw"),
            OP_RORIW => disasm_shift(instr, "roriw"),
            OP_ORC_B => disasm_unary(instr, "orc.b"),
            OP_REV8 => disasm_unary(instr, "rev8"),
            _ => "???".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_ZBB.iter().find(|info| info.opid == opid).copied()
    }
}

// === Lift helpers ===

fn lift_andn<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // rd = rs1 & ~rs2
    let result = Expr::and(Expr::reg(rs1), Expr::not(Expr::reg(rs2)));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_orn<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // rd = rs1 | ~rs2
    let result = Expr::or(Expr::reg(rs1), Expr::not(Expr::reg(rs2)));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_xnor<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // rd = ~(rs1 ^ rs2)
    let result = Expr::not(Expr::xor(Expr::reg(rs1), Expr::reg(rs2)));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_unary<X: Xlen, F>(instr: &DecodedInstr<X>, op: F) -> InstrIR<X>
where
    F: FnOnce(Expr<X>) -> Expr<X>,
{
    let (rd, rs1) = match instr.args {
        InstrArgs::I { rd, rs1, .. } => (rd, rs1),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    let result = op(Expr::reg(rs1));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_binary<X: Xlen, F>(instr: &DecodedInstr<X>, op: F) -> InstrIR<X>
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    let result = op(Expr::reg(rs1), Expr::reg(rs2));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_rol<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // rd = (rs1 << (rs2 & mask)) | (rs1 >> ((XLEN - rs2) & mask))
    let mask = X::from_u64((X::VALUE - 1) as u64);
    let xlen = X::from_u64(X::VALUE as u64);
    let val = Expr::reg(rs1);
    let shamt = Expr::and(Expr::reg(rs2), Expr::imm(mask));
    let complement = Expr::and(Expr::sub(Expr::imm(xlen), Expr::reg(rs2)), Expr::imm(mask));
    let left = Expr::sll(val.clone(), shamt);
    let right = Expr::srl(val, complement);
    let result = Expr::or(left, right);
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_ror<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // rd = (rs1 >> (rs2 & mask)) | (rs1 << ((XLEN - rs2) & mask))
    let mask = X::from_u64((X::VALUE - 1) as u64);
    let xlen = X::from_u64(X::VALUE as u64);
    let val = Expr::reg(rs1);
    let shamt = Expr::and(Expr::reg(rs2), Expr::imm(mask));
    let complement = Expr::and(Expr::sub(Expr::imm(xlen), Expr::reg(rs2)), Expr::imm(mask));
    let right = Expr::srl(val.clone(), shamt);
    let left = Expr::sll(val, complement);
    let result = Expr::or(right, left);
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_rori<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, shamt) = match instr.args {
        InstrArgs::I { rd, rs1, imm } => (rd, rs1, imm as u8),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // rd = (rs1 >> shamt) | (rs1 << (XLEN - shamt))
    let xlen = X::VALUE as u64;
    let complement = (xlen - shamt as u64) & (xlen - 1);
    let val = Expr::reg(rs1);
    let right = Expr::srl(val.clone(), Expr::imm(X::from_u64(shamt as u64)));
    let left = Expr::sll(val, Expr::imm(X::from_u64(complement)));
    let result = Expr::or(right, left);
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_rolw<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // 32-bit rotate left, sign-extend result
    // rd = sext32((rs1[31:0] << (rs2 & 0x1F)) | (rs1[31:0] >> (32 - (rs2 & 0x1F))))
    let val = Expr::zext32(Expr::reg(rs1));
    let shamt = Expr::and(Expr::reg(rs2), Expr::imm(X::from_u64(0x1F)));
    let complement = Expr::and(
        Expr::sub(Expr::imm(X::from_u64(32)), Expr::reg(rs2)),
        Expr::imm(X::from_u64(0x1F)),
    );
    let left = Expr::sll(val.clone(), shamt);
    let right = Expr::srl(val, complement);
    let result = Expr::sext32(Expr::or(left, right));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_rorw<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // 32-bit rotate right, sign-extend result
    let val = Expr::zext32(Expr::reg(rs1));
    let shamt = Expr::and(Expr::reg(rs2), Expr::imm(X::from_u64(0x1F)));
    let complement = Expr::and(
        Expr::sub(Expr::imm(X::from_u64(32)), Expr::reg(rs2)),
        Expr::imm(X::from_u64(0x1F)),
    );
    let right = Expr::srl(val.clone(), shamt);
    let left = Expr::sll(val, complement);
    let result = Expr::sext32(Expr::or(right, left));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_roriw<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, shamt) = match instr.args {
        InstrArgs::I { rd, rs1, imm } => (rd, rs1, imm as u8),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                Vec::new(),
                Terminator::trap("bad args"),
            )
        }
    };
    // 32-bit rotate right immediate, sign-extend result
    let complement = (32 - shamt as u64) & 0x1F;
    let val = Expr::zext32(Expr::reg(rs1));
    let right = Expr::srl(val.clone(), Expr::imm(X::from_u64(shamt as u64)));
    let left = Expr::sll(val, Expr::imm(X::from_u64(complement)));
    let result = Expr::sext32(Expr::or(right, left));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

// === Disasm helpers ===

fn disasm_r<X: Xlen>(instr: &DecodedInstr<X>, mnemonic: &str) -> String {
    match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => {
            format!(
                "{} {}, {}, {}",
                mnemonic,
                reg_name(rd),
                reg_name(rs1),
                reg_name(rs2)
            )
        }
        _ => format!("{} ???", mnemonic),
    }
}

fn disasm_unary<X: Xlen>(instr: &DecodedInstr<X>, mnemonic: &str) -> String {
    match instr.args {
        InstrArgs::I { rd, rs1, .. } => {
            format!("{} {}, {}", mnemonic, reg_name(rd), reg_name(rs1))
        }
        _ => format!("{} ???", mnemonic),
    }
}

fn disasm_shift<X: Xlen>(instr: &DecodedInstr<X>, mnemonic: &str) -> String {
    match instr.args {
        InstrArgs::I { rd, rs1, imm } => {
            format!("{} {}, {}, {}", mnemonic, reg_name(rd), reg_name(rs1), imm)
        }
        _ => format!("{} ???", mnemonic),
    }
}

/// Table-driven OpInfo for Zbb extension.
const OP_INFO_ZBB: &[OpInfo] = &[
    OpInfo {
        opid: OP_ANDN,
        name: "andn",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ORN,
        name: "orn",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_XNOR,
        name: "xnor",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_CLZ,
        name: "clz",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_CTZ,
        name: "ctz",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_CPOP,
        name: "cpop",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_CLZW,
        name: "clzw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_CTZW,
        name: "ctzw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_CPOPW,
        name: "cpopw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MAX,
        name: "max",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MAXU,
        name: "maxu",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MIN,
        name: "min",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MINU,
        name: "minu",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SEXT_B,
        name: "sext.b",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SEXT_H,
        name: "sext.h",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ZEXT_H,
        name: "zext.h",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ROL,
        name: "rol",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ROR,
        name: "ror",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_RORI,
        name: "rori",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ROLW,
        name: "rolw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_RORW,
        name: "rorw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_RORIW,
        name: "roriw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ORC_B,
        name: "orc.b",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_REV8,
        name: "rev8",
        class: OpClass::Alu,
        size_hint: 4,
    },
];

/// Get mnemonic for Zbb instruction.
pub fn zbb_mnemonic(opid: OpId) -> Option<&'static str> {
    OP_INFO_ZBB
        .iter()
        .find(|info| info.opid == opid)
        .map(|info| info.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    fn encode_r(opcode: u32, funct3: u32, funct7: u8, rd: u8, rs1: u8, rs2: u8) -> [u8; 4] {
        let raw = opcode
            | ((rd as u32) << 7)
            | (funct3 << 12)
            | ((rs1 as u32) << 15)
            | ((rs2 as u32) << 20)
            | ((funct7 as u32) << 25);
        raw.to_le_bytes()
    }

    fn encode_i(opcode: u32, funct3: u32, rd: u8, rs1: u8, imm12: u32) -> [u8; 4] {
        let raw =
            opcode | ((rd as u32) << 7) | (funct3 << 12) | ((rs1 as u32) << 15) | (imm12 << 20);
        raw.to_le_bytes()
    }

    #[test]
    fn test_andn_decode() {
        let ext = ZbbExtension;
        let bytes = encode_r(OPCODE_OP, 0b111, FUNCT7_ANDN_ORN_XNOR, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_ANDN);
    }

    #[test]
    fn test_clz_decode() {
        let ext = ZbbExtension;
        let bytes = encode_i(OPCODE_OP_IMM, 0b001, 1, 2, IMM_CLZ);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_CLZ);
    }

    #[test]
    fn test_max_decode() {
        let ext = ZbbExtension;
        let bytes = encode_r(OPCODE_OP, 0b110, FUNCT7_MINMAX, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_MAX);
    }

    #[test]
    fn test_rol_decode() {
        let ext = ZbbExtension;
        let bytes = encode_r(OPCODE_OP, 0b001, FUNCT7_ROL_ROR, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_ROL);
    }

    #[test]
    fn test_rori_decode() {
        let ext = ZbbExtension;
        // rori rd, rs1, 5 (shamt=5)
        let imm = (0b011000u32 << 6) | 5;
        let bytes = encode_i(OPCODE_OP_IMM, 0b101, 1, 2, imm);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_RORI);
        match instr.args {
            InstrArgs::I { imm, .. } => assert_eq!(imm, 5),
            _ => panic!("Expected I-type args"),
        }
    }

    #[test]
    fn test_rev8_decode() {
        let ext = ZbbExtension;
        let bytes = encode_i(OPCODE_OP_IMM, 0b101, 1, 2, IMM_REV8_64);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_REV8);
    }

    #[test]
    fn test_disasm() {
        let ext = ZbbExtension;
        let bytes = encode_r(OPCODE_OP, 0b111, FUNCT7_ANDN_ORN_XNOR, 10, 11, 12);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let disasm = InstructionExtension::<Rv64>::disasm(&ext, &instr);
        assert_eq!(disasm, "andn a0, a1, a2");
    }

    #[test]
    fn test_lift_andn() {
        let ext = ZbbExtension;
        let bytes = encode_r(OPCODE_OP, 0b111, FUNCT7_ANDN_ORN_XNOR, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let ir = InstructionExtension::<Rv64>::lift(&ext, &instr);
        assert_eq!(ir.statements.len(), 1);
    }

    #[test]
    fn test_op_info() {
        let ext = ZbbExtension;
        let info = InstructionExtension::<Rv64>::op_info(&ext, OP_CLZ).unwrap();
        assert_eq!(info.name, "clz");
        assert_eq!(info.class, OpClass::Alu);
    }
}
