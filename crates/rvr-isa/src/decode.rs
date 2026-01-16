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
fn decode_compressed<X: Xlen>(_instr: u16, _pc: X::Reg) -> Option<DecodedInstr<X>> {
    // TODO: Implement compressed instruction decoding
    None
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
}
