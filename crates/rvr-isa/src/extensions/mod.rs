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
mod zifencei;

// Re-export extension structs
pub use base::BaseExtension;
pub use m::MExtension;
pub use a::AExtension;
pub use c::CExtension;
pub use zicsr::ZicsrExtension;
pub use zifencei::ZifenceiExtension;

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
    CSR_CYCLE, CSR_TIME, CSR_INSTRET, CSR_CYCLEH, CSR_TIMEH, CSR_INSTRETH,
    CSR_MISA, CSR_MVENDORID, CSR_MARCHID, CSR_MIMPID, CSR_MHARTID,
    zicsr_mnemonic, csr_name,
};
pub use zifencei::OP_FENCE_I;

use rvr_ir::{InstrIR, Xlen, Terminator};
use crate::{DecodedInstr, OpId, OpInfo};

/// Extension point for instruction decoding and lifting.
///
/// Each extension implements decode, lift, and disasm for its instructions.
/// Extensions are composable and can be registered with an `ExtensionRegistry`.
pub trait InstructionExtension<X: Xlen>: Send + Sync {
    /// Human-readable extension name (e.g., "I", "M", "C").
    fn name(&self) -> &'static str;

    /// Extension ID constant (EXT_I, EXT_M, etc.).
    fn ext_id(&self) -> u8;

    /// Try to decode a 16-bit (compressed) instruction.
    /// Return None to fall through to next decoder or if not a compressed instruction.
    fn decode16(&self, _raw: u16, _pc: X::Reg) -> Option<DecodedInstr<X>> {
        None
    }

    /// Try to decode a 32-bit instruction.
    /// Return None to fall through to next decoder.
    fn decode32(&self, _raw: u32, _pc: X::Reg) -> Option<DecodedInstr<X>> {
        None
    }

    /// Try to decode bytes at pc. Default implementation dispatches to decode16/decode32.
    fn decode(&self, bytes: &[u8], pc: X::Reg) -> Option<DecodedInstr<X>> {
        if bytes.len() < 2 {
            return None;
        }
        let low = u16::from_le_bytes([bytes[0], bytes[1]]);

        // Check for compressed instruction (bits 0-1 != 0b11)
        if (low & 0x3) != 0x3 {
            self.decode16(low, pc)
        } else if bytes.len() >= 4 {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            self.decode32(raw, pc)
        } else {
            None
        }
    }

    /// Lift instruction to IR.
    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X>;

    /// Disassembly string for debugging.
    fn disasm(&self, instr: &DecodedInstr<X>) -> String;

    /// Get metadata for an instruction. Returns None for unknown OpIds.
    fn op_info(&self, _opid: OpId) -> Option<OpInfo> {
        None
    }
}

/// Registry for RISC-V instruction set extensions.
///
/// Chains multiple extensions and dispatches decode/lift/disasm to the appropriate one.
/// Extensions are tried in order; C extension should be first to handle compressed instructions.
pub struct ExtensionRegistry<X: Xlen> {
    extensions: Vec<Box<dyn InstructionExtension<X>>>,
}

impl<X: Xlen> ExtensionRegistry<X> {
    /// Create a new registry with the given extensions.
    pub fn new(extensions: Vec<Box<dyn InstructionExtension<X>>>) -> Self {
        Self { extensions }
    }

    /// Create a registry with all standard RISC-V extensions.
    /// Order: C (compressed first), I (base), M (multiply), A (atomic), Zicsr, Zifencei.
    pub fn standard() -> Self {
        Self::new(vec![
            Box::new(CExtension),        // C first (handles 16-bit instructions)
            Box::new(BaseExtension),     // Base I
            Box::new(MExtension),        // M extension
            Box::new(AExtension),        // A extension
            Box::new(ZicsrExtension),    // Zicsr (CSR instructions)
            Box::new(ZifenceiExtension), // Zifencei (instruction fence)
        ])
    }

    /// Create an empty registry (no extensions).
    pub fn empty() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    /// Add an extension to the registry chain.
    pub fn with_extension(mut self, ext: impl InstructionExtension<X> + 'static) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Get all registered extensions.
    pub fn extensions(&self) -> &[Box<dyn InstructionExtension<X>>] {
        &self.extensions
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
            if ext.ext_id() == instr.opid.ext {
                return ext.lift(instr);
            }
        }
        // No extension handles this - return trap
        InstrIR::new(instr.pc, instr.size, Vec::new(), Terminator::trap("unhandled extension"))
    }

    /// Disassemble an instruction.
    pub fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        for ext in &self.extensions {
            if ext.ext_id() == instr.opid.ext {
                return ext.disasm(instr);
            }
        }
        format!("??? (ext={})", instr.opid.ext)
    }

    /// Get metadata for an instruction by OpId.
    ///
    /// Tries all extensions since some extensions (like Zicsr) handle multiple ext_ids.
    pub fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        for ext in &self.extensions {
            if let Some(info) = ext.op_info(opid) {
                return Some(info);
            }
        }
        None
    }
}

impl<X: Xlen> Default for ExtensionRegistry<X> {
    fn default() -> Self {
        Self::standard()
    }
}

