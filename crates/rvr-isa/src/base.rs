//! RV32I/RV64I base instruction set.

use crate::{OpId, EXT_I};

// Base I extension OpId constants
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

// RV64I additions
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

/// Get the mnemonic for a base instruction.
pub fn base_mnemonic(opid: OpId) -> &'static str {
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
        _ => "???",
    }
}
