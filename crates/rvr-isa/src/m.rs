//! M extension (multiply/divide).

use crate::{OpId, EXT_M};

pub const OP_MUL: OpId = OpId::new(EXT_M, 0);
pub const OP_MULH: OpId = OpId::new(EXT_M, 1);
pub const OP_MULHSU: OpId = OpId::new(EXT_M, 2);
pub const OP_MULHU: OpId = OpId::new(EXT_M, 3);
pub const OP_DIV: OpId = OpId::new(EXT_M, 4);
pub const OP_DIVU: OpId = OpId::new(EXT_M, 5);
pub const OP_REM: OpId = OpId::new(EXT_M, 6);
pub const OP_REMU: OpId = OpId::new(EXT_M, 7);

// RV64M W variants
pub const OP_MULW: OpId = OpId::new(EXT_M, 8);
pub const OP_DIVW: OpId = OpId::new(EXT_M, 9);
pub const OP_DIVUW: OpId = OpId::new(EXT_M, 10);
pub const OP_REMW: OpId = OpId::new(EXT_M, 11);
pub const OP_REMUW: OpId = OpId::new(EXT_M, 12);

/// Get the mnemonic for an M extension instruction.
pub fn m_mnemonic(opid: OpId) -> &'static str {
    match opid.idx {
        0 => "mul",
        1 => "mulh",
        2 => "mulhsu",
        3 => "mulhu",
        4 => "div",
        5 => "divu",
        6 => "rem",
        7 => "remu",
        8 => "mulw",
        9 => "divw",
        10 => "divuw",
        11 => "remw",
        12 => "remuw",
        _ => "???",
    }
}
