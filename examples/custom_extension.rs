//! Custom extension example.
//!
//! Demonstrates how to add a small custom instruction extension and register
//! it with the pipeline. The example decodes a custom-0 opcode and treats it
//! like an "add" instruction.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example custom_extension -- path/to/program.elf output_dir/
//! ```
//!
//! Note: the ELF must actually contain the custom opcode for the extension to
//! be exercised. This example shows how to wire it in and still compiles
//! normal binaries.

use std::path::PathBuf;
use std::process::Command;

use rvr::{EmitConfig, Pipeline, Rv64};
use rvr_ir::{Expr, InstrIR, Stmt, Terminator};
use rvr_isa::{
    DecodedInstr, ExtensionRegistry, InstrArgs, InstructionExtension, OpClass, OpId, OpInfo,
};

const EXT_TOY: u8 = 200;
const OP_TOY_ADD: OpId = OpId::new(EXT_TOY, 0);

struct ToyExtension;

impl InstructionExtension<Rv64> for ToyExtension {
    fn name(&self) -> &'static str {
        "toy"
    }

    fn ext_id(&self) -> u8 {
        EXT_TOY
    }

    fn decode32(&self, raw: u32, pc: u64) -> Option<DecodedInstr<Rv64>> {
        // custom-0 opcode: 0b0001011
        if (raw & 0x7f) != 0x0b {
            return None;
        }
        let funct3 = (raw >> 12) & 0x7;
        let funct7 = (raw >> 25) & 0x7f;
        if funct3 != 0 || funct7 != 0 {
            return None;
        }

        let rd = ((raw >> 7) & 0x1f) as u8;
        let rs1 = ((raw >> 15) & 0x1f) as u8;
        let rs2 = ((raw >> 20) & 0x1f) as u8;
        let args = InstrArgs::R { rd, rs1, rs2 };

        Some(DecodedInstr::new(OP_TOY_ADD, pc, 4, args))
    }

    fn lift(&self, instr: &DecodedInstr<Rv64>) -> InstrIR<Rv64> {
        let (stmts, term) = match &instr.args {
            InstrArgs::R { rd, rs1, rs2 } => {
                let stmts = if *rd != 0 {
                    vec![Stmt::write_reg(*rd, Expr::add(Expr::read(*rs1), Expr::read(*rs2)))]
                } else {
                    Vec::new()
                };
                (stmts, Terminator::Fall { target: None })
            }
            _ => (Vec::new(), Terminator::trap("invalid args")),
        };

        InstrIR::new(instr.pc, instr.size, instr.opid.pack(), stmts, term)
    }

    fn disasm(&self, instr: &DecodedInstr<Rv64>) -> String {
        match &instr.args {
            InstrArgs::R { rd, rs1, rs2 } => format!("toy.add x{}, x{}, x{}", rd, rs1, rs2),
            _ => "toy.add <invalid>".to_string(),
        }
    }

    fn op_info(&self, opid: OpId) -> Option<OpInfo> {
        if opid == OP_TOY_ADD {
            Some(OpInfo {
                opid,
                name: "toy.add",
                class: OpClass::Alu,
                size_hint: 4,
            })
        } else {
            None
        }
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

    let data = std::fs::read(&elf_path)?;
    let image = rvr::ElfImage::<Rv64>::parse(&data)?;

    let registry = ExtensionRegistry::<Rv64>::standard().with_extension(ToyExtension);
    let config = EmitConfig::<Rv64>::default();
    let mut pipeline = Pipeline::with_registry(image, config, registry);

    pipeline.build_cfg()?;
    pipeline.lift_to_ir()?;

    std::fs::create_dir_all(&output_dir)?;
    let base_name = output_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rv");
    pipeline.emit_c(&output_dir, base_name)?;

    let status = Command::new("make")
        .arg("-C")
        .arg(&output_dir)
        .arg("shared")
        .status()?;
    if !status.success() {
        return Err("make failed".into());
    }

    println!("Generated C code with ToyExtension registered.");

    Ok(())
}
