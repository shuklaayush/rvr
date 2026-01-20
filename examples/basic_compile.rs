//! Basic compilation example.
//!
//! Demonstrates compiling a RISC-V ELF and running it via the built-in runner.
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
//! - Run output from the shared library

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (elf_path, output_dir) = parse_args()?;

    // Compile with auto-detected XLEN (RV32 or RV64).
    let lib_path = rvr::compile(&elf_path, &output_dir)?;
    println!("Compiled to: {}", lib_path.display());

    // Run the compiled program
    let runner = rvr::Runner::load(&output_dir)?;
    let result = runner.run()?;
    println!("Exit code: {}", result.exit_code);
    println!("Instructions: {}", result.instret);
    println!("Time: {:.6}s", result.time_secs);
    println!("Speed: {:.2} MIPS", result.mips);
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
