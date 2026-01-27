//! Test suite commands.

use std::path::PathBuf;

use rvr::tests::{self, BuildConfig, TestCategory, TestConfig};
use rvr::Compiler;

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};

/// Build riscv-tests from source.
pub fn riscv_tests_build(
    category_str: &str,
    output: Option<PathBuf>,
    toolchain: Option<String>,
) -> i32 {
    // Parse categories
    let categories = match TestCategory::parse_list(category_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            return EXIT_FAILURE;
        }
    };

    // Find toolchain
    let toolchain = match toolchain.or_else(tests::find_toolchain) {
        Some(t) => t,
        None => {
            eprintln!("Error: RISC-V toolchain not found");
            eprintln!("Install riscv64-unknown-elf-gcc or specify --toolchain");
            return EXIT_FAILURE;
        }
    };

    let project_dir = std::env::current_dir().expect("failed to get current directory");

    let mut config = BuildConfig::new(categories)
        .with_src_dir(project_dir.join("programs/riscv-tests/isa"))
        .with_toolchain(&toolchain);

    if let Some(out) = output {
        config = config.with_out_dir(out);
    } else {
        config = config.with_out_dir(project_dir.join("bin/riscv-tests"));
    }

    eprintln!("Using toolchain: {}gcc", toolchain);
    eprintln!("Source: {}", config.src_dir.display());
    eprintln!("Output: {}", config.out_dir.display());
    eprintln!();

    eprintln!("Building {} categories...", config.categories.len());

    let results = match tests::build_tests(&config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {}", e);
            return EXIT_FAILURE;
        }
    };

    // Print per-category results
    for result in &results {
        if result.failed > 0 {
            eprintln!(
                "  {}: {} built, {} failed",
                result.category, result.built, result.failed
            );
        } else {
            eprintln!("  {}: {} tests", result.category, result.built);
        }
    }

    tests::print_build_summary(&results);
    EXIT_SUCCESS
}

/// Run riscv-tests suite.
pub fn riscv_tests_run(
    filter: Option<String>,
    verbose: bool,
    timeout: u64,
    cc: &str,
    linker: Option<&str>,
) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");
    let test_dir = project_dir.join("bin/riscv-tests");

    if !test_dir.exists() {
        eprintln!("Error: test directory not found: {}", test_dir.display());
        eprintln!("Place riscv-tests ELF binaries in bin/riscv-tests/");
        return EXIT_FAILURE;
    }

    let mut compiler: Compiler = cc.parse().unwrap_or_else(|e: String| {
        eprintln!("error: invalid compiler: {}", e);
        std::process::exit(EXIT_FAILURE);
    });
    if let Some(ld) = linker {
        compiler = compiler.with_linker(ld);
    }

    let config = TestConfig::default()
        .with_test_dir(test_dir)
        .with_verbose(verbose)
        .with_timeout(timeout)
        .with_compiler(compiler);
    let config = if let Some(f) = filter {
        config.with_filter(f)
    } else {
        config
    };

    let summary = tests::run_all(&config);
    tests::print_summary(&summary);

    if summary.all_passed() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    }
}
