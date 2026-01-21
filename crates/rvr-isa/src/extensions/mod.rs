//! RISC-V instruction set extensions.
//!
//! Each extension provides decode, lift, and disasm in a single file.
//! All instruction sets (including base I, M, A, C, Zicsr) are implemented
//! as extensions - there is no special "built-in" handling.

mod a;
mod base;
mod c;
mod m;
mod zba;
mod zbb;
mod zbkb;
mod zbs;
mod zicond;
mod zicsr;
mod zifencei;

// Re-export extension structs
pub use a::AExtension;
pub use base::BaseExtension;
pub use c::CExtension;
pub use m::MExtension;
pub use zba::ZbaExtension;
pub use zbb::ZbbExtension;
pub use zbkb::ZbkbExtension;
pub use zbs::ZbsExtension;
pub use zicond::ZicondExtension;
pub use zicsr::ZicsrExtension;
pub use zifencei::ZifenceiExtension;

// Re-export OpId constants and mnemonic functions from each extension
pub use a::{
    a_mnemonic, OP_AMOADD_D, OP_AMOADD_W, OP_AMOAND_D, OP_AMOAND_W, OP_AMOMAXU_D, OP_AMOMAXU_W,
    OP_AMOMAX_D, OP_AMOMAX_W, OP_AMOMINU_D, OP_AMOMINU_W, OP_AMOMIN_D, OP_AMOMIN_W, OP_AMOOR_D,
    OP_AMOOR_W, OP_AMOSWAP_D, OP_AMOSWAP_W, OP_AMOXOR_D, OP_AMOXOR_W, OP_LR_D, OP_LR_W, OP_SC_D,
    OP_SC_W,
};
pub use base::{
    base_mnemonic, OP_ADD, OP_ADDI, OP_ADDIW, OP_ADDW, OP_AND, OP_ANDI, OP_AUIPC, OP_BEQ, OP_BGE,
    OP_BGEU, OP_BLT, OP_BLTU, OP_BNE, OP_EBREAK, OP_ECALL, OP_FENCE, OP_JAL, OP_JALR, OP_LB,
    OP_LBU, OP_LD, OP_LH, OP_LHU, OP_LUI, OP_LW, OP_LWU, OP_MRET, OP_OR, OP_ORI, OP_SB, OP_SD,
    OP_SH, OP_SLL, OP_SLLI, OP_SLLIW, OP_SLLW, OP_SLT, OP_SLTI, OP_SLTIU, OP_SLTU, OP_SRA, OP_SRAI,
    OP_SRAIW, OP_SRAW, OP_SRL, OP_SRLI, OP_SRLIW, OP_SRLW, OP_SUB, OP_SUBW, OP_SW, OP_XOR, OP_XORI,
};
pub use c::{
    c_mnemonic, OP_C_ADD, OP_C_ADDI, OP_C_ADDI16SP, OP_C_ADDI4SPN, OP_C_ADDIW, OP_C_ADDW, OP_C_AND,
    OP_C_ANDI, OP_C_BEQZ, OP_C_BNEZ, OP_C_EBREAK, OP_C_J, OP_C_JAL, OP_C_JALR, OP_C_JR, OP_C_LD,
    OP_C_LDSP, OP_C_LI, OP_C_LUI, OP_C_LW, OP_C_LWSP, OP_C_MV, OP_C_NOP, OP_C_OR, OP_C_SD,
    OP_C_SDSP, OP_C_SLLI, OP_C_SRAI, OP_C_SRLI, OP_C_SUB, OP_C_SUBW, OP_C_SW, OP_C_SWSP, OP_C_XOR,
};
pub use m::{
    m_mnemonic, OP_DIV, OP_DIVU, OP_DIVUW, OP_DIVW, OP_MUL, OP_MULH, OP_MULHSU, OP_MULHU, OP_MULW,
    OP_REM, OP_REMU, OP_REMUW, OP_REMW,
};
pub use zba::{
    zba_mnemonic, OP_ADD_UW, OP_SH1ADD, OP_SH1ADD_UW, OP_SH2ADD, OP_SH2ADD_UW, OP_SH3ADD,
    OP_SH3ADD_UW, OP_SLLI_UW,
};
pub use zbb::{
    zbb_mnemonic, OP_ANDN, OP_CLZ, OP_CLZW, OP_CPOP, OP_CPOPW, OP_CTZ, OP_CTZW, OP_MAX, OP_MAXU,
    OP_MIN, OP_MINU, OP_ORC_B, OP_ORN, OP_REV8, OP_ROL, OP_ROLW, OP_ROR, OP_RORI, OP_RORIW,
    OP_RORW, OP_SEXT_B, OP_SEXT_H, OP_XNOR, OP_ZEXT_H,
};
pub use zbkb::{zbkb_mnemonic, OP_BREV8, OP_PACK, OP_PACKH, OP_PACKW, OP_UNZIP, OP_ZIP};
pub use zbs::{
    zbs_mnemonic, OP_BCLR, OP_BCLRI, OP_BEXT, OP_BEXTI, OP_BINV, OP_BINVI, OP_BSET, OP_BSETI,
};
pub use zicond::{zicond_mnemonic, OP_CZERO_EQZ, OP_CZERO_NEZ};
pub use zicsr::{
    csr_name, zicsr_mnemonic, CSR_CYCLE, CSR_CYCLEH, CSR_INSTRET, CSR_INSTRETH, CSR_MARCHID,
    CSR_MHARTID, CSR_MIMPID, CSR_MISA, CSR_MVENDORID, CSR_TIME, CSR_TIMEH, OP_CSRRC, OP_CSRRCI,
    OP_CSRRS, OP_CSRRSI, OP_CSRRW, OP_CSRRWI,
};
pub use zifencei::OP_FENCE_I;

