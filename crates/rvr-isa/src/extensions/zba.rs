//! Zba extension (address generation) - decode, lift, disasm.
//!
//! Instructions: sh1add, sh2add, sh3add, add.uw, sh1add.uw, sh2add.uw, sh3add.uw, slli.uw

use rvr_ir::{Expr, InstrIR, Terminator, Xlen};

use super::InstructionExtension;
use crate::{
    DecodedInstr, EXT_ZBA, InstrArgs, OpClass, OpId, OpInfo,
    encode::{decode_funct3, decode_funct7, decode_rd, decode_rs1, decode_rs2},
    reg_name,
};

// Instruction indices
pub const OP_SH1ADD: OpId = OpId::new(EXT_ZBA, 0);
pub const OP_SH2ADD: OpId = OpId::new(EXT_ZBA, 1);
pub const OP_SH3ADD: OpId = OpId::new(EXT_ZBA, 2);
pub const OP_ADD_UW: OpId = OpId::new(EXT_ZBA, 3);
pub const OP_SH1ADD_UW: OpId = OpId::new(EXT_ZBA, 4);
pub const OP_SH2ADD_UW: OpId = OpId::new(EXT_ZBA, 5);
pub const OP_SH3ADD_UW: OpId = OpId::new(EXT_ZBA, 6);
pub const OP_SLLI_UW: OpId = OpId::new(EXT_ZBA, 7);

// Opcodes
const OPCODE_OP: u32 = 0b0110011; // R-type 32-bit
const OPCODE_OP_32: u32 = 0b0111011; // R-type 64-bit word ops
const OPCODE_OP_IMM_32: u32 = 0b0011011; // I-type 64-bit word ops

// Encoding constants
const FUNCT7_SHXADD: u8 = 0b0010000;
const FUNCT7_ADD_UW: u8 = 0b0000100;
const FUNCT6_SLLI_UW: u32 = 0b000010;

/// Zba extension (address generation).
pub struct ZbaExtension;

