//! Zicond extension (integer conditional operations) - decode, lift, disasm.
//!
//! Instructions: czero.eqz, czero.nez

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use super::InstructionExtension;
use crate::{
    DecodedInstr, EXT_ZICOND, InstrArgs, OpClass, OpId, OpInfo,
    encode::{decode_funct3, decode_funct7, decode_rd, decode_rs1, decode_rs2},
    reg_name,
};

// Instruction OpIds
pub const OP_CZERO_EQZ: OpId = OpId::new(EXT_ZICOND, 0);
pub const OP_CZERO_NEZ: OpId = OpId::new(EXT_ZICOND, 1);

// Opcodes
const OPCODE_OP: u32 = 0b0110011;

// Encoding constants
const FUNCT7_ZICOND: u8 = 0b0000111;
const FUNCT3_CZERO_EQZ: u8 = 0b101;
const FUNCT3_CZERO_NEZ: u8 = 0b111;

/// Zicond extension (integer conditional operations).
pub struct ZicondExtension;

impl<X: Xlen> InstructionExtension<X> for ZicondExtension {
    fn name(&self) -> &'static str {
        "Zicond"
    }

    fn ext_id(&self) -> u8 {
        EXT_ZICOND
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = decode_funct3(raw);
        let funct7 = decode_funct7(raw);
        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let rs2 = decode_rs2(raw);

        // R-type on OPCODE_OP: czero.eqz, czero.nez
        if opcode == OPCODE_OP && funct7 == FUNCT7_ZICOND {
            if funct3 == FUNCT3_CZERO_EQZ {
                return Some(DecodedInstr::new(
                    OP_CZERO_EQZ,
                    pc,
                    4,
                    raw,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
            if funct3 == FUNCT3_CZERO_NEZ {
                return Some(DecodedInstr::new(
                    OP_CZERO_NEZ,
                    pc,
                    4,
                    raw,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
        }

        None
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        match instr.opid {
            OP_CZERO_EQZ => lift_czero_eqz::<X>(instr),
            OP_CZERO_NEZ => lift_czero_nez::<X>(instr),
            _ => InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                instr.raw,
                Vec::new(),
                Terminator::trap("unknown Zicond opid"),
            ),
        }
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        match instr.opid {
            OP_CZERO_EQZ => disasm_r(instr, "czero.eqz"),
            OP_CZERO_NEZ => disasm_r(instr, "czero.nez"),
            _ => "???".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_ZICOND
            .iter()
            .find(|info| info.opid == opid)
            .copied()
    }
}

// === Lift helpers ===

fn lift_czero_eqz<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
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
    // czero.eqz: rd = (rs2 == 0) ? 0 : rs1
    let cond = Expr::eq(Expr::reg(rs2), Expr::imm(X::from_u64(0)));
    let result = Expr::select(cond, Expr::imm(X::from_u64(0)), Expr::reg(rs1));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(
        instr.pc,
        instr.size,
        instr.opid.pack(),
        instr.raw,
        vec![stmt],
        Terminator::Fall { target: None },
    )
}

fn lift_czero_nez<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
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
    // czero.nez: rd = (rs2 != 0) ? 0 : rs1
    let cond = Expr::ne(Expr::reg(rs2), Expr::imm(X::from_u64(0)));
    let result = Expr::select(cond, Expr::imm(X::from_u64(0)), Expr::reg(rs1));
    let stmt = Stmt::write_reg(rd, result);
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

/// Table-driven OpInfo for Zicond extension.
const OP_INFO_ZICOND: &[OpInfo] = &[
    OpInfo {
        opid: OP_CZERO_EQZ,
        name: "czero.eqz",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_CZERO_NEZ,
        name: "czero.nez",
        class: OpClass::Alu,
        size_hint: 4,
    },
];

/// Get mnemonic for Zicond instruction.
pub fn zicond_mnemonic(opid: OpId) -> Option<&'static str> {
    OP_INFO_ZICOND
        .iter()
        .find(|info| info.opid == opid)
        .map(|info| info.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    fn encode_r(opcode: u32, funct3: u8, funct7: u8, rd: u8, rs1: u8, rs2: u8) -> [u8; 4] {
        let raw = opcode
            | ((rd as u32) << 7)
            | ((funct3 as u32) << 12)
            | ((rs1 as u32) << 15)
            | ((rs2 as u32) << 20)
            | ((funct7 as u32) << 25);
        raw.to_le_bytes()
    }

    #[test]
    fn test_czero_eqz_decode() {
        let ext = ZicondExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_CZERO_EQZ, FUNCT7_ZICOND, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_CZERO_EQZ);
    }

    #[test]
    fn test_czero_nez_decode() {
        let ext = ZicondExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_CZERO_NEZ, FUNCT7_ZICOND, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_CZERO_NEZ);
    }

    #[test]
    fn test_disasm() {
        let ext = ZicondExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_CZERO_EQZ, FUNCT7_ZICOND, 10, 11, 12);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let disasm = InstructionExtension::<Rv64>::disasm(&ext, &instr);
        assert_eq!(disasm, "czero.eqz a0, a1, a2");
    }

    #[test]
    fn test_lift_czero_eqz() {
        let ext = ZicondExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_CZERO_EQZ, FUNCT7_ZICOND, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let ir = InstructionExtension::<Rv64>::lift(&ext, &instr);
        assert_eq!(ir.statements.len(), 1);
    }

    #[test]
    fn test_op_info() {
        let ext = ZicondExtension;
        let info = InstructionExtension::<Rv64>::op_info(&ext, OP_CZERO_EQZ).unwrap();
        assert_eq!(info.name, "czero.eqz");
        assert_eq!(info.class, OpClass::Alu);
    }
}
