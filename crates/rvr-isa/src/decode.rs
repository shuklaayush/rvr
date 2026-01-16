//! Main instruction decoder.

use crate::{OpId, Xlen};

/// Decoded instruction with all fields extracted.
#[derive(Clone, Debug)]
pub struct DecodedInstr<X: Xlen> {
    /// Instruction identifier.
    pub opid: OpId,
    /// Program counter.
    pub pc: X::Reg,
    /// Instruction size in bytes (2 for compressed, 4 for normal).
    pub size: u8,
    /// Instruction arguments.
    pub args: InstrArgs,
}

/// Instruction argument patterns (covers all RISC-V formats + custom).
#[derive(Clone, Debug, PartialEq)]
pub enum InstrArgs {
    /// R-type: rd, rs1, rs2
    R { rd: u8, rs1: u8, rs2: u8 },
    /// R4-type: rd, rs1, rs2, rs3 (for fused ops)
    R4 { rd: u8, rs1: u8, rs2: u8, rs3: u8 },
    /// I-type: rd, rs1, imm
    I { rd: u8, rs1: u8, imm: i32 },
    /// S-type: rs1, rs2, imm
    S { rs1: u8, rs2: u8, imm: i32 },
    /// B-type: rs1, rs2, imm
    B { rs1: u8, rs2: u8, imm: i32 },
    /// U-type: rd, imm
    U { rd: u8, imm: i32 },
    /// J-type: rd, imm
    J { rd: u8, imm: i32 },
    /// CSR: rd, rs1, csr
    Csr { rd: u8, rs1: u8, csr: u16 },
    /// CSRI: rd, imm, csr
    CsrI { rd: u8, imm: u8, csr: u16 },
    /// AMO: rd, rs1, rs2, aq, rl
    Amo {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// No arguments (ECALL, EBREAK, etc.)
    None,
    /// Custom instruction arguments
    Custom(Box<[u32]>),
}

impl<X: Xlen> DecodedInstr<X> {
    pub fn new(opid: OpId, pc: X::Reg, size: u8, args: InstrArgs) -> Self {
        Self { opid, pc, size, args }
    }
}

/// Decode a single instruction at the given address.
pub fn decode<X: Xlen>(bytes: &[u8], pc: X::Reg) -> Option<DecodedInstr<X>> {
    if bytes.is_empty() {
        return None;
    }

    // Check if compressed (bits [1:0] != 0b11)
    let is_compressed = (bytes[0] & 0x03) != 0x03;

    if is_compressed {
        if bytes.len() < 2 {
            return None;
        }
        let instr = u16::from_le_bytes([bytes[0], bytes[1]]);
        decode_compressed(instr, pc)
    } else {
        if bytes.len() < 4 {
            return None;
        }
        let instr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        decode_32bit(instr, pc)
    }
}

/// Decode a 32-bit instruction.
fn decode_32bit<X: Xlen>(instr: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
    use crate::encode::*;
    use crate::base::*;
    use crate::m::*;
    use crate::a::*;
    use crate::zicsr::*;

    let opcode = decode_opcode(instr);
    let funct3 = decode_funct3(instr);
    let funct7 = decode_funct7(instr);
    let rd = decode_rd(instr);
    let rs1 = decode_rs1(instr);
    let rs2 = decode_rs2(instr);

    let (opid, args) = match opcode {
        // LUI
        0x37 => (OP_LUI, InstrArgs::U { rd, imm: decode_u_imm(instr) }),
        // AUIPC
        0x17 => (OP_AUIPC, InstrArgs::U { rd, imm: decode_u_imm(instr) }),
        // JAL
        0x6F => (OP_JAL, InstrArgs::J { rd, imm: decode_j_imm(instr) }),
        // JALR
        0x67 => (OP_JALR, InstrArgs::I { rd, rs1, imm: decode_i_imm(instr) }),
        // Branch
        0x63 => {
            let imm = decode_b_imm(instr);
            let op = match funct3 {
                0 => OP_BEQ,
                1 => OP_BNE,
                4 => OP_BLT,
                5 => OP_BGE,
                6 => OP_BLTU,
                7 => OP_BGEU,
                _ => return None,
            };
            (op, InstrArgs::B { rs1, rs2, imm })
        }
        // Load
        0x03 => {
            let imm = decode_i_imm(instr);
            let op = match funct3 {
                0 => OP_LB,
                1 => OP_LH,
                2 => OP_LW,
                3 if X::VALUE == 64 => OP_LD,
                4 => OP_LBU,
                5 => OP_LHU,
                6 if X::VALUE == 64 => OP_LWU,
                _ => return None,
            };
            (op, InstrArgs::I { rd, rs1, imm })
        }
        // Store
        0x23 => {
            let imm = decode_s_imm(instr);
            let op = match funct3 {
                0 => OP_SB,
                1 => OP_SH,
                2 => OP_SW,
                3 if X::VALUE == 64 => OP_SD,
                _ => return None,
            };
            (op, InstrArgs::S { rs1, rs2, imm })
        }
        // OP-IMM
        0x13 => {
            let imm = decode_i_imm(instr);
            let shamt = (instr >> 20) & 0x3F;
            let op = match funct3 {
                0 => OP_ADDI,
                1 if funct7 == 0 => OP_SLLI,
                2 => OP_SLTI,
                3 => OP_SLTIU,
                4 => OP_XORI,
                5 if (funct7 & 0x20) == 0 => OP_SRLI,
                5 => OP_SRAI,
                6 => OP_ORI,
                7 => OP_ANDI,
                _ => return None,
            };
            let imm = if funct3 == 1 || funct3 == 5 {
                shamt as i32
            } else {
                imm
            };
            (op, InstrArgs::I { rd, rs1, imm })
        }
        // OP-IMM-32 (RV64 only)
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
        // OP
        0x33 => {
            let op = match (funct7, funct3) {
                (0x00, 0) => OP_ADD,
                (0x20, 0) => OP_SUB,
                (0x00, 1) => OP_SLL,
                (0x00, 2) => OP_SLT,
                (0x00, 3) => OP_SLTU,
                (0x00, 4) => OP_XOR,
                (0x00, 5) => OP_SRL,
                (0x20, 5) => OP_SRA,
                (0x00, 6) => OP_OR,
                (0x00, 7) => OP_AND,
                // M extension
                (0x01, 0) => OP_MUL,
                (0x01, 1) => OP_MULH,
                (0x01, 2) => OP_MULHSU,
                (0x01, 3) => OP_MULHU,
                (0x01, 4) => OP_DIV,
                (0x01, 5) => OP_DIVU,
                (0x01, 6) => OP_REM,
                (0x01, 7) => OP_REMU,
                _ => return None,
            };
            (op, InstrArgs::R { rd, rs1, rs2 })
        }
        // OP-32 (RV64 only)
        0x3B if X::VALUE == 64 => {
            let op = match (funct7, funct3) {
                (0x00, 0) => OP_ADDW,
                (0x20, 0) => OP_SUBW,
                (0x00, 1) => OP_SLLW,
                (0x00, 5) => OP_SRLW,
                (0x20, 5) => OP_SRAW,
                // M extension W variants
                (0x01, 0) => OP_MULW,
                (0x01, 4) => OP_DIVW,
                (0x01, 5) => OP_DIVUW,
                (0x01, 6) => OP_REMW,
                (0x01, 7) => OP_REMUW,
                _ => return None,
            };
            (op, InstrArgs::R { rd, rs1, rs2 })
        }
        // FENCE
        0x0F => {
            match funct3 {
                0 => (OP_FENCE, InstrArgs::None),
                1 => (OP_FENCE_I, InstrArgs::None),
                _ => return None,
            }
        }
        // SYSTEM
        0x73 => {
            let csr = ((instr >> 20) & 0xFFF) as u16;
            match funct3 {
                0 => {
                    // ECALL/EBREAK
                    if instr == 0x00000073 {
                        (OP_ECALL, InstrArgs::None)
                    } else if instr == 0x00100073 {
                        (OP_EBREAK, InstrArgs::None)
                    } else {
                        return None;
                    }
                }
                1 => (OP_CSRRW, InstrArgs::Csr { rd, rs1, csr }),
                2 => (OP_CSRRS, InstrArgs::Csr { rd, rs1, csr }),
                3 => (OP_CSRRC, InstrArgs::Csr { rd, rs1, csr }),
                5 => (OP_CSRRWI, InstrArgs::CsrI { rd, imm: rs1, csr }),
                6 => (OP_CSRRSI, InstrArgs::CsrI { rd, imm: rs1, csr }),
                7 => (OP_CSRRCI, InstrArgs::CsrI { rd, imm: rs1, csr }),
                _ => return None,
            }
        }
        // AMO
        0x2F => {
            let aq = ((instr >> 26) & 1) != 0;
            let rl = ((instr >> 25) & 1) != 0;
            let funct5 = (instr >> 27) & 0x1F;
            let width = funct3;

            // Only .W (funct3=2) and .D (funct3=3, RV64) are supported
            if width != 2 && !(width == 3 && X::VALUE == 64) {
                return None;
            }

            let is_64 = width == 3;
            let op = match funct5 {
                0x02 => if is_64 { OP_LR_D } else { OP_LR_W },
                0x03 => if is_64 { OP_SC_D } else { OP_SC_W },
                0x01 => if is_64 { OP_AMOSWAP_D } else { OP_AMOSWAP_W },
                0x00 => if is_64 { OP_AMOADD_D } else { OP_AMOADD_W },
                0x04 => if is_64 { OP_AMOXOR_D } else { OP_AMOXOR_W },
                0x0C => if is_64 { OP_AMOAND_D } else { OP_AMOAND_W },
                0x08 => if is_64 { OP_AMOOR_D } else { OP_AMOOR_W },
                0x10 => if is_64 { OP_AMOMIN_D } else { OP_AMOMIN_W },
                0x14 => if is_64 { OP_AMOMAX_D } else { OP_AMOMAX_W },
                0x18 => if is_64 { OP_AMOMINU_D } else { OP_AMOMINU_W },
                0x1C => if is_64 { OP_AMOMAXU_D } else { OP_AMOMAXU_W },
                _ => return None,
            };
            (op, InstrArgs::Amo { rd, rs1, rs2, aq, rl })
        }
        _ => return None,
    };

    Some(DecodedInstr::new(opid, pc, 4, args))
}

/// Decode a 16-bit compressed instruction.
fn decode_compressed<X: Xlen>(instr: u16, pc: X::Reg) -> Option<DecodedInstr<X>> {
    let quadrant = instr & 0x3;
    let funct3 = ((instr >> 13) & 0x7) as u8;

    let (opid, args) = match quadrant {
        // Quadrant 0
        0b00 => decode_compressed_q0::<X>(instr, funct3)?,
        // Quadrant 1
        0b01 => decode_compressed_q1::<X>(instr, funct3)?,
        // Quadrant 2
        0b10 => decode_compressed_q2::<X>(instr, funct3)?,
        _ => return None,
    };

    Some(DecodedInstr::new(opid, pc, 2, args))
}

/// Decode compressed instruction quadrant 0.
fn decode_compressed_q0<X: Xlen>(instr: u16, funct3: u8) -> Option<(OpId, InstrArgs)> {
    use crate::c::*;

    match funct3 {
        // C.ADDI4SPN
        0b000 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8; // x8-x15
            let nzuimm = decode_addi4spn_imm(instr);
            if nzuimm == 0 {
                return None; // Reserved
            }
            Some((OP_C_ADDI4SPN, InstrArgs::I { rd, rs1: 2, imm: nzuimm as i32 }))
        }
        // C.LW
        0b010 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cl_lw_offset(instr);
            Some((OP_C_LW, InstrArgs::I { rd, rs1, imm: offset as i32 }))
        }
        // C.LD (RV64C) or C.FLW (RV32FC - not supported)
        0b011 => {
            if X::VALUE != 64 {
                return None;
            }
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cl_ld_offset(instr);
            Some((OP_C_LD, InstrArgs::I { rd, rs1, imm: offset as i32 }))
        }
        // C.SW
        0b110 => {
            let rs2 = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cs_sw_offset(instr);
            Some((OP_C_SW, InstrArgs::S { rs1, rs2, imm: offset as i32 }))
        }
        // C.SD (RV64C) or C.FSW (RV32FC - not supported)
        0b111 => {
            if X::VALUE != 64 {
                return None;
            }
            let rs2 = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cs_sd_offset(instr);
            Some((OP_C_SD, InstrArgs::S { rs1, rs2, imm: offset as i32 }))
        }
        _ => None,
    }
}

