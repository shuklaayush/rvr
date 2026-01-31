//! Test suite commands.

use std::path::PathBuf;

use rvr::Compiler;
use rvr::tests::{self, BuildConfig, TestCategory, TestConfig};
use rvr_emit::Backend;

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
    backend: Backend,
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

    let backend_name = match backend {
        Backend::C => "C",
        Backend::X86Asm => "x86",
    };
    eprintln!("Using backend: {}", backend_name);

    // Check for x86 backend on non-x86 host
    if matches!(backend, Backend::X86Asm) {
        let is_x86_host = cfg!(target_arch = "x86_64") || cfg!(target_arch = "x86");
        if !is_x86_host {
            eprintln!();
            eprintln!("Error: x86 backend cannot be tested on non-x86 host");
            eprintln!("The x86 backend generates x86-64 shared libraries that cannot be");
            eprintln!("loaded on {} hosts.", std::env::consts::ARCH);
            eprintln!();
            eprintln!("Options:");
            eprintln!("  - Run tests on an x86-64 machine");
            eprintln!("  - Use the C backend (--backend c) which generates portable C code");
            return EXIT_FAILURE;
        }
    }

    let config = TestConfig::default()
        .with_test_dir(test_dir)
        .with_verbose(verbose)
        .with_timeout(timeout)
        .with_compiler(compiler)
        .with_backend(backend);
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
