//! Simplified instruction decoder for CFG analysis.
//!
//! Only decodes what's needed for control flow analysis: branches, jumps,
//! and instructions that affect potential jump targets (LUI, AUIPC, ADDI, ADD, loads).

/// Decoded instruction kind for CFG analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CfgInstrKind {
    Unknown,
    Lui,
    Auipc,
    Addi,
    Add,
    Move,
    Jal,
    Jalr,
    Load,
    Store,
    Branch,
}

/// Decoded instruction for CFG analysis.
#[derive(Clone, Debug)]
pub struct CfgInstr {
    pub kind: CfgInstrKind,
    pub rd: i32,
    pub rs1: i32,
    pub rs2: i32,
    pub imm: i32,
    pub width: u8,
    pub is_unsigned: bool,
}

impl Default for CfgInstr {
    fn default() -> Self {
        Self::unknown()
    }
}

impl CfgInstr {
    /// Create an unknown instruction.
    pub fn unknown() -> Self {
        Self {
            kind: CfgInstrKind::Unknown,
            rd: -1,
            rs1: -1,
            rs2: -1,
            imm: 0,
            width: 0,
            is_unsigned: false,
        }
    }

    /// Decode instruction from raw bytes.
    pub fn decode(raw: u32, size: u8) -> Self {
        if size == 2 {
            Self::decode_compressed(raw as u16)
        } else {
            Self::decode_standard(raw)
        }
    }