/// Decode compressed instruction quadrant 1.
fn decode_compressed_q1<X: Xlen>(instr: u16, funct3: u8) -> Option<(OpId, InstrArgs)> {
    use crate::c::*;

    match funct3 {
        // C.ADDI / C.NOP
        0b000 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let imm = decode_ci_imm(instr);
            if rd == 0 && imm == 0 {
                return Some((OP_C_NOP, InstrArgs::None));
            }
            Some((OP_C_ADDI, InstrArgs::I { rd, rs1: rd, imm: imm as i32 }))
        }
        // C.JAL (RV32 only) / C.ADDIW (RV64 only)
        0b001 => {
            if X::VALUE == 64 {
                let rd = ((instr >> 7) & 0x1F) as u8;
                let imm = decode_ci_imm(instr);
                if rd == 0 {
                    return None; // Reserved
                }
                Some((OP_C_ADDIW, InstrArgs::I { rd, rs1: rd, imm: imm as i32 }))
            } else {
                let offset = decode_cj_imm(instr);
                Some((OP_C_JAL, InstrArgs::J { rd: 1, imm: offset as i32 }))
            }
        }
        // C.LI
        0b010 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let imm = decode_ci_imm(instr);
            Some((OP_C_LI, InstrArgs::I { rd, rs1: 0, imm: imm as i32 }))
        }
        // C.ADDI16SP / C.LUI
        0b011 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            if rd == 2 {
                // C.ADDI16SP
                let imm = decode_ci16sp_imm(instr);
                if imm == 0 {
                    return None; // Reserved
                }
                Some((OP_C_ADDI16SP, InstrArgs::I { rd: 2, rs1: 2, imm: imm as i32 }))
            } else {
                // C.LUI
                let imm = decode_ci_lui_imm(instr);
                if imm == 0 || rd == 0 {
                    return None; // Reserved
                }
                Some((OP_C_LUI, InstrArgs::U { rd, imm: imm as i32 }))
            }
        }
        // Misc ALU
        0b100 => decode_compressed_misc_alu::<X>(instr),
        // C.J
        0b101 => {
            let offset = decode_cj_imm(instr);
            Some((OP_C_J, InstrArgs::J { rd: 0, imm: offset as i32 }))
        }
        // C.BEQZ
        0b110 => {
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cb_imm(instr);
            Some((OP_C_BEQZ, InstrArgs::B { rs1, rs2: 0, imm: offset as i32 }))
        }
        // C.BNEZ
        0b111 => {
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cb_imm(instr);
            Some((OP_C_BNEZ, InstrArgs::B { rs1, rs2: 0, imm: offset as i32 }))
        }
        _ => None,
    }
}

