//! Zicsr extension (CSR instructions) - decode, lift, disasm.

use rvr_ir::{Xlen, InstrIR, Expr, Stmt, Terminator};

use crate::{
    DecodedInstr, InstrArgs, OpId, OpInfo, OpClass, EXT_ZICSR, reg_name,
    encode::{decode_opcode, decode_funct3, decode_rd, decode_rs1},
};
use super::InstructionExtension;

// CSR instructions
pub const OP_CSRRW: OpId = OpId::new(EXT_ZICSR, 0);
pub const OP_CSRRS: OpId = OpId::new(EXT_ZICSR, 1);
pub const OP_CSRRC: OpId = OpId::new(EXT_ZICSR, 2);
pub const OP_CSRRWI: OpId = OpId::new(EXT_ZICSR, 3);
pub const OP_CSRRSI: OpId = OpId::new(EXT_ZICSR, 4);
pub const OP_CSRRCI: OpId = OpId::new(EXT_ZICSR, 5);

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

/// Zicsr extension (CSR instructions).
pub struct ZicsrExtension;

impl<X: Xlen> InstructionExtension<X> for ZicsrExtension {
    fn name(&self) -> &'static str {
        "Zicsr"
    }

    fn ext_id(&self) -> u8 {
        EXT_ZICSR
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = decode_opcode(raw);
        let funct3 = decode_funct3(raw);

        // CSR instructions: opcode=0x73, funct3 != 0
        if opcode != 0x73 || funct3 == 0 {
            return None;
        }

        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let csr = ((raw >> 20) & 0xFFF) as u16;

        let (opid, args) = match funct3 {
            1 => (OP_CSRRW, InstrArgs::Csr { rd, rs1, csr }),
            2 => (OP_CSRRS, InstrArgs::Csr { rd, rs1, csr }),
            3 => (OP_CSRRC, InstrArgs::Csr { rd, rs1, csr }),
            5 => (OP_CSRRWI, InstrArgs::CsrI { rd, imm: rs1, csr }),
            6 => (OP_CSRRSI, InstrArgs::CsrI { rd, imm: rs1, csr }),
            7 => (OP_CSRRCI, InstrArgs::CsrI { rd, imm: rs1, csr }),
            _ => return None,
        };
        Some(DecodedInstr::new(opid, pc, 4, args))
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let (stmts, term) = lift_zicsr(&instr.args, instr.opid);
        InstrIR::new(instr.pc, instr.size, stmts, term)
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        let mnemonic = zicsr_mnemonic(instr.opid);
        match &instr.args {
            InstrArgs::Csr { rd, rs1, csr } => {
                format!("{} {}, {}, {}", mnemonic, reg_name(*rd), csr_name(*csr), reg_name(*rs1))
            }
            InstrArgs::CsrI { rd, imm, csr } => {
                format!("{} {}, {}, {}", mnemonic, reg_name(*rd), csr_name(*csr), imm)
            }
            _ => format!("{} <?>", mnemonic),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_ZICSR.iter().find(|info| info.opid == opid).copied()
    }
}

/// Table-driven OpInfo for Zicsr extension.
const OP_INFO_ZICSR: &[OpInfo] = &[
    OpInfo { opid: OP_CSRRW, name: "csrrw", class: OpClass::Csr, size_hint: 4 },
    OpInfo { opid: OP_CSRRS, name: "csrrs", class: OpClass::Csr, size_hint: 4 },
    OpInfo { opid: OP_CSRRC, name: "csrrc", class: OpClass::Csr, size_hint: 4 },
    OpInfo { opid: OP_CSRRWI, name: "csrrwi", class: OpClass::Csr, size_hint: 4 },
    OpInfo { opid: OP_CSRRSI, name: "csrrsi", class: OpClass::Csr, size_hint: 4 },
    OpInfo { opid: OP_CSRRCI, name: "csrrci", class: OpClass::Csr, size_hint: 4 },
];

fn lift_zicsr<X: Xlen>(args: &InstrArgs, opid: crate::OpId) -> (Vec<Stmt<X>>, Terminator<X>) {
    match opid {
        OP_CSRRW => lift_csrrw(args),
        OP_CSRRS => lift_csrrs(args),
        OP_CSRRC => lift_csrrc(args),
        OP_CSRRWI => lift_csrrwi(args),
        OP_CSRRSI => lift_csrrsi(args),
        OP_CSRRCI => lift_csrrci(args),
        _ => (Vec::new(), Terminator::trap("unknown zicsr instruction")),
    }
}

fn lift_csrrw<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::Csr { rd, rs1, csr } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, Expr::csr(*csr)));
            }
            stmts.push(Stmt::write_csr(*csr, Expr::read(*rs1)));
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_csrrs<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::Csr { rd, rs1, csr } => {
            let mut stmts = Vec::new();
            let old_val = Expr::csr(*csr);
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, old_val.clone()));
            }
            if *rs1 != 0 {
                stmts.push(Stmt::write_csr(*csr, Expr::or(old_val, Expr::read(*rs1))));
            }
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_csrrc<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::Csr { rd, rs1, csr } => {
            let mut stmts = Vec::new();
            let old_val = Expr::csr(*csr);
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, old_val.clone()));
            }
            if *rs1 != 0 {
                stmts.push(Stmt::write_csr(*csr, Expr::and(old_val, Expr::not(Expr::read(*rs1)))));
            }
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_csrrwi<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::CsrI { rd, imm, csr } => {
            let mut stmts = Vec::new();
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, Expr::csr(*csr)));
            }
            stmts.push(Stmt::write_csr(*csr, Expr::imm(X::from_u64(*imm as u64))));
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_csrrsi<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::CsrI { rd, imm, csr } => {
            let mut stmts = Vec::new();
            let old_val = Expr::csr(*csr);
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, old_val.clone()));
            }
            if *imm != 0 {
                stmts.push(Stmt::write_csr(*csr, Expr::or(old_val, Expr::imm(X::from_u64(*imm as u64)))));
            }
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_csrrci<X: Xlen>(args: &InstrArgs) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::CsrI { rd, imm, csr } => {
            let mut stmts = Vec::new();
            let old_val = Expr::csr(*csr);
            if *rd != 0 {
                stmts.push(Stmt::write_reg(*rd, old_val.clone()));
            }
            if *imm != 0 {
                stmts.push(Stmt::write_csr(*csr, Expr::and(old_val, Expr::not(Expr::imm(X::from_u64(*imm as u64))))));
            }
            (stmts, Terminator::Fall { target: None })
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}