use crate::{
    EXT_A, EXT_C, EXT_I, EXT_M, EXT_ZBA, EXT_ZBB, EXT_ZBKB, EXT_ZBS, EXT_ZICOND, EXT_ZICSR,
    EXT_ZIFENCEI,
};
use std::collections::HashMap;

/// Get instruction mnemonic from packed OpId (ext << 8 | idx).
///
/// Returns uppercase mnemonic for use in comments.
pub fn op_mnemonic(packed: u16) -> &'static str {
    let ext = (packed >> 8) as u8;
    let opid = OpId {
        ext,
        idx: packed as u8,
    };
    match ext {
        EXT_I => base_mnemonic(opid),
        EXT_M => m_mnemonic(opid),
        EXT_A => a_mnemonic(opid),
        EXT_C => c_mnemonic(opid),
        EXT_ZICSR => zicsr_mnemonic(opid),
        EXT_ZIFENCEI => "fence.i",
        EXT_ZBA => zba_mnemonic(opid).unwrap_or("???"),
        EXT_ZBB => zbb_mnemonic(opid).unwrap_or("???"),
        EXT_ZBS => zbs_mnemonic(opid).unwrap_or("???"),
        EXT_ZBKB => zbkb_mnemonic(opid).unwrap_or("???"),
        EXT_ZICOND => zicond_mnemonic(opid).unwrap_or("???"),
        _ => "???",
    }
}

use crate::syscalls::{BareMetalHandler, SyscallHandler};
use crate::{DecodedInstr, OpId, OpInfo};
use rvr_ir::{InstrIR, Terminator, Xlen};

