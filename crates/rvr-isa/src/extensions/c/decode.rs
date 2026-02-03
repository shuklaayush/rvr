use super::*;

pub(super) fn decode_q0<X: Xlen>(instr: u16, funct3: u8) -> Option<(crate::OpId, InstrArgs)> {
    match funct3 {
        0b000 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let nzuimm = decode_addi4spn_imm(instr);
            if nzuimm == 0 {
                return None;
            }
            Some((
                OP_C_ADDI4SPN,
                InstrArgs::I {
                    rd,
                    rs1: 2,
                    imm: nzuimm as i32,
                },
            ))
        }
        0b010 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cl_lw_offset(instr);
            Some((
                OP_C_LW,
                InstrArgs::I {
                    rd,
                    rs1,
                    imm: offset as i32,
                },
            ))
        }
        0b011 if X::VALUE == 64 => {
            let rd = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cl_ld_offset(instr);
            Some((
                OP_C_LD,
                InstrArgs::I {
                    rd,
                    rs1,
                    imm: offset as i32,
                },
            ))
        }
        0b110 => {
            let rs2 = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cs_sw_offset(instr);
            Some((
                OP_C_SW,
                InstrArgs::S {
                    rs1,
                    rs2,
                    imm: offset as i32,
                },
            ))
        }
        0b111 if X::VALUE == 64 => {
            let rs2 = ((instr >> 2) & 0x7) as u8 + 8;
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cs_sd_offset(instr);
            Some((
                OP_C_SD,
                InstrArgs::S {
                    rs1,
                    rs2,
                    imm: offset as i32,
                },
            ))
        }
        _ => None,
    }
}