/// Decode compressed instruction quadrant 1 misc ALU.
fn decode_compressed_misc_alu<X: Xlen>(instr: u16) -> Option<(OpId, InstrArgs)> {
    use crate::c::*;

    let funct2 = ((instr >> 10) & 0x3) as u8;
    let rd = ((instr >> 7) & 0x7) as u8 + 8;

    match funct2 {
        // C.SRLI
        0b00 => {
            let shamt = decode_ci_shamt(instr);
            Some((OP_C_SRLI, InstrArgs::I { rd, rs1: rd, imm: shamt as i32 }))
        }
        // C.SRAI
        0b01 => {
            let shamt = decode_ci_shamt(instr);
            Some((OP_C_SRAI, InstrArgs::I { rd, rs1: rd, imm: shamt as i32 }))
        }
        // C.ANDI
        0b10 => {
            let imm = decode_ci_imm(instr);
            Some((OP_C_ANDI, InstrArgs::I { rd, rs1: rd, imm: imm as i32 }))
        }
        // C.SUB/C.XOR/C.OR/C.AND/C.SUBW/C.ADDW
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
                // RV64C only
                if X::VALUE != 64 {
                    return None;
                }
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

/// Decode compressed instruction quadrant 2.
fn decode_compressed_q2<X: Xlen>(instr: u16, funct3: u8) -> Option<(OpId, InstrArgs)> {
    use crate::c::*;

    match funct3 {
        // C.SLLI
        0b000 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let shamt = decode_ci_shamt(instr);
            if rd == 0 {
                return None; // Reserved
            }
            Some((OP_C_SLLI, InstrArgs::I { rd, rs1: rd, imm: shamt as i32 }))
        }
        // C.LWSP
        0b010 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let offset = decode_ci_lwsp_offset(instr);
            if rd == 0 {
                return None; // Reserved
            }
            Some((OP_C_LWSP, InstrArgs::I { rd, rs1: 2, imm: offset as i32 }))
        }
        // C.LDSP (RV64C) or C.FLWSP (RV32FC - not supported)
        0b011 => {
            if X::VALUE != 64 {
                return None;
            }
            let rd = ((instr >> 7) & 0x1F) as u8;
            let offset = decode_ci_ldsp_offset(instr);
            if rd == 0 {
                return None; // Reserved
            }
            Some((OP_C_LDSP, InstrArgs::I { rd, rs1: 2, imm: offset as i32 }))
        }
        // C.JR/C.MV/C.EBREAK/C.JALR/C.ADD
        0b100 => {
            let funct4 = ((instr >> 12) & 0x1) as u8;
            let rs1 = ((instr >> 7) & 0x1F) as u8;
            let rs2 = ((instr >> 2) & 0x1F) as u8;

            if funct4 == 0 {
                if rs2 == 0 {
                    // C.JR
                    if rs1 == 0 {
                        return None; // Reserved
                    }
                    Some((OP_C_JR, InstrArgs::I { rd: 0, rs1, imm: 0 }))
                } else {
                    // C.MV
                    Some((OP_C_MV, InstrArgs::R { rd: rs1, rs1: 0, rs2 }))
                }
            } else {
                if rs1 == 0 && rs2 == 0 {
                    // C.EBREAK
                    Some((OP_C_EBREAK, InstrArgs::None))
                } else if rs2 == 0 {
                    // C.JALR
                    Some((OP_C_JALR, InstrArgs::I { rd: 1, rs1, imm: 0 }))
                } else {
                    // C.ADD
                    Some((OP_C_ADD, InstrArgs::R { rd: rs1, rs1, rs2 }))
                }
            }
        }
        // C.SWSP
        0b110 => {
            let rs2 = ((instr >> 2) & 0x1F) as u8;
            let offset = decode_css_swsp_offset(instr);
            Some((OP_C_SWSP, InstrArgs::S { rs1: 2, rs2, imm: offset as i32 }))
        }
        // C.SDSP (RV64C) or C.FSWSP (RV32FC - not supported)
        0b111 => {
            if X::VALUE != 64 {
                return None;
            }
            let rs2 = ((instr >> 2) & 0x1F) as u8;
            let offset = decode_css_sdsp_offset(instr);
            Some((OP_C_SDSP, InstrArgs::S { rs1: 2, rs2, imm: offset as i32 }))
        }
        _ => None,
    }
}

