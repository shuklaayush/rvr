//! Zbs extension (single-bit operations) - decode, lift, disasm.
//!
//! Instructions: bclr, bclri, bext, bexti, binv, binvi, bset, bseti

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use crate::{
    reg_name, DecodedInstr, InstrArgs, OpClass, OpId, OpInfo, EXT_ZBS,
    encode::{decode_funct3, decode_funct7, decode_rd, decode_rs1, decode_rs2},
};
use super::InstructionExtension;

// Instruction OpIds
pub const OP_BCLR: OpId = OpId::new(EXT_ZBS, 0);
pub const OP_BCLRI: OpId = OpId::new(EXT_ZBS, 1);
pub const OP_BEXT: OpId = OpId::new(EXT_ZBS, 2);
pub const OP_BEXTI: OpId = OpId::new(EXT_ZBS, 3);
pub const OP_BINV: OpId = OpId::new(EXT_ZBS, 4);
pub const OP_BINVI: OpId = OpId::new(EXT_ZBS, 5);
pub const OP_BSET: OpId = OpId::new(EXT_ZBS, 6);
pub const OP_BSETI: OpId = OpId::new(EXT_ZBS, 7);

// Opcodes
const OPCODE_OP: u32 = 0b0110011;
const OPCODE_OP_IMM: u32 = 0b0010011;

// Encoding constants (funct7 values for R-type)
const FUNCT7_BCLR: u8 = 0b0100100;  // bclr, bext
const FUNCT7_BINV: u8 = 0b0110100;  // binv
const FUNCT7_BSET: u8 = 0b0010100;  // bset

// funct6 values for I-type (funct7 >> 1)
const FUNCT6_BCLRI: u32 = 0b010010;  // bclri, bexti
const FUNCT6_BINVI: u32 = 0b011010;  // binvi
const FUNCT6_BSETI: u32 = 0b001010;  // bseti

/// Zbs extension (single-bit operations).
pub struct ZbsExtension;

impl<X: Xlen> InstructionExtension<X> for ZbsExtension {
    fn name(&self) -> &'static str {
        "Zbs"
    }

    fn ext_id(&self) -> u8 {
        EXT_ZBS
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = decode_funct3(raw);
        let funct7 = decode_funct7(raw);
        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let rs2 = decode_rs2(raw);

        // R-type: bclr, bext, binv, bset (OPCODE_OP)
        if opcode == OPCODE_OP {
            if funct3 == 0b001 {
                // bclr, binv, bset
                let opid = match funct7 {
                    FUNCT7_BCLR => OP_BCLR,
                    FUNCT7_BINV => OP_BINV,
                    FUNCT7_BSET => OP_BSET,
                    _ => return None,
                };
                return Some(DecodedInstr::new(opid, pc, 4, InstrArgs::R { rd, rs1, rs2 }));
            }
            if funct3 == 0b101 && funct7 == FUNCT7_BCLR {
                // bext (same funct7 as bclr)
                return Some(DecodedInstr::new(OP_BEXT, pc, 4, InstrArgs::R { rd, rs1, rs2 }));
            }
        }

        // I-type: bclri, bexti, binvi, bseti (OPCODE_OP_IMM)
        if opcode == OPCODE_OP_IMM {
            // RV32: bit 25 must be 0
            if X::VALUE == 32 && ((raw >> 25) & 1) != 0 {
                return None;
            }
            let shamt_mask = if X::VALUE == 32 { 0x1F } else { 0x3F };
            let shamt = ((raw >> 20) & shamt_mask) as i32;
            let funct6 = (raw >> 26) & 0x3F;

            if funct3 == 0b001 {
                // bclri, binvi, bseti
                let opid = match funct6 {
                    FUNCT6_BCLRI => OP_BCLRI,
                    FUNCT6_BINVI => OP_BINVI,
                    FUNCT6_BSETI => OP_BSETI,
                    _ => return None,
                };
                return Some(DecodedInstr::new(opid, pc, 4, InstrArgs::I { rd, rs1, imm: shamt }));
            }
            if funct3 == 0b101 && funct6 == FUNCT6_BCLRI {
                // bexti (same funct6 as bclri)
                return Some(DecodedInstr::new(OP_BEXTI, pc, 4, InstrArgs::I { rd, rs1, imm: shamt }));
            }
        }

        None
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        match instr.opid {
            OP_BCLR => lift_bclr::<X>(instr),
            OP_BCLRI => lift_bclri::<X>(instr),
            OP_BEXT => lift_bext::<X>(instr),
            OP_BEXTI => lift_bexti::<X>(instr),
            OP_BINV => lift_binv::<X>(instr),
            OP_BINVI => lift_binvi::<X>(instr),
            OP_BSET => lift_bset::<X>(instr),
            OP_BSETI => lift_bseti::<X>(instr),
            _ => InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("unknown Zbs opid")),
        }
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        match instr.opid {
            OP_BCLR => disasm_r(instr, "bclr"),
            OP_BCLRI => disasm_i(instr, "bclri"),
            OP_BEXT => disasm_r(instr, "bext"),
            OP_BEXTI => disasm_i(instr, "bexti"),
            OP_BINV => disasm_r(instr, "binv"),
            OP_BINVI => disasm_i(instr, "binvi"),
            OP_BSET => disasm_r(instr, "bset"),
            OP_BSETI => disasm_i(instr, "bseti"),
            _ => "???".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_ZBS.iter().find(|info| info.opid == opid).copied()
    }
}

