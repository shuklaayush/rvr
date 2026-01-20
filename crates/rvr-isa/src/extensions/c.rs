//! C extension (compressed instructions) - decode, lift, disasm.

use rvr_ir::{Xlen, InstrIR, Expr, Stmt, Terminator};

use crate::{DecodedInstr, InstrArgs, OpId, OpInfo, OpClass, EXT_C, reg_name};
use super::InstructionExtension;

// Quadrant 0
pub const OP_C_ADDI4SPN: OpId = OpId::new(EXT_C, 0);
pub const OP_C_LW: OpId = OpId::new(EXT_C, 1);
pub const OP_C_SW: OpId = OpId::new(EXT_C, 2);
pub const OP_C_LD: OpId = OpId::new(EXT_C, 3);  // RV64C
pub const OP_C_SD: OpId = OpId::new(EXT_C, 4);  // RV64C

// Quadrant 1
pub const OP_C_NOP: OpId = OpId::new(EXT_C, 5);
pub const OP_C_ADDI: OpId = OpId::new(EXT_C, 6);
pub const OP_C_JAL: OpId = OpId::new(EXT_C, 7);  // RV32C only
pub const OP_C_ADDIW: OpId = OpId::new(EXT_C, 8);  // RV64C
pub const OP_C_LI: OpId = OpId::new(EXT_C, 9);
pub const OP_C_ADDI16SP: OpId = OpId::new(EXT_C, 10);
pub const OP_C_LUI: OpId = OpId::new(EXT_C, 11);
pub const OP_C_SRLI: OpId = OpId::new(EXT_C, 12);
pub const OP_C_SRAI: OpId = OpId::new(EXT_C, 13);
pub const OP_C_ANDI: OpId = OpId::new(EXT_C, 14);
pub const OP_C_SUB: OpId = OpId::new(EXT_C, 15);
pub const OP_C_XOR: OpId = OpId::new(EXT_C, 16);
pub const OP_C_OR: OpId = OpId::new(EXT_C, 17);
pub const OP_C_AND: OpId = OpId::new(EXT_C, 18);
pub const OP_C_SUBW: OpId = OpId::new(EXT_C, 19);  // RV64C
pub const OP_C_ADDW: OpId = OpId::new(EXT_C, 20);  // RV64C
pub const OP_C_J: OpId = OpId::new(EXT_C, 21);
pub const OP_C_BEQZ: OpId = OpId::new(EXT_C, 22);
pub const OP_C_BNEZ: OpId = OpId::new(EXT_C, 23);

// Quadrant 2
pub const OP_C_SLLI: OpId = OpId::new(EXT_C, 24);
pub const OP_C_LWSP: OpId = OpId::new(EXT_C, 25);
pub const OP_C_LDSP: OpId = OpId::new(EXT_C, 26);  // RV64C
pub const OP_C_JR: OpId = OpId::new(EXT_C, 27);
pub const OP_C_MV: OpId = OpId::new(EXT_C, 28);
pub const OP_C_EBREAK: OpId = OpId::new(EXT_C, 29);
pub const OP_C_JALR: OpId = OpId::new(EXT_C, 30);
pub const OP_C_ADD: OpId = OpId::new(EXT_C, 31);
pub const OP_C_SWSP: OpId = OpId::new(EXT_C, 32);
pub const OP_C_SDSP: OpId = OpId::new(EXT_C, 33);  // RV64C

/// Get the mnemonic for a C extension instruction.
pub fn c_mnemonic(opid: OpId) -> &'static str {
    match opid.idx {
        0 => "c.addi4spn",
        1 => "c.lw",
        2 => "c.sw",
        3 => "c.ld",
        4 => "c.sd",
        5 => "c.nop",
        6 => "c.addi",
        7 => "c.jal",
        8 => "c.addiw",
        9 => "c.li",
        10 => "c.addi16sp",
        11 => "c.lui",
        12 => "c.srli",
        13 => "c.srai",
        14 => "c.andi",
        15 => "c.sub",
        16 => "c.xor",
        17 => "c.or",
        18 => "c.and",
        19 => "c.subw",
        20 => "c.addw",
        21 => "c.j",
        22 => "c.beqz",
        23 => "c.bnez",
        24 => "c.slli",
        25 => "c.lwsp",
        26 => "c.ldsp",
        27 => "c.jr",
        28 => "c.mv",
        29 => "c.ebreak",
        30 => "c.jalr",
        31 => "c.add",
        32 => "c.swsp",
        33 => "c.sdsp",
        _ => "???",
    }
}

