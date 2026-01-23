//! A extension (atomics) - decode, lift, disasm.

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use super::InstructionExtension;
use crate::{
    DecodedInstr, EXT_A, InstrArgs, OpClass, OpId, OpInfo,
    encode::{decode_funct3, decode_opcode, decode_rd, decode_rs1, decode_rs2},
    reg_name,
};

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

/// A extension (atomics).
pub struct AExtension;

impl<X: Xlen> InstructionExtension<X> for AExtension {
    fn name(&self) -> &'static str {
        "A"
    }

    fn ext_id(&self) -> u8 {
        EXT_A
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = decode_opcode(raw);
        if opcode != 0x2F {
            return None;
        }

        let funct3 = decode_funct3(raw);
        let rd = decode_rd(raw);
        let rs1 = decode_rs1(raw);
        let rs2 = decode_rs2(raw);
        let aq = ((raw >> 26) & 1) != 0;
        let rl = ((raw >> 25) & 1) != 0;
        let funct5 = (raw >> 27) & 0x1F;

        if funct3 != 2 && !(funct3 == 3 && X::VALUE == 64) {
            return None;
        }

        let is_64 = funct3 == 3;
        let opid = match funct5 {
            0x02 => {
                if is_64 {
                    OP_LR_D
                } else {
                    OP_LR_W
                }
            }
            0x03 => {
                if is_64 {
                    OP_SC_D
                } else {
                    OP_SC_W
                }
            }
            0x01 => {
                if is_64 {
                    OP_AMOSWAP_D
                } else {
                    OP_AMOSWAP_W
                }
            }
            0x00 => {
                if is_64 {
                    OP_AMOADD_D
                } else {
                    OP_AMOADD_W
                }
            }
            0x04 => {
                if is_64 {
                    OP_AMOXOR_D
                } else {
                    OP_AMOXOR_W
                }
            }
            0x0C => {
                if is_64 {
                    OP_AMOAND_D
                } else {
                    OP_AMOAND_W
                }
            }
            0x08 => {
                if is_64 {
                    OP_AMOOR_D
                } else {
                    OP_AMOOR_W
                }
            }
            0x10 => {
                if is_64 {
                    OP_AMOMIN_D
                } else {
                    OP_AMOMIN_W
                }
            }
            0x14 => {
                if is_64 {
                    OP_AMOMAX_D
                } else {
                    OP_AMOMAX_W
                }
            }
            0x18 => {
                if is_64 {
                    OP_AMOMINU_D
                } else {
                    OP_AMOMINU_W
                }
            }
            0x1C => {
                if is_64 {
                    OP_AMOMAXU_D
                } else {
                    OP_AMOMAXU_W
                }
            }
            _ => return None,
        };

        Some(DecodedInstr::new(
            opid,
            pc,
            4,
            InstrArgs::Amo {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            },
        ))
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let (stmts, term) = lift_a(&instr.args, instr.opid);
        InstrIR::new(instr.pc, instr.size, instr.opid.pack(), stmts, term)
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        let mnemonic = a_mnemonic(instr.opid);
        match &instr.args {
            InstrArgs::Amo {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => {
                let suffix = match (aq, rl) {
                    (true, true) => ".aqrl",
                    (true, false) => ".aq",
                    (false, true) => ".rl",
                    (false, false) => "",
                };
                format!(
                    "{}{} {}, {}, ({})",
                    mnemonic,
                    suffix,
                    reg_name(*rd),
                    reg_name(*rs2),
                    reg_name(*rs1)
                )
            }
            _ => format!("{} <?>", mnemonic),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_A.iter().find(|info| info.opid == opid).copied()
    }
}

/// Table-driven OpInfo for A extension.
const OP_INFO_A: &[OpInfo] = &[
    // .W variants (32-bit)
    OpInfo {
        opid: OP_LR_W,
        name: "lr.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SC_W,
        name: "sc.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOSWAP_W,
        name: "amoswap.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOADD_W,
        name: "amoadd.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOXOR_W,
        name: "amoxor.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOAND_W,
        name: "amoand.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOOR_W,
        name: "amoor.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMIN_W,
        name: "amomin.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMAX_W,
        name: "amomax.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMINU_W,
        name: "amominu.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMAXU_W,
        name: "amomaxu.w",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    // .D variants (64-bit)
    OpInfo {
        opid: OP_LR_D,
        name: "lr.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_SC_D,
        name: "sc.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOSWAP_D,
        name: "amoswap.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOADD_D,
        name: "amoadd.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOXOR_D,
        name: "amoxor.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOAND_D,
        name: "amoand.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOOR_D,
        name: "amoor.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMIN_D,
        name: "amomin.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMAX_D,
        name: "amomax.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMINU_D,
        name: "amominu.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
    OpInfo {
        opid: OP_AMOMAXU_D,
        name: "amomaxu.d",
        class: OpClass::Atomic,
        size_hint: 4,
    },
];

fn lift_a<X: Xlen>(args: &InstrArgs, opid: crate::OpId) -> (Vec<Stmt<X>>, Terminator<X>) {
    match args {
        InstrArgs::Amo { rd, rs1, rs2, .. } => {
            let rd = *rd;
            let rs1 = *rs1;
            let rs2 = *rs2;

            let is_64 = matches!(
                opid,
                OP_LR_D
                    | OP_SC_D
                    | OP_AMOSWAP_D
                    | OP_AMOADD_D
                    | OP_AMOXOR_D
                    | OP_AMOAND_D
                    | OP_AMOOR_D
                    | OP_AMOMIN_D
                    | OP_AMOMAX_D
                    | OP_AMOMINU_D
                    | OP_AMOMAXU_D
            );
            let width: u8 = if is_64 { 8 } else { 4 };

            if opid == OP_LR_W || opid == OP_LR_D {
                // LR: Set reservation and load value
                // LR.W sign-extends, LR.D doesn't need sign extension
                let mem_read = if is_64 {
                    Expr::mem_u(Expr::read(rs1), width)
                } else {
                    Expr::mem_s(Expr::read(rs1), width)
                };
                let stmts = vec![
                    Stmt::write_res_addr(Expr::read(rs1)),
                    Stmt::write_res_valid(Expr::imm(X::from_u64(1))),
                    Stmt::write_reg(rd, mem_read),
                ];
                return (stmts, Terminator::Fall { target: None });
            }

            if opid == OP_SC_W || opid == OP_SC_D {
                // SC: Conditional store based on reservation
                // cond = res_valid && (res_addr == rs1)
                let cond = Expr::and(
                    Expr::ne(Expr::res_valid(), Expr::imm(X::from_u64(0))),
                    Expr::eq(Expr::res_addr(), Expr::read(rs1)),
                );
                // If valid: store, write 0 to rd, clear reservation
                let then_stmts = vec![
                    Stmt::write_mem(Expr::read(rs1), Expr::read(rs2), width),
                    Stmt::write_reg(rd, Expr::imm(X::from_u64(0))),
                    Stmt::write_res_valid(Expr::imm(X::from_u64(0))),
                ];
                // Else: write 1 to rd (failure)
                let else_stmts = vec![Stmt::write_reg(rd, Expr::imm(X::from_u64(1)))];
                let stmts = vec![Stmt::if_then_else(cond, then_stmts, else_stmts)];
                return (stmts, Terminator::Fall { target: None });
            }

            // .W operations sign-extend, .D operations don't
            let signed = !is_64;
            match opid {
                OP_AMOSWAP_W | OP_AMOSWAP_D => lift_amo(rd, rs1, rs2, width, signed, |_, b| b),
                OP_AMOADD_W | OP_AMOADD_D => lift_amo(rd, rs1, rs2, width, signed, Expr::add),
                OP_AMOXOR_W | OP_AMOXOR_D => lift_amo(rd, rs1, rs2, width, signed, Expr::xor),
                OP_AMOAND_W | OP_AMOAND_D => lift_amo(rd, rs1, rs2, width, signed, Expr::and),
                OP_AMOOR_W | OP_AMOOR_D => lift_amo(rd, rs1, rs2, width, signed, Expr::or),
                // For .W min/max: use 32-bit comparison functions
                OP_AMOMIN_W => lift_amo(rd, rs1, rs2, width, signed, Expr::min32),
                OP_AMOMAX_W => lift_amo(rd, rs1, rs2, width, signed, Expr::max32),
                OP_AMOMINU_W => lift_amo(rd, rs1, rs2, width, signed, Expr::minu32),
                OP_AMOMAXU_W => lift_amo(rd, rs1, rs2, width, signed, Expr::maxu32),
                // For .D min/max: use full 64-bit comparison
                OP_AMOMIN_D => lift_amo(rd, rs1, rs2, width, signed, Expr::min),
                OP_AMOMAX_D => lift_amo(rd, rs1, rs2, width, signed, Expr::max),
                OP_AMOMINU_D => lift_amo(rd, rs1, rs2, width, signed, Expr::minu),
                OP_AMOMAXU_D => lift_amo(rd, rs1, rs2, width, signed, Expr::maxu),
                _ => (Vec::new(), Terminator::trap("unknown A instruction")),
            }
        }
        _ => (Vec::new(), Terminator::trap("invalid args")),
    }
}

fn lift_amo<X: Xlen, F>(
    rd: u8,
    rs1: u8,
    rs2: u8,
    width: u8,
    signed: bool,
    op: F,
) -> (Vec<Stmt<X>>, Terminator<X>)
where
    F: FnOnce(Expr<X>, Expr<X>) -> Expr<X>,
{
    let mut stmts = Vec::new();

    // Preserve address when rd == rs1 so the AMO store uses the original pointer.
    let addr = if rd == rs1 {
        stmts.push(Stmt::write_temp(1, Expr::read(rs1)));
        Expr::temp(1)
    } else {
        Expr::read(rs1)
    };

    // Preserve rs2 when rd == rs2 so we use the original value, not the overwritten one.
    let src = if rd == rs2 {
        stmts.push(Stmt::write_temp(3, Expr::read(rs2)));
        Expr::temp(3)
    } else {
        Expr::read(rs2)
    };

    // Read old value from memory ONCE and save to temp 2.
    // .W operations sign-extend the loaded value, .D operations don't need extension.
    let mem_read = if signed {
        Expr::mem_s(addr.clone(), width)
    } else {
        Expr::mem_u(addr.clone(), width)
    };
    stmts.push(Stmt::write_temp(2, mem_read));
    let old = Expr::temp(2);

    // Compute new value using the saved old value and preserved rs2.
    let new = op(old.clone(), src);
    stmts.push(Stmt::write_reg(rd, old));
    stmts.push(Stmt::write_mem(addr, new, width));

    // Clear reservation on any AMO (per RISC-V spec).
    stmts.push(Stmt::write_res_valid(Expr::imm(X::from_u64(0))));
    (stmts, Terminator::Fall { target: None })
}
