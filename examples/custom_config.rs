//! Custom configuration example.
//!
//! Demonstrates configuring the recompiler with:
//! - Hot registers (passed as function arguments for speed)
//! - Instruction retirement counting
//! - Address bounds checking
//! - HTIF support for riscv-tests
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

use rvr::{CompileOptions, EmitConfig, InstretMode, Recompiler, Rv64};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <elf_path> <output_dir>", args[0]);
        std::process::exit(1);
    }

    let elf_path = PathBuf::from(&args[1]);
    let output_dir = PathBuf::from(&args[2]);

    // Method 1: Using CompileOptions (simple, auto-detects XLEN)
    let options = CompileOptions::new()
        .with_instret_mode(InstretMode::Count) // Count retired instructions
        .with_addr_check(true) // Enable bounds checking
        .with_tohost(true) // Enable HTIF for riscv-tests
        .with_jobs(0); // Auto-detect parallelism

    let lib_path = rvr::compile_with_options(&elf_path, &output_dir, options)?;
    println!("Method 1 - Compiled to: {}", lib_path.display());

    // Method 2: Using Recompiler directly (more control, explicit XLEN)
    let mut config = EmitConfig::<Rv64>::default();

    // Configure hot registers - these become function parameters
    // Register indices: ra=1, sp=2, a0=10, a1=11, a2=12
    config.hot_regs = vec![1, 2, 10, 11, 12];

    // Configure memory (32-bit address space = 4GB)
    config.memory_bits = 32;

    // Enable instruction retirement counting
    config.instret_mode = InstretMode::Count;

    // Enable address bounds checking (slower but safer)
    config.addr_check = true;

    // Enable HTIF support for riscv-tests
    config.tohost_enabled = true;

    // Enable LTO in generated Makefile
    config.enable_lto = true;

    let recompiler = Recompiler::new(config);
    let lib_path = recompiler.compile(&elf_path, &output_dir, 0)?;
    println!("Method 2 - Compiled to: {}", lib_path.display());

    Ok(())
}