// Compressed immediate decoders

/// Decode C.ADDI4SPN immediate: nzuimm[5:4|9:6|2|3]
#[inline]
fn decode_addi4spn_imm(instr: u16) -> u16 {
    (((instr >> 6) & 0x1) << 2)
        | (((instr >> 5) & 0x1) << 3)
        | (((instr >> 11) & 0x3) << 4)
        | (((instr >> 7) & 0xF) << 6)
}

/// Decode C.LW offset: uimm[5:3|2|6]
#[inline]
fn decode_cl_lw_offset(instr: u16) -> u8 {
    ((((instr >> 6) & 0x1) << 2)
        | (((instr >> 10) & 0x7) << 3)
        | (((instr >> 5) & 0x1) << 6)) as u8
}

/// Decode C.SW offset (same as C.LW)
#[inline]
fn decode_cs_sw_offset(instr: u16) -> u8 {
    decode_cl_lw_offset(instr)
}

/// Decode C.LD offset: uimm[5:3|7:6]
#[inline]
fn decode_cl_ld_offset(instr: u16) -> u8 {
    ((((instr >> 10) & 0x7) << 3) | (((instr >> 5) & 0x3) << 6)) as u8
}

/// Decode C.SD offset (same as C.LD)
#[inline]
fn decode_cs_sd_offset(instr: u16) -> u8 {
    decode_cl_ld_offset(instr)
}

