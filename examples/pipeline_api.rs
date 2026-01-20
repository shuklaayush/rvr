//! Pipeline API example.
//!
//! Demonstrates the lower-level Pipeline API for fine-grained control over:
//!
//! - CFG construction and analysis
//! - IR lifting and inspection
//! - Code emission
//!
//! # Pipeline Stages
//!
//! ```text
//! ELF Binary
//!     │
//!     ▼
//! ┌─────────────────┐
//! │  ElfImage::parse │  Parse ELF, extract segments
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │   build_cfg()   │  Decode instructions, build CFG
//! └────────┬────────┘  Optimize: merge, tail-dup, superblock
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  lift_to_ir()   │  Convert to IR using extension registry
//! └────────┬────────┘  Apply instruction overrides
//!          │
//!          ▼
//! ┌─────────────────┐
//! │    emit_c()     │  Generate C code, headers, Makefile
//! └────────┬────────┘
//!          │
//!          ▼
//!    C Source Files
//! ```
//!
//! # Usage
//!
//! ```bash
//! cargo run --example pipeline_api -- path/to/program.elf output_dir/
//! ```

use std::path::PathBuf;

use rvr::{ElfImage, EmitConfig, Pipeline, Rv64};
use rvr_ir::Terminator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <elf_path> <output_dir>", args[0]);
        std::process::exit(1);
    }

    let elf_path = PathBuf::from(&args[1]);
    let output_dir = PathBuf::from(&args[2]);

    // Stage 1: Parse ELF
    println!("=== Stage 1: Parse ELF ===");
    let data = std::fs::read(&elf_path)?;
    let image = ElfImage::<Rv64>::parse(&data)?;

    println!("Entry point: {:#x}", image.entry_point);
    println!("Memory segments: {}", image.memory_segments.len());
    for (i, seg) in image.memory_segments.iter().enumerate() {
        println!(
            "  [{i}] {:#x} - {:#x} ({} bytes, {})",
            seg.virtual_start,
            seg.virtual_end,
            seg.data.len(),
            if seg.is_executable() {
                "exec"
            } else if seg.is_readonly() {
                "ro"
            } else {
                "rw"
            }
        );
    }

    // Stage 2: Create Pipeline
    println!("\n=== Stage 2: Create Pipeline ===");
    let config = EmitConfig::<Rv64>::default();
    let mut pipeline = Pipeline::new(image, config);

    // Stage 3: Build CFG
    println!("\n=== Stage 3: Build CFG ===");
    pipeline.build_cfg()?;

    if let Some(block_table) = pipeline.block_table() {
        println!("Basic blocks: {}", block_table.len());
        println!("Absorbed blocks: {}", block_table.absorbed_to_merged.len());

        // Show first few blocks
        println!("\nFirst 5 blocks:");
        for (i, block) in block_table.iter().take(5).enumerate() {
            println!(
                "  [{i}] {:#x} - {:#x} ({} bytes)",
                block.start,
                block.end,
                block.end - block.start
            );
        }
    }

    // Stage 4: Lift to IR
    println!("\n=== Stage 4: Lift to IR ===");
    pipeline.lift_to_ir()?;

    println!("IR blocks: {}", pipeline.ir_blocks().len());

    // Analyze IR blocks
    let mut branch_count = 0;
    let mut jump_count = 0;
    let mut call_count = 0;
    let mut exit_count = 0;

    for block in pipeline.ir_blocks().values() {
        for instr in &block.instructions {
            match &instr.terminator {
                Terminator::Branch { .. } => branch_count += 1,
                Terminator::Jump { .. } | Terminator::JumpDynamic { .. } => jump_count += 1,
                Terminator::Call { .. } => call_count += 1,
                Terminator::Exit { .. } => exit_count += 1,
                _ => {}
            }
        }
    }

    println!("Control flow:");
    println!("  Branches: {branch_count}");
    println!("  Jumps: {jump_count}");
    println!("  Calls: {call_count}");
    println!("  Exits: {exit_count}");

    // Stage 5: Emit C code
    println!("\n=== Stage 5: Emit C Code ===");
    std::fs::create_dir_all(&output_dir)?;
    let base_name = output_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rv");
    pipeline.emit_c(&output_dir, base_name)?;

    println!("Output directory: {}", output_dir.display());
    println!("Generated files:");
    for entry in std::fs::read_dir(&output_dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        println!("  {} ({} bytes)", entry.file_name().to_string_lossy(), meta.len());
    }

    // Summary stats
    println!("\n=== Summary ===");
    let stats = pipeline.stats();
    println!("Blocks: {}", stats.num_blocks);
    println!("Basic blocks: {}", stats.num_basic_blocks);
    println!("Absorbed: {}", stats.num_absorbed);

    Ok(())
}