/// Type alias for backward compatibility.
pub type CompositeDecoder<X> = ExtensionRegistry<X>;

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;
    use crate::{OP_ADDI, EXT_I, EXT_C};

    #[test]
    fn test_extension_registry_default() {
        let registry = ExtensionRegistry::<Rv64>::default();
        // ADDI x1, x0, 42
        let bytes = [0x93, 0x00, 0xa0, 0x02];
        let instr = registry.decode(&bytes, 0u64).unwrap();
        assert_eq!(instr.opid, OP_ADDI);
    }

    #[test]
    fn test_backward_compat_alias() {
        // CompositeDecoder is now an alias for ExtensionRegistry
        let decoder = CompositeDecoder::<Rv64>::default();
        let bytes = [0x93, 0x00, 0xa0, 0x02];
        let instr = decoder.decode(&bytes, 0u64).unwrap();
        assert_eq!(instr.opid, OP_ADDI);
    }

    #[test]
    fn test_disasm() {
        let registry = ExtensionRegistry::<Rv64>::default();
        let bytes = [0x93, 0x00, 0xa0, 0x02]; // addi x1, x0, 42
        let instr = registry.decode(&bytes, 0u64).unwrap();
        let disasm = registry.disasm(&instr);
        assert!(disasm.contains("addi"));
    }

    #[test]
    fn test_decode16_compressed() {
        let registry = ExtensionRegistry::<Rv64>::default();
        // c.addi x1, 1 (encoded as 0x0085)
        let bytes = [0x85, 0x00];
        let instr = registry.decode(&bytes, 0u64).unwrap();
        assert_eq!(instr.opid.ext, EXT_C);
        assert_eq!(instr.size, 2);
    }

    #[test]
    fn test_extension_name_and_id() {
        let base = BaseExtension;
        assert_eq!(InstructionExtension::<Rv64>::name(&base), "I");
        assert_eq!(InstructionExtension::<Rv64>::ext_id(&base), EXT_I);

        let c = CExtension;
        assert_eq!(InstructionExtension::<Rv64>::name(&c), "C");
        assert_eq!(InstructionExtension::<Rv64>::ext_id(&c), EXT_C);
    }

    #[test]
    fn test_registry_extensions() {
        let registry = ExtensionRegistry::<Rv64>::standard();
        let extensions = registry.extensions();
        assert_eq!(extensions.len(), 6); // C, I, M, A, Zicsr, Zifencei
        assert_eq!(extensions[0].name(), "C"); // C first
        assert_eq!(extensions[1].name(), "I");
    }

    #[test]
    fn test_op_info_base() {
        use crate::{OpClass, OP_JAL, OP_LW, OP_SW, OP_FENCE, OP_ECALL};
        let registry = ExtensionRegistry::<Rv64>::standard();

        let info = registry.op_info(OP_ADDI).unwrap();
        assert_eq!(info.name, "addi");
        assert_eq!(info.class, OpClass::Alu);
        assert_eq!(info.size_hint, 4);

        let info = registry.op_info(OP_JAL).unwrap();
        assert_eq!(info.class, OpClass::Jump);

        let info = registry.op_info(OP_LW).unwrap();
        assert_eq!(info.class, OpClass::Load);

        let info = registry.op_info(OP_SW).unwrap();
        assert_eq!(info.class, OpClass::Store);

        let info = registry.op_info(OP_FENCE).unwrap();
        assert_eq!(info.class, OpClass::Fence);

        let info = registry.op_info(OP_ECALL).unwrap();
        assert_eq!(info.class, OpClass::System);
    }

    #[test]
    fn test_op_info_extensions() {
        use crate::{OpClass, OP_MUL, OP_DIV, OP_LR_W, OP_C_J, OP_C_LW, OP_CSRRW};
        let registry = ExtensionRegistry::<Rv64>::standard();

        let info = registry.op_info(OP_MUL).unwrap();
        assert_eq!(info.name, "mul");
        assert_eq!(info.class, OpClass::Mul);

        let info = registry.op_info(OP_DIV).unwrap();
        assert_eq!(info.class, OpClass::Div);

        let info = registry.op_info(OP_LR_W).unwrap();
        assert_eq!(info.name, "lr.w");
        assert_eq!(info.class, OpClass::Atomic);

        let info = registry.op_info(OP_C_J).unwrap();
        assert_eq!(info.name, "c.j");
        assert_eq!(info.class, OpClass::Jump);
        assert_eq!(info.size_hint, 2); // compressed

        let info = registry.op_info(OP_C_LW).unwrap();
        assert_eq!(info.class, OpClass::Load);

        let info = registry.op_info(OP_CSRRW).unwrap();
        assert_eq!(info.name, "csrrw");
        assert_eq!(info.class, OpClass::Csr);
    }

    #[test]
    fn test_op_info_zifencei() {
        use crate::{OpClass, OP_FENCE_I, EXT_ZIFENCEI};
        let registry = ExtensionRegistry::<Rv64>::standard();

        // Zifencei extension handles FENCE.I instruction
        assert_eq!(OP_FENCE_I.ext, EXT_ZIFENCEI);
        let info = registry.op_info(OP_FENCE_I).unwrap();
        assert_eq!(info.name, "fence.i");
        assert_eq!(info.class, OpClass::Fence);
        assert_eq!(info.size_hint, 4);
    }

    #[test]
    fn test_zifencei_decode_lift_disasm() {
        use crate::{OP_FENCE_I, EXT_ZIFENCEI};
        let registry = ExtensionRegistry::<Rv64>::standard();

        // FENCE.I encoding: opcode=0x0F, funct3=1, rest is zero
        // 0x0000100F
        let bytes = [0x0F, 0x10, 0x00, 0x00];
        let instr = registry.decode(&bytes, 0x1000u64).unwrap();
        assert_eq!(instr.opid, OP_FENCE_I);
        assert_eq!(instr.opid.ext, EXT_ZIFENCEI);

        // Test lift works
        let ir = registry.lift(&instr);
        assert!(!ir.terminator.is_control_flow()); // FENCE.I is not a control flow instruction

        // Test disasm works
        let disasm = registry.disasm(&instr);
        assert_eq!(disasm, "fence.i");
    }
}
