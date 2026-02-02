//! Zbkb extension (bit manipulation for cryptography) - decode, lift, disasm.
//!
//! Instructions: pack, packh, packw, brev8, zip, unzip
//!
//! Note: rol, ror, rori, rolw, rorw, roriw, andn, orn, xnor, rev8 are shared
//! with Zbb and already implemented there.

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use super::InstructionExtension;
use crate::{
    DecodedInstr, EXT_ZBKB, InstrArgs, OpClass, OpId, OpInfo,
    encode::{decode_funct3, decode_funct7, decode_rd, decode_rs1, decode_rs2},
    reg_name,
};

// Instruction OpIds
pub const OP_PACK: OpId = OpId::new(EXT_ZBKB, 0);
pub const OP_PACKH: OpId = OpId::new(EXT_ZBKB, 1);
pub const OP_PACKW: OpId = OpId::new(EXT_ZBKB, 2);
pub const OP_BREV8: OpId = OpId::new(EXT_ZBKB, 3);
pub const OP_ZIP: OpId = OpId::new(EXT_ZBKB, 4);
pub const OP_UNZIP: OpId = OpId::new(EXT_ZBKB, 5);

// Opcodes
const OPCODE_OP: u32 = 0b0110011;
const OPCODE_OP_IMM: u32 = 0b0010011;
const OPCODE_OP_32: u32 = 0b0111011;

// Encoding constants
const FUNCT7_PACK: u8 = 0b0000100;
const FUNCT3_PACK: u8 = 0b100;
const FUNCT3_PACKH: u8 = 0b111;

// BREV8 is encoded as grevi with shamt=7
const IMM_BREV8: u32 = 0b011010000111;

// ZIP/UNZIP (RV32 only)
const IMM_ZIP: u32 = 0b000010001111;
const IMM_UNZIP: u32 = 0b000010001111;

/// Zbkb extension (bit manipulation for cryptography).
pub struct ZbkbExtension;

impl<X: Xlen> InstructionExtension<X> for ZbkbExtension {
    fn name(&self) -> &'static str {
        "Zbkb"
    }

    fn ext_id(&self) -> u8 {
        EXT_ZBKB
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = decode_funct3(raw);
        let funct7 = decode_funct7(raw);
        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let rs2 = decode_rs2(raw);
        let imm12 = (raw >> 20) & 0xFFF;

        // R-type on OPCODE_OP: pack, packh
        if opcode == OPCODE_OP && funct7 == FUNCT7_PACK {
            if funct3 == FUNCT3_PACK {
                return Some(DecodedInstr::new(
                    OP_PACK,
                    pc,
                    4,
                    raw,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
            if funct3 == FUNCT3_PACKH {
                return Some(DecodedInstr::new(
                    OP_PACKH,
                    pc,
                    4,
                    raw,
                    InstrArgs::R { rd, rs1, rs2 },
                ));
            }
        }

        // I-type on OPCODE_OP_IMM: brev8, zip, unzip
        if opcode == OPCODE_OP_IMM {
            if funct3 == 0b101 && imm12 == IMM_BREV8 {
                return Some(DecodedInstr::new(
                    OP_BREV8,
                    pc,
                    4,
                    raw,
                    InstrArgs::I { rd, rs1, imm: 0 },
                ));
            }
            // ZIP/UNZIP: RV32 only
            if X::VALUE == 32 {
                if funct3 == 0b101 && imm12 == IMM_UNZIP {
                    return Some(DecodedInstr::new(
                        OP_UNZIP,
                        pc,
                        4,
                        raw,
                        InstrArgs::I { rd, rs1, imm: 0 },
                    ));
                }
                if funct3 == 0b001 && imm12 == IMM_ZIP {
                    return Some(DecodedInstr::new(
                        OP_ZIP,
                        pc,
                        4,
                        raw,
                        InstrArgs::I { rd, rs1, imm: 0 },
                    ));
                }
            }
        }

        // R-type on OPCODE_OP_32: packw (RV64 only)
        if opcode == OPCODE_OP_32
            && X::VALUE == 64
            && funct7 == FUNCT7_PACK
            && funct3 == FUNCT3_PACK
        {
            return Some(DecodedInstr::new(
                OP_PACKW,
                pc,
                4,
                raw,
                InstrArgs::R { rd, rs1, rs2 },
            ));
        }

        None
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        match instr.opid {
            OP_PACK => lift_pack::<X>(instr),
            OP_PACKH => lift_packh::<X>(instr),
            OP_PACKW => lift_packw::<X>(instr),
            OP_BREV8 => lift_unary::<X, _>(instr, |v| Expr::brev8(v)),
            OP_ZIP => lift_unary::<X, _>(instr, |v| Expr::zip(v)),
            OP_UNZIP => lift_unary::<X, _>(instr, |v| Expr::unzip(v)),
            _ => InstrIR::new(
                instr.pc,
                instr.size,
                instr.opid.pack(),
                instr.raw,
                Vec::new(),
                Terminator::trap("unknown Zbkb opid"),
            ),
        }
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        match instr.opid {
            OP_PACK => disasm_r(instr, "pack"),
            OP_PACKH => disasm_r(instr, "packh"),
            OP_PACKW => disasm_r(instr, "packw"),
            OP_BREV8 => disasm_unary(instr, "brev8"),
            OP_ZIP => disasm_unary(instr, "zip"),
            OP_UNZIP => disasm_unary(instr, "unzip"),
            _ => "???".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_ZBKB.iter().find(|info| info.opid == opid).copied()
    }
}

// === Lift helpers ===

fn lift_pack<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
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
    // pack: rd = (rs2[XLEN/2-1:0] << (XLEN/2)) | rs1[XLEN/2-1:0]
    let result = Expr::pack(Expr::reg(rs1), Expr::reg(rs2));
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

fn lift_packh<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
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
    // packh: rd = (rs2[7:0] << 8) | rs1[7:0]
    let result = Expr::pack8(Expr::reg(rs1), Expr::reg(rs2));
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

fn lift_packw<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
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
    // packw: rd = sext((rs2[15:0] << 16) | rs1[15:0])
    let result = Expr::pack16(Expr::reg(rs1), Expr::reg(rs2));
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
                instr.raw,
                Vec::new(),
                Terminator::trap("bad args"),
            );
        }
    };
    let result = op(Expr::reg(rs1));
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