/// Decode CI-format 6-bit signed immediate.
#[inline]
fn decode_ci_imm(instr: u16) -> i8 {
    let imm = (((instr >> 2) & 0x1F) | (((instr >> 12) & 0x1) << 5)) as u8;
    // Sign extend from 6 bits
    ((imm as i8) << 2) >> 2
}

/// Decode CJ-format 12-bit signed immediate.
#[inline]
fn decode_cj_imm(instr: u16) -> i16 {
    let imm = (((instr >> 3) & 0x7) << 1)
        | (((instr >> 11) & 0x1) << 4)
        | (((instr >> 2) & 0x1) << 5)
        | (((instr >> 7) & 0x1) << 6)
        | (((instr >> 6) & 0x1) << 7)
        | (((instr >> 9) & 0x3) << 8)
        | (((instr >> 8) & 0x1) << 10)
        | (((instr >> 12) & 0x1) << 11);
    // Sign extend from 12 bits
    ((imm as i16) << 4) >> 4
}

/// Decode C.ADDI16SP 10-bit signed immediate.
#[inline]
fn decode_ci16sp_imm(instr: u16) -> i16 {
    let imm = (((instr >> 6) & 0x1) << 4)
        | (((instr >> 2) & 0x1) << 5)
        | (((instr >> 5) & 0x1) << 6)
        | (((instr >> 3) & 0x3) << 7)
        | (((instr >> 12) & 0x1) << 9);
    // Sign extend from 10 bits
    ((imm as i16) << 6) >> 6
}

