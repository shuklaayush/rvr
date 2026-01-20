//! Example: Custom Keccak-F extension for RVR.
//!
//! This example demonstrates how to create a custom RISC-V extension
//! without modifying core ISA code. The extension adds a hypothetical
//! `keccakf.permute` instruction that calls an external Keccak-F permutation.
//!
//! Run with: `cargo run --example keccakf_ext -p rvr-isa`

use rvr_ir::{Expr, InstrIR, Rv64, Stmt, Terminator, Xlen};
use rvr_isa::{
    DecodedInstr, ExtensionRegistry, InstrArgs, InstructionExtension, OpClass, OpId, OpInfo,
};

// Custom opcode for keccakf.permute
// Custom extensions should define their own extension IDs.
const EXT_KECCAKF: u8 = 128;
const OP_KECCAKF_PERMUTE: OpId = OpId::new(EXT_KECCAKF, 0);

/// Custom R-type encoding for keccakf.permute:
///   31-25: funct7 = 0b0000001 (identifies keccakf.permute)
///   24-20: rs2 (unused, should be zero)
///   19-15: rs1 (state pointer register)
///   14-12: funct3 = 0b000
///   11-7:  rd (unused, should be zero)
///   6-0:   opcode = 0b0001011 (custom-0)
const CUSTOM_0_OPCODE: u32 = 0b0001011;
const KECCAKF_FUNCT3: u32 = 0b000;
const KECCAKF_FUNCT7: u32 = 0b0000001;

/// Keccak-F extension for RISC-V.
///
/// Adds the `keccakf.permute` instruction which performs the full
/// 24-round Keccak-F permutation. The instruction reads a state pointer
/// from rs1 and calls the external `keccakf_permute(state_ptr)` function.
pub struct KeccakFExtension;

impl<X: Xlen> InstructionExtension<X> for KeccakFExtension {
    fn name(&self) -> &'static str {
        "Xkeccakf"
    }

    fn ext_id(&self) -> u8 {
        EXT_KECCAKF
    }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = (raw >> 12) & 0x7;
        let funct7 = (raw >> 25) & 0x7F;
        let rd = ((raw >> 7) & 0x1F) as u8;
        let rs1 = ((raw >> 15) & 0x1F) as u8;
        let rs2 = ((raw >> 20) & 0x1F) as u8;

        // Check for custom-0 opcode with our funct7/funct3
        if opcode != CUSTOM_0_OPCODE || funct3 != KECCAKF_FUNCT3 || funct7 != KECCAKF_FUNCT7 {
            return None;
        }

        Some(DecodedInstr::new(
            OP_KECCAKF_PERMUTE,
            pc,
            4,
            InstrArgs::R { rd, rs1, rs2 },
        ))
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        // Extract rs1 (state pointer) from custom args
        let rs1 = match &instr.args {
            InstrArgs::R { rs1, .. } => *rs1,
            _ => panic!("keccakf.permute requires R args with rs1"),
        };

        let state_ptr = Expr::reg(rs1);
        let call = Stmt::extern_call("keccakf_permute", vec![state_ptr]);

        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            vec![call],
            Terminator::Fall { target: None },
        )
    }

    fn disasm(&self, instr: &DecodedInstr<X>) -> String {
        match &instr.args {
            InstrArgs::R { rs1, .. } => format!("keccakf.permute x{}", rs1),
            _ => "keccakf.permute ???".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        if opid == OP_KECCAKF_PERMUTE {
            Some(OpInfo {
                opid: OP_KECCAKF_PERMUTE,
                name: "keccakf.permute",
                class: OpClass::Other,
                size_hint: 4,
            })
        } else {
            None
        }
    }
}

/// Encode a keccakf.permute instruction.
fn encode_keccakf_permute(rs1: u8) -> [u8; 4] {
    let raw: u32 = CUSTOM_0_OPCODE
        | (KECCAKF_FUNCT3 << 12)
        | ((rs1 as u32 & 0x1F) << 15)
        | (KECCAKF_FUNCT7 << 25);
    raw.to_le_bytes()
}

