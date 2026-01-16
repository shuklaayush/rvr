//! RISC-V instruction set extensions.
//!
//! Each extension provides decode, lift, and disasm in a single file.
//! All instruction sets (including base I, M, A, C, Zicsr) are implemented
//! as extensions - there is no special "built-in" handling.

mod base;
mod m;
mod a;
mod c;
mod zicsr;

// Re-export extension structs
pub use base::BaseExtension;
pub use m::MExtension;
pub use a::AExtension;
pub use c::CExtension;
pub use zicsr::ZicsrExtension;

// Re-export OpId constants and mnemonic functions from each extension
pub use base::{
    OP_LUI, OP_AUIPC, OP_JAL, OP_JALR,
    OP_BEQ, OP_BNE, OP_BLT, OP_BGE, OP_BLTU, OP_BGEU,
    OP_LB, OP_LH, OP_LW, OP_LBU, OP_LHU,
    OP_SB, OP_SH, OP_SW,
    OP_ADDI, OP_SLTI, OP_SLTIU, OP_XORI, OP_ORI, OP_ANDI,
    OP_SLLI, OP_SRLI, OP_SRAI,
    OP_ADD, OP_SUB, OP_SLL, OP_SLT, OP_SLTU,
    OP_XOR, OP_SRL, OP_SRA, OP_OR, OP_AND,
    OP_FENCE, OP_ECALL, OP_EBREAK,
    OP_LWU, OP_LD, OP_SD,
    OP_ADDIW, OP_SLLIW, OP_SRLIW, OP_SRAIW,
    OP_ADDW, OP_SUBW, OP_SLLW, OP_SRLW, OP_SRAW,
    base_mnemonic,
};
pub use m::{
    OP_MUL, OP_MULH, OP_MULHSU, OP_MULHU, OP_DIV, OP_DIVU, OP_REM, OP_REMU,
    OP_MULW, OP_DIVW, OP_DIVUW, OP_REMW, OP_REMUW,
    m_mnemonic,
};
pub use a::{
    OP_LR_W, OP_SC_W, OP_AMOSWAP_W, OP_AMOADD_W, OP_AMOXOR_W, OP_AMOAND_W, OP_AMOOR_W,
    OP_AMOMIN_W, OP_AMOMAX_W, OP_AMOMINU_W, OP_AMOMAXU_W,
    OP_LR_D, OP_SC_D, OP_AMOSWAP_D, OP_AMOADD_D, OP_AMOXOR_D, OP_AMOAND_D, OP_AMOOR_D,
    OP_AMOMIN_D, OP_AMOMAX_D, OP_AMOMINU_D, OP_AMOMAXU_D,
    a_mnemonic,
};
pub use c::{
    OP_C_ADDI4SPN, OP_C_LW, OP_C_SW, OP_C_LD, OP_C_SD,
    OP_C_NOP, OP_C_ADDI, OP_C_JAL, OP_C_ADDIW, OP_C_LI, OP_C_ADDI16SP, OP_C_LUI,
    OP_C_SRLI, OP_C_SRAI, OP_C_ANDI, OP_C_SUB, OP_C_XOR, OP_C_OR, OP_C_AND,
    OP_C_SUBW, OP_C_ADDW, OP_C_J, OP_C_BEQZ, OP_C_BNEZ,
    OP_C_SLLI, OP_C_LWSP, OP_C_LDSP, OP_C_JR, OP_C_MV, OP_C_EBREAK, OP_C_JALR,
    OP_C_ADD, OP_C_SWSP, OP_C_SDSP,
    c_mnemonic,
};
pub use zicsr::{
    OP_CSRRW, OP_CSRRS, OP_CSRRC, OP_CSRRWI, OP_CSRRSI, OP_CSRRCI,
    OP_FENCE_I,
    CSR_CYCLE, CSR_TIME, CSR_INSTRET, CSR_CYCLEH, CSR_TIMEH, CSR_INSTRETH,
    CSR_MISA, CSR_MVENDORID, CSR_MARCHID, CSR_MIMPID, CSR_MHARTID,
    zicsr_mnemonic, csr_name,
};

