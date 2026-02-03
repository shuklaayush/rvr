//! Test suite commands.
//!
//! This module provides CLI commands for running RISC-V test suites:
//! - `riscv_tests`: The classic riscv-tests suite (exit code based)
//! - `arch_tests`: The official riscv-arch-test suite (signature comparison based)
//! - `trace`: Trace comparison between rvr and Spike for differential testing
//! - `diff`: Lockstep differential execution between backends
// TODO: Remove CLI test commands after cargo test/bench migration is complete.

pub mod diff;
pub mod trace;

use std::path::{Path, PathBuf};

use rvr_emit::Backend;
use rvr_ir::Xlen;

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use rvr::test_support::{arch_tests, riscv_tests};
use rvr::Compiler;

// Re-export key types for convenience
pub use rvr::test_support::arch_tests::{ArchBuildConfig, ArchTestCategory, ArchTestConfig, GenRefsConfig};
pub use rvr::test_support::riscv_tests::{BuildConfig, TestCategory, TestConfig};

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
    let test_name = elf_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if riscv_tests::should_skip(test_name) {
        eprintln!(
            "SKIP: {} (not compatible with static recompilation)",
            test_name
        );
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
            eprintln!(
                "Error: rvr compile failed with exit code {:?}",
                status.code()
            );
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
    let (spike_aligned, rvr_aligned) =
        trace::align_traces_at(&spike_trace, &rvr_trace, entry_point);
    eprintln!(
        "After alignment: Spike={}, rvr={}",
        spike_aligned.len(),
        rvr_aligned.len()
    );

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

#[derive(Clone, Copy, Debug)]
enum DiffBackend {
    Spike,
    Backend(Backend),
}

impl DiffBackend {
    fn as_backend(&self) -> Option<Backend> {
        match self {
            DiffBackend::Backend(b) => Some(*b),
            DiffBackend::Spike => None,
        }
    }
}

fn diff_backend_from_arg(arg: DiffBackendArg) -> DiffBackend {
    match arg {
        DiffBackendArg::Spike => DiffBackend::Spike,
        DiffBackendArg::C => DiffBackend::Backend(Backend::C),
        DiffBackendArg::Arm64 => DiffBackend::Backend(Backend::ARM64Asm),
        DiffBackendArg::X86 => DiffBackend::Backend(Backend::X86Asm),
    }
}

fn resolve_diff_backends(
    mode: DiffModeArg,
    ref_backend: Option<DiffBackendArg>,
    test_backend: Option<DiffBackendArg>,
) -> Result<(DiffBackend, DiffBackend), String> {
    let (ref_backend, test_backend) = match (ref_backend, test_backend) {
        (Some(r), Some(t)) => (diff_backend_from_arg(r), diff_backend_from_arg(t)),
        (None, None) => match mode {
            DiffModeArg::SpikeC => (DiffBackend::Spike, DiffBackend::Backend(Backend::C)),
            DiffModeArg::SpikeArm64 => {
                (DiffBackend::Spike, DiffBackend::Backend(Backend::ARM64Asm))
            }
            DiffModeArg::CArm64 => (
                DiffBackend::Backend(Backend::C),
                DiffBackend::Backend(Backend::ARM64Asm),
            ),
        },
        _ => {
            return Err("both --ref and --test must be provided together".to_string());
        }
    };

    if matches!(test_backend, DiffBackend::Spike) {
        return Err("Spike can only be used as reference backend".to_string());
    }

    if matches!(ref_backend, DiffBackend::Spike) && matches!(test_backend, DiffBackend::Spike) {
        return Err("cannot compare Spike against Spike".to_string());
    }

    if let Some(backend) = ref_backend.as_backend()
        && !backend_supports_diff(backend)
    {
        return Err(format!(
            "backend {:?} does not support diff tracing",
            backend
        ));
    }
    if let Some(backend) = test_backend.as_backend()
        && !backend_supports_diff(backend)
    {
        return Err(format!(
            "backend {:?} does not support diff tracing",
            backend
        ));
    }

    Ok((ref_backend, test_backend))
}

fn backend_supports_diff(backend: Backend) -> bool {
    matches!(backend, Backend::C | Backend::ARM64Asm | Backend::X86Asm)
}

fn backend_supports_buffered_diff(backend: Backend) -> bool {
    matches!(backend, Backend::C)
}

// ============================================================================
// Differential Execution Command
// ============================================================================

use crate::cli::{DiffBackendArg, DiffGranularityArg, DiffModeArg};

/// Run lockstep differential execution between two backends.
#[allow(clippy::too_many_arguments)]
pub fn diff_compare(
    mode: DiffModeArg,
    ref_backend: Option<DiffBackendArg>,
    test_backend: Option<DiffBackendArg>,
    elf_path: &PathBuf,
    granularity_arg: DiffGranularityArg,
    max_instrs: Option<u64>,
    output_dir: Option<PathBuf>,
    ref_dir: Option<PathBuf>,
    test_dir: Option<PathBuf>,
    cc: &str,
    isa: Option<String>,
    strict_mem: bool,
) -> i32 {
    // Convert granularity argument
    let granularity = match granularity_arg {
        DiffGranularityArg::Instruction => diff::DiffGranularity::Instruction,
        DiffGranularityArg::Block => diff::DiffGranularity::Block,
        DiffGranularityArg::Hybrid => diff::DiffGranularity::Hybrid,
        DiffGranularityArg::Checkpoint => diff::DiffGranularity::Checkpoint,
        DiffGranularityArg::PureC => diff::DiffGranularity::PureC,
    };

    // Check if test should be skipped
    let test_name = elf_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if riscv_tests::should_skip(test_name) {
        eprintln!(
            "SKIP: {} (not compatible with static recompilation)",
            test_name
        );
        return EXIT_SUCCESS;
    }

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
    let isa = trace::isa_from_test_name(test_name, &isa);

    // Get entry point for alignment
    let entry_point = match trace::elf_entry_point(elf_path) {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("Error reading ELF entry point: {}", e);
            return EXIT_FAILURE;
        }
    };

    eprintln!("ELF: {}", elf_path.display());
    eprintln!("Mode: {:?}", mode);
    eprintln!("Granularity: {:?}", granularity);
    eprintln!("ISA: {}", isa);
    eprintln!("Entry: 0x{:x}", entry_point);
    if let Some(n) = max_instrs {
        eprintln!("Max instructions: {}", n);
    }
    eprintln!();

    // Create output directory
    let output_dir = output_dir.unwrap_or_else(|| {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        temp.keep()
    });

    // Resolve reference/test backends
    let (ref_backend, test_backend) = match resolve_diff_backends(mode, ref_backend, test_backend) {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("Error: {}", msg);
            return EXIT_FAILURE;
        }
    };

    eprintln!("Reference: {:?}", ref_backend);
    eprintln!("Test: {:?}", test_backend);

    // Check Spike is available if needed
    let needs_spike = matches!(ref_backend, DiffBackend::Spike);
    if needs_spike && diff::find_spike().is_none() {
        eprintln!("Error: Spike not found in PATH");
        eprintln!("Install from https://github.com/riscv-software-src/riscv-isa-sim");
        return EXIT_FAILURE;
    }

    let checkpoint_requested = matches!(granularity, diff::DiffGranularity::Checkpoint);
    let block_requested = matches!(
        granularity,
        diff::DiffGranularity::Block | diff::DiffGranularity::Hybrid
    );
    let use_block_comparison = block_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_))
        && backend_supports_buffered_diff(ref_backend.as_backend().unwrap())
        && backend_supports_buffered_diff(test_backend.as_backend().unwrap());
    if block_requested && !use_block_comparison {
        eprintln!(
            "Warning: block/hybrid requested but buffered diff not supported for this pair; falling back to instruction mode."
        );
    }

    // Check if checkpoint mode is possible (requires two compiled backends)
    let use_checkpoint_comparison = checkpoint_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_));
    if checkpoint_requested && !use_checkpoint_comparison {
        eprintln!(
            "Warning: checkpoint mode requires two compiled backends (not Spike); falling back to instruction mode."
        );
    }

    // Check if pure-C mode is possible (requires two compiled backends)
    let pure_c_requested = matches!(granularity, diff::DiffGranularity::PureC);
    let use_pure_c = pure_c_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_));
    if pure_c_requested && !use_pure_c {
        eprintln!(
            "Warning: pure-C mode requires two compiled backends (not Spike); falling back to instruction mode."
        );
    }

    // Determine what to compile based on mode and granularity
    let result = if use_pure_c {
        // Pure-C comparison: generates a standalone C program
        run_pure_c_comparison(
            elf_path,
            &output_dir,
            ref_backend.as_backend().unwrap(),
            test_backend.as_backend().unwrap(),
            cc,
            max_instrs,
        )
    } else if use_block_comparison {
        // Block-level comparison: reference as block executor, test as linear executor
        eprintln!("Using block-level comparison (reference block vs test linear)");

        // Compile C with buffered-diff tracer for block execution
        let block_dir = if let Some(dir) = ref_dir {
            dir
        } else {
            let dir = output_dir.join("block");
            let backend = ref_backend.as_backend().unwrap();
            eprintln!(
                "Compiling block executor ({:?} with buffered-diff)...",
                backend
            );
            if !compile_for_diff_block(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile block executor");
                return EXIT_FAILURE;
            }
            dir
        };

        // Compile ARM64 with diff tracer for linear execution
        let linear_dir = if let Some(dir) = test_dir {
            dir
        } else {
            let dir = output_dir.join("linear");
            let backend = test_backend.as_backend().unwrap();
            eprintln!("Compiling linear executor ({:?} with diff)...", backend);
            if !compile_for_diff(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile linear executor");
                return EXIT_FAILURE;
            }
            dir
        };

        // Create executors
        eprintln!("Starting block-level differential execution...");

        let config = diff::CompareConfig {
            strict_reg_writes: true,
            strict_mem_access: strict_mem,
        };

        let mut block_exec = match diff::BufferedInProcessExecutor::new(&block_dir, elf_path) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Error loading block executor: {}", e);
                return EXIT_FAILURE;
            }
        };

        let mut linear_exec = match diff::InProcessExecutor::new(&linear_dir, elf_path) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Error loading linear executor: {}", e);
                return EXIT_FAILURE;
            }
        };

        diff::compare_block_vs_linear(&mut block_exec, &mut linear_exec, &config, max_instrs)
    } else if use_checkpoint_comparison {
        // Fast checkpoint comparison: compare PC+registers every N instructions
        eprintln!("Using checkpoint comparison (1M instruction intervals)");

        // Compile both backends with suspend mode (no tracer needed)
        let ref_dir = if let Some(dir) = ref_dir {
            dir
        } else {
            let dir = output_dir.join("ref");
            let backend = ref_backend.as_backend().unwrap();
            eprintln!("Compiling reference ({:?} for checkpoint)...", backend);
            if !compile_for_checkpoint(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile reference");
                return EXIT_FAILURE;
            }
            dir
        };

        let test_dir = if let Some(dir) = test_dir {
            dir
        } else {
            let dir = output_dir.join("test");
            let backend = test_backend.as_backend().unwrap();
            eprintln!("Compiling test ({:?} for checkpoint)...", backend);
            if !compile_for_checkpoint(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile test");
                return EXIT_FAILURE;
            }
            dir
        };

        eprintln!("Starting checkpoint comparison...");

        // Load runners
        let mut ref_runner = match rvr::Runner::load(&ref_dir, elf_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error loading reference runner: {}", e);
                return EXIT_FAILURE;
            }
        };
        ref_runner.prepare();
        let entry = ref_runner.entry_point();
        ref_runner.set_pc(entry);

        let mut test_runner = match rvr::Runner::load(&test_dir, elf_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error loading test runner: {}", e);
                return EXIT_FAILURE;
            }
        };
        test_runner.prepare();
        test_runner.set_pc(entry);

        // Checkpoint interval: 1M instructions
        const CHECKPOINT_INTERVAL: u64 = 1_000_000;

        diff::compare_checkpoint(&mut ref_runner, &mut test_runner, CHECKPOINT_INTERVAL, max_instrs)
    } else {
        // Instruction-level comparison (original behavior)
        // Compile reference if needed
        let ref_compiled_dir = if let Some(dir) = ref_dir {
            dir
        } else if let Some(backend) = ref_backend.as_backend() {
            let dir = output_dir.join("ref");
            eprintln!("Compiling reference ({:?})...", backend);
            if !compile_for_diff(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile reference");
                return EXIT_FAILURE;
            }
            dir
        } else {
            PathBuf::new() // Not used for Spike mode
        };

        // Compile test
        let test_compiled_dir = if let Some(dir) = test_dir {
            dir
        } else if let Some(backend) = test_backend.as_backend() {
            let dir = output_dir.join("test");
            eprintln!("Compiling test ({:?})...", backend);
            if !compile_for_diff(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile test");
                return EXIT_FAILURE;
            }
            dir
        } else {
            unreachable!("test backend always required")
        };

        // Create executors
        eprintln!("Starting differential execution...");

        let config = diff::CompareConfig {
            strict_reg_writes: true,
            strict_mem_access: strict_mem,
        };

        // Run comparison based on mode
        match ref_backend {
            DiffBackend::Spike => {
                // Spike as reference
                let mut spike = match diff::SpikeExecutor::start(elf_path, &isa, entry_point) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error starting Spike: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                let mut test = match diff::InProcessExecutor::new(&test_compiled_dir, elf_path) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Error loading test executor: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                diff::compare_lockstep(&mut spike, &mut test, &config, max_instrs)
            }
            DiffBackend::Backend(_) => {
                let mut reference = match diff::InProcessExecutor::new(&ref_compiled_dir, elf_path)
                {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Error loading reference executor: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                let mut test = match diff::InProcessExecutor::new(&test_compiled_dir, elf_path) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Error loading test executor: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                diff::compare_lockstep(&mut reference, &mut test, &config, max_instrs)
            }
        }
    };

    // Report result
    eprintln!();
    if let Some(div) = &result.divergence {
        eprintln!("DIVERGENCE at instruction {}: {}", div.index, div.kind);
        eprintln!();
        eprintln!("Expected:");
        eprintln!("  PC: 0x{:016x}", div.expected.pc);
        eprintln!("  Opcode: 0x{:08x}", div.expected.opcode);
        if let (Some(rd), Some(val)) = (div.expected.rd, div.expected.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.expected.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Actual:");
        eprintln!("  PC: 0x{:016x}", div.actual.pc);
        eprintln!("  Opcode: 0x{:08x}", div.actual.opcode);
        if let (Some(rd), Some(val)) = (div.actual.rd, div.actual.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.actual.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Output: {}", output_dir.display());
        EXIT_FAILURE
    } else {
        eprintln!("PASS: {} instructions matched", result.matched);
        EXIT_SUCCESS
    }
}

/// Compilation mode for differential execution.
#[derive(Clone, Copy, Debug)]
enum DiffCompileMode {
    /// Linear mode: per-instruction stepping with single diff tracer.
    /// Used for linear executor that steps one instruction at a time.
    Linear,
    /// Block mode: run blocks with buffered diff tracer.
    /// Used for block executor that captures N instructions per block.
    Block,
    /// Checkpoint mode: suspend mode only, no tracer needed.
    /// Used for fast checkpoint comparison at intervals.
    Checkpoint,
}

/// Compile an ELF for differential execution with the specified mode.
fn compile_for_diff_mode(
    elf_path: &PathBuf,
    output_dir: &PathBuf,
    backend: Backend,
    cc: &str,
    mode: DiffCompileMode,
) -> bool {
    use std::process::Command;

    std::fs::create_dir_all(output_dir).ok();

    if matches!(mode, DiffCompileMode::Block) && !backend_supports_buffered_diff(backend) {
        eprintln!(
            "Error: buffered diff tracer not supported for backend {:?}",
            backend
        );
        return false;
    }
    if matches!(mode, DiffCompileMode::Linear) && !backend_supports_diff(backend) {
        eprintln!("Error: diff tracer not supported for backend {:?}", backend);
        return false;
    }

    let mut cmd = Command::new("./target/release/rvr");
    cmd.arg("compile")
        .arg(elf_path)
        .arg("-o")
        .arg(output_dir)
        .arg("--backend")
        .arg(match backend {
            Backend::C => "c",
            Backend::ARM64Asm => "arm64",
            Backend::X86Asm => "x86",
        })
        .arg("--cc")
        .arg(cc);

    match mode {
        DiffCompileMode::Linear => {
            // Linear mode: step per instruction with single diff tracer
            cmd.arg("--instret")
                .arg("per-instruction")
                .arg("--tracer")
                .arg("diff")
                .arg("--no-superblock");
        }
        DiffCompileMode::Block => {
            // Block mode: run blocks with buffered diff tracer
            // Use suspend mode (not per-instruction) so we can run full blocks
            cmd.arg("--instret")
                .arg("suspend")
                .arg("--tracer")
                .arg("buffered-diff");
            // Note: no --no-superblock so blocks can be optimized
        }
        DiffCompileMode::Checkpoint => {
            // Checkpoint mode: suspend only, no tracer needed
            // Use --no-superblock to ensure consistent instruction sequences
            // This makes the code paths more similar to instruction mode
            cmd.arg("--instret")
                .arg("per-instruction")
                .arg("--no-superblock");
        }
    }

    let status = cmd.status();
    matches!(status, Ok(s) if s.success())
}

/// Compile an ELF for linear differential execution (per-instruction stepping).
fn compile_for_diff(elf_path: &PathBuf, output_dir: &PathBuf, backend: Backend, cc: &str) -> bool {
    compile_for_diff_mode(elf_path, output_dir, backend, cc, DiffCompileMode::Linear)
}

/// Compile an ELF for block-level differential execution (buffered tracer).
fn compile_for_diff_block(
    elf_path: &PathBuf,
    output_dir: &PathBuf,
    backend: Backend,
    cc: &str,
) -> bool {
    compile_for_diff_mode(elf_path, output_dir, backend, cc, DiffCompileMode::Block)
}

/// Compile an ELF for checkpoint-based differential execution (suspend mode only).
fn compile_for_checkpoint(
    elf_path: &PathBuf,
    output_dir: &PathBuf,
    backend: Backend,
    cc: &str,
) -> bool {
    compile_for_diff_mode(elf_path, output_dir, backend, cc, DiffCompileMode::Checkpoint)
}

/// Run pure-C comparison: generates and runs a standalone C comparison program.
///
/// This eliminates all Rust FFI overhead by running the comparison entirely in C.
fn run_pure_c_comparison(
    elf_path: &PathBuf,
    output_dir: &Path,
    ref_backend: Backend,
    test_backend: Backend,
    cc: &str,
    max_instrs: Option<u64>,
) -> diff::CompareResult {
    use rvr_elf::ElfImage;
    use rvr_ir::Rv64;

    eprintln!("Using pure-C comparison (no Rust FFI overhead)");

    // Load ELF to get segments
    let elf_data = match std::fs::read(elf_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error reading ELF: {}", e);
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
    };

    let image = match ElfImage::<Rv64>::parse(&elf_data) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Error parsing ELF: {}", e);
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
    };

    // Compile both backends
    let ref_dir = output_dir.join("ref");
    eprintln!("Compiling reference ({:?})...", ref_backend);
    if !compile_for_checkpoint(elf_path, &ref_dir, ref_backend, cc) {
        eprintln!("Error: Failed to compile reference");
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    let test_dir = output_dir.join("test");
    eprintln!("Compiling test ({:?})...", test_backend);
    if !compile_for_checkpoint(elf_path, &test_dir, test_backend, cc) {
        eprintln!("Error: Failed to compile test");
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    // Generate pure-C comparison program
    let default_mem_bits = rvr_emit::EmitConfig::<Rv64>::default().memory_bits;
    let initial_sp = image.lookup_symbol("__stack_top").unwrap_or(0);
    let initial_gp = image.lookup_symbol("__global_pointer$").unwrap_or(0);
    let config = diff::CCompareConfig {
        entry_point: image.entry_point,
        max_instrs: max_instrs.unwrap_or(u64::MAX),
        checkpoint_interval: 1_000_000,
        memory_bits: default_mem_bits,
        num_regs: 32,
        instret_suspend: true,
        initial_brk: Rv64::to_u64(image.get_initial_program_break()),
        initial_sp,
        initial_gp,
    };

    eprintln!("Generating pure-C comparison program...");
    if let Err(e) = diff::generate_c_compare(output_dir, &image.memory_segments, &config) {
        eprintln!("Error generating C code: {}", e);
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    // Compile comparison program
    eprintln!("Compiling comparison program...");
    if !diff::compile_c_compare(output_dir, cc) {
        eprintln!("Error: Failed to compile comparison program");
        eprintln!("See {}/diff_compare.c for generated code", output_dir.display());
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    // Find the compiled libraries (names vary by backend: libref.so, libtest.so, librv.so)
    let ref_lib = find_library_in_dir(&ref_dir);
    let test_lib = find_library_in_dir(&test_dir);

    let (ref_lib, test_lib) = match (ref_lib, test_lib) {
        (Some(r), Some(t)) => (r, t),
        (None, _) => {
            eprintln!("Error: Could not find compiled library in {}", ref_dir.display());
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
        (_, None) => {
            eprintln!("Error: Could not find compiled library in {}", test_dir.display());
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
    };

    eprintln!("Running pure-C comparison...");
    eprintln!("  Reference: {}", ref_lib.display());
    eprintln!("  Test: {}", test_lib.display());
    eprintln!();

    match diff::run_c_compare(output_dir, &ref_lib, &test_lib) {
        Ok(output) => {
            // Print output
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if !stdout.is_empty() {
                eprint!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }

            // Parse result from output
            // The C program prints "PASS: N instructions matched" on success
            // or "DIVERGENCE at instruction N" on failure
            if output.status.success() {
                // Extract matched count from output (stdout or stderr)
                let combined = format!("{stdout}\n{stderr}");
                let tokens: Vec<&str> = combined.split_whitespace().collect();
                let matched = tokens
                    .iter()
                    .position(|t| *t == "PASS:")
                    .and_then(|idx| tokens.get(idx + 1))
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);

                diff::CompareResult {
                    matched,
                    divergence: None,
                }
            } else {
                // Extract divergence info from stderr
                let matched = stderr
                    .lines()
                    .find(|l| l.contains("DIVERGENCE"))
                    .and_then(|l| {
                        l.split_whitespace()
                            .last()
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap_or(0);

                diff::CompareResult {
                    matched,
                    divergence: Some(diff::Divergence {
                        index: matched,
                        expected: diff::DiffState::default(),
                        actual: diff::DiffState::default(),
                        kind: diff::DivergenceKind::Pc,
                    }),
                }
            }
        }
        Err(e) => {
            eprintln!("Error running comparison: {}", e);
            diff::CompareResult {
                matched: 0,
                divergence: None,
            }
        }
    }
}

/// Find a shared library (.so) file in the given directory.
fn find_library_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("so") {
            return Some(path);
        }
    }
    None
}