impl<X: Xlen> InstructionExtension<X> for ZbaExtension {
    fn name(&self) -> &'static str {
        "Zba"
    }

    fn ext_id(&self) -> u8 {
        EXT_ZBA
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = decode_funct3(raw);
        let funct7 = decode_funct7(raw);
        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let rs2 = decode_rs2(raw);

        // sh1add, sh2add, sh3add (OPCODE_OP with funct7=0010000)
        if opcode == OPCODE_OP && funct7 == FUNCT7_SHXADD {
            let opid = match funct3 {
                0b010 => OP_SH1ADD,
                0b100 => OP_SH2ADD,
                0b110 => OP_SH3ADD,
                _ => return None,
            };
            return Some(DecodedInstr::new(
                opid,
                pc,
                4,
                raw,
                InstrArgs::R { rd, rs1, rs2 },
            ));
        }

        // RV64-only: add.uw, sh1add.uw, sh2add.uw, sh3add.uw (OPCODE_OP_32)
        if opcode == OPCODE_OP_32 && X::VALUE == 64 {
            // add.uw (funct7=0000100, funct3=000)
            if funct7 == FUNCT7_ADD_UW && funct3 == 0b000 {
                return Some(DecodedInstr::new(
                    OP_ADD_UW,
                    pc,
                    4,
                    raw,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
            // sh1add.uw, sh2add.uw, sh3add.uw (funct7=0010000)
            if funct7 == FUNCT7_SHXADD {
                let opid = match funct3 {
                    0b010 => OP_SH1ADD_UW,
                    0b100 => OP_SH2ADD_UW,
                    0b110 => OP_SH3ADD_UW,
                    _ => return None,
                };
                return Some(DecodedInstr::new(
                    opid,
                    pc,
                    4,
                    raw,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
        }

        // RV64-only: slli.uw (OPCODE_OP_IMM_32, funct3=001, funct6=000010)
        if opcode == OPCODE_OP_IMM_32 && X::VALUE == 64 {
            let funct6 = (raw >> 26) & 0x3F;
            if funct3 == 0b001 && funct6 == FUNCT6_SLLI_UW {
                let shamt = ((raw >> 20) & 0x3F) as i32;
                return Some(DecodedInstr::new(
                    OP_SLLI_UW,
                    pc,
                    4,
                    raw,
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
            OP_SH1ADD => lift_shxadd::<X>(instr, 1),
            OP_SH2ADD => lift_shxadd::<X>(instr, 2),
            OP_SH3ADD => lift_shxadd::<X>(instr, 3),
            OP_ADD_UW => lift_add_uw::<X>(instr),
            OP_SH1ADD_UW => lift_shxadd_uw::<X>(instr, 1),
            OP_SH2ADD_UW => lift_shxadd_uw::<X>(instr, 2),
            OP_SH3ADD_UW => lift_shxadd_uw::<X>(instr, 3),
            OP_SLLI_UW => lift_slli_uw::<X>(instr),
            _ => InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                instr.raw,
                Vec::new(),
                Terminator::trap("unknown Zba opid"),
            ),
        }
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        match instr.opid {
            OP_SH1ADD => disasm_r(instr, "sh1add"),
            OP_SH2ADD => disasm_r(instr, "sh2add"),
            OP_SH3ADD => disasm_r(instr, "sh3add"),
            OP_ADD_UW => disasm_r(instr, "add.uw"),
            OP_SH1ADD_UW => disasm_r(instr, "sh1add.uw"),
            OP_SH2ADD_UW => disasm_r(instr, "sh2add.uw"),
            OP_SH3ADD_UW => disasm_r(instr, "sh3add.uw"),
            OP_SLLI_UW => disasm_i(instr, "slli.uw"),
            _ => "???".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_ZBA.iter().find(|info| info.opid == opid).copied()
    }
}

// === Lift helpers ===

fn lift_shxadd<X: Xlen>(instr: &DecodedInstr<X>, shift: u8) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                instr.raw,
                Vec::new(),
                Terminator::trap("bad args"),
            );
        }
    };

    // rd = rs2 + (rs1 << shift)
    // Note: add(reg(0), x) is optimized to x by Expr::add
    let shifted = Expr::sll(Expr::reg(rs1), Expr::imm(X::from_u64(shift as u64)));
    let result = Expr::add(Expr::reg(rs2), shifted);
    let stmt = rvr_ir::Stmt::write_reg(rd, result);

    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        instr.raw,
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_add_uw<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                instr.raw,
                Vec::new(),
                Terminator::trap("bad args"),
            );
        }
    };

    // rd = rs2 + zext32(rs1)
    // Note: add(reg(0), x) is optimized to x by Expr::add
    let rs1_zext = Expr::zext32(Expr::reg(rs1));
    let result = Expr::add(Expr::reg(rs2), rs1_zext);
    let stmt = rvr_ir::Stmt::write_reg(rd, result);

    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        instr.raw,
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_shxadd_uw<X: Xlen>(instr: &DecodedInstr<X>, shift: u8) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                instr.raw,
                Vec::new(),
                Terminator::trap("bad args"),
            );
        }
    };

    // rd = rs2 + (zext32(rs1) << shift)
    // Note: add(reg(0), x) is optimized to x by Expr::add
    let rs1_zext = Expr::zext32(Expr::reg(rs1));
    let shifted = Expr::sll(rs1_zext, Expr::imm(X::from_u64(shift as u64)));
    let result = Expr::add(Expr::reg(rs2), shifted);
    let stmt = rvr_ir::Stmt::write_reg(rd, result);

    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        instr.raw,
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_slli_uw<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, shamt) = match instr.args {
        InstrArgs::I { rd, rs1, imm } => (rd, rs1, imm as u8),
        _ => {
            return InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                instr.raw,
                Vec::new(),
                Terminator::trap("bad args"),
            );
        }
    };

    // rd = zext32(rs1) << shamt
    let rs1_zext = Expr::zext32(Expr::reg(rs1));
    let result = Expr::sll(rs1_zext, Expr::imm(X::from_u64(shamt as u64)));
    let stmt = rvr_ir::Stmt::write_reg(rd, result);

    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        instr.raw,
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

