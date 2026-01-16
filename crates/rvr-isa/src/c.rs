//! C extension (compressed instructions).

use crate::{OpId, EXT_C};

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