// === Lift helpers ===

fn lift_bclr<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = rs1 & ~(1 << (rs2 & (XLEN-1)))
    let mask = X::from_u64((X::VALUE - 1) as u64);
    let index = Expr::and(Expr::reg(rs2), Expr::imm(mask));
    let bit = Expr::sll(Expr::imm(X::from_u64(1)), index);
    let result = Expr::and(Expr::reg(rs1), Expr::not(bit));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

fn lift_bclri<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, shamt) = match instr.args {
        InstrArgs::I { rd, rs1, imm } => (rd, rs1, imm as u8),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = rs1 & ~(1 << shamt)
    let bit = Expr::sll(Expr::imm(X::from_u64(1)), Expr::imm(X::from_u64(shamt as u64)));
    let result = Expr::and(Expr::reg(rs1), Expr::not(bit));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

fn lift_bext<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = (rs1 >> (rs2 & (XLEN-1))) & 1
    let mask = X::from_u64((X::VALUE - 1) as u64);
    let index = Expr::and(Expr::reg(rs2), Expr::imm(mask));
    let shifted = Expr::srl(Expr::reg(rs1), index);
    let result = Expr::and(shifted, Expr::imm(X::from_u64(1)));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

fn lift_bexti<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, shamt) = match instr.args {
        InstrArgs::I { rd, rs1, imm } => (rd, rs1, imm as u8),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = (rs1 >> shamt) & 1
    let shifted = Expr::srl(Expr::reg(rs1), Expr::imm(X::from_u64(shamt as u64)));
    let result = Expr::and(shifted, Expr::imm(X::from_u64(1)));
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

fn lift_binv<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = rs1 ^ (1 << (rs2 & (XLEN-1)))
    let mask = X::from_u64((X::VALUE - 1) as u64);
    let index = Expr::and(Expr::reg(rs2), Expr::imm(mask));
    let bit = Expr::sll(Expr::imm(X::from_u64(1)), index);
    let result = Expr::xor(Expr::reg(rs1), bit);
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

fn lift_binvi<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, shamt) = match instr.args {
        InstrArgs::I { rd, rs1, imm } => (rd, rs1, imm as u8),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = rs1 ^ (1 << shamt)
    let bit = Expr::sll(Expr::imm(X::from_u64(1)), Expr::imm(X::from_u64(shamt as u64)));
    let result = Expr::xor(Expr::reg(rs1), bit);
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

fn lift_bset<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, rs2) = match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => (rd, rs1, rs2),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = rs1 | (1 << (rs2 & (XLEN-1)))
    let mask = X::from_u64((X::VALUE - 1) as u64);
    let index = Expr::and(Expr::reg(rs2), Expr::imm(mask));
    let bit = Expr::sll(Expr::imm(X::from_u64(1)), index);
    let result = Expr::or(Expr::reg(rs1), bit);
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

