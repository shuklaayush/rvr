//! Test suite commands.
//!
//! This module provides CLI commands for running RISC-V test suites:
//! - `riscv_tests`: The classic riscv-tests suite (exit code based)
//! - `arch_tests`: The official riscv-arch-test suite (signature comparison based)

pub mod arch_tests;
pub mod riscv_tests;

use std::path::PathBuf;

use rvr_emit::Backend;

use rvr::Compiler;
use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};

// Re-export key types for convenience
pub use arch_tests::{ArchBuildConfig, ArchTestCategory, ArchTestConfig, GenRefsConfig};
pub use riscv_tests::{BuildConfig, TestCategory, TestConfig};

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
    let toolchain = match toolchain.or_else(riscv_tests::find_toolchain) {
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

    let results = match riscv_tests::build_tests(&config) {
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

    riscv_tests::print_build_summary(&results);
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
        Backend::ARM64Asm => "arm64",
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

    // Check for ARM64 backend on non-ARM64 host
    if matches!(backend, Backend::ARM64Asm) {
        let is_arm64_host = cfg!(target_arch = "aarch64");
        if !is_arm64_host {
            eprintln!();
            eprintln!("Error: arm64 backend cannot be tested on non-ARM64 host");
            eprintln!("The arm64 backend generates ARM64 shared libraries that cannot be");
            eprintln!("loaded on {} hosts.", std::env::consts::ARCH);
            eprintln!();
            eprintln!("Options:");
            eprintln!("  - Run tests on an ARM64 machine");
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

    let summary = riscv_tests::run_all(&config);
    riscv_tests::print_summary(&summary);

    if summary.all_passed() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    }
}

// ============================================================================
// Arch Test Commands
// ============================================================================

/// Build riscv-arch-test from source.
pub fn arch_tests_build(
    category_str: &str,
    output: Option<PathBuf>,
    toolchain: Option<String>,
    no_refs: bool,
) -> i32 {
    // Parse categories
    let categories = match ArchTestCategory::parse_list(category_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            return EXIT_FAILURE;
        }
    };

    // Find toolchain
    let toolchain = match toolchain.or_else(arch_tests::find_toolchain) {
        Some(t) => t,
        None => {
            eprintln!("Error: RISC-V toolchain not found");
            eprintln!("Install riscv64-unknown-elf-gcc or specify --toolchain");
            return EXIT_FAILURE;
        }
    };

    // Check for Spike if we need to generate references
    let has_spike = arch_tests::find_spike().is_some();
    if !no_refs && !has_spike {
        eprintln!("Warning: Spike not found, skipping reference generation");
        eprintln!("Install Spike from https://github.com/riscv-software-src/riscv-isa-sim");
        eprintln!("Or use --no-refs to skip reference generation");
    }

    let project_dir = std::env::current_dir().expect("failed to get current directory");

    let mut config = ArchBuildConfig::new(categories)
        .with_src_dir(project_dir.join("programs/riscv-arch-test/riscv-test-suite"))
        .with_toolchain(&toolchain)
        .with_gen_refs(!no_refs && has_spike);

    if let Some(out) = output {
        config = config.with_out_dir(out.clone());
        config = config.with_refs_dir(out.join("references"));
    } else {
        config = config.with_out_dir(project_dir.join("bin/riscv-arch-test"));
        config = config.with_refs_dir(project_dir.join("bin/riscv-arch-test/references"));
    }

    eprintln!("Using toolchain: {}gcc", toolchain);
    eprintln!("Source: {}", config.src_dir.display());
    eprintln!("Output: {}", config.out_dir.display());
    eprintln!();

    eprintln!("Building {} categories...", config.categories.len());

    let results = match arch_tests::build_tests(&config) {
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
                "  {}: {} built, {} failed, {} refs",
                result.category, result.built, result.failed, result.refs_generated
            );
        } else {
            eprintln!(
                "  {}: {} tests, {} refs",
                result.category, result.built, result.refs_generated
            );
        }
    }

    arch_tests::print_build_summary(&results);
    EXIT_SUCCESS
}

