//! Integration tests for the recompiler pipeline.

use std::path::Path;
use rvr::{Recompiler, Rv64, Pipeline, ElfImage, EmitConfig};

const RISCV_TESTS_DIR: &str = "/home/ayush/projects/openvm-mojo/bin/riscv/tests";

fn test_binary_path(name: &str) -> std::path::PathBuf {
    Path::new(RISCV_TESTS_DIR).join(name)
}

#[test]
fn test_lift_rv64ui_add() {
    let path = test_binary_path("rv64ui/rv64ui-p-add");
    if !path.exists() {
        eprintln!("Skipping test: {} not found", path.display());
        return;
    }

    // Load and parse ELF
    let data = std::fs::read(&path).expect("Failed to read test binary");
    let image = ElfImage::parse(&data).expect("Failed to parse ELF");

    // Create pipeline
    let config = EmitConfig::default();
    let mut pipeline = Pipeline::<Rv64>::new(image, config);

    // Run CFG analysis
    pipeline.analyze_cfg();
    let cfg = pipeline.cfg_result.as_ref().expect("CFG analysis failed");

    // Verify we found some basic blocks
    assert!(!cfg.leaders.is_empty(), "No basic block leaders found");
    assert!(!cfg.function_entries.is_empty(), "No function entries found");

    // Lift to IR
    pipeline.lift_to_ir();
    assert!(!pipeline.ir_blocks.is_empty(), "No IR blocks generated");

    // Get stats
    let stats = pipeline.stats();
    println!("Test binary: {}", path.display());
    println!("  Blocks: {}", stats.num_blocks);
    println!("  Leaders: {}", stats.num_leaders);
    println!("  Functions: {}", stats.num_functions);
}

#[test]
fn test_lift_rv64ui_addi() {
    let path = test_binary_path("rv64ui/rv64ui-p-addi");
    if !path.exists() {
        eprintln!("Skipping test: {} not found", path.display());
        return;
    }

    let data = std::fs::read(&path).expect("Failed to read test binary");
    let image = ElfImage::parse(&data).expect("Failed to parse ELF");

    let config = EmitConfig::default();
    let mut pipeline = Pipeline::<Rv64>::new(image, config);

    pipeline.analyze_cfg();
    pipeline.lift_to_ir();

    let stats = pipeline.stats();
    assert!(stats.num_blocks > 0, "No blocks generated");
    println!("rv64ui-p-addi: {} blocks, {} functions", stats.num_blocks, stats.num_functions);
}

#[test]
fn test_emit_c_code() {
    let path = test_binary_path("rv64ui/rv64ui-p-add");
    if !path.exists() {
        eprintln!("Skipping test: {} not found", path.display());
        return;
    }

    // Create temp output directory
    let temp_dir = std::env::temp_dir().join("rvr_test_emit");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let data = std::fs::read(&path).expect("Failed to read test binary");
    let image = ElfImage::parse(&data).expect("Failed to parse ELF");

    let config = EmitConfig::default();
    let mut pipeline = Pipeline::<Rv64>::new(image, config);

    pipeline.analyze_cfg();
    pipeline.lift_to_ir();
    pipeline.emit_c(&temp_dir, "rv64").expect("Failed to emit C code");

    // Verify generated files
    assert!(temp_dir.join("rv64.h").exists(), "Header file not generated");
    assert!(temp_dir.join("rv64_part0.c").exists(), "Partition file not generated");
    assert!(temp_dir.join("rv64_dispatch.c").exists(), "Dispatch file not generated");
    assert!(temp_dir.join("Makefile").exists(), "Makefile not generated");

    // Read and verify header content
    let header = std::fs::read_to_string(temp_dir.join("rv64.h")).expect("Failed to read header");
    assert!(header.contains("#define XLEN 64"), "XLEN not set correctly in header");
    assert!(header.contains("typedef struct RvState"), "RvState not defined");

    // Read and verify partition content
    let partition = std::fs::read_to_string(temp_dir.join("rv64_part0.c")).expect("Failed to read partition");
    assert!(partition.contains("#include \"rv64.h\""), "Include missing in partition");
    assert!(partition.contains("void B_0x"), "Block functions not generated");

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_recompiler_lift() {
    let path = test_binary_path("rv64ui/rv64ui-p-add");
    if !path.exists() {
        eprintln!("Skipping test: {} not found", path.display());
        return;
    }

    let temp_dir = std::env::temp_dir().join("rvr_test_recompiler");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let recompiler = Recompiler::<Rv64>::with_defaults();
    let result = recompiler.lift(&path, &temp_dir);

    assert!(result.is_ok(), "Lift failed: {:?}", result.err());

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
