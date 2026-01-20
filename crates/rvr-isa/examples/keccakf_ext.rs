//! Example: Custom Keccak-F extension for RVR.
//!
//! This example demonstrates how to create a custom RISC-V extension
//! without modifying core ISA code. The extension adds a hypothetical
//! `keccakf.round` instruction that calls an external Keccak-F round function.
//!
//! Run with: `cargo run --example keccakf_ext -p rvr-isa`

use rvr_ir::{Expr, InstrIR, Rv64, Stmt, Terminator, Xlen};
use rvr_isa::{
    DecodedInstr, ExtensionRegistry, InstrArgs, InstructionExtension, OpClass, OpId, OpInfo,
};

// Custom opcode for keccakf.round
// Custom extensions should define their own extension IDs.
const EXT_KECCAKF: u8 = 128;
const OP_KECCAKF_ROUND: OpId = OpId::new(EXT_KECCAKF, 0);

/// Custom R4-type encoding for keccakf.round:
///   31-27: funct5 = 0b00001 (identifies keccakf.round)
///   26-25: fmt = 0b00
///   24-20: rs3 (round constant index)
///   19-15: rs2 (unused, typically zero)
///   14-12: funct3 = 0b000
///   11-7:  rs1 (state pointer register)
///   6-0:   opcode = 0b0001011 (custom-0)
const CUSTOM_0_OPCODE: u32 = 0b0001011;
const KECCAKF_FUNCT5: u32 = 0b00001;
const KECCAKF_FUNCT3: u32 = 0b000;

/// Keccak-F extension for RISC-V.
///
/// Adds the `keccakf.round` instruction which performs one round of
/// the Keccak-F permutation. The instruction reads a state pointer
/// from rs1 and a round constant index from rs3, and calls the
/// external `keccakf_round(state_ptr, round_idx)` function.
pub struct KeccakFExtension;

impl<X: Xlen> InstructionExtension<X> for KeccakFExtension {
    fn name(&self) -> &'static str {
        "Xkeccakf"
    }

    fn ext_id(&self) -> u8 {
        }

    fn decode32(&self, raw: u32, pc: X::Reg) -> Option<DecodedInstr<X>> {
        let opcode = raw & 0x7F;
        let funct3 = (raw >> 12) & 0x7;
        let funct5 = (raw >> 27) & 0x1F;

        // Check for custom-0 opcode with our funct5/funct3
        if opcode != CUSTOM_0_OPCODE || funct3 != KECCAKF_FUNCT3 || funct5 != KECCAKF_FUNCT5 {
            return None;
        }

        // Extract register fields (R4-type format)
        let rs1 = ((raw >> 15) & 0x1F) as u8; // state pointer
        let rs3 = ((raw >> 20) & 0x1F) as u8; // round constant index

        Some(DecodedInstr::new(
            OP_KECCAKF_ROUND,
            pc,
            4,
            InstrArgs::Custom(Box::new([rs1 as u32, rs3 as u32])),
        ))
    }

    fn lift(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        // Extract rs1 (state pointer) and rs3 (round index) from custom args
        let (rs1, rs3) = match &instr.args {
            InstrArgs::Custom(args) if args.len() >= 2 => (args[0] as u8, args[1] as u8),
            _ => panic!("keccakf.round requires Custom args with rs1 and rs3"),
        };

        // Build extern call: keccakf_round(state_ptr, round_idx)
        let state_ptr = Expr::reg(rs1);
        let round_idx = Expr::reg(rs3);
        let call = Stmt::extern_call("keccakf_round", vec![state_ptr, round_idx]);

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
            InstrArgs::Custom(args) if args.len() >= 2 => {
                let rs1 = args[0];
                let rs3 = args[1];
                format!("keccakf.round x{}, x{}", rs1, rs3)
            }
            _ => "keccakf.round ???".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        if opid == OP_KECCAKF_ROUND {
            Some(OpInfo {
                opid: OP_KECCAKF_ROUND,
                name: "keccakf.round",
                class: OpClass::Other,
                size_hint: 4,
            })
        } else {
            None
        }
    }
}

/// Encode a keccakf.round instruction.
fn encode_keccakf_round(rs1: u8, rs3: u8) -> [u8; 4] {
    let raw: u32 = CUSTOM_0_OPCODE
        | ((KECCAKF_FUNCT3 as u32) << 12)
        | ((rs1 as u32 & 0x1F) << 15)
        | ((rs3 as u32 & 0x1F) << 20)
        | (KECCAKF_FUNCT5 << 27);
    raw.to_le_bytes()
}

fn main() {
    println!("=== KeccakF Custom Extension Example ===\n");

    // Create a registry with standard extensions plus our custom extension
    let registry = ExtensionRegistry::<Rv64>::standard().with_extension(KeccakFExtension);

    // Encode keccakf.round x10, x5 (state ptr in a0, round idx in t0)
    let instr_bytes = encode_keccakf_round(10, 5);
    println!(
        "Encoded keccakf.round x10, x5: {:02x} {:02x} {:02x} {:02x}",
        instr_bytes[0], instr_bytes[1], instr_bytes[2], instr_bytes[3]
    );

    // Decode the instruction
    let pc = 0x1000u64;
    let instr = registry.decode(&instr_bytes, pc).expect("Failed to decode keccakf.round");

    println!("Decoded: opid={}, pc=0x{:x}, size={}", instr.opid, pc, instr.size);
    assert_eq!(instr.opid, OP_KECCAKF_ROUND);
    assert_eq!(instr.size, 4);

    // Disassemble
    let disasm = registry.disasm(&instr);
    println!("Disassembly: {}", disasm);
    assert!(disasm.contains("keccakf.round"));
    assert!(disasm.contains("x10"));
    assert!(disasm.contains("x5"));

    // Lift to IR
    let ir = registry.lift(&instr);
    println!("\nLifted IR:");
    println!("  PC: 0x{:x}", Rv64::to_u64(ir.pc));
    println!("  Size: {} bytes", ir.size);
    println!("  Statements: {} (expect 1 extern call)", ir.statements.len());
    println!("  Terminator: {:?}", ir.terminator);

    // Verify IR structure
    assert_eq!(ir.statements.len(), 1);
    match &ir.statements[0] {
        Stmt::ExternCall { fn_name, args } => {
            println!("\n  ExternCall: {}({} args)", fn_name, args.len());
            assert_eq!(fn_name, "keccakf_round");
            assert_eq!(args.len(), 2);
        }
        _ => panic!("Expected ExternCall statement"),
    }

    // Get op_info
    let info = registry.op_info(OP_KECCAKF_ROUND).expect("OpInfo not found");
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
        let bytes = encode_keccakf_round(10, 5);
        let instr = registry.decode(&bytes, 0x1000u64).unwrap();
        assert_eq!(instr.opid, OP_KECCAKF_ROUND);

        // Lift
        let ir = registry.lift(&instr);
        assert_eq!(ir.statements.len(), 1);
        match &ir.statements[0] {
            Stmt::ExternCall { fn_name, args } => {
                assert_eq!(fn_name, "keccakf_round");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected ExternCall"),
        }

        // Disasm
        let disasm = registry.disasm(&instr);
        assert_eq!(disasm, "keccakf.round x10, x5");
    }

    #[test]
    fn test_keccakf_op_info() {
        let ext = KeccakFExtension;
        let info: Option<OpInfo> =
            InstructionExtension::<Rv64>::op_info(&ext, OP_KECCAKF_ROUND);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.name, "keccakf.round");
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