/// Override trait for intercepting instruction lifting.
///
/// Allows custom handling of specific instructions by OpId.
/// The override receives the original instruction and a default lift function.
pub trait InstructionOverride<X: Xlen>: Send + Sync {
    /// Lift instruction, with access to the default lift implementation.
    ///
    /// # Arguments
    /// * `instr` - The decoded instruction
    /// * `default_lift` - Closure to call the standard lift for this instruction
    fn lift(
        &self,
        instr: &DecodedInstr<X>,
        default_lift: &dyn Fn(&DecodedInstr<X>) -> InstrIR<X>,
    ) -> InstrIR<X>;
}

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
///
/// Supports per-OpId overrides for custom instruction handling.
///
/// # Building a Registry
///
/// Use the builder pattern to construct a registry with specific extensions:
///
/// ```ignore
/// use rvr_isa::{ExtensionRegistry, Rv64};
///
/// // Minimal: just base I extension
/// let minimal = ExtensionRegistry::<Rv64>::base();
///
/// // Common embedded: I + M + C
/// let embedded = ExtensionRegistry::<Rv64>::base()
///     .with_m()
///     .with_c();
///
/// // Full Linux userspace: I + M + A + C + Zicsr
/// let linux = ExtensionRegistry::<Rv64>::base()
///     .with_m()
///     .with_a()
///     .with_c()
///     .with_zicsr();
///
/// // All standard extensions
/// let full = ExtensionRegistry::<Rv64>::standard();
/// ```
///
/// # Extension Order
///
/// Extensions are tried in registration order during decode. The C extension
/// should be added first (via `with_c()`) to handle 16-bit compressed instructions
/// before 32-bit decoders see them.
pub struct ExtensionRegistry<X: Xlen> {
    extensions: Vec<Box<dyn InstructionExtension<X>>>,
    overrides: HashMap<OpId, Box<dyn InstructionOverride<X>>>,
    syscall_handler: Box<dyn SyscallHandler<X>>,
}

impl<X: Xlen> ExtensionRegistry<X> {
    /// Create a new registry with the given extensions.
    pub fn new(extensions: Vec<Box<dyn InstructionExtension<X>>>) -> Self {
        Self {
            extensions,
            overrides: HashMap::new(),
            syscall_handler: Box::new(BareMetalHandler),
        }
    }

    /// Create a registry with just the base I extension.
    ///
    /// This is the minimal RISC-V configuration. Use builder methods
    /// to add more extensions:
    ///
    /// ```ignore
    /// let registry = ExtensionRegistry::<Rv64>::base()
    ///     .with_m()    // Integer multiply/divide
    ///     .with_c();   // Compressed instructions
    /// ```
    pub fn base() -> Self {
        Self {
            extensions: vec![Box::new(BaseExtension)],
            overrides: HashMap::new(),
            syscall_handler: Box::new(BareMetalHandler),
        }
    }

    /// Create a registry with all standard RISC-V extensions.
    ///
    /// Includes: I, M, A, C, Zicsr, Zifencei, Zba, Zbb, Zbs, Zbkb, Zicond.
    ///
    /// Order: C (compressed first), then I, M, A, Zicsr, Zifencei, Zba, Zbb, Zbs, Zbkb, Zicond.
    pub fn standard() -> Self {
        Self::base()
            .with_c() // C first (handles 16-bit instructions)
            .with_m()
            .with_a()
            .with_zicsr()
            .with_zifencei()
            .with_zba()
            .with_zbb()
            .with_zbs()
            .with_zbkb()
            .with_zicond()
    }

    /// Create an empty registry (no extensions).
    ///
    /// Useful for testing or building a completely custom extension set.
    pub fn empty() -> Self {
        Self {
            extensions: Vec::new(),
            overrides: HashMap::new(),
            syscall_handler: Box::new(BareMetalHandler),
        }
    }

    // =========================================================================
    // Standard extension builder methods
    // =========================================================================

    /// Add M extension (integer multiply/divide).
    ///
    /// Instructions: MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU,
    /// and W variants for RV64.
    pub fn with_m(self) -> Self {
        self.with_extension(MExtension)
    }

    /// Add A extension (atomic operations).
    ///
    /// Instructions: LR.W, SC.W, AMO*.W, and D variants for RV64.
    pub fn with_a(self) -> Self {
        self.with_extension(AExtension)
    }

    /// Add C extension (compressed 16-bit instructions).
    ///
    /// **Important**: Should be added first (before other extensions) so
    /// compressed instructions are decoded before 32-bit decoders see them.
    ///
    /// Instructions: C.LW, C.SW, C.ADDI, C.JAL, C.J, etc.
    pub fn with_c(mut self) -> Self {
        // Insert C at the front for correct decode order
        self.extensions.insert(0, Box::new(CExtension));
        self
    }

