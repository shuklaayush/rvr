//! Basic compilation example.
//!
//! Demonstrates the simplest usage of rvr: compiling a RISC-V ELF to a shared library.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example basic_compile -- path/to/program.elf output_dir/
//! ```
//!
//! The output directory will contain:
//! - `{name}.h` - Header with RvState struct and helpers
//! - `{name}_blocks.h` - Block declarations
//! - `{name}_part*.c` - Recompiled code partitions
//! - `{name}_dispatch.c` - Dispatch table
//! - `{name}_memory.c` - Memory initialization
//! - `Makefile` - Build system
//! - `lib{name}.so` - Compiled shared library (after make)

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <elf_path> <output_dir>", args[0]);
        std::process::exit(1);
    }

    let elf_path = PathBuf::from(&args[1]);
    let output_dir = PathBuf::from(&args[2]);

    // Compile with auto-detected XLEN (RV32 or RV64)
    let lib_path = rvr::compile(&elf_path, &output_dir)?;

    println!("Compiled to: {}", lib_path.display());
    Ok(())
}
