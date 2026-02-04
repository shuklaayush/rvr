//! Base I extension (RV32I/RV64I) - decode, lift, disasm.

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use super::InstructionExtension;
use crate::{
    DecodedInstr, EXT_I, InstrArgs, OpClass, OpId, OpInfo,
    encode::{
        decode_b_imm, decode_funct3, decode_funct7, decode_i_imm, decode_j_imm, decode_opcode,
        decode_rd, decode_rs1, decode_rs2, decode_s_imm, decode_u_imm,
    },
    reg_name,
};

mod decode;
mod disasm;
mod lift;

use decode::decode_32bit;
use disasm::format_instr;
use lift::lift_base;

// ===== OpId Constants =====

pub const OP_LUI: OpId = OpId::new(EXT_I, 0);
pub const OP_AUIPC: OpId = OpId::new(EXT_I, 1);
pub const OP_JAL: OpId = OpId::new(EXT_I, 2);
pub const OP_JALR: OpId = OpId::new(EXT_I, 3);
pub const OP_BEQ: OpId = OpId::new(EXT_I, 4);
pub const OP_BNE: OpId = OpId::new(EXT_I, 5);
pub const OP_BLT: OpId = OpId::new(EXT_I, 6);
pub const OP_BGE: OpId = OpId::new(EXT_I, 7);
pub const OP_BLTU: OpId = OpId::new(EXT_I, 8);
pub const OP_BGEU: OpId = OpId::new(EXT_I, 9);
pub const OP_LB: OpId = OpId::new(EXT_I, 10);
pub const OP_LH: OpId = OpId::new(EXT_I, 11);
pub const OP_LW: OpId = OpId::new(EXT_I, 12);
pub const OP_LBU: OpId = OpId::new(EXT_I, 13);
pub const OP_LHU: OpId = OpId::new(EXT_I, 14);
pub const OP_SB: OpId = OpId::new(EXT_I, 15);
pub const OP_SH: OpId = OpId::new(EXT_I, 16);
pub const OP_SW: OpId = OpId::new(EXT_I, 17);
pub const OP_ADDI: OpId = OpId::new(EXT_I, 18);
pub const OP_SLTI: OpId = OpId::new(EXT_I, 19);
pub const OP_SLTIU: OpId = OpId::new(EXT_I, 20);
pub const OP_XORI: OpId = OpId::new(EXT_I, 21);
pub const OP_ORI: OpId = OpId::new(EXT_I, 22);
pub const OP_ANDI: OpId = OpId::new(EXT_I, 23);
pub const OP_SLLI: OpId = OpId::new(EXT_I, 24);
pub const OP_SRLI: OpId = OpId::new(EXT_I, 25);
pub const OP_SRAI: OpId = OpId::new(EXT_I, 26);
pub const OP_ADD: OpId = OpId::new(EXT_I, 27);
pub const OP_SUB: OpId = OpId::new(EXT_I, 28);
pub const OP_SLL: OpId = OpId::new(EXT_I, 29);
pub const OP_SLT: OpId = OpId::new(EXT_I, 30);
pub const OP_SLTU: OpId = OpId::new(EXT_I, 31);
pub const OP_XOR: OpId = OpId::new(EXT_I, 32);
pub const OP_SRL: OpId = OpId::new(EXT_I, 33);
pub const OP_SRA: OpId = OpId::new(EXT_I, 34);
pub const OP_OR: OpId = OpId::new(EXT_I, 35);
pub const OP_AND: OpId = OpId::new(EXT_I, 36);
pub const OP_FENCE: OpId = OpId::new(EXT_I, 37);
pub const OP_ECALL: OpId = OpId::new(EXT_I, 38);
pub const OP_EBREAK: OpId = OpId::new(EXT_I, 39);
// RV64I
pub const OP_LWU: OpId = OpId::new(EXT_I, 40);
pub const OP_LD: OpId = OpId::new(EXT_I, 41);
pub const OP_SD: OpId = OpId::new(EXT_I, 42);
pub const OP_ADDIW: OpId = OpId::new(EXT_I, 43);
pub const OP_SLLIW: OpId = OpId::new(EXT_I, 44);
pub const OP_SRLIW: OpId = OpId::new(EXT_I, 45);
pub const OP_SRAIW: OpId = OpId::new(EXT_I, 46);
pub const OP_ADDW: OpId = OpId::new(EXT_I, 47);
pub const OP_SUBW: OpId = OpId::new(EXT_I, 48);
pub const OP_SLLW: OpId = OpId::new(EXT_I, 49);
pub const OP_SRLW: OpId = OpId::new(EXT_I, 50);
pub const OP_SRAW: OpId = OpId::new(EXT_I, 51);
pub const OP_MRET: OpId = OpId::new(EXT_I, 52);

