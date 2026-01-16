//! Zicsr extension (CSR instructions) and Zifencei.

use crate::{OpId, EXT_ZICSR, EXT_ZIFENCEI};

// CSR instructions
pub const OP_CSRRW: OpId = OpId::new(EXT_ZICSR, 0);
pub const OP_CSRRS: OpId = OpId::new(EXT_ZICSR, 1);
pub const OP_CSRRC: OpId = OpId::new(EXT_ZICSR, 2);
pub const OP_CSRRWI: OpId = OpId::new(EXT_ZICSR, 3);
pub const OP_CSRRSI: OpId = OpId::new(EXT_ZICSR, 4);
pub const OP_CSRRCI: OpId = OpId::new(EXT_ZICSR, 5);

// Zifencei
pub const OP_FENCE_I: OpId = OpId::new(EXT_ZIFENCEI, 0);

// Common CSR addresses
pub const CSR_CYCLE: u16 = 0xC00;
pub const CSR_TIME: u16 = 0xC01;
pub const CSR_INSTRET: u16 = 0xC02;
pub const CSR_CYCLEH: u16 = 0xC80;
pub const CSR_TIMEH: u16 = 0xC81;
pub const CSR_INSTRETH: u16 = 0xC82;
pub const CSR_MISA: u16 = 0x301;
pub const CSR_MVENDORID: u16 = 0xF11;
pub const CSR_MARCHID: u16 = 0xF12;
pub const CSR_MIMPID: u16 = 0xF13;
pub const CSR_MHARTID: u16 = 0xF14;

/// Get the mnemonic for a Zicsr instruction.
pub fn zicsr_mnemonic(opid: OpId) -> &'static str {
    match opid.idx {
        0 => "csrrw",
        1 => "csrrs",
        2 => "csrrc",
        3 => "csrrwi",
        4 => "csrrsi",
        5 => "csrrci",
        _ => "???",
    }
}

/// Get CSR name from address.
pub fn csr_name(csr: u16) -> &'static str {
    match csr {
        0xC00 => "cycle",
        0xC01 => "time",
        0xC02 => "instret",
        0xC80 => "cycleh",
        0xC81 => "timeh",
        0xC82 => "instreth",
        0x301 => "misa",
        0xF11 => "mvendorid",
        0xF12 => "marchid",
        0xF13 => "mimpid",
        0xF14 => "mhartid",
        _ => "???",
    }
}
