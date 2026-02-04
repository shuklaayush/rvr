//! C extension (compressed instructions) - decode, lift, disasm.

mod decode;
mod disasm;
mod lift;

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use super::InstructionExtension;
use crate::{DecodedInstr, EXT_C, InstrArgs, OpClass, OpId, OpInfo, reg_name};
use decode::{decode_q0, decode_q1, decode_q2};
use disasm::format_c_instr;
use lift::lift_c;

// Quadrant 0
pub const OP_C_ADDI4SPN: OpId = OpId::new(EXT_C, 0);
pub const OP_C_LW: OpId = OpId::new(EXT_C, 1);
pub const OP_C_SW: OpId = OpId::new(EXT_C, 2);
pub const OP_C_LD: OpId = OpId::new(EXT_C, 3); // RV64C
pub const OP_C_SD: OpId = OpId::new(EXT_C, 4); // RV64C

// Quadrant 1
pub const OP_C_NOP: OpId = OpId::new(EXT_C, 5);
pub const OP_C_ADDI: OpId = OpId::new(EXT_C, 6);
pub const OP_C_JAL: OpId = OpId::new(EXT_C, 7); // RV32C only
pub const OP_C_ADDIW: OpId = OpId::new(EXT_C, 8); // RV64C
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
pub const OP_C_SUBW: OpId = OpId::new(EXT_C, 19); // RV64C
pub const OP_C_ADDW: OpId = OpId::new(EXT_C, 20); // RV64C
pub const OP_C_J: OpId = OpId::new(EXT_C, 21);
pub const OP_C_BEQZ: OpId = OpId::new(EXT_C, 22);
pub const OP_C_BNEZ: OpId = OpId::new(EXT_C, 23);

// Quadrant 2
pub const OP_C_SLLI: OpId = OpId::new(EXT_C, 24);
pub const OP_C_LWSP: OpId = OpId::new(EXT_C, 25);
pub const OP_C_LDSP: OpId = OpId::new(EXT_C, 26); // RV64C
pub const OP_C_JR: OpId = OpId::new(EXT_C, 27);
pub const OP_C_MV: OpId = OpId::new(EXT_C, 28);
pub const OP_C_EBREAK: OpId = OpId::new(EXT_C, 29);
pub const OP_C_JALR: OpId = OpId::new(EXT_C, 30);
pub const OP_C_ADD: OpId = OpId::new(EXT_C, 31);
pub const OP_C_SWSP: OpId = OpId::new(EXT_C, 32);
pub const OP_C_SDSP: OpId = OpId::new(EXT_C, 33); // RV64C

// Invalid encoding
pub const OP_C_INVALID: OpId = OpId::new(EXT_C, 34);

/// Get the mnemonic for a C extension instruction.
#[must_use]
pub const fn c_mnemonic(opid: OpId) -> &'static str {
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
        34 => "c.invalid",
        _ => "???",
    }
}

/// C extension (compressed instructions).
pub struct CExtension;

impl<X: Xlen> InstructionExtension<X> for CExtension {
    fn name(&self) -> &'static str {
        "C"
    }

    fn ext_id(&self) -> u8 {
        EXT_C
    }

    fn decode16(&self, raw: u16, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let quadrant = raw & 0x3;
        let funct3 = ((raw >> 13) & 0x7) as u8;

        let result = match quadrant {
            0b00 => decode_q0::<X>(raw, funct3),
            0b01 => decode_q1::<X>(raw, funct3),
            0b10 => decode_q2::<X>(raw, funct3),
            _ => None,
        };

        // Return invalid instruction instead of None for illegal encodings
        let (opid, args) = result.unwrap_or_else(|| {
            (
                OP_C_INVALID,
                InstrArgs::I {
                    rd: 0,
                    rs1: 0,
                    imm: i32::from(raw),
                },
            )
        });

        Some(DecodedInstr::new(opid, pc, 2, u32::from(raw), args))
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let (stmts, term) = lift_c(&instr.args, instr.opid, instr.pc, instr.size);
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
        let mnemonic = c_mnemonic(instr.opid);
        format_c_instr(mnemonic, &instr.args, instr.opid)
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_C.iter().find(|info| info.opid == opid).copied()
    }
}

/// Table-driven `OpInfo` for C extension.
const OP_INFO_C: &[OpInfo] = &[
    // Quadrant 0
    OpInfo {
        opid: OP_C_ADDI4SPN,
        name: "c.addi4spn",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_LW,
        name: "c.lw",
        class: OpClass::Load,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SW,
        name: "c.sw",
        class: OpClass::Store,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_LD,
        name: "c.ld",
        class: OpClass::Load,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SD,
        name: "c.sd",
        class: OpClass::Store,
        size_hint: 2,
    },
    // Quadrant 1
    OpInfo {
        opid: OP_C_NOP,
        name: "c.nop",
        class: OpClass::Nop,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_ADDI,
        name: "c.addi",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_JAL,
        name: "c.jal",
        class: OpClass::Jump,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_ADDIW,
        name: "c.addiw",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_LI,
        name: "c.li",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_ADDI16SP,
        name: "c.addi16sp",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_LUI,
        name: "c.lui",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SRLI,
        name: "c.srli",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SRAI,
        name: "c.srai",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_ANDI,
        name: "c.andi",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SUB,
        name: "c.sub",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_XOR,
        name: "c.xor",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_OR,
        name: "c.or",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_AND,
        name: "c.and",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SUBW,
        name: "c.subw",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_ADDW,
        name: "c.addw",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_J,
        name: "c.j",
        class: OpClass::Jump,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_BEQZ,
        name: "c.beqz",
        class: OpClass::Branch,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_BNEZ,
        name: "c.bnez",
        class: OpClass::Branch,
        size_hint: 2,
    },
    // Quadrant 2
    OpInfo {
        opid: OP_C_SLLI,
        name: "c.slli",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_LWSP,
        name: "c.lwsp",
        class: OpClass::Load,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_LDSP,
        name: "c.ldsp",
        class: OpClass::Load,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_JR,
        name: "c.jr",
        class: OpClass::JumpIndirect,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_MV,
        name: "c.mv",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_EBREAK,
        name: "c.ebreak",
        class: OpClass::System,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_JALR,
        name: "c.jalr",
        class: OpClass::JumpIndirect,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_ADD,
        name: "c.add",
        class: OpClass::Alu,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SWSP,
        name: "c.swsp",
        class: OpClass::Store,
        size_hint: 2,
    },
    OpInfo {
        opid: OP_C_SDSP,
        name: "c.sdsp",
        class: OpClass::Store,
        size_hint: 2,
    },
];