fn lift_bseti<X: Xlen>(instr: &DecodedInstr<X>) -> InstrIR<X> {
    let (rd, rs1, shamt) = match instr.args {
        InstrArgs::I { rd, rs1, imm } => (rd, rs1, imm as u8),
        _ => return InstrIR::new(instr.pc, instr.size, instr.opid.pack(), Vec::new(), Terminator::trap("bad args")),
    };
    // rd = rs1 | (1 << shamt)
    let bit = Expr::sll(Expr::imm(X::from_u64(1)), Expr::imm(X::from_u64(shamt as u64)));
    let result = Expr::or(Expr::reg(rs1), bit);
    let stmt = Stmt::write_reg(rd, result);
    InstrIR::new(instr.pc, instr.size, instr.opid.pack(), vec![stmt], Terminator::Fall { target: None })
}

// === Disasm helpers ===

fn disasm_r<X: Xlen>(instr: &DecodedInstr<X>, mnemonic: &str) -> String {
    match instr.args {
        InstrArgs::R { rd, rs1, rs2 } => {
            format!("{} {}, {}, {}", mnemonic, reg_name(rd), reg_name(rs1), reg_name(rs2))
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

/// Table-driven OpInfo for Zbs extension.
const OP_INFO_ZBS: &[OpInfo] = &[
    OpInfo { opid: OP_BCLR, name: "bclr", class: OpClass::Alu, size_hint: 4 },
    OpInfo { opid: OP_BCLRI, name: "bclri", class: OpClass::Alu, size_hint: 4 },
    OpInfo { opid: OP_BEXT, name: "bext", class: OpClass::Alu, size_hint: 4 },
    OpInfo { opid: OP_BEXTI, name: "bexti", class: OpClass::Alu, size_hint: 4 },
    OpInfo { opid: OP_BINV, name: "binv", class: OpClass::Alu, size_hint: 4 },
    OpInfo { opid: OP_BINVI, name: "binvi", class: OpClass::Alu, size_hint: 4 },
    OpInfo { opid: OP_BSET, name: "bset", class: OpClass::Alu, size_hint: 4 },
    OpInfo { opid: OP_BSETI, name: "bseti", class: OpClass::Alu, size_hint: 4 },
];

/// Get mnemonic for Zbs instruction.
pub fn zbs_mnemonic(opid: OpId) -> Option<&'static str> {
    OP_INFO_ZBS.iter().find(|info| info.opid == opid).map(|info| info.name)
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
        let raw = opcode
            | ((rd as u32) << 7)
            | (funct3 << 12)
            | ((rs1 as u32) << 15)
            | (imm12 << 20);
        raw.to_le_bytes()
    }

    #[test]
    fn test_bclr_decode() {
        let ext = ZbsExtension;
        let bytes = encode_r(OPCODE_OP, 0b001, FUNCT7_BCLR, 1, 2, 3);
        let instr = InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_BCLR);
    }

    #[test]
    fn test_bext_decode() {
        let ext = ZbsExtension;
        let bytes = encode_r(OPCODE_OP, 0b101, FUNCT7_BCLR, 1, 2, 3);
        let instr = InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_BEXT);
    }

    #[test]
    fn test_bclri_decode() {
        let ext = ZbsExtension;
        let imm = (FUNCT6_BCLRI << 6) | 5;
        let bytes = encode_i(OPCODE_OP_IMM, 0b001, 1, 2, imm);
        let instr = InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        assert_eq!(instr.opid, OP_BCLRI);
        match instr.args {
            InstrArgs::I { imm, .. } => assert_eq!(imm, 5),
            _ => panic!("Expected I-type args"),
        }
    }

    #[test]
    fn test_disasm() {
        let ext = ZbsExtension;
        let bytes = encode_r(OPCODE_OP, 0b001, FUNCT7_BCLR, 10, 11, 12);
        let instr = InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let disasm = InstructionExtension::<Rv64>::disasm(&ext, &instr);
        assert_eq!(disasm, "bclr a0, a1, a2");
    }

    #[test]
    fn test_lift_bclr() {
        let ext = ZbsExtension;
        let bytes = encode_r(OPCODE_OP, 0b001, FUNCT7_BCLR, 1, 2, 3);
        let instr = InstructionExtension::<Rv64>::decode32(&ext, u32::from_le_bytes(bytes), 0u64).unwrap();
        let ir = InstructionExtension::<Rv64>::lift(&ext, &instr);
        assert_eq!(ir.statements.len(), 1);
    }

    #[test]
    fn test_op_info() {
        let ext = ZbsExtension;
        let info = InstructionExtension::<Rv64>::op_info(&ext, OP_BCLR).unwrap();
        assert_eq!(info.name, "bclr");
        assert_eq!(info.class, OpClass::Alu);
    }
}