/// Run riscv-arch-test suite.
pub fn arch_tests_run(
    filter: Option<String>,
    verbose: bool,
    timeout: u64,
    cc: &str,
    linker: Option<&str>,
    backend: Backend,
) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");
    let test_dir = project_dir.join("bin/riscv-arch-test");
    let refs_dir = project_dir.join("bin/riscv-arch-test/references");

    if !test_dir.exists() {
        eprintln!("Error: test directory not found: {}", test_dir.display());
        eprintln!("Run 'rvr test arch build' first to build tests");
        return EXIT_FAILURE;
    }

    if !refs_dir.exists() {
        eprintln!(
            "Error: reference directory not found: {}",
            refs_dir.display()
        );
        eprintln!("Run 'rvr test arch gen-refs' to generate reference signatures");
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
        Backend::ARM64Asm => "arm64",
    };
    eprintln!("Using backend: {}", backend_name);

    // Check for x86 backend on non-x86 host
    if matches!(backend, Backend::X86Asm) {
        let is_x86_host = cfg!(target_arch = "x86_64") || cfg!(target_arch = "x86");
        if !is_x86_host {
            eprintln!();
            eprintln!("Error: x86 backend cannot be tested on non-x86 host");
            eprintln!("Use the C backend (--backend c) or run on x86-64 machine");
            return EXIT_FAILURE;
        }
    }

    // Check for ARM64 backend on non-ARM64 host
    if matches!(backend, Backend::ARM64Asm) {
        let is_arm64_host = cfg!(target_arch = "aarch64");
        if !is_arm64_host {
            eprintln!();
            eprintln!("Error: arm64 backend cannot be tested on non-ARM64 host");
            eprintln!("Use the C backend (--backend c) or run on ARM64 machine");
            return EXIT_FAILURE;
        }
    }

    let config = ArchTestConfig::default()
        .with_test_dir(test_dir)
        .with_refs_dir(refs_dir)
        .with_verbose(verbose)
        .with_timeout(timeout)
        .with_compiler(compiler)
        .with_backend(backend);
    let config = if let Some(f) = filter {
        config.with_filter(f)
    } else {
        config
    };

    let summary = arch_tests::run_all(&config);
    riscv_tests::print_summary(&summary);

    if summary.all_passed() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    }
}

/// Generate reference signatures using Spike.
///
/// This runs each test on Spike (the RISC-V reference simulator) and dumps
/// the signature region. These references serve as ground truth for verifying
/// rvr's correctness.
pub fn arch_tests_gen_refs(category_str: &str, output: Option<PathBuf>, force: bool) -> i32 {
    // Parse categories
    let categories = match ArchTestCategory::parse_list(category_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            return EXIT_FAILURE;
        }
    };

    // Check for Spike
    if arch_tests::find_spike().is_none() {
        eprintln!("Error: Spike not found");
        eprintln!("Install from https://github.com/riscv-software-src/riscv-isa-sim");
        return EXIT_FAILURE;
    }

    let project_dir = std::env::current_dir().expect("failed to get current directory");

    let mut config = GenRefsConfig::new(categories).with_force(force);

    if let Some(out) = output {
        config = config.with_refs_dir(out);
    } else {
        config = config.with_refs_dir(project_dir.join("bin/riscv-arch-test/references"));
    }
    config = config.with_test_dir(project_dir.join("bin/riscv-arch-test"));

    eprintln!("Test dir: {}", config.test_dir.display());
    eprintln!("Refs dir: {}", config.refs_dir.display());
    eprintln!();

    match arch_tests::generate_references(&config) {
        Ok(count) => {
            eprintln!("Generated {} reference signatures", count);
            EXIT_SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            EXIT_FAILURE
        }
    }
}