fn main() {
    println!("=== KeccakF Custom Extension Example ===\n");

    // Create a registry with standard extensions plus our custom extension
    let registry = ExtensionRegistry::<Rv64>::standard().with_extension(KeccakFExtension);

    // Encode keccakf.permute x10 (state ptr in a0)
    let instr_bytes = encode_keccakf_permute(10);
    println!(
        "Encoded keccakf.permute x10: {:02x} {:02x} {:02x} {:02x}",
        instr_bytes[0], instr_bytes[1], instr_bytes[2], instr_bytes[3]
    );

    // Decode the instruction
    let pc = 0x1000u64;
    let instr = registry
        .decode(&instr_bytes, pc)
        .expect("Failed to decode keccakf.permute");

    println!(
        "Decoded: opid={}, pc=0x{:x}, size={}",
        instr.opid, pc, instr.size
    );
    assert_eq!(instr.opid, OP_KECCAKF_PERMUTE);
    assert_eq!(instr.size, 4);

    // Disassemble
    let disasm = registry.disasm(&instr);
    println!("Disassembly: {}", disasm);
    assert!(disasm.contains("keccakf.permute"));
    assert!(disasm.contains("x10"));

    // Lift to IR
    let ir = registry.lift(&instr);
    println!("\nLifted IR:");
    println!("  PC: 0x{:x}", Rv64::to_u64(ir.pc));
    println!("  Size: {} bytes", ir.size);
    println!(
        "  Statements: {} (expect 1 extern call)",
        ir.statements.len()
    );
    println!("  Terminator: {:?}", ir.terminator);

    // Verify IR structure
    assert_eq!(ir.statements.len(), 1);
    match &ir.statements[0] {
        Stmt::ExternCall { fn_name, args } => {
            println!("\n  ExternCall: {}({} args)", fn_name, args.len());
            assert_eq!(fn_name, "keccakf_permute");
            assert_eq!(args.len(), 1);
        }
        _ => panic!("Expected ExternCall statement"),
    }

    // Get op_info
    let info = registry
        .op_info(OP_KECCAKF_PERMUTE)
        .expect("OpInfo not found");
    println!("\nOpInfo:");
    println!("  Name: {}", info.name);
    println!("  Class: {:?}", info.class);
    println!("  Size hint: {} bytes", info.size_hint);

    println!("\n=== All assertions passed! ===");
    println!("\nThis example demonstrates that custom extensions can be added");
    println!("without modifying core ISA code - just implement InstructionExtension");
    println!("and register with ExtensionRegistry::with_extension().");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keccakf_decode_lift_disasm() {
        let registry = ExtensionRegistry::<Rv64>::standard().with_extension(KeccakFExtension);

        // Encode and decode
        let bytes = encode_keccakf_permute(10);
        let instr = registry.decode(&bytes, 0x1000u64).unwrap();
        assert_eq!(instr.opid, OP_KECCAKF_PERMUTE);

        // Lift
        let ir = registry.lift(&instr);
        assert_eq!(ir.statements.len(), 1);
        match &ir.statements[0] {
            Stmt::ExternCall { fn_name, args } => {
                assert_eq!(fn_name, "keccakf_permute");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected ExternCall"),
        }

        // Disasm
        let disasm = registry.disasm(&instr);
        assert_eq!(disasm, "keccakf.permute x10");
    }

    #[test]
    fn test_keccakf_op_info() {
        let ext = KeccakFExtension;
        let info: Option<OpInfo> = InstructionExtension::<Rv64>::op_info(&ext, OP_KECCAKF_PERMUTE);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.name, "keccakf.permute");
        assert_eq!(info.class, OpClass::Other);
    }

    #[test]
    fn test_registry_still_decodes_standard_instructions() {
        let registry = ExtensionRegistry::<Rv64>::standard().with_extension(KeccakFExtension);

        // ADDI x1, x0, 42 should still work
        let bytes = [0x93, 0x00, 0xa0, 0x02];
        let instr = registry.decode(&bytes, 0u64).unwrap();
        assert_eq!(instr.opid, rvr_isa::OP_ADDI);
    }
}