/// C extension (compressed instructions).
pub struct CExtension;

impl<X: Xlen> InstructionExtension<X> for CExtension {
    fn name(&self) -> &'static str {
        "C"
    }

    fn ext_id(&self) -> u8 {
        EXT_C
    }

    fn decode16(&self, raw: u16, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let quadrant = raw & 0x3;
        let funct3 = ((raw >> 13) & 0x7) as u8;

        let (opid, args) = match quadrant {
            0b00 => decode_q0::<X>(raw, funct3)?,
            0b01 => decode_q1::<X>(raw, funct3)?,
            0b10 => decode_q2::<X>(raw, funct3)?,
            _ => return None,
        };

        Some(DecodedInstr::new(opid, pc, 2, args))
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let (stmts, term) = lift_c(&instr.args, instr.opid, instr.pc, instr.size);
        InstrIR::new(instr.pc, instr.size, instr.opid.pack(), stmts, term)
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        let mnemonic = c_mnemonic(instr.opid);
        format_c_instr(mnemonic, &instr.args, instr.opid)
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_C.iter().find(|info| info.opid == opid).copied()
    }
}

/// Table-driven OpInfo for C extension.
const OP_INFO_C: &[OpInfo] = &[
    // Quadrant 0
    OpInfo { opid: OP_C_ADDI4SPN, name: "c.addi4spn", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_LW, name: "c.lw", class: OpClass::Load, size_hint: 2 },
    OpInfo { opid: OP_C_SW, name: "c.sw", class: OpClass::Store, size_hint: 2 },
    OpInfo { opid: OP_C_LD, name: "c.ld", class: OpClass::Load, size_hint: 2 },
    OpInfo { opid: OP_C_SD, name: "c.sd", class: OpClass::Store, size_hint: 2 },
    // Quadrant 1
    OpInfo { opid: OP_C_NOP, name: "c.nop", class: OpClass::Nop, size_hint: 2 },
    OpInfo { opid: OP_C_ADDI, name: "c.addi", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_JAL, name: "c.jal", class: OpClass::Jump, size_hint: 2 },
    OpInfo { opid: OP_C_ADDIW, name: "c.addiw", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_LI, name: "c.li", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_ADDI16SP, name: "c.addi16sp", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_LUI, name: "c.lui", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_SRLI, name: "c.srli", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_SRAI, name: "c.srai", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_ANDI, name: "c.andi", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_SUB, name: "c.sub", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_XOR, name: "c.xor", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_OR, name: "c.or", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_AND, name: "c.and", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_SUBW, name: "c.subw", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_ADDW, name: "c.addw", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_J, name: "c.j", class: OpClass::Jump, size_hint: 2 },
    OpInfo { opid: OP_C_BEQZ, name: "c.beqz", class: OpClass::Branch, size_hint: 2 },
    OpInfo { opid: OP_C_BNEZ, name: "c.bnez", class: OpClass::Branch, size_hint: 2 },
    // Quadrant 2
    OpInfo { opid: OP_C_SLLI, name: "c.slli", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_LWSP, name: "c.lwsp", class: OpClass::Load, size_hint: 2 },
    OpInfo { opid: OP_C_LDSP, name: "c.ldsp", class: OpClass::Load, size_hint: 2 },
    OpInfo { opid: OP_C_JR, name: "c.jr", class: OpClass::JumpIndirect, size_hint: 2 },
    OpInfo { opid: OP_C_MV, name: "c.mv", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_EBREAK, name: "c.ebreak", class: OpClass::System, size_hint: 2 },
    OpInfo { opid: OP_C_JALR, name: "c.jalr", class: OpClass::JumpIndirect, size_hint: 2 },
    OpInfo { opid: OP_C_ADD, name: "c.add", class: OpClass::Alu, size_hint: 2 },
    OpInfo { opid: OP_C_SWSP, name: "c.swsp", class: OpClass::Store, size_hint: 2 },
    OpInfo { opid: OP_C_SDSP, name: "c.sdsp", class: OpClass::Store, size_hint: 2 },
];