fn disasm_unary<X: Xlen>(instr: &DecodedInstr<X>, mnemonic: &str) -> String {
    match instr.args {
        InstrArgs::I { rd, rs1, .. } => {
            format!("{} {}, {}", mnemonic, reg_name(rd), reg_name(rs1))
        }
        _ => format!("{} ???", mnemonic),
    }
}

/// Table-driven OpInfo for Zbkb extension.
const OP_INFO_ZBKB: &[OpInfo] = &[
    OpInfo {
        opid: OP_PACK,
        name: "pack",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_PACKH,
        name: "packh",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_PACKW,
        name: "packw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_BREV8,
        name: "brev8",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ZIP,
        name: "zip",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_UNZIP,
        name: "unzip",
        class: OpClass::Alu,
        size_hint: 4,
    },
];

/// Get mnemonic for Zbkb instruction.
pub fn zbkb_mnemonic(opid: OpId) -> Option<&'static str> {
    OP_INFO_ZBKB
        .iter()
        .find(|info| info.opid == opid)
        .map(|info| info.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::{Rv32, Rv64};

    fn encode_r(opcode: u32, funct3: u8, funct7: u8, rd: u8, rs1: u8, rs2: u8) -> [u8; 4] {
        let raw = opcode
            | ((rd as u32) << 7)
            | ((funct3 as u32) << 12)
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
    fn test_pack_decode() {
        let ext = ZbkbExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_PACK, FUNCT7_PACK, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_PACK);
    }

    #[test]
    fn test_packh_decode() {
        let ext = ZbkbExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_PACKH, FUNCT7_PACK, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_PACKH);
    }

    #[test]
    fn test_packw_decode() {
        let ext = ZbkbExtension;
        let bytes = encode_r(OPCODE_OP_32, FUNCT3_PACK, FUNCT7_PACK, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_PACKW);
    }

    #[test]
    fn test_brev8_decode() {
        let ext = ZbkbExtension;
        let bytes = encode_i(OPCODE_OP_IMM, 0b101, 1, 2, IMM_BREV8);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_BREV8);
    }

    #[test]
    fn test_disasm() {
        let ext = ZbkbExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_PACK, FUNCT7_PACK, 10, 11, 12);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let disasm = InstructionExtension::<Rv64>::disasm(&ext, &instr);
        assert_eq!(disasm, "pack a0, a1, a2");
    }

    #[test]
    fn test_lift_pack() {
        let ext = ZbkbExtension;
        let bytes = encode_r(OPCODE_OP, FUNCT3_PACK, FUNCT7_PACK, 1, 2, 3);
        let instr =
            InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let ir = InstructionExtension::<Rv64>::lift(&ext, &instr);
        assert_eq!(ir.statements.len(), 1);
    }

    #[test]
    fn test_op_info() {
        let ext = ZbkbExtension;
        let info = InstructionExtension::<Rv64>::op_info(&ext, OP_PACK).unwrap();
        assert_eq!(info.name, "pack");
        assert_eq!(info.class, OpClass::Alu);
    }

    // RV32-only ZIP/UNZIP tests
    #[test]
    fn test_zip_decode_rv32() {
        let ext = ZbkbExtension;
        // ZIP: funct3=0b001, imm12=0b000010001111
        let bytes = encode_i(OPCODE_OP_IMM, 0b001, 1, 2, IMM_ZIP);
        let instr =
            InstructionExtension::<Rv32>::decode32(&ext, u32::from_le_bytes(bytes), 0u32).unwrap();
        assert_eq!(instr.opid, OP_ZIP);
    }

    #[test]
    fn test_unzip_decode_rv32() {
        let ext = ZbkbExtension;
        // UNZIP: funct3=0b101, imm12=0b000010001111
        let bytes = encode_i(OPCODE_OP_IMM, 0b101, 1, 2, IMM_UNZIP);
        let instr =
            InstructionExtension::<Rv32>::decode32(&ext, u32::from_le_bytes(bytes), 0u32).unwrap();
        assert_eq!(instr.opid, OP_UNZIP);
    }

    #[test]
    fn test_zip_not_decoded_rv64() {
        let ext = ZbkbExtension;
        // ZIP should NOT decode on RV64 (RV32-only)
        let bytes = encode_i(OPCODE_OP_IMM, 0b001, 1, 2, IMM_ZIP);
        let result = InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64);
        assert!(result.is_none());
    }

    #[test]
    fn test_unzip_not_decoded_rv64() {
        let ext = ZbkbExtension;
        // UNZIP should NOT decode on RV64 (RV32-only)
        let bytes = encode_i(OPCODE_OP_IMM, 0b101, 1, 2, IMM_UNZIP);
        // Note: This will decode as BREV8 on RV64 since they share the same encoding
        // but with different imm12 values. Actually IMM_UNZIP and IMM_BREV8 differ.
        let result = InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64);
        // Should not be OP_UNZIP
        assert!(result.is_none() || result.unwrap().opid != OP_UNZIP);
    }

    #[test]
    fn test_lift_zip_rv32() {
        let ext = ZbkbExtension;
        let bytes = encode_i(OPCODE_OP_IMM, 0b001, 1, 2, IMM_ZIP);
        let instr =
            InstructionExtension::<Rv32>::decode32(&ext, u32::from_le_bytes(bytes), 0u32).unwrap();
        let ir = InstructionExtension::<Rv32>::lift(&ext, &instr);
        assert_eq!(ir.statements.len(), 1);
    }

    #[test]
    fn test_lift_unzip_rv32() {
        let ext = ZbkbExtension;
        let bytes = encode_i(OPCODE_OP_IMM, 0b101, 1, 2, IMM_UNZIP);
        let instr =
            InstructionExtension::<Rv32>::decode32(&ext, u32::from_le_bytes(bytes), 0u32).unwrap();
        let ir = InstructionExtension::<Rv32>::lift(&ext, &instr);
        assert_eq!(ir.statements.len(), 1);
    }

    #[test]
    fn test_disasm_zip_rv32() {
        let ext = ZbkbExtension;
        let bytes = encode_i(OPCODE_OP_IMM, 0b001, 10, 11, IMM_ZIP);
        let instr =
            InstructionExtension::<Rv32>::decode32(&ext, u32::from_le_bytes(bytes), 0u32).unwrap();
        let disasm = InstructionExtension::<Rv32>::disasm(&ext, &instr);
        assert_eq!(disasm, "zip a0, a1");
    }

    #[test]
    fn test_disasm_unzip_rv32() {
        let ext = ZbkbExtension;
        let bytes = encode_i(OPCODE_OP_IMM, 0b101, 10, 11, IMM_UNZIP);
        let instr =
            InstructionExtension::<Rv32>::decode32(&ext, u32::from_le_bytes(bytes), 0u32).unwrap();
        let disasm = InstructionExtension::<Rv32>::disasm(&ext, &instr);
        assert_eq!(disasm, "unzip a0, a1");
    }
}
