use super::*;

pub(super) fn decode_32bit<X: Xlen>(instr: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
    let opcode = decode_opcode(instr);
    let funct3 = decode_funct3(instr);
    let funct7 = decode_funct7(instr);
    let rd = decode_rd(instr);
    let rs1 = decode_rs1(instr);
    let rs2 = decode_rs2(instr);

    let (opid, args) = match opcode {
        0x37 => (
            OP_LUI,
            InstrArgs::U {
                rd,
                imm: decode_u_imm(instr),
            },
        ),
        0x17 => (
            OP_AUIPC,
            InstrArgs::U {
                rd,
                imm: decode_u_imm(instr),
            },
        ),
        0x6F => (
            OP_JAL,
            InstrArgs::J {
                rd,
                imm: decode_j_imm(instr),
            },
        ),
        0x67 if funct3 == 0 => (
            OP_JALR,
            InstrArgs::I {
                rd,
                rs1,
                imm: decode_i_imm(instr),
            },
        ),
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
        0x13 => {
            let imm = decode_i_imm(instr);
            let shamt = (instr >> 20) & 0x3F;
            let op = match funct3 {
                0 => OP_ADDI,
                1 if (funct7 & 0xFE) == 0 => OP_SLLI,
                2 => OP_SLTI,
                3 => OP_SLTIU,
                4 => OP_XORI,
                5 if (funct7 & 0xFE) == 0 => OP_SRLI,
                5 if (funct7 & 0xFE) == 0x20 => OP_SRAI,
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
            let imm = if funct3 == 1 || funct3 == 5 {
                shamt
            } else {
                imm
            };
            (op, InstrArgs::I { rd, rs1, imm })
        }
        0x33 if funct7 != 0x01 => {
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
                _ => return None,
            };
            (op, InstrArgs::R { rd, rs1, rs2 })
        }
        0x3B if X::VALUE == 64 && funct7 != 0x01 => {
            let op = match (funct7, funct3) {
                (0x00, 0) => OP_ADDW,
                (0x20, 0) => OP_SUBW,
                (0x00, 1) => OP_SLLW,
                (0x00, 5) => OP_SRLW,
                (0x20, 5) => OP_SRAW,
                _ => return None,
            };
            (op, InstrArgs::R { rd, rs1, rs2 })
        }
        0x0F if funct3 == 0 => (OP_FENCE, InstrArgs::None),
        0x73 if funct3 == 0 => {
            if instr == 0x00000073 {
                (OP_ECALL, InstrArgs::None)
            } else if instr == 0x00100073 {
                (OP_EBREAK, InstrArgs::None)
            } else if instr == 0x30200073 {
                (OP_MRET, InstrArgs::None)
            } else {
                return None;
            }
        }
        _ => return None,
    };

    Some(DecodedInstr::new(opid, pc, 4, instr, args))
}

// ===== Lift =====