// Decode helpers

fn decode_q0<X: Xlen>(instr: u16, funct3: u8) -> Option<(crate::OpId, InstrArgs)> {
    match funct3 {
        0b000 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let nzuimm = decode_addi4spn_imm(instr);
            if nzuimm == 0 { return None; }
            Some((OP_C_ADDI4SPN, InstrArgs::I { rd, rs1: 2, imm: nzuimm as i32 }))
        }
        0b010 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cl_lw_offset(instr);
            Some((OP_C_LW, InstrArgs::I { rd, rs1, imm: offset as i32 }))
        }
        0b011 if X::VALUE == 64 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cl_ld_offset(instr);
            Some((OP_C_LD, InstrArgs::I { rd, rs1, imm: offset as i32 }))
        }
        0b110 => {
            let rs2 = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cs_sw_offset(instr);
            Some((OP_C_SW, InstrArgs::S { rs1, rs2, imm: offset as i32 }))
        }
        0b111 if X::VALUE == 64 => {
            let rs2 = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cs_sd_offset(instr);
            Some((OP_C_SD, InstrArgs::S { rs1, rs2, imm: offset as i32 }))
        }
        _ => None,
    }
}

fn decode_q1<X: Xlen>(instr: u16, funct3: u8) -> Option<(crate::OpId, InstrArgs)> {
    match funct3 {
        0b000 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let imm = decode_ci_imm(instr);
            if rd == 0 && imm == 0 {
                return Some((OP_C_NOP, InstrArgs::None));
            }
            Some((OP_C_ADDI, InstrArgs::I { rd, rs1: rd, imm: imm as i32 }))
        }
        0b001 => {
            if X::VALUE == 64 {
                let rd = ((instr >> 7) & 0x1F) as u8;
                let imm = decode_ci_imm(instr);
                if rd == 0 { return None; }
                Some((OP_C_ADDIW, InstrArgs::I { rd, rs1: rd, imm: imm as i32 }))
            } else {
                let offset = decode_cj_imm(instr);
                Some((OP_C_JAL, InstrArgs::J { rd: 1, imm: offset as i32 }))
            }
        }
        0b010 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let imm = decode_ci_imm(instr);
            Some((OP_C_LI, InstrArgs::I { rd, rs1: 0, imm: imm as i32 }))
        }
        0b011 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            if rd == 2 {
                let imm = decode_ci16sp_imm(instr);
                if imm == 0 { return None; }
                Some((OP_C_ADDI16SP, InstrArgs::I { rd: 2, rs1: 2, imm: imm as i32 }))
            } else {
                let imm = decode_ci_lui_imm(instr);
                if imm == 0 || rd == 0 { return None; }
                Some((OP_C_LUI, InstrArgs::U { rd, imm }))
            }
        }
        0b100 => decode_misc_alu::<X>(instr),
        0b101 => {
            let offset = decode_cj_imm(instr);
            Some((OP_C_J, InstrArgs::J { rd: 0, imm: offset as i32 }))
        }
        0b110 => {
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cb_imm(instr);
            Some((OP_C_BEQZ, InstrArgs::B { rs1, rs2: 0, imm: offset as i32 }))
        }
        0b111 => {
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cb_imm(instr);
            Some((OP_C_BNEZ, InstrArgs::B { rs1, rs2: 0, imm: offset as i32 }))
        }
        _ => None,
    }
}

