//! Custom configuration example.
//!
//! Demonstrates configuring the recompiler with:
//! - Hot registers (passed as function arguments for speed)
//! - Instruction retirement counting
//! - Address bounds checking
//! - Linux syscall handling for user-space workloads
//!
//! # Hot Registers
//!
//! Hot registers are passed directly as function arguments instead of
//! being read from the state struct. This enables better register allocation
//! and reduces memory traffic. Common choices:
//!
//! - `ra` (x1): Return address - frequently read/written
//! - `sp` (x2): Stack pointer - frequently used as base address
//! - `a0-a7` (x10-x17): Argument/return registers
//! - `t0-t6`: Temporaries
//!
//! # Usage
//!
//! ```bash
//! cargo run --example custom_config -- path/to/program.elf output_dir/
//! ```

use std::path::PathBuf;
use std::process::Command;

use rvr::{EmitConfig, InstretMode, Pipeline, Rv64};
use rvr_isa::{syscalls::LinuxHandler, syscalls::SyscallAbi, ExtensionRegistry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (elf_path, output_dir) = parse_args()?;

    // Explicit RV64 configuration (use RV32 for rv32 binaries)
    let mut config = EmitConfig::<Rv64>::default();

    // Configure hot registers - these become function parameters
    // Register indices: ra=1, sp=2, a0=10, a1=11, a2=12
    config.hot_regs = vec![1, 2, 10, 11, 12];

    // Configure memory (32-bit address space = 4GB).
    config.memory_bits = 32;

    // Enable instruction retirement counting.
    config.instret_mode = InstretMode::Count;

    // Enable address bounds checking (slower but safer).
    config.addr_check = true;

    // Linux syscall handling for user-space workloads.
    let registry = ExtensionRegistry::<Rv64>::standard()
        .with_syscall_handler(LinuxHandler::new(SyscallAbi::Standard));

    // Load ELF and build pipeline with custom registry.
    let data = std::fs::read(&elf_path)?;
    let image = rvr::ElfImage::<Rv64>::parse(&data)?;
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

    println!("Compiled to: {}", output_dir.display());

    let runner = rvr::Runner::load(&output_dir)?;
    let result = runner.run()?;
    result.print_mojo_format();

    Ok(())
}

fn parse_args() -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <elf_path> <output_dir>", args[0]);
        std::process::exit(1);
    }
    Ok((PathBuf::from(&args[1]), PathBuf::from(&args[2])))
}