/// Decode C.LUI 18-bit signed immediate (already shifted by 12).
#[inline]
fn decode_ci_lui_imm(instr: u16) -> i32 {
    let imm = (((instr >> 2) & 0x1F) | (((instr >> 12) & 0x1) << 5)) as u32;
    let imm = imm << 12;
    // Sign extend from 18 bits
    ((imm as i32) << 14) >> 14
}

/// Decode CI-format shift amount.
#[inline]
fn decode_ci_shamt(instr: u16) -> u8 {
    (((instr >> 2) & 0x1F) | (((instr >> 12) & 0x1) << 5)) as u8
}

/// Decode CB-format 9-bit signed branch offset.
#[inline]
fn decode_cb_imm(instr: u16) -> i16 {
    let imm = (((instr >> 3) & 0x3) << 1)
        | (((instr >> 10) & 0x3) << 3)
        | (((instr >> 2) & 0x1) << 5)
        | (((instr >> 5) & 0x3) << 6)
        | (((instr >> 12) & 0x1) << 8);
    // Sign extend from 9 bits
    ((imm as i16) << 7) >> 7
}

/// Decode C.LWSP offset.
#[inline]
fn decode_ci_lwsp_offset(instr: u16) -> u8 {
    ((((instr >> 4) & 0x7) << 2)
        | (((instr >> 12) & 0x1) << 5)
        | (((instr >> 2) & 0x3) << 6)) as u8
}

/// Decode C.SWSP offset.
#[inline]
fn decode_css_swsp_offset(instr: u16) -> u8 {
    ((((instr >> 9) & 0xF) << 2) | (((instr >> 7) & 0x3) << 6)) as u8
}