fn decode_misc_alu<X: Xlen>(instr: u16) -> Option<(crate::OpId, InstrArgs)> {
    let funct2 = ((instr >> 10) & 0x3) as u8;
    let rd = ((instr >> 7) & 0x7) as u8 + 8;

    match funct2 {
        0b00 => {
            let shamt = decode_ci_shamt(instr);
            Some((OP_C_SRLI, InstrArgs::I { rd, rs1: rd, imm: shamt as i32 }))
        }
        0b01 => {
            let shamt = decode_ci_shamt(instr);
            Some((OP_C_SRAI, InstrArgs::I { rd, rs1: rd, imm: shamt as i32 }))
        }
        0b10 => {
            let imm = decode_ci_imm(instr);
            Some((OP_C_ANDI, InstrArgs::I { rd, rs1: rd, imm: imm as i32 }))
        }
        0b11 => {
            let rs2 = ((instr >> 2) & 0x7) as u8 + 8;
            let funct6 = ((instr >> 12) & 0x1) as u8;
            let funct2_low = ((instr >> 5) & 0x3) as u8;

            if funct6 == 0 {
                let op = match funct2_low {
                    0b00 => OP_C_SUB,
                    0b01 => OP_C_XOR,
                    0b10 => OP_C_OR,
                    0b11 => OP_C_AND,
                    _ => return None,
                };
                Some((op, InstrArgs::R { rd, rs1: rd, rs2 }))
            } else {
                if X::VALUE != 64 { return None; }
                let op = match funct2_low {
                    0b00 => OP_C_SUBW,
                    0b01 => OP_C_ADDW,
                    _ => return None,
                };
                Some((op, InstrArgs::R { rd, rs1: rd, rs2 }))
            }
        }
        _ => None,
    }
}

fn decode_q2<X: Xlen>(instr: u16, funct3: u8) -> Option<(crate::OpId, InstrArgs)> {
    match funct3 {
        0b000 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let shamt = decode_ci_shamt(instr);
            if rd == 0 { return None; }
            Some((OP_C_SLLI, InstrArgs::I { rd, rs1: rd, imm: shamt as i32 }))
        }
        0b010 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let offset = decode_ci_lwsp_offset(instr);
            if rd == 0 { return None; }
            Some((OP_C_LWSP, InstrArgs::I { rd, rs1: 2, imm: offset as i32 }))
        }
        0b011 if X::VALUE == 64 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let offset = decode_ci_ldsp_offset(instr);
            if rd == 0 { return None; }
            Some((OP_C_LDSP, InstrArgs::I { rd, rs1: 2, imm: offset as i32 }))
        }
        0b100 => {
            let funct4 = ((instr >> 12) & 0x1) as u8;
            let rs1 = ((instr >> 7) & 0x1F) as u8;
            let rs2 = ((instr >> 2) & 0x1F) as u8;

            if funct4 == 0 {
                if rs2 == 0 {
                    if rs1 == 0 { return None; }
                    Some((OP_C_JR, InstrArgs::I { rd: 0, rs1, imm: 0 }))
                } else {
                    Some((OP_C_MV, InstrArgs::R { rd: rs1, rs1: 0, rs2 }))
                }
            } else {
                if rs1 == 0 && rs2 == 0 {
                    Some((OP_C_EBREAK, InstrArgs::None))
                } else if rs2 == 0 {
                    Some((OP_C_JALR, InstrArgs::I { rd: 1, rs1, imm: 0 }))
                } else {
                    Some((OP_C_ADD, InstrArgs::R { rd: rs1, rs1, rs2 }))
                }
            }
        }
        0b110 => {
            let rs2 = ((instr >> 2) & 0x1F) as u8;
            let offset = decode_css_swsp_offset(instr);
            Some((OP_C_SWSP, InstrArgs::S { rs1: 2, rs2, imm: offset as i32 }))
        }
        0b111 if X::VALUE == 64 => {
            let rs2 = ((instr >> 2) & 0x1F) as u8;
            let offset = decode_css_sdsp_offset(instr);
            Some((OP_C_SDSP, InstrArgs::S { rs1: 2, rs2, imm: offset as i32 }))
        }
        _ => None,
    }
}

// Disassembly

