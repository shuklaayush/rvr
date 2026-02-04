//! Zifencei extension (instruction fence) - decode, lift, disasm.

use rvr_ir::{InstrIR, Terminator, Xlen};

use super::InstructionExtension;
use crate::{DecodedInstr, EXT_ZIFENCEI, InstrArgs, OpClass, OpId, OpInfo, encode::decode_funct3};

/// Zifencei instruction
pub const OP_FENCE_I: OpId = OpId::new(EXT_ZIFENCEI, 0);

/// Zifencei extension (instruction fence).
pub struct ZifenceiExtension;

impl<X: Xlen> InstructionExtension<X> for ZifenceiExtension {
    fn name(&self) -> &'static str {
        "Zifencei"
    }

    fn ext_id(&self) -> u8 {
        EXT_ZIFENCEI
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = decode_funct3(raw);

        // FENCE.I: opcode=0x0F, funct3=1
        if opcode == 0x0F && funct3 == 1 {
            Some(DecodedInstr::new(OP_FENCE_I, pc, 4, raw, InstrArgs::None))
        } else {
            None
        }
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        // FENCE.I is a no-op in recompilation (instruction cache is always coherent)
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            instr.raw,
            Vec::new(),
            Terminator::Fall { target: None },
        )
    }

    fn disasm(&self, _instr: &DecodedInstr<X>) -> String {
        "fence.i".to_string()
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        OP_INFO_ZIFENCEI
            .iter()
            .find(|info| info.opid == opid)
            .copied()
    }
}

/// Table-driven `OpInfo` for Zifencei extension.
const OP_INFO_ZIFENCEI: &[OpInfo] = &[OpInfo {
    opid: OP_FENCE_I,
    name: "fence.i",
    class: OpClass::Fence,
    size_hint: 4,
}];