/// Decode C.LDSP offset (RV64C).
#[inline]
fn decode_ci_ldsp_offset(instr: u16) -> u16 {
    (((instr >> 5) & 0x3) << 3)
        | (((instr >> 12) & 0x1) << 5)
        | (((instr >> 2) & 0x7) << 6)
}

/// Decode C.SDSP offset (RV64C).
#[inline]
fn decode_css_sdsp_offset(instr: u16) -> u16 {
    (((instr >> 10) & 0x7) << 3) | (((instr >> 7) & 0x7) << 6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rv64;

    #[test]
    fn test_decode_addi() {
        // ADDI x1, x0, 1 (0x00100093)
        let bytes = [0x93, 0x00, 0x10, 0x00];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        assert_eq!(decoded.opid, crate::base::OP_ADDI);
        assert_eq!(decoded.size, 4);
        match decoded.args {
            InstrArgs::I { rd, rs1, imm } => {
                assert_eq!(rd, 1);
                assert_eq!(rs1, 0);
                assert_eq!(imm, 1);
            }
            _ => panic!("Expected I-type args"),
        }
    }

    #[test]
    fn test_decode_add() {
        // ADD x1, x2, x3 (0x003100B3)
        let bytes = [0xB3, 0x00, 0x31, 0x00];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        assert_eq!(decoded.opid, crate::base::OP_ADD);
        match decoded.args {
            InstrArgs::R { rd, rs1, rs2 } => {
                assert_eq!(rd, 1);
                assert_eq!(rs1, 2);
                assert_eq!(rs2, 3);
            }
            _ => panic!("Expected R-type args"),
        }
    }

    #[test]
    fn test_decode_c_addi() {
        // C.ADDI x10, 1 (0x0505)
        // Encoding: 000|imm[5]|rd|imm[4:0]|01
        // rd = 10 (01010), imm = 1 (000001)
        // 000 0 01010 00001 01 = 0x0505
        let bytes = [0x05, 0x05];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        assert_eq!(decoded.opid, crate::c::OP_C_ADDI);
        assert_eq!(decoded.size, 2);
        match decoded.args {
            InstrArgs::I { rd, rs1, imm } => {
                assert_eq!(rd, 10);
                assert_eq!(rs1, 10);
                assert_eq!(imm, 1);
            }
            _ => panic!("Expected I-type args, got {:?}", decoded.args),
        }
    }

    #[test]
    fn test_decode_c_li() {
        // C.LI x10, 5 (0x4515)
        // Encoding: 010|imm[5]|rd|imm[4:0]|01
        // rd = 10, imm = 5
        // 010 0 01010 00101 01 = 0x4515
        let bytes = [0x15, 0x45];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        assert_eq!(decoded.opid, crate::c::OP_C_LI);
        assert_eq!(decoded.size, 2);
        match decoded.args {
            InstrArgs::I { rd, rs1, imm } => {
                assert_eq!(rd, 10);
                assert_eq!(rs1, 0);
                assert_eq!(imm, 5);
            }
            _ => panic!("Expected I-type args, got {:?}", decoded.args),
        }
    }

    #[test]
    fn test_decode_c_add() {
        // C.ADD x10, x11 (0x952e)
        // Encoding: 1001|rd|rs2|10
        // rd = 10 (01010), rs2 = 11 (01011)
        // 1001 01010 01011 10 = 0x952e
        let bytes = [0x2e, 0x95];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        assert_eq!(decoded.opid, crate::c::OP_C_ADD);
        assert_eq!(decoded.size, 2);
        match decoded.args {
            InstrArgs::R { rd, rs1, rs2 } => {
                assert_eq!(rd, 10);
                assert_eq!(rs1, 10);
                assert_eq!(rs2, 11);
            }
            _ => panic!("Expected R-type args, got {:?}", decoded.args),
        }
    }

    #[test]
    fn test_decode_c_nop() {
        // C.NOP (0x0001)
        let bytes = [0x01, 0x00];
        let decoded = decode::<Rv64>(&bytes, 0u64).unwrap();
        assert_eq!(decoded.opid, crate::c::OP_C_NOP);
        assert_eq!(decoded.size, 2);
        assert_eq!(decoded.args, InstrArgs::None);
    }
}