fn format_c_instr(mnemonic: &str, args: &InstrArgs, opid: crate::OpId) -> String {
    match args {
        InstrArgs::R { rd, rs1: _, rs2 } => {
            if opid == OP_C_MV {
                format!("{} {}, {}", mnemonic, reg_name(*rd), reg_name(*rs2))
            } else {
                format!("{} {}, {}", mnemonic, reg_name(*rd), reg_name(*rs2))
            }
        }
        InstrArgs::I { rd, rs1: _, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rd), imm)
        }
        InstrArgs::S { rs1: _, rs2, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rs2), imm)
        }
        InstrArgs::U { rd, imm } => {
            format!("{} {}, {:#x}", mnemonic, reg_name(*rd), (*imm as u32) >> 12)
        }
        InstrArgs::J { rd: _, imm } => {
            format!("{} {}", mnemonic, imm)
        }
        InstrArgs::B { rs1, rs2: _, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rs1), imm)
        }
        InstrArgs::None => mnemonic.to_string(),
        _ => format!("{} <?>", mnemonic),
    }
}

// Compressed immediate decoders

fn decode_addi4spn_imm(instr: u16) -> u16 {
    (((instr >> 6) & 0x1) << 2)
        | (((instr >> 5) & 0x1) << 3)
        | (((instr >> 11) & 0x3) << 4)
        | (((instr >> 7) & 0xF) << 6)
}

fn decode_cl_lw_offset(instr: u16) -> u8 {
    ((((instr >> 6) & 0x1) << 2)
        | (((instr >> 10) & 0x7) << 3)
        | (((instr >> 5) & 0x1) << 6)) as u8
}

fn decode_cs_sw_offset(instr: u16) -> u8 {
    decode_cl_lw_offset(instr)
}

fn decode_cl_ld_offset(instr: u16) -> u8 {
    ((((instr >> 10) & 0x7) << 3) | (((instr >> 5) & 0x3) << 6)) as u8
}

fn decode_cs_sd_offset(instr: u16) -> u8 {
    decode_cl_ld_offset(instr)
}

fn decode_ci_imm(instr: u16) -> i8 {
    let imm = (((instr >> 2) & 0x1F) | (((instr >> 12) & 0x1) << 5)) as u8;
    ((imm as i8) << 2) >> 2
}

fn decode_cj_imm(instr: u16) -> i16 {
    let imm = (((instr >> 3) & 0x7) << 1)
        | (((instr >> 11) & 0x1) << 4)
        | (((instr >> 2) & 0x1) << 5)
        | (((instr >> 7) & 0x1) << 6)
        | (((instr >> 6) & 0x1) << 7)
        | (((instr >> 9) & 0x3) << 8)
        | (((instr >> 8) & 0x1) << 10)
        | (((instr >> 12) & 0x1) << 11);
    ((imm as i16) << 4) >> 4
}

fn decode_ci16sp_imm(instr: u16) -> i16 {
    let imm = (((instr >> 6) & 0x1) << 4)
        | (((instr >> 2) & 0x1) << 5)
        | (((instr >> 5) & 0x1) << 6)
        | (((instr >> 3) & 0x3) << 7)
        | (((instr >> 12) & 0x1) << 9);
    ((imm as i16) << 6) >> 6
}

fn decode_ci_lui_imm(instr: u16) -> i32 {
    let imm = (((instr >> 2) & 0x1F) | (((instr >> 12) & 0x1) << 5)) as u32;
    let imm = imm << 12;
    ((imm as i32) << 14) >> 14
}

fn decode_ci_shamt(instr: u16) -> u8 {
    (((instr >> 2) & 0x1F) | (((instr >> 12) & 0x1) << 5)) as u8
}

fn decode_cb_imm(instr: u16) -> i16 {
    let imm = (((instr >> 3) & 0x3) << 1)
        | (((instr >> 10) & 0x3) << 3)
        | (((instr >> 2) & 0x1) << 5)
        | (((instr >> 5) & 0x3) << 6)
        | (((instr >> 12) & 0x1) << 8);
    ((imm as i16) << 7) >> 7
}

fn decode_ci_lwsp_offset(instr: u16) -> u8 {
    ((((instr >> 4) & 0x7) << 2)
        | (((instr >> 12) & 0x1) << 5)
        | (((instr >> 2) & 0x3) << 6)) as u8
}