/// Get mnemonic for a base instruction.
#[must_use]
pub const fn base_mnemonic(opid: OpId) -> &'static str {
    match opid.idx {
        0 => "lui",
        1 => "auipc",
        2 => "jal",
        3 => "jalr",
        4 => "beq",
        5 => "bne",
        6 => "blt",
        7 => "bge",
        8 => "bltu",
        9 => "bgeu",
        10 => "lb",
        11 => "lh",
        12 => "lw",
        13 => "lbu",
        14 => "lhu",
        15 => "sb",
        16 => "sh",
        17 => "sw",
        18 => "addi",
        19 => "slti",
        20 => "sltiu",
        21 => "xori",
        22 => "ori",
        23 => "andi",
        24 => "slli",
        25 => "srli",
        26 => "srai",
        27 => "add",
        28 => "sub",
        29 => "sll",
        30 => "slt",
        31 => "sltu",
        32 => "xor",
        33 => "srl",
        34 => "sra",
        35 => "or",
        36 => "and",
        37 => "fence",
        38 => "ecall",
        39 => "ebreak",
        40 => "lwu",
        41 => "ld",
        42 => "sd",
        43 => "addiw",
        44 => "slliw",
        45 => "srliw",
        46 => "sraiw",
        47 => "addw",
        48 => "subw",
        49 => "sllw",
        50 => "srlw",
        51 => "sraw",
        52 => "mret",
        _ => "???",
    }
}

// ===== Extension Implementation =====

/// Base I extension (RV32I/RV64I).
pub struct BaseExtension;

impl<X: Xlen> InstructionExtension<X> for BaseExtension {
    fn name(&self) -> &'static str {
        "I"
    }

    fn ext_id(&self) -> u8 {
        EXT_I
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        decode_32bit(raw, pc)
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let (stmts, term) = lift_base(&instr.args, instr.opid, instr.pc, instr.size);
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            instr.raw,
            stmts,
            term,
        )
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        format_instr(base_mnemonic(instr.opid), &instr.args)
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_I.iter().find(|info| info.opid == opid).copied()
    }
}

/// Table-driven `OpInfo` for base I extension.
const OP_INFO_I: &[OpInfo] = &[
    OpInfo {
        opid: OP_LUI,
        name: "lui",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AUIPC,
        name: "auipc",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_JAL,
        name: "jal",
        class: OpClass::Jump,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_JALR,
        name: "jalr",
        class: OpClass::JumpIndirect,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_BEQ,
        name: "beq",
        class: OpClass::Branch,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_BNE,
        name: "bne",
        class: OpClass::Branch,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_BLT,
        name: "blt",
        class: OpClass::Branch,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_BGE,
        name: "bge",
        class: OpClass::Branch,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_BLTU,
        name: "bltu",
        class: OpClass::Branch,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_BGEU,
        name: "bgeu",
        class: OpClass::Branch,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_LB,
        name: "lb",
        class: OpClass::Load,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_LH,
        name: "lh",
        class: OpClass::Load,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_LW,
        name: "lw",
        class: OpClass::Load,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_LBU,
        name: "lbu",
        class: OpClass::Load,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_LHU,
        name: "lhu",
        class: OpClass::Load,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SB,
        name: "sb",
        class: OpClass::Store,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SH,
        name: "sh",
        class: OpClass::Store,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SW,
        name: "sw",
        class: OpClass::Store,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ADDI,
        name: "addi",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLTI,
        name: "slti",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLTIU,
        name: "sltiu",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_XORI,
        name: "xori",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ORI,
        name: "ori",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ANDI,
        name: "andi",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLLI,
        name: "slli",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRLI,
        name: "srli",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRAI,
        name: "srai",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ADD,
        name: "add",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SUB,
        name: "sub",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLL,
        name: "sll",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLT,
        name: "slt",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLTU,
        name: "sltu",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_XOR,
        name: "xor",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRL,
        name: "srl",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRA,
        name: "sra",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_OR,
        name: "or",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AND,
        name: "and",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_FENCE,
        name: "fence",
        class: OpClass::Fence,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ECALL,
        name: "ecall",
        class: OpClass::System,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_EBREAK,
        name: "ebreak",
        class: OpClass::System,
        size_hint: 4,
    },
    // RV64I
    OpInfo {
        opid: OP_LWU,
        name: "lwu",
        class: OpClass::Load,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_LD,
        name: "ld",
        class: OpClass::Load,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SD,
        name: "sd",
        class: OpClass::Store,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ADDIW,
        name: "addiw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLLIW,
        name: "slliw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRLIW,
        name: "srliw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRAIW,
        name: "sraiw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_ADDW,
        name: "addw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SUBW,
        name: "subw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SLLW,
        name: "sllw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRLW,
        name: "srlw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SRAW,
        name: "sraw",
        class: OpClass::Alu,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_MRET,
        name: "mret",
        class: OpClass::System,
        size_hint: 4,
    },
];

// ===== Decode =====