fn disasm_i<X: Xlen>(instr: &DecodedInstr<X>, mnemonic: &str) -> String {
    match instr.args {
        InstrArgs::I { rd, rs1, imm } => {
            format!("{} {}, {}, {}", mnemonic, reg_name(rd), reg_name(rs1), imm)
        }
        _ => format!("{} ???", mnemonic),
    }
}

/// Table-driven OpInfo for Zba extension.
const OP_INFO_ZBA: &[OpInfo] = &[
    OpInfo {
        opid: OP_SH1ADD,
        name: "sh1add",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SH2ADD,
        name: "sh2add",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SH3ADD,
        name: "sh3add",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ADD_UW,
        name: "add.uw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SH1ADD_UW,
        name: "sh1add.uw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SH2ADD_UW,
        name: "sh2add.uw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SH3ADD_UW,
        name: "sh3add.uw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLLI_UW,
        name: "slli.uw",
        class: OpClass::Alu,
        size_hint: 4,
    },
];

/// Get mnemonic for Zba instruction.
pub fn zba_mnemonic(opid: OpId) -> Option<&'static str> {
    OP_INFO_ZBA
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

    fn encode_i(opcode: u32, funct3: u32, rd: u8, rs1: u8, imm: u32) -> [u8; 4] {
        let raw = opcode | ((rd as u32) << 7) | (funct3 << 12) | ((rs1 as u32) << 15) | (imm << 20);
        raw.to_le_bytes()
    }

    #[test]
    fn test_sh1add_decode() {
        let ext = ZbaExtension;
        // sh1add x1, x2, x3
        let bytes = encode_r(OPCODE_OP, 0b010, FUNCT7_SHXADD, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_SH1ADD);
        assert_eq!(
            instr.args,
            InstrArgs::R {
                rd: 1,
                rs1: 2,
                rs2: 3
            }
        );
    }

    #[test]
    fn test_sh2add_decode() {
        let ext = ZbaExtension;
        let bytes = encode_r(OPCODE_OP, 0b100, FUNCT7_SHXADD, 5, 6, 7);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_SH2ADD);
    }

    #[test]
    fn test_sh3add_decode() {
        let ext = ZbaExtension;
        let bytes = encode_r(OPCODE_OP, 0b110, FUNCT7_SHXADD, 10, 11, 12);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_SH3ADD);
    }

    #[test]
    fn test_add_uw_decode() {
        let ext = ZbaExtension;
        // add.uw x1, x2, x3
        let bytes = encode_r(OPCODE_OP_32, 0b000, FUNCT7_ADD_UW, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_ADD_UW);
    }

    #[test]
    fn test_slli_uw_decode() {
        let ext = ZbaExtension;
        // slli.uw x1, x2, 5
        let imm = (FUNCT6_SLLI_UW << 6) | 5;
        let bytes = encode_i(OPCODE_OP_IMM_32, 0b001, 1, 2, imm);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_SLLI_UW);
        match instr.args {
            InstrArgs::I { rd, rs1, imm } => {
                assert_eq!(rd, 1);
                assert_eq!(rs1, 2);
                assert_eq!(imm, 5);
            }
            _ => panic!("Expected I-type args"),
        }
    }

    #[test]
    fn test_disasm() {
        let ext = ZbaExtension;
        let bytes = encode_r(OPCODE_OP, 0b010, FUNCT7_SHXADD, 10, 11, 12);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let disasm = InstructionExtension::<Rv64>::disasm(&ext, &instr);
        assert_eq!(disasm, "sh1add a0, a1, a2");
    }

    #[test]
    fn test_lift_sh1add() {
        let ext = ZbaExtension;
        let bytes = encode_r(OPCODE_OP, 0b010, FUNCT7_SHXADD, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let ir = InstructionExtension::<Rv64>::lift(&ext, &instr);
        assert_eq!(ir.statements.len(), 1);
        assert!(!ir.terminator.is_control_flow());
    }

    #[test]
    fn test_op_info() {
        let ext = ZbaExtension;
        let info = InstructionExtension::<Rv64>::op_info(&ext, OP_SH1ADD).unwrap();
        assert_eq!(info.name, "sh1add");
        assert_eq!(info.class, OpClass::Alu);
    }
}