fn decode_css_swsp_offset(instr: u16) -> u8 {
    ((((instr >> 9) & 0xF) << 2) | (((instr >> 7) & 0x3) << 6)) as u8
}

fn decode_ci_ldsp_offset(instr: u16) -> u16 {
    (((instr >> 5) & 0x3) << 3)
        | (((instr >> 12) & 0x1) << 5)
        | (((instr >> 2) & 0x7) << 6)
}

fn decode_css_sdsp_offset(instr: u16) -> u16 {
    (((instr >> 10) & 0x7) << 3) | (((instr >> 7) & 0x7) << 6)
}

// Lift functions

fn lift_c<X: Xlen>(args: &InstrArgs, opid: crate::OpId, pc: X::Reg, size: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        // R-type
        OP_C_ADD => lift_r(args, |a, b| Expr::add(a, b)),
        OP_C_SUB => lift_r(args, |a, b| Expr::sub(a, b)),
        OP_C_XOR => lift_r(args, |a, b| Expr::xor(a, b)),
        OP_C_OR => lift_r(args, |a, b| Expr::or(a, b)),
        OP_C_AND => lift_r(args, |a, b| Expr::and(a, b)),
        OP_C_MV => lift_mv(args),
        OP_C_ADDW => lift_r(args, |a, b| Expr::addw(a, b)),
        OP_C_SUBW => lift_r(args, |a, b| Expr::subw(a, b)),

        // I-type
        OP_C_ADDI | OP_C_ADDI4SPN | OP_C_ADDI16SP => lift_i(args, |a, b| Expr::add(a, b)),
        OP_C_ADDIW => lift_i(args, |a, b| Expr::addw(a, b)),
        OP_C_LI => lift_i(args, |_, b| b),
        OP_C_ANDI => lift_i(args, |a, b| Expr::and(a, b)),
        OP_C_SLLI => lift_shamt(args, |a, b| Expr::sll(a, b)),
        OP_C_SRLI => lift_shamt(args, |a, b| Expr::srl(a, b)),
        OP_C_SRAI => lift_shamt(args, |a, b| Expr::sra(a, b)),

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
        OP_C_EBREAK => (Vec::new(), Terminator::trap("ebreak")),

        _ => (Vec::new(), Terminator::trap("unknown C instruction")),
    }
}

fn lift_r<X: Xlen, F>(args: &InstrArgs, op: F) -> (Vec<Stmt<X>>, Terminator<X>)
where F: FnOnce(Expr<X>, Expr<X>) -> Expr<X> {
    match args {
        InstrArgs::R { rd, rs1, rs2 } => {
            let stmts = if *rd != 0 {
                vec![Stmt::write_reg(*rd, op(Expr::read(*rs1), Expr::read(*rs2)))]
            } else { Vec::new() };
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
            } else { Vec::new() };
            (stmts, Terminator::Fall { target: None })
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
            (stmts, Terminator::Fall { target: None })
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
            (stmts, Terminator::Fall { target: None })
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
            (stmts, Terminator::Fall { target: None })
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
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_store<X: Xlen>(args: &InstrArgs, width: u8) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::S { rs1, rs2, imm } => {
            let addr = Expr::add(Expr::read(*rs1), Expr::imm(X::sign_extend_32(*imm as u32)));
            (vec![Stmt::write_mem(addr, Expr::read(*rs2), width)], Terminator::Fall { target: None })
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
                stmts.push(Stmt::write_reg(*rd, Expr::imm(pc + X::from_u64(size as u64))));
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
            let target = Expr::and(Expr::read(*rs1), Expr::not(Expr::imm(X::from_u64(1))));
            (Vec::new(), Terminator::jump_dyn(target))
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
                stmts.push(Stmt::write_reg(*rd, Expr::imm(pc + X::from_u64(size as u64))));
            }
            let target = Expr::and(base, Expr::not(Expr::imm(X::from_u64(1))));
            (stmts, Terminator::jump_dyn(target))
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
