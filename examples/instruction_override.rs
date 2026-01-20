//! Instruction override example.
//!
//! Demonstrates customizing instruction behavior using the override mechanism.
//! This is useful for:
//!
//! - Custom ECALL/syscall handling
//! - Instruction tracing/profiling
//! - Implementing custom extensions
//! - Testing and debugging
//!
//! # Architecture
//!
//! The override mechanism intercepts instruction lifting (IR generation).
//! When an override is registered for an opcode, the override's `lift` method
//! is called instead of the default. The override can:
//!
//! 1. Generate completely custom IR
//! 2. Call the default lift and modify the result
//! 3. Wrap the default behavior with pre/post processing
//!
//! # Usage
//!
//! ```bash
//! cargo run --example instruction_override -- path/to/program.elf output_dir/
//! ```

use std::path::PathBuf;

use rvr::{EmitConfig, Rv64};
use rvr_ir::{Expr, InstrIR, Terminator};
use rvr_isa::{DecodedInstr, ExtensionRegistry, InstructionOverride, OP_ECALL};

/// Custom ECALL handler that treats ECALL as an exit with a0 as return code.
///
/// This is the default riscv-tests behavior, shown here for illustration.
struct RiscvTestsEcall;

impl InstructionOverride<Rv64> for RiscvTestsEcall {
    fn lift(
        &self,
        instr: &DecodedInstr<Rv64>,
        _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
    ) -> InstrIR<Rv64> {
        // Exit with a0 (register 10) as the exit code
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            Vec::new(),
            Terminator::exit(Expr::read(10)), // a0 = x10
        )
    }
}

/// Custom ECALL handler that generates a trap for debugging.
struct DebugEcall;

impl InstructionOverride<Rv64> for DebugEcall {
    fn lift(
        &self,
        instr: &DecodedInstr<Rv64>,
        _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
    ) -> InstrIR<Rv64> {
        // Generate a trap with a descriptive message
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            Vec::new(),
            Terminator::trap(&format!("ECALL at PC {:#x}", instr.pc)),
        )
    }
}

/// Passthrough override that logs and delegates to default.
struct LoggingOverride;

impl InstructionOverride<Rv64> for LoggingOverride {
    fn lift(
        &self,
        instr: &DecodedInstr<Rv64>,
        default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
    ) -> InstrIR<Rv64> {
        println!(
            "Lifting ECALL at PC {:#x}, opid={:?}",
            instr.pc, instr.opid
        );
        // Delegate to default implementation
        default(instr)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <elf_path> <output_dir>", args[0]);
        std::process::exit(1);
    }

    let elf_path = PathBuf::from(&args[1]);
    let output_dir = PathBuf::from(&args[2]);

    // Create extension registry with custom ECALL override
    let registry = ExtensionRegistry::<Rv64>::standard().with_override(OP_ECALL, RiscvTestsEcall);

    // Load ELF
    let data = std::fs::read(&elf_path)?;
    let image = rvr::ElfImage::<Rv64>::parse(&data)?;

    // Create pipeline with custom registry
    let config = EmitConfig::<Rv64>::default();
    let mut pipeline = rvr::Pipeline::with_registry(image, config, registry);

    // Build CFG and lift to IR (overrides are applied during lift)
    pipeline.build_cfg()?;
    pipeline.lift_to_ir()?;

    // Emit C code
    std::fs::create_dir_all(&output_dir)?;
    let base_name = output_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rv");
    pipeline.emit_c(&output_dir, base_name)?;

    println!("Generated C code with custom ECALL handling");
    println!("Stats: {:?}", pipeline.stats());

    Ok(())
}