    /// Add Zicsr extension (CSR instructions).
    ///
    /// Instructions: CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI.
    pub fn with_zicsr(self) -> Self {
        self.with_extension(ZicsrExtension)
    }

    /// Add Zifencei extension (instruction-fetch fence).
    ///
    /// Instructions: FENCE.I.
    pub fn with_zifencei(self) -> Self {
        self.with_extension(ZifenceiExtension)
    }

    /// Add Zba extension (address generation).
    ///
    /// Instructions: SH1ADD, SH2ADD, SH3ADD, ADD.UW, SH*ADD.UW, SLLI.UW.
    pub fn with_zba(self) -> Self {
        self.with_extension(ZbaExtension)
    }

    /// Add Zbb extension (basic bit manipulation).
    ///
    /// Instructions: ANDN, ORN, XNOR, CLZ, CTZ, CPOP, MAX, MIN, SEXT, ZEXT,
    /// ROL, ROR, ORC.B, REV8.
    pub fn with_zbb(self) -> Self {
        self.with_extension(ZbbExtension)
    }

    /// Add Zbs extension (single-bit operations).
    ///
    /// Instructions: BCLR, BEXT, BINV, BSET and immediate variants.
    pub fn with_zbs(self) -> Self {
        self.with_extension(ZbsExtension)
    }

    /// Add Zbkb extension (bit manipulation for cryptography).
    ///
    /// Instructions: BREV8, PACK, PACKH, PACKW, ZIP, UNZIP.
    pub fn with_zbkb(self) -> Self {
        self.with_extension(ZbkbExtension)
    }

    /// Add Zicond extension (conditional operations).
    ///
    /// Instructions: CZERO.EQZ, CZERO.NEZ.
    pub fn with_zicond(self) -> Self {
        self.with_extension(ZicondExtension)
    }

    // =========================================================================
    // Generic extension and override methods
    // =========================================================================

    /// Add a custom extension to the registry.
    ///
    /// Extensions are appended to the end of the decode chain.
    /// For the C extension, use `with_c()` which inserts at the front.
    pub fn with_extension(mut self, ext: impl InstructionExtension<X> + 'static) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Register an override for a specific OpId.
    ///
    /// When the given OpId is lifted, the override's `lift()` method is called
    /// instead of the standard extension lift.
    pub fn with_override(
        mut self,
        opid: OpId,
        handler: impl InstructionOverride<X> + 'static,
    ) -> Self {
        self.overrides.insert(opid, Box::new(handler));
        self
    }

    /// Register multiple overrides at once.
    pub fn with_overrides(
        mut self,
        overrides: HashMap<OpId, Box<dyn InstructionOverride<X>>>,
    ) -> Self {
        self.overrides.extend(overrides);
        self
    }

    /// Set the syscall handler for ECALL instructions.
    ///
    /// The syscall handler is called when an ECALL instruction is lifted,
    /// unless an explicit override for OP_ECALL is registered.
    ///
    /// Default: `RiscvTestsHandler` (exits with a0 as exit code).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rvr_isa::{ExtensionRegistry, syscalls::LinuxHandler};
    /// use rvr_ir::Rv64;
    ///
    /// let registry = ExtensionRegistry::<Rv64>::standard()
    ///     .with_syscall_handler(LinuxHandler::default());
    /// ```
    pub fn with_syscall_handler(mut self, handler: impl SyscallHandler<X> + 'static) -> Self {
        self.syscall_handler = Box::new(handler);
        self
    }

    /// Check if there are any overrides registered.
    #[inline]
    pub fn has_overrides(&self) -> bool {
        !self.overrides.is_empty()
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
    ///
    /// Order of precedence:
    /// 1. Explicit override for the OpId (highest priority)
    /// 2. Syscall handler for ECALL instructions
    /// 3. Default extension lift
    pub fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        // Check for explicit override (highest priority)
        if let Some(handler) = self.overrides.get(&instr.opid) {
            let default_lift = |i: &DecodedInstr<X>| self.lift_without_override(i);
            return handler.lift(instr, &default_lift);
        }

        self.lift_without_override(instr)
    }

    /// Lift without checking overrides (for syscall handler and default).
    fn lift_without_override(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        // ECALL is handled by the syscall handler
        if instr.opid == OP_ECALL {
            return self.syscall_handler.handle_ecall(instr);
        }

        self.lift_default(instr)
    }

    /// Default lift implementation (no override).
    fn lift_default(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        for ext in &self.extensions {
            if ext.ext_id() == instr.opid.ext {
                return ext.lift(instr);
            }
        }
        // No extension handles this - return trap
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            Vec::new(),
            Terminator::trap("unhandled extension"),
        )
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
    use crate::{EXT_C, EXT_I, OP_ADDI};
    use rvr_ir::Rv64;

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
        assert_eq!(extensions.len(), 11); // C, I, M, A, Zicsr, Zifencei, Zba, Zbb, Zbs, Zbkb, Zicond
        assert_eq!(extensions[0].name(), "C"); // C first (inserted at front)
        assert_eq!(extensions[1].name(), "I"); // Base I second
    }