    fn decode_standard(raw: u32) -> Self {
        let opcode = raw & 0x7F;
        let rd = ((raw >> 7) & 0x1F) as i32;
        let rs1 = ((raw >> 15) & 0x1F) as i32;
        let rs2 = ((raw >> 20) & 0x1F) as i32;
        let funct3 = (raw >> 12) & 0x7;
        let funct7 = (raw >> 25) & 0x7F;

        match opcode {
            0b0110111 => {
                // LUI
                let imm = (raw & 0xFFFF_F000) as i32;
                Self {
                    kind: CfgInstrKind::Lui,
                    rd,
                    rs1: -1,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b0010111 => {
                // AUIPC
                let imm = (raw & 0xFFFF_F000) as i32;
                Self {
                    kind: CfgInstrKind::Auipc,
                    rd,
                    rs1: -1,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b1101111 => {
                // JAL
                let imm = decode_j_imm(raw);
                Self {
                    kind: CfgInstrKind::Jal,
                    rd,
                    rs1: -1,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b1100111 => {
                // JALR
                let imm = decode_i_imm(raw);
                Self {
                    kind: CfgInstrKind::Jalr,
                    rd,
                    rs1,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b1100011 => {
                // BRANCH
                let imm = decode_b_imm(raw);
                Self {
                    kind: CfgInstrKind::Branch,
                    rd: -1,
                    rs1,
                    rs2,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b0000011 => {
                // LOAD
                let imm = decode_i_imm(raw);
                let width = match funct3 {
                    0b000 | 0b100 => 1, // LB, LBU
                    0b001 | 0b101 => 2, // LH, LHU
                    0b010 | 0b110 => 4, // LW, LWU
                    0b011 => 8,         // LD
                    _ => 0,
                };
                let is_unsigned = funct3 == 0b100 || funct3 == 0b101 || funct3 == 0b110;
                Self {
                    kind: CfgInstrKind::Load,
                    rd,
                    rs1,
                    rs2: -1,
                    imm,
                    width,
                    is_unsigned,
                }
            }
            0b0100011 => {
                // STORE
                let imm = decode_s_imm(raw);
                let width = match funct3 {
                    0b000 => 1, // SB
                    0b001 => 2, // SH
                    0b010 => 4, // SW
                    0b011 => 8, // SD
                    _ => 0,
                };
                Self {
                    kind: CfgInstrKind::Store,
                    rd: -1,
                    rs1,
                    rs2,
                    imm,
                    width,
                    is_unsigned: false,
                }
            }
            0b0010011 if funct3 == 0b000 => {
                // ADDI
                let imm = decode_i_imm(raw);
                Self {
                    kind: CfgInstrKind::Addi,
                    rd,
                    rs1,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b0110011 if funct3 == 0b000 && funct7 == 0b0000000 => {
                // ADD
                Self {
                    kind: CfgInstrKind::Add,
                    rd,
                    rs1,
                    rs2,
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b0110011 if funct3 == 0b000 && funct7 == 0b0100000 => {
                // SUB - treat as unknown for value tracking
                Self {
                    kind: CfgInstrKind::Unknown,
                    rd,
                    rs1: -1,
                    rs2: -1,
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                }
            }
            _ => {
                // Unknown but may write a register
                if opcode != 0b0100011 && opcode != 0b1100011 {
                    Self {
                        kind: CfgInstrKind::Unknown,
                        rd,
                        rs1: -1,
                        rs2: -1,
                        imm: 0,
                        width: 0,
                        is_unsigned: false,
                    }
                } else {
                    Self::unknown()
                }
            }
        }
    }

    fn decode_compressed(raw: u16) -> Self {
        let quadrant = raw & 0x3;
        let funct3 = ((raw >> 13) & 0x7) as u32;

        match quadrant {
            0 => Self::decode_c_quadrant0(raw, funct3),
            1 => Self::decode_c_quadrant1(raw, funct3),
            2 => Self::decode_c_quadrant2(raw, funct3),
            _ => Self::unknown(),
        }
    }

    fn decode_c_quadrant0(raw: u16, funct3: u32) -> Self {
        match funct3 {
            0b000 => {
                // C.ADDI4SPN
                let rd_p = (((raw >> 2) & 0x7) + 8) as i32;
                let imm = decode_addi4spn_imm(raw);
                Self {
                    kind: CfgInstrKind::Addi,
                    rd: rd_p,
                    rs1: 2, // sp
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b010 => {
                // C.LW
                let rd = (((raw >> 2) & 0x7) + 8) as i32;
                let rs1 = (((raw >> 7) & 0x7) + 8) as i32;
                let imm = decode_cl_lw_offset(raw);
                Self {
                    kind: CfgInstrKind::Load,
                    rd,
                    rs1,
                    rs2: -1,
                    imm,
                    width: 4,
                    is_unsigned: false,
                }
            }
            0b011 => {
                // C.LD (RV64) or C.FLW (RV32)
                let rd = (((raw >> 2) & 0x7) + 8) as i32;
                let rs1 = (((raw >> 7) & 0x7) + 8) as i32;
                let imm = decode_cl_ld_offset(raw);
                Self {
                    kind: CfgInstrKind::Load,
                    rd,
                    rs1,
                    rs2: -1,
                    imm,
                    width: 8,
                    is_unsigned: false,
                }
            }
            0b110 => {
                // C.SW
                let rs2 = (((raw >> 2) & 0x7) + 8) as i32;
                let rs1 = (((raw >> 7) & 0x7) + 8) as i32;
                let imm = decode_cs_sw_offset(raw);
                Self {
                    kind: CfgInstrKind::Store,
                    rd: -1,
                    rs1,
                    rs2,
                    imm,
                    width: 4,
                    is_unsigned: false,
                }
            }
            0b111 => {
                // C.SD (RV64)
                let rs2 = (((raw >> 2) & 0x7) + 8) as i32;
                let rs1 = (((raw >> 7) & 0x7) + 8) as i32;
                let imm = decode_cs_sd_offset(raw);
                Self {
                    kind: CfgInstrKind::Store,
                    rd: -1,
                    rs1,
                    rs2,
                    imm,
                    width: 8,
                    is_unsigned: false,
                }
            }
            _ => Self::unknown(),
        }
    }

    fn decode_c_quadrant1(raw: u16, funct3: u32) -> Self {
        match funct3 {
            0b000 => {
                // C.ADDI / C.NOP
                let rd = ((raw >> 7) & 0x1F) as i32;
                let imm = decode_ci_imm(raw);
                Self {
                    kind: CfgInstrKind::Addi,
                    rd,
                    rs1: rd,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b001 => {
                // C.JAL (RV32) or C.ADDIW (RV64)
                // For CFG purposes, treat C.JAL as JAL
                let imm = decode_cj_imm(raw);
                Self {
                    kind: CfgInstrKind::Jal,
                    rd: 1, // ra
                    rs1: -1,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b010 => {
                // C.LI
                let rd = ((raw >> 7) & 0x1F) as i32;
                let imm = decode_ci_imm(raw);
                Self {
                    kind: CfgInstrKind::Addi,
                    rd,
                    rs1: 0,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b011 => {
                // C.ADDI16SP or C.LUI
                let rd = ((raw >> 7) & 0x1F) as i32;
                if rd == 2 {
                    // C.ADDI16SP
                    let imm = decode_ci16sp_imm(raw);
                    Self {
                        kind: CfgInstrKind::Addi,
                        rd,
                        rs1: rd,
                        rs2: -1,
                        imm,
                        width: 0,
                        is_unsigned: false,
                    }
                } else {
                    // C.LUI
                    let imm = decode_ci_lui_imm(raw);
                    Self {
                        kind: CfgInstrKind::Lui,
                        rd,
                        rs1: -1,
                        rs2: -1,
                        imm,
                        width: 0,
                        is_unsigned: false,
                    }
                }
            }
            0b100 => {
                // Various ALU ops
                let rd_rs1_p = (((raw >> 7) & 0x7) + 8) as i32;
                Self {
                    kind: CfgInstrKind::Unknown,
                    rd: rd_rs1_p,
                    rs1: -1,
                    rs2: -1,
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b101 => {
                // C.J
                let imm = decode_cj_imm(raw);
                Self {
                    kind: CfgInstrKind::Jal,
                    rd: 0,
                    rs1: -1,
                    rs2: -1,
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b110 | 0b111 => {
                // C.BEQZ / C.BNEZ
                let rs1 = (((raw >> 7) & 0x7) + 8) as i32;
                let imm = decode_cb_imm(raw);
                Self {
                    kind: CfgInstrKind::Branch,
                    rd: -1,
                    rs1,
                    rs2: if funct3 == 0b110 { 0 } else { -1 },
                    imm,
                    width: 0,
                    is_unsigned: false,
                }
            }
            _ => Self::unknown(),
        }
    }

    fn decode_c_quadrant2(raw: u16, funct3: u32) -> Self {
        match funct3 {
            0b000 => {
                // C.SLLI
                let rd = ((raw >> 7) & 0x1F) as i32;
                Self {
                    kind: CfgInstrKind::Unknown,
                    rd,
                    rs1: rd,
                    rs2: -1,
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                }
            }
            0b010 => {
                // C.LWSP
                let rd = ((raw >> 7) & 0x1F) as i32;
                if rd == 0 {
                    return Self::unknown();
                }
                let imm = decode_ci_lwsp_offset(raw);
                Self {
                    kind: CfgInstrKind::Load,
                    rd,
                    rs1: 2, // sp
                    rs2: -1,
                    imm,
                    width: 4,
                    is_unsigned: false,
                }
            }
            0b011 => {
                // C.LDSP (RV64)
                let rd = ((raw >> 7) & 0x1F) as i32;
                if rd == 0 {
                    return Self::unknown();
                }
                let imm = decode_ci_ldsp_offset(raw);
                Self {
                    kind: CfgInstrKind::Load,
                    rd,
                    rs1: 2, // sp
                    rs2: -1,
                    imm,
                    width: 8,
                    is_unsigned: false,
                }
            }
            0b100 => {
                let bit12 = (raw >> 12) & 0x1;
                let rs1 = ((raw >> 7) & 0x1F) as i32;
                let rs2 = ((raw >> 2) & 0x1F) as i32;
                if bit12 == 0 && rs2 == 0 {
                    // C.JR
                    Self {
                        kind: CfgInstrKind::Jalr,
                        rd: 0,
                        rs1,
                        rs2: -1,
                        imm: 0,
                        width: 0,
                        is_unsigned: false,
                    }
                } else if bit12 == 0 {
                    // C.MV
                    Self {
                        kind: CfgInstrKind::Move,
                        rd: rs1,
                        rs1: rs2,
                        rs2: -1,
                        imm: 0,
                        width: 0,
                        is_unsigned: false,
                    }
                } else if rs2 == 0 {
                    // C.JALR
                    Self {
                        kind: CfgInstrKind::Jalr,
                        rd: 1, // ra
                        rs1,
                        rs2: -1,
                        imm: 0,
                        width: 0,
                        is_unsigned: false,
                    }
                } else {
                    // C.ADD
                    Self {
                        kind: CfgInstrKind::Add,
                        rd: rs1,
                        rs1,
                        rs2,
                        imm: 0,
                        width: 0,
                        is_unsigned: false,
                    }
                }
            }
            0b110 => {
                // C.SWSP
                let rs2 = ((raw >> 2) & 0x1F) as i32;
                let imm = decode_css_swsp_offset(raw);
                Self {
                    kind: CfgInstrKind::Store,
                    rd: -1,
                    rs1: 2, // sp
                    rs2,
                    imm,
                    width: 4,
                    is_unsigned: false,
                }
            }
            0b111 => {
                // C.SDSP (RV64)
                let rs2 = ((raw >> 2) & 0x1F) as i32;
                let imm = decode_css_sdsp_offset(raw);
                Self {
                    kind: CfgInstrKind::Store,
                    rd: -1,
                    rs1: 2, // sp
                    rs2,
                    imm,
                    width: 8,
                    is_unsigned: false,
                }
            }
            _ => Self::unknown(),
        }
    }

    /// Check if this is a control flow instruction.
    pub fn is_control_flow(&self) -> bool {
        matches!(
            self.kind,
            CfgInstrKind::Jal | CfgInstrKind::Jalr | CfgInstrKind::Branch
        )
    }

    /// Check if this is a static call (JAL with link).
    pub fn is_static_call(&self) -> bool {
        self.kind == CfgInstrKind::Jal && self.rd != 0
    }

    /// Check if this is any call.
    pub fn is_call(&self) -> bool {
        match self.kind {
            CfgInstrKind::Jal => self.rd != 0,
            CfgInstrKind::Jalr => self.rd != 0,
            _ => false,
        }
    }

    /// Check if this is a return (JALR x0, ra, 0).
    pub fn is_return(&self) -> bool {
        self.kind == CfgInstrKind::Jalr && self.rd == 0 && self.rs1 == 1
    }

    /// Check if this is an indirect jump (JALR x0, rs1, imm where rs1 != ra).
    pub fn is_indirect_jump(&self) -> bool {
        self.kind == CfgInstrKind::Jalr && self.rd == 0 && self.rs1 != 1
    }

    /// Extend loaded value based on width and signedness.
    pub fn extend_loaded_value(value: u64, width: u8, is_unsigned: bool) -> u64 {
        match width {
            1 => {
                let masked = value & 0xFF;
                if is_unsigned || (masked & 0x80) == 0 {
                    masked
                } else {
                    masked | !0xFF
                }
            }
            2 => {
                let masked = value & 0xFFFF;
                if is_unsigned || (masked & 0x8000) == 0 {
                    masked
                } else {
                    masked | !0xFFFF
                }
            }
            4 => {
                let masked = value & 0xFFFF_FFFF;
                if is_unsigned || (masked & 0x8000_0000) == 0 {
                    masked
                } else {
                    masked | !0xFFFF_FFFF
                }
            }
            _ => value,
        }
    }
}

// Immediate decoding helpers

fn decode_i_imm(instr: u32) -> i32 {
    (instr as i32) >> 20
}

fn decode_s_imm(instr: u32) -> i32 {
    let imm_11_5 = (instr >> 25) & 0x7F;
    let imm_4_0 = (instr >> 7) & 0x1F;
    let imm = (imm_11_5 << 5) | imm_4_0;
    ((imm as i32) << 20) >> 20
}

fn decode_b_imm(instr: u32) -> i32 {
    let imm_12 = (instr >> 31) & 0x1;
    let imm_11 = (instr >> 7) & 0x1;
    let imm_10_5 = (instr >> 25) & 0x3F;
    let imm_4_1 = (instr >> 8) & 0xF;
    let imm = (imm_12 << 12) | (imm_11 << 11) | (imm_10_5 << 5) | (imm_4_1 << 1);
    ((imm as i32) << 19) >> 19
}

fn decode_j_imm(instr: u32) -> i32 {
    let imm_20 = (instr >> 31) & 0x1;
    let imm_19_12 = (instr >> 12) & 0xFF;
    let imm_11 = (instr >> 20) & 0x1;
    let imm_10_1 = (instr >> 21) & 0x3FF;
    let imm = (imm_20 << 20) | (imm_19_12 << 12) | (imm_11 << 11) | (imm_10_1 << 1);
    ((imm as i32) << 11) >> 11
}

// Compressed instruction immediate decoders

fn decode_ci_imm(raw: u16) -> i32 {
    let imm_5 = ((raw >> 12) & 0x1) as u32;
    let imm_4_0 = ((raw >> 2) & 0x1F) as u32;
    let imm = (imm_5 << 5) | imm_4_0;
    ((imm as i32) << 26) >> 26
}

fn decode_ci_lui_imm(raw: u16) -> i32 {
    let imm_17 = ((raw >> 12) & 0x1) as u32;
    let imm_16_12 = ((raw >> 2) & 0x1F) as u32;
    let imm = (imm_17 << 17) | (imm_16_12 << 12);
    ((imm as i32) << 14) >> 14
}

fn decode_ci16sp_imm(raw: u16) -> i32 {
    let imm_9 = ((raw >> 12) & 0x1) as u32;
    let imm_4 = ((raw >> 6) & 0x1) as u32;
    let imm_6 = ((raw >> 5) & 0x1) as u32;
    let imm_8_7 = ((raw >> 3) & 0x3) as u32;
    let imm_5 = ((raw >> 2) & 0x1) as u32;
    let imm = (imm_9 << 9) | (imm_8_7 << 7) | (imm_6 << 6) | (imm_5 << 5) | (imm_4 << 4);
    ((imm as i32) << 22) >> 22
}

fn decode_ci_lwsp_offset(raw: u16) -> i32 {
    let imm_5 = ((raw >> 12) & 0x1) as u32;
    let imm_4_2 = ((raw >> 4) & 0x7) as u32;
    let imm_7_6 = ((raw >> 2) & 0x3) as u32;
    ((imm_7_6 << 6) | (imm_5 << 5) | (imm_4_2 << 2)) as i32
}

fn decode_ci_ldsp_offset(raw: u16) -> i32 {
    let imm_5 = ((raw >> 12) & 0x1) as u32;
    let imm_4_3 = ((raw >> 5) & 0x3) as u32;
    let imm_8_6 = ((raw >> 2) & 0x7) as u32;
    ((imm_8_6 << 6) | (imm_5 << 5) | (imm_4_3 << 3)) as i32
}

fn decode_cj_imm(raw: u16) -> i32 {
    let bits = raw as u32;
    let imm_11 = (bits >> 12) & 0x1;
    let imm_4 = (bits >> 11) & 0x1;
    let imm_9_8 = (bits >> 9) & 0x3;
    let imm_10 = (bits >> 8) & 0x1;
    let imm_6 = (bits >> 7) & 0x1;
    let imm_7 = (bits >> 6) & 0x1;
    let imm_3_1 = (bits >> 3) & 0x7;
    let imm_5 = (bits >> 2) & 0x1;
    let imm = (imm_11 << 11)
        | (imm_10 << 10)
        | (imm_9_8 << 8)
        | (imm_7 << 7)
        | (imm_6 << 6)
        | (imm_5 << 5)
        | (imm_4 << 4)
        | (imm_3_1 << 1);
    ((imm as i32) << 20) >> 20
}

fn decode_cb_imm(raw: u16) -> i32 {
    let bits = raw as u32;
    let imm_8 = (bits >> 12) & 0x1;
    let imm_4_3 = (bits >> 10) & 0x3;
    let imm_7_6 = (bits >> 5) & 0x3;
    let imm_2_1 = (bits >> 3) & 0x3;
    let imm_5 = (bits >> 2) & 0x1;
    let imm = (imm_8 << 8) | (imm_7_6 << 6) | (imm_5 << 5) | (imm_4_3 << 3) | (imm_2_1 << 1);
    ((imm as i32) << 23) >> 23
}

fn decode_addi4spn_imm(raw: u16) -> i32 {
    let bits = raw as u32;
    let imm_5_4 = (bits >> 11) & 0x3;
    let imm_9_6 = (bits >> 7) & 0xF;
    let imm_2 = (bits >> 6) & 0x1;
    let imm_3 = (bits >> 5) & 0x1;
    ((imm_9_6 << 6) | (imm_5_4 << 4) | (imm_3 << 3) | (imm_2 << 2)) as i32
}

fn decode_cl_lw_offset(raw: u16) -> i32 {
    let bits = raw as u32;
    let imm_5_3 = (bits >> 10) & 0x7;
    let imm_2 = (bits >> 6) & 0x1;
    let imm_6 = (bits >> 5) & 0x1;
    ((imm_6 << 6) | (imm_5_3 << 3) | (imm_2 << 2)) as i32
}

fn decode_cl_ld_offset(raw: u16) -> i32 {
    let bits = raw as u32;
    let imm_5_3 = (bits >> 10) & 0x7;
    let imm_7_6 = (bits >> 5) & 0x3;
    ((imm_7_6 << 6) | (imm_5_3 << 3)) as i32
}

fn decode_cs_sw_offset(raw: u16) -> i32 {
    decode_cl_lw_offset(raw) // Same encoding
}

fn decode_cs_sd_offset(raw: u16) -> i32 {
    decode_cl_ld_offset(raw) // Same encoding
}

fn decode_css_swsp_offset(raw: u16) -> i32 {
    let bits = raw as u32;
    let imm_5_2 = (bits >> 9) & 0xF;
    let imm_7_6 = (bits >> 7) & 0x3;
    ((imm_7_6 << 6) | (imm_5_2 << 2)) as i32
}

fn decode_css_sdsp_offset(raw: u16) -> i32 {
    let bits = raw as u32;
    let imm_5_3 = (bits >> 10) & 0x7;
    let imm_8_6 = (bits >> 7) & 0x7;
    ((imm_8_6 << 6) | (imm_5_3 << 3)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_lui() {
        // lui x1, 0x12345
        let raw = 0x12345_0b7;
        let decoded = CfgInstr::decode(raw, 4);
        assert_eq!(decoded.kind, CfgInstrKind::Lui);
        assert_eq!(decoded.rd, 1);
        assert_eq!(decoded.imm, 0x12345000u32 as i32);
    }

    #[test]
    fn test_decode_jal() {
        // jal x1, 8 (0x008000ef)
        let raw = 0x008000efu32;
        let decoded = CfgInstr::decode(raw, 4);
        assert_eq!(decoded.kind, CfgInstrKind::Jal);
        assert_eq!(decoded.rd, 1);
        assert_eq!(decoded.imm, 8);
    }

    #[test]
    fn test_decode_branch() {
        // beq x1, x2, 4
        let raw = 0x00208263u32;
        let decoded = CfgInstr::decode(raw, 4);
        assert_eq!(decoded.kind, CfgInstrKind::Branch);
        assert_eq!(decoded.rs1, 1);
        assert_eq!(decoded.rs2, 2);
    }

    #[test]
    fn test_is_call() {
        // jal ra, 0
        let jal = CfgInstr {
            kind: CfgInstrKind::Jal,
            rd: 1,
            rs1: -1,
            rs2: -1,
            imm: 0,
            width: 0,
            is_unsigned: false,
        };
        assert!(jal.is_call());
        assert!(jal.is_static_call());

        // j 0 (jal x0, 0)
        let j = CfgInstr {
            kind: CfgInstrKind::Jal,
            rd: 0,
            rs1: -1,
            rs2: -1,
            imm: 0,
            width: 0,
            is_unsigned: false,
        };
        assert!(!j.is_call());
    }

    #[test]
    fn test_is_return() {
        // ret (jalr x0, ra, 0)
        let ret = CfgInstr {
            kind: CfgInstrKind::Jalr,
            rd: 0,
            rs1: 1,
            rs2: -1,
            imm: 0,
            width: 0,
            is_unsigned: false,
        };
        assert!(ret.is_return());
        assert!(!ret.is_call());
    }
}