use rvr_ir::{InstrIR, Xlen, Terminator};
use crate::DecodedInstr;

/// Extension point for instruction decoding and lifting.
///
/// Each extension implements decode, lift, and disasm for its instructions.
pub trait InstructionExtension<X: Xlen>: Send + Sync {
    /// Extension IDs this handles (EXT_I, EXT_M, etc.).
    /// Return empty slice to try all instructions.
    fn handled_extensions(&self) -> &[u8] {
        &[]
    }

    /// Try to decode bytes at pc. Return None to fall through to next decoder.
    fn decode(&self, bytes: &[u8], pc: X::Reg) -> Option<DecodedInstr<X>>;

    /// Lift instruction to IR.
    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X>;

    /// Disassembly string for debugging.
    fn disasm(&self, instr: &DecodedInstr<X>) -> String;
}

/// Composite decoder that chains multiple extensions.
///
/// Tries extensions in order until one handles the instruction.
pub struct CompositeDecoder<X: Xlen> {
    extensions: Vec<Box<dyn InstructionExtension<X>>>,
}

impl<X: Xlen> CompositeDecoder<X> {
    /// Create a new composite decoder with the given extensions.
    pub fn new(extensions: Vec<Box<dyn InstructionExtension<X>>>) -> Self {
        Self { extensions }
    }

    /// Create a composite decoder with all standard RISC-V extensions.
    pub fn standard() -> Self {
        Self::new(vec![
            Box::new(CExtension),      // C first (handles 16-bit instructions)
            Box::new(BaseExtension),   // Base I
            Box::new(MExtension),      // M extension
            Box::new(AExtension),      // A extension
            Box::new(ZicsrExtension),  // Zicsr/Zifencei
        ])
    }

    /// Create an empty composite decoder (no extensions).
    pub fn empty() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    /// Add an extension to the decoder chain.
    pub fn with_extension(mut self, ext: impl InstructionExtension<X> + 'static) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Decode an instruction using registered extensions.
    pub fn decode(&self, bytes: &[u8], pc: X::Reg) -> Option<DecodedInstr<X>> {
        for ext in &self.extensions {
            if let Some(instr) = ext.decode(bytes, pc) {
                return Some(instr);
            }
        }
        None
    }

    /// Lift an instruction using the appropriate extension.
    pub fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        for ext in &self.extensions {
            let handled = ext.handled_extensions();
            if handled.is_empty() || handled.contains(&instr.opid.ext) {
                return ext.lift(instr);
            }
        }
        // No extension handles this - return trap
        InstrIR::new(instr.pc, instr.size, Vec::new(), Terminator::trap("unhandled extension"))
    }

    /// Disassemble an instruction.
    pub fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        for ext in &self.extensions {
            let handled = ext.handled_extensions();
            if handled.is_empty() || handled.contains(&instr.opid.ext) {
                return ext.disasm(instr);
            }
        }
        format!("??? (ext={})", instr.opid.ext)
    }
}

impl<X: Xlen> Default for CompositeDecoder<X> {
    fn default() -> Self {
        Self::standard()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;
    use crate::OP_ADDI;

    #[test]
    fn test_composite_decoder_default() {
        let decoder = CompositeDecoder::<Rv64>::default();
        // ADDI x1, x0, 42
        let bytes = [0x93, 0x00, 0xa0, 0x02];
        let instr = decoder.decode(&bytes, 0u64).unwrap();
        assert_eq!(instr.opid, OP_ADDI);
    }

    #[test]
    fn test_disasm() {
        let decoder = CompositeDecoder::<Rv64>::default();
        let bytes = [0x93, 0x00, 0xa0, 0x02]; // addi x1, x0, 42
        let instr = decoder.decode(&bytes, 0u64).unwrap();
        let disasm = decoder.disasm(&instr);
        assert!(disasm.contains("addi"));
    }
}
