//! A extension (atomics).

use crate::{OpId, EXT_A};

// .W variants (32-bit)
pub const OP_LR_W: OpId = OpId::new(EXT_A, 0);
pub const OP_SC_W: OpId = OpId::new(EXT_A, 1);
pub const OP_AMOSWAP_W: OpId = OpId::new(EXT_A, 2);
pub const OP_AMOADD_W: OpId = OpId::new(EXT_A, 3);
pub const OP_AMOXOR_W: OpId = OpId::new(EXT_A, 4);
pub const OP_AMOAND_W: OpId = OpId::new(EXT_A, 5);
pub const OP_AMOOR_W: OpId = OpId::new(EXT_A, 6);
pub const OP_AMOMIN_W: OpId = OpId::new(EXT_A, 7);
pub const OP_AMOMAX_W: OpId = OpId::new(EXT_A, 8);
pub const OP_AMOMINU_W: OpId = OpId::new(EXT_A, 9);
pub const OP_AMOMAXU_W: OpId = OpId::new(EXT_A, 10);

// .D variants (64-bit, RV64 only)
pub const OP_LR_D: OpId = OpId::new(EXT_A, 11);
pub const OP_SC_D: OpId = OpId::new(EXT_A, 12);
pub const OP_AMOSWAP_D: OpId = OpId::new(EXT_A, 13);
pub const OP_AMOADD_D: OpId = OpId::new(EXT_A, 14);
pub const OP_AMOXOR_D: OpId = OpId::new(EXT_A, 15);
pub const OP_AMOAND_D: OpId = OpId::new(EXT_A, 16);
pub const OP_AMOOR_D: OpId = OpId::new(EXT_A, 17);
pub const OP_AMOMIN_D: OpId = OpId::new(EXT_A, 18);
pub const OP_AMOMAX_D: OpId = OpId::new(EXT_A, 19);
pub const OP_AMOMINU_D: OpId = OpId::new(EXT_A, 20);
pub const OP_AMOMAXU_D: OpId = OpId::new(EXT_A, 21);

/// Get the mnemonic for an A extension instruction.
pub fn a_mnemonic(opid: OpId) -> &'static str {
    match opid.idx {
        0 => "lr.w",
        1 => "sc.w",
        2 => "amoswap.w",
        3 => "amoadd.w",
        4 => "amoxor.w",
        5 => "amoand.w",
        6 => "amoor.w",
        7 => "amomin.w",
        8 => "amomax.w",
        9 => "amominu.w",
        10 => "amomaxu.w",
        11 => "lr.d",
        12 => "sc.d",
        13 => "amoswap.d",
        14 => "amoadd.d",
        15 => "amoxor.d",
        16 => "amoand.d",
        17 => "amoor.d",
        18 => "amomin.d",
        19 => "amomax.d",
        20 => "amominu.d",
        21 => "amomaxu.d",
        _ => "???",
    }
}