pub(super) fn decode_q1<X: Xlen>(instr: u16, funct3: u8) -> Option<(crate::OpId, InstrArgs)> {
    match funct3 {
        0b000 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let imm = decode_ci_imm(instr);
            if rd == 0 && imm == 0 {
                return Some((OP_C_NOP, InstrArgs::None));
            }
            Some((
                OP_C_ADDI,
                InstrArgs::I {
                    rd,
                    rs1: rd,
                    imm: imm as i32,
                },
            ))
        }
        0b001 => {
            if X::VALUE == 64 {
                let rd = ((instr >> 7) & 0x1F) as u8;
                let imm = decode_ci_imm(instr);
                if rd == 0 {
                    return None;
                }
                Some((
                    OP_C_ADDIW,
                    InstrArgs::I {
                        rd,
                        rs1: rd,
                        imm: imm as i32,
                    },
                ))
            } else {
                let offset = decode_cj_imm(instr);
                Some((
                    OP_C_JAL,
                    InstrArgs::J {
                        rd: 1,
                        imm: offset as i32,
                    },
                ))
            }
        }
        0b010 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let imm = decode_ci_imm(instr);
            Some((
                OP_C_LI,
                InstrArgs::I {
                    rd,
                    rs1: 0,
                    imm: imm as i32,
                },
            ))
        }
        0b011 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            if rd == 2 {
                let imm = decode_ci16sp_imm(instr);
                if imm == 0 {
                    return None;
                }
                Some((
                    OP_C_ADDI16SP,
                    InstrArgs::I {
                        rd: 2,
                        rs1: 2,
                        imm: imm as i32,
                    },
                ))
            } else {
                let imm = decode_ci_lui_imm(instr);
                if imm == 0 || rd == 0 {
                    return None;
                }
                Some((OP_C_LUI, InstrArgs::U { rd, imm }))
            }
        }
        0b100 => decode_misc_alu::<X>(instr),
        0b101 => {
            let offset = decode_cj_imm(instr);
            Some((
                OP_C_J,
                InstrArgs::J {
                    rd: 0,
                    imm: offset as i32,
                },
            ))
        }
        0b110 => {
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cb_imm(instr);
            Some((
                OP_C_BEQZ,
                InstrArgs::B {
                    rs1,
                    rs2: 0,
                    imm: offset as i32,
                },
            ))
        }
        0b111 => {
            let rs1 = ((instr >> 7) & 0x7) as u8 + 8;
            let offset = decode_cb_imm(instr);
            Some((
                OP_C_BNEZ,
                InstrArgs::B {
                    rs1,
                    rs2: 0,
                    imm: offset as i32,
                },
            ))
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
            Some((
                OP_C_SRLI,
                InstrArgs::I {
                    rd,
                    rs1: rd,
                    imm: shamt as i32,
                },
            ))
        }
        0b01 => {
            let shamt = decode_ci_shamt(instr);
            Some((
                OP_C_SRAI,
                InstrArgs::I {
                    rd,
                    rs1: rd,
                    imm: shamt as i32,
                },
            ))
        }
        0b10 => {
            let imm = decode_ci_imm(instr);
            Some((
                OP_C_ANDI,
                InstrArgs::I {
                    rd,
                    rs1: rd,
                    imm: imm as i32,
                },
            ))
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

pub(super) fn decode_q2<X: Xlen>(instr: u16, funct3: u8) -> Option<(crate::OpId, InstrArgs)> {
    match funct3 {
        0b000 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let shamt = decode_ci_shamt(instr);
            if rd == 0 {
                return None;
            }
            Some((
                OP_C_SLLI,
                InstrArgs::I {
                    rd,
                    rs1: rd,
                    imm: shamt as i32,
                },
            ))
        }
        0b010 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let offset = decode_ci_lwsp_offset(instr);
            if rd == 0 {
                return None;
            }
            Some((
                OP_C_LWSP,
                InstrArgs::I {
                    rd,
                    rs1: 2,
                    imm: offset as i32,
                },
            ))
        }
        0b011 if X::VALUE == 64 => {
            let rd = ((instr >> 7) & 0x1F) as u8;
            let offset = decode_ci_ldsp_offset(instr);
            if rd == 0 {
                return None;
            }
            Some((
                OP_C_LDSP,
                InstrArgs::I {
                    rd,
                    rs1: 2,
                    imm: offset as i32,
                },
            ))
        }
        0b100 => {
            let funct4 = ((instr >> 12) & 0x1) as u8;
            let rs1 = ((instr >> 7) & 0x1F) as u8;
            let rs2 = ((instr >> 2) & 0x1F) as u8;

            if funct4 == 0 {
                if rs2 == 0 {
                    if rs1 == 0 {
                        return None;
                    }
                    Some((OP_C_JR, InstrArgs::I { rd: 0, rs1, imm: 0 }))
                } else {
                    Some((
                        OP_C_MV,
                        InstrArgs::R {
                            rd: rs1,
                            rs1: 0,
                            rs2,
                        },
                    ))
                }
            } else if rs1 == 0 && rs2 == 0 {
                Some((OP_C_EBREAK, InstrArgs::None))
            } else if rs2 == 0 {
                Some((OP_C_JALR, InstrArgs::I { rd: 1, rs1, imm: 0 }))
            } else {
                Some((OP_C_ADD, InstrArgs::R { rd: rs1, rs1, rs2 }))
            }
        }
        0b110 => {
            let rs2 = ((instr >> 2) & 0x1F) as u8;
            let offset = decode_css_swsp_offset(instr);
            Some((
                OP_C_SWSP,
                InstrArgs::S {
                    rs1: 2,
                    rs2,
                    imm: offset as i32,
                },
            ))
        }
        0b111 if X::VALUE == 64 => {
            let rs2 = ((instr >> 2) & 0x1F) as u8;
            let offset = decode_css_sdsp_offset(instr);
            Some((
                OP_C_SDSP,
                InstrArgs::S {
                    rs1: 2,
                    rs2,
                    imm: offset as i32,
                },
            ))
        }
        _ => None,
    }
}

// Disassembly

fn decode_addi4spn_imm(instr: u16) -> u16 {
    (((instr >> 6) & 0x1) << 2)
        | (((instr >> 5) & 0x1) << 3)
        | (((instr >> 11) & 0x3) << 4)
        | (((instr >> 7) & 0xF) << 6)
}

fn decode_cl_lw_offset(instr: u16) -> u8 {
    ((((instr >> 6) & 0x1) << 2) | (((instr >> 10) & 0x7) << 3) | (((instr >> 5) & 0x1) << 6)) as u8
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
    ((((instr >> 4) & 0x7) << 2) | (((instr >> 12) & 0x1) << 5) | (((instr >> 2) & 0x3) << 6)) as u8
}

fn decode_css_swsp_offset(instr: u16) -> u8 {
    ((((instr >> 9) & 0xF) << 2) | (((instr >> 7) & 0x3) << 6)) as u8
}

fn decode_ci_ldsp_offset(instr: u16) -> u16 {
    (((instr >> 5) & 0x3) << 3) | (((instr >> 12) & 0x1) << 5) | (((instr >> 2) & 0x7) << 6)
}

fn decode_css_sdsp_offset(instr: u16) -> u16 {
    (((instr >> 10) & 0x7) << 3) | (((instr >> 7) & 0x7) << 6)
}

// Lift functions