    #[test]
    fn test_builder_base_only() {
        let registry = ExtensionRegistry::<Rv64>::base();
        let extensions = registry.extensions();
        assert_eq!(extensions.len(), 1);
        assert_eq!(extensions[0].name(), "I");

        // Should decode base instructions
        let bytes = [0x93, 0x00, 0xa0, 0x02]; // addi
        assert!(registry.decode(&bytes, 0u64).is_some());

        // Should NOT decode M extension instructions
        // MUL x1, x2, x3 = 0x023100b3
        let mul_bytes = [0xb3, 0x00, 0x31, 0x02];
        assert!(registry.decode(&mul_bytes, 0u64).is_none());
    }

    #[test]
    fn test_builder_incremental() {
        // Build up extensions one by one
        let registry = ExtensionRegistry::<Rv64>::base().with_m().with_c();

        let extensions = registry.extensions();
        assert_eq!(extensions.len(), 3);
        // C should be first (inserted at front)
        assert_eq!(extensions[0].name(), "C");
        assert_eq!(extensions[1].name(), "I");
        assert_eq!(extensions[2].name(), "M");

        // Should decode compressed instructions
        let c_addi_bytes = [0x85, 0x00]; // c.addi x1, 1
        let instr = registry.decode(&c_addi_bytes, 0u64).unwrap();
        assert_eq!(instr.opid.ext, EXT_C);

        // Should decode M extension
        let mul_bytes = [0xb3, 0x00, 0x31, 0x02]; // mul x1, x2, x3
        let instr = registry.decode(&mul_bytes, 0u64).unwrap();
        assert_eq!(instr.opid.ext, crate::EXT_M);
    }

    #[test]
    fn test_builder_linux_userspace() {
        // Typical Linux userspace configuration
        let registry = ExtensionRegistry::<Rv64>::base()
            .with_m()
            .with_a()
            .with_c()
            .with_zicsr();

        let extensions = registry.extensions();
        assert_eq!(extensions.len(), 5);

        // Verify C is first
        assert_eq!(extensions[0].name(), "C");
    }

    #[test]
    fn test_builder_with_override() {
        use crate::OP_ECALL;
        use rvr_ir::{Expr, Terminator};

        struct CustomEcall;
        impl InstructionOverride<Rv64> for CustomEcall {
            fn lift(
                &self,
                instr: &DecodedInstr<Rv64>,
                _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
            ) -> InstrIR<Rv64> {
                InstrIR::new(
                    instr.pc,
                    instr.size,
                    instr.opid.pack(),
                    Vec::new(),
                    Terminator::exit(Expr::Imm(99)),
                )
            }
        }

        let registry = ExtensionRegistry::<Rv64>::base()
            .with_m()
            .with_override(OP_ECALL, CustomEcall);

        assert!(registry.has_overrides());

        // ECALL should use our override
        let ecall_bytes = [0x73, 0x00, 0x00, 0x00];
        let instr = registry.decode(&ecall_bytes, 0u64).unwrap();
        let ir = registry.lift(&instr);
        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }

    #[test]
    fn test_op_info_base() {
        use crate::{OpClass, OP_ECALL, OP_FENCE, OP_JAL, OP_LW, OP_SW};
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
        use crate::{OpClass, OP_CSRRW, OP_C_J, OP_C_LW, OP_DIV, OP_LR_W, OP_MUL};
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
        use crate::{OpClass, EXT_ZIFENCEI, OP_FENCE_I};
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
        use crate::{EXT_ZIFENCEI, OP_FENCE_I};
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

    #[test]
    fn test_override_ecall_fixed_exit() {
        use crate::OP_ECALL;
        use rvr_ir::{Expr, Terminator};

        // Override that replaces ECALL with fixed exit code 42
        struct ExitOverride;
        impl InstructionOverride<Rv64> for ExitOverride {
            fn lift(
                &self,
                instr: &DecodedInstr<Rv64>,
                _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
            ) -> InstrIR<Rv64> {
                InstrIR::new(
                    instr.pc,
                    instr.size,
                    instr.opid.pack(),
                    Vec::new(),
                    Terminator::exit(Expr::Imm(42)),
                )
            }
        }

        let registry = ExtensionRegistry::<Rv64>::standard().with_override(OP_ECALL, ExitOverride);

        // Encode ECALL: 0x00000073
        let bytes = [0x73, 0x00, 0x00, 0x00];
        let instr = registry.decode(&bytes, 0x1000u64).unwrap();
        assert_eq!(instr.opid, OP_ECALL);

        let ir = registry.lift(&instr);
        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }

    #[test]
    fn test_override_calls_default() {
        use crate::OP_ADDI;

        // Override that calls default and verifies it returns something
        struct PassthroughOverride {
            called: std::sync::atomic::AtomicBool,
        }
        impl InstructionOverride<Rv64> for PassthroughOverride {
            fn lift(
                &self,
                instr: &DecodedInstr<Rv64>,
                default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
            ) -> InstrIR<Rv64> {
                self.called.store(true, std::sync::atomic::Ordering::SeqCst);
                default(instr)
            }
        }

        let override_impl = std::sync::Arc::new(PassthroughOverride {
            called: std::sync::atomic::AtomicBool::new(false),
        });

        // Need to create a wrapper that implements InstructionOverride
        struct ArcWrapper(std::sync::Arc<PassthroughOverride>);
        impl InstructionOverride<Rv64> for ArcWrapper {
            fn lift(
                &self,
                instr: &DecodedInstr<Rv64>,
                default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
            ) -> InstrIR<Rv64> {
                self.0.lift(instr, default)
            }
        }

        let registry = ExtensionRegistry::<Rv64>::standard()
            .with_override(OP_ADDI, ArcWrapper(override_impl.clone()));

        // ADDI x1, x0, 42
        let bytes = [0x93, 0x00, 0xa0, 0x02];
        let instr = registry.decode(&bytes, 0u64).unwrap();
        assert_eq!(instr.opid, OP_ADDI);

        let ir = registry.lift(&instr);
        // Verify override was called
        assert!(override_impl
            .called
            .load(std::sync::atomic::Ordering::SeqCst));
        // Default lift should have produced statements
        assert!(!ir.statements.is_empty());
    }

    #[test]
    fn test_override_no_regression_fast_path() {
        // Ensure standard registry without overrides works fast
        let registry = ExtensionRegistry::<Rv64>::standard();
        assert!(!registry.has_overrides());

        // ADDI x1, x0, 42
        let bytes = [0x93, 0x00, 0xa0, 0x02];
        let instr = registry.decode(&bytes, 0u64).unwrap();
        let ir = registry.lift(&instr);
        assert!(!ir.statements.is_empty()); // Should have register write
    }

    #[test]
    fn test_override_with_multiple() {
        use crate::{OP_ADD, OP_SUB};
        use rvr_ir::Terminator;

        struct TrapOverride;
        impl InstructionOverride<Rv64> for TrapOverride {
            fn lift(
                &self,
                instr: &DecodedInstr<Rv64>,
                _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
            ) -> InstrIR<Rv64> {
                InstrIR::new(
                    instr.pc,
                    instr.size,
                    instr.opid.pack(),
                    Vec::new(),
                    Terminator::trap("overridden"),
                )
            }
        }

        let mut overrides: HashMap<OpId, Box<dyn InstructionOverride<Rv64>>> = HashMap::new();
        overrides.insert(OP_ADD, Box::new(TrapOverride));
        overrides.insert(OP_SUB, Box::new(TrapOverride));

        let registry = ExtensionRegistry::<Rv64>::standard().with_overrides(overrides);
        assert!(registry.has_overrides());

        // ADD x1, x2, x3: 0x003100b3
        let add_bytes = [0xb3, 0x00, 0x31, 0x00];
        let add_instr = registry.decode(&add_bytes, 0u64).unwrap();
        assert_eq!(add_instr.opid, OP_ADD);
        let ir = registry.lift(&add_instr);
        assert!(matches!(ir.terminator, Terminator::Trap { .. }));
    }

    #[test]
    fn test_ecall_uses_syscall_handler() {
        use crate::OP_ECALL;
        use rvr_ir::Terminator;

        // Default registry uses RiscvTestsHandler
        let registry = ExtensionRegistry::<Rv64>::standard();

        // ECALL encoding: 0x00000073
        let bytes = [0x73, 0x00, 0x00, 0x00];
        let instr = registry.decode(&bytes, 0x1000u64).unwrap();
        assert_eq!(instr.opid, OP_ECALL);

        let ir = registry.lift(&instr);
        // RiscvTestsHandler exits with a0
        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }

    #[test]
    fn test_ecall_custom_syscall_handler() {
        use crate::syscalls::LinuxHandler;
        use crate::OP_ECALL;
        use rvr_ir::Terminator;

        // Use LinuxHandler instead of default
        let registry =
            ExtensionRegistry::<Rv64>::standard().with_syscall_handler(LinuxHandler::default());

        // ECALL encoding: 0x00000073
        let bytes = [0x73, 0x00, 0x00, 0x00];
        let instr = registry.decode(&bytes, 0x1000u64).unwrap();
        assert_eq!(instr.opid, OP_ECALL);

        let ir = registry.lift(&instr);
        // LinuxHandler uses Fall terminator (runtime checks exited flag)
        assert!(matches!(ir.terminator, Terminator::Fall { .. }));
        // LinuxHandler generates syscall dispatch statements
        assert!(!ir.statements.is_empty());
    }

    #[test]
    fn test_ecall_override_takes_precedence() {
        use crate::syscalls::LinuxHandler;
        use crate::OP_ECALL;
        use rvr_ir::{Expr, Terminator};

        // Custom override that returns fixed exit code 99
        struct FixedExitOverride;
        impl InstructionOverride<Rv64> for FixedExitOverride {
            fn lift(
                &self,
                instr: &DecodedInstr<Rv64>,
                _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
            ) -> InstrIR<Rv64> {
                InstrIR::new(
                    instr.pc,
                    instr.size,
                    instr.opid.pack(),
                    Vec::new(),
                    Terminator::exit(Expr::Imm(99)),
                )
            }
        }

        // Override takes precedence over syscall handler
        let registry = ExtensionRegistry::<Rv64>::standard()
            .with_syscall_handler(LinuxHandler::default())
            .with_override(OP_ECALL, FixedExitOverride);

        // ECALL encoding: 0x00000073
        let bytes = [0x73, 0x00, 0x00, 0x00];
        let instr = registry.decode(&bytes, 0x1000u64).unwrap();
        let ir = registry.lift(&instr);

        // Override should win, returning exit with 99
        match ir.terminator {
            Terminator::Exit { code } => {
                assert!(matches!(code, Expr::Imm(99)));
            }
            _ => panic!("Expected Exit terminator"),
        }
    }
}
