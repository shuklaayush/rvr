//! Test suite commands.
//!
//! This module provides CLI commands for running RISC-V test suites:
//! - `riscv_tests`: The classic riscv-tests suite (exit code based)
//! - `arch_tests`: The official riscv-arch-test suite (signature comparison based)
//! - `trace`: Trace comparison between rvr and Spike for differential testing

pub mod arch_tests;
pub mod riscv_tests;
pub mod trace;

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

// ============================================================================
// Trace Comparison Command
// ============================================================================

/// Compare instruction traces between rvr and Spike.
pub fn trace_compare(
    elf_path: &PathBuf,
    output_dir: Option<PathBuf>,
    cc: &str,
    isa: Option<String>,
    timeout: u64,
    stop_on_first: bool,
) -> i32 {
    use std::process::Command;
    use std::time::Duration;

    // Check if test should be skipped
    let test_name = elf_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if riscv_tests::should_skip(test_name) {
        eprintln!("SKIP: {} (not compatible with static recompilation)", test_name);
        return EXIT_SUCCESS;
    }
    if test_name.contains("lrsc") {
        eprintln!("SKIP: {} (LR/SC trace is nondeterministic)", test_name);
        return EXIT_SUCCESS;
    }

    // Check Spike is available
    let spike_path = match trace::find_spike() {
        Some(p) => p,
        None => {
            eprintln!("Error: Spike not found in PATH");
            eprintln!("Install from https://github.com/riscv-software-src/riscv-isa-sim");
            return EXIT_FAILURE;
        }
    };

    // Determine ISA
    let isa = match isa {
        Some(i) => i,
        None => match trace::elf_to_isa(elf_path) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("Error detecting ISA: {}", e);
                return EXIT_FAILURE;
            }
        },
    };
    let isa = trace::isa_from_test_name(
        elf_path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
        &isa,
    );

    // Get entry point for alignment
    let entry_point = match trace::elf_entry_point(elf_path) {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("Error reading ELF entry point: {}", e);
            return EXIT_FAILURE;
        }
    };

    eprintln!("ELF: {}", elf_path.display());
    eprintln!("ISA: {}", isa);
    eprintln!("Entry: 0x{:x}", entry_point);
    eprintln!("Spike: {}", spike_path.display());

    // Create output directory
    let output_dir = output_dir.unwrap_or_else(|| {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        temp.keep()
    });

    eprintln!("Output: {}", output_dir.display());
    eprintln!();

    // Step 1: Compile ELF with rvr using spike tracer
    eprintln!("Step 1: Compiling with rvr (spike tracer)...");
    let compile_status = Command::new("./target/release/rvr")
        .arg("compile")
        .arg(elf_path)
        .arg("-o")
        .arg(&output_dir)
        .arg("--tracer")
        .arg("spike")
        .arg("--cc")
        .arg(cc)
        .status();

    match compile_status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("Error: rvr compile failed with exit code {:?}", status.code());
            return EXIT_FAILURE;
        }
        Err(e) => {
            eprintln!("Error: failed to run rvr compile: {}", e);
            return EXIT_FAILURE;
        }
    }

    // Step 2: Run Spike and capture trace
    eprintln!("Step 2: Running Spike...");
    let spike_trace_path = output_dir.join("spike_trace.log");
    let spike_timeout = Duration::from_secs(timeout);
    let mut spike_cmd = Command::new(&spike_path);
    spike_cmd
        .arg(format!("--isa={}", isa))
        .arg("--log-commits")
        .arg(format!("--log={}", spike_trace_path.display()))
        .arg(elf_path);
    let spike_status = trace::run_command_with_timeout(&mut spike_cmd, spike_timeout);

    match spike_status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            if test_name.contains("ma_data") {
                eprintln!("SKIP: Spike reference failed for {}", test_name);
                return EXIT_SUCCESS;
            }
            eprintln!("Error: Spike failed with exit code {:?}", status.code());
            return EXIT_FAILURE;
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            eprintln!("Error: Spike timed out after {}s", timeout);
            return EXIT_FAILURE;
        }
        Err(e) => {
            eprintln!("Error: failed to run Spike: {}", e);
            return EXIT_FAILURE;
        }
    }

    // Step 3: Run rvr and capture trace
    eprintln!("Step 3: Running rvr...");
    let rvr_trace_path = output_dir.join("rvr_trace.log");
    // SAFETY: We're single-threaded at this point and immediately remove the var
    unsafe { std::env::set_var("RVR_TRACE_FILE", &rvr_trace_path) };

    let mut rvr_cmd = Command::new("./target/release/rvr");
    rvr_cmd.arg("run").arg(&output_dir).arg(elf_path);
    let rvr_status = trace::run_command_with_timeout(&mut rvr_cmd, Duration::from_secs(timeout));

    // SAFETY: We're single-threaded and just cleaning up
    unsafe { std::env::remove_var("RVR_TRACE_FILE") };

    match rvr_status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("Error: rvr run failed with exit code {:?}", status.code());
            return EXIT_FAILURE;
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            eprintln!("Error: rvr run timed out after {}s", timeout);
            return EXIT_FAILURE;
        }
        Err(e) => {
            eprintln!("Error: failed to run rvr: {}", e);
            return EXIT_FAILURE;
        }
    }

    // Step 4: Parse and compare traces
    eprintln!("Step 4: Comparing traces...");

    let spike_trace = match trace::parse_trace_file(&spike_trace_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing Spike trace: {}", e);
            return EXIT_FAILURE;
        }
    };

    let rvr_trace = match trace::parse_trace_file(&rvr_trace_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing rvr trace: {}", e);
            return EXIT_FAILURE;
        }
    };

    eprintln!("Spike trace: {} entries", spike_trace.len());
    eprintln!("rvr trace: {} entries", rvr_trace.len());

    // Align traces (skip Spike's startup code)
    let (spike_aligned, rvr_aligned) = trace::align_traces_at(&spike_trace, &rvr_trace, entry_point);
    eprintln!("After alignment: Spike={}, rvr={}", spike_aligned.len(), rvr_aligned.len());

    // Compare with entry point for ECALL handling
    let config = trace::CompareConfig {
        entry_point,
        strict_reg_writes: true,
        strict_mem_access: false, // Spike doesn't always log mem for loads
        stop_on_first,
    };
    let result = trace::compare_traces_with_config(&spike_aligned, &rvr_aligned, &config);

    eprintln!();
    if let Some(div) = &result.divergence {
        eprintln!("DIVERGENCE at instruction {}: {}", div.index, div.kind);
        eprintln!();
        eprintln!("Expected (Spike):");
        eprintln!("  PC: 0x{:016x}", div.expected.pc);
        eprintln!("  Opcode: 0x{:08x}", div.expected.opcode);
        if let (Some(rd), Some(val)) = (div.expected.rd, div.expected.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.expected.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Actual (rvr):");
        eprintln!("  PC: 0x{:016x}", div.actual.pc);
        eprintln!("  Opcode: 0x{:08x}", div.actual.opcode);
        if let (Some(rd), Some(val)) = (div.actual.rd, div.actual.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.actual.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Traces saved to:");
        eprintln!("  Spike: {}", spike_trace_path.display());
        eprintln!("  rvr: {}", rvr_trace_path.display());
        EXIT_FAILURE
    } else {
        eprintln!("PASS: {} instructions matched", result.matched);
        EXIT_SUCCESS
    }
}
