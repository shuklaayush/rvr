//! Benchmark commands and registry.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::{self, Arch};
use rvr::polkavm;
use rvr::{CompileOptions, Compiler, InstretMode};

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::commands::build::build_rust_project;
use crate::terminal::{self, Spinner};

/// Helper to run a command silently and return success/failure.
fn run_silent(cmd: &mut Command) -> bool {
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Find the project root directory (git root or cwd).
fn find_project_root() -> PathBuf {
    // Try to find git root first
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                return PathBuf::from(path.trim());
            }
        }
    }
    // Fall back to current directory
    std::env::current_dir().expect("failed to get current directory")
}

/// Build a riscv-tests benchmark using riscv-gcc.
/// Returns the path to the built ELF on success.
fn build_riscv_tests_benchmark(
    project_dir: &std::path::Path,
    name: &str,
    arch: &bench::Arch,
) -> Result<PathBuf, String> {
    // Find riscv-gcc toolchain (reuse from tests module)
    let toolchain = rvr::tests::find_toolchain()
        .ok_or_else(|| "RISC-V toolchain not found (need riscv64-unknown-elf-gcc)".to_string())?;

    let gcc = format!("{}gcc", toolchain);
    let bench_dir = project_dir.join("programs/riscv-tests/benchmarks");
    let common_dir = bench_dir.join("common");
    let out_dir = project_dir.join("bin").join(arch.as_str());

    // Check source exists
    let src_dir = bench_dir.join(name);
    if !src_dir.exists() {
        return Err(format!("benchmark source not found: {}", src_dir.display()));
    }

    // Create output directory
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create output dir: {}", e))?;

    // Find all C source files in the benchmark directory
    let mut c_files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&src_dir).map_err(|e| format!("failed to read dir: {}", e))? {
        let entry = entry.map_err(|e| format!("failed to read entry: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("c") {
            c_files.push(path);
        }
    }

    if c_files.is_empty() {
        return Err(format!("no C files found in {}", src_dir.display()));
    }

    let out_path = out_dir.join(name);
    // Use original riscv-tests files from common/
    // Note: test.ld has a bug where it uses section flags (SHF_*) instead of
    // program header flags (PF_*), so the ELF won't have PF_X set. rvr handles
    // this by falling back to checking section flags (SHF_EXECINSTR).
    let link_ld = common_dir.join("test.ld");
    let crt_s = common_dir.join("crt.S");
    let syscalls_c = common_dir.join("syscalls.c");
    let env_dir = project_dir.join("programs/riscv-tests/env");

    // Select march/mabi based on target architecture
    // Use im_zicsr (no FPU) since rvr doesn't support F/D extensions yet
    let (march, mabi) = match arch {
        bench::Arch::Rv32i | bench::Arch::Rv32e => ("rv32im_zicsr", "ilp32"),
        bench::Arch::Rv64i | bench::Arch::Rv64e => ("rv64im_zicsr", "lp64"),
    };

    // Build command: compile with original CRT and syscalls from common/
    let mut cmd = Command::new(&gcc);
    cmd.arg(format!("-march={}", march))
        .arg(format!("-mabi={}", mabi))
        .args(["-static", "-mcmodel=medany", "-fvisibility=hidden"])
        .args(["-nostdlib", "-nostartfiles"])
        .args(["-std=gnu99", "-O2", "-ffast-math", "-fno-common", "-fno-builtin-printf"])
        .args(["-fno-tree-loop-distribute-patterns"])
        .args(["-Wno-implicit-function-declaration", "-Wno-implicit-int"])
        .arg("-DPREALLOCATE=1")
        .arg(format!("-I{}", common_dir.display()))
        .arg(format!("-I{}", env_dir.display()))
        .arg(format!("-T{}", link_ld.display()))
        .arg(&crt_s)
        .arg(&syscalls_c);

    for f in &c_files {
        cmd.arg(f);
    }

    // Add libgcc for 64-bit operations on rv32
    cmd.arg("-lgcc");

    cmd.arg("-o").arg(&out_path);

    let status = cmd
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| format!("failed to run gcc: {}", e))?;

    if !status.success() {
        return Err(format!("gcc failed with exit code {:?}", status.code()));
    }

    Ok(out_path)
}

// ============================================================================
// Benchmark registry
// ============================================================================

/// How to build a benchmark.
#[derive(Clone, Copy)]
pub enum BenchmarkSource {
    /// Rust project - build with `rvr build`
    Rust {
        /// Path to project directory (relative to repo root)
        path: &'static str,
    },
    /// Polkavm benchmark - build with benchmarks/build.sh
    Polkavm,
    /// C benchmark from riscv-tests - build with riscv-gcc
    RiscvTests,
}

/// Benchmark metadata.
pub struct BenchmarkInfo {
    /// Benchmark name (used in CLI and paths).
    pub name: &'static str,
    /// Short description.
    pub description: &'static str,
    /// Whether benchmark uses export_functions mode (initialize/run pattern).
    /// If false, runs from ELF entry point.
    pub uses_exports: bool,
    /// Path to host binary relative to project root (for comparison).
    /// None if no host binary available.
    pub host_binary: Option<&'static str>,
    /// Default architectures for this benchmark.
    pub default_archs: &'static str,
    /// How to build this benchmark.
    pub source: BenchmarkSource,
}

/// All registered benchmarks.
/// Ordered: riscv-tests first, then polkavm, then rust (reth).
/// ELF binaries are at: bin/{arch}/{name}
const BENCHMARKS: &[BenchmarkInfo] = &[
    // riscv-tests benchmarks (C-based)
    BenchmarkInfo {
        name: "towers",
        description: "Towers of Hanoi (recursive)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "qsort",
        description: "Quick sort algorithm",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "rsort",
        description: "Radix sort algorithm",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "median",
        description: "Median filter",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "multiply",
        description: "Software multiply",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "vvadd",
        description: "Vector-vector addition",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "memcpy",
        description: "Memory copy operations",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "dhrystone",
        description: "Classic Dhrystone benchmark",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    // Note: spmv fails verification due to FP precision, mm needs threading
    // polkavm benchmarks
    BenchmarkInfo {
        name: "minimal",
        description: "Minimal function call overhead",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "prime-sieve",
        description: "Prime number sieve algorithm",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "pinky",
        description: "NES emulator (cycle-accurate)",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "memset",
        description: "Memory set operations",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    // rust benchmarks
    BenchmarkInfo {
        name: "reth",
        description: "Reth block validator",
        uses_exports: false,
        host_binary: Some("programs/reth/target/release/reth-validator"),
        default_archs: "rv64i",
        source: BenchmarkSource::Rust {
            path: "programs/reth",
        },
    },
];

/// Find benchmark by name.
fn find_benchmark(name: &str) -> Option<&'static BenchmarkInfo> {
    BENCHMARKS.iter().find(|b| b.name == name)
}

// ============================================================================
// Benchmark commands
// ============================================================================

/// List available benchmarks.
pub fn bench_list() {
    println!("Available benchmarks:");
    println!();
    for b in BENCHMARKS {
        let mut markers = Vec::new();
        match b.source {
            BenchmarkSource::Rust { .. } => markers.push("rust"),
            BenchmarkSource::Polkavm => markers.push("polkavm"),
            BenchmarkSource::RiscvTests => markers.push("riscv-tests"),
        }
        if b.host_binary.is_some() {
            markers.push("has host");
        }
        let marker_str = if markers.is_empty() {
            String::new()
        } else {
            format!(" [{}]", markers.join(", "))
        };
        println!("  {:<20} {}{}", b.name, b.description, marker_str);
    }
    println!();
    println!("Commands:");
    println!("  rvr bench build [name]     Build ELF from source");
    println!("  rvr bench compile [name]   Compile ELF to native .so");
    println!("  rvr bench run [name]       Run benchmark");
    println!();
    println!("Omit [name] to operate on all benchmarks.");
}

/// Build benchmark ELF from source.
pub fn bench_build(name: Option<&str>, arch: Option<&str>, no_host: bool) -> i32 {
    let project_dir = find_project_root();

    // Determine which benchmarks to build
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                terminal::error(&format!("Unknown benchmark '{}'", n));
                terminal::info("Run 'rvr bench list' to see available benchmarks");
                return EXIT_FAILURE;
            }
        },
        None => BENCHMARKS.iter().collect(),
    };

    for benchmark in &benchmarks {
        // Determine architectures
        let arch_str = arch.unwrap_or(benchmark.default_archs);

        match benchmark.source {
            BenchmarkSource::Rust { path } => {
                // Build host binary first (unless --no-host)
                if !no_host && benchmark.host_binary.is_some() {
                    let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
                    let mut cmd = Command::new("cargo");
                    cmd.arg("build")
                        .arg("--release")
                        .arg("--manifest-path")
                        .arg(project_dir.join(path).join("Cargo.toml"));

                    if run_silent(&mut cmd) {
                        spinner.finish_with_success(&format!("{} (host)", benchmark.name));
                    } else {
                        spinner.finish_with_failure(&format!(
                            "{} (host) build failed",
                            benchmark.name
                        ));
                        return EXIT_FAILURE;
                    }
                }

                // Build RISC-V ELFs using rvr build
                let archs = match Arch::parse_list(arch_str) {
                    Ok(a) => a,
                    Err(e) => {
                        terminal::error(&e);
                        return EXIT_FAILURE;
                    }
                };

                for a in &archs {
                    let spinner =
                        Spinner::new(format!("Building {} ({})", benchmark.name, a.as_str()));
                    let project_path = project_dir.join(path);
                    let result = build_rust_project(
                        &project_path,
                        a.as_str(),
                        None,
                        Some(benchmark.name),
                        "nightly",
                        None,
                        true,
                        false,
                        true, // quiet mode
                    );

                    if result == EXIT_SUCCESS {
                        let output_path = project_dir
                            .join("bin")
                            .join(a.as_str())
                            .join(benchmark.name);
                        spinner.finish_with_success(&format!(
                            "{} ({}) → {}",
                            benchmark.name,
                            a.as_str(),
                            output_path.display()
                        ));
                    } else {
                        spinner.finish_with_failure(&format!(
                            "{} ({}) build failed",
                            benchmark.name,
                            a.as_str()
                        ));
                        return result;
                    }
                }
            }
            BenchmarkSource::Polkavm => {
                // Build for each architecture
                let archs = match Arch::parse_list(arch_str) {
                    Ok(a) => a,
                    Err(e) => {
                        terminal::error(&e);
                        return EXIT_FAILURE;
                    }
                };

                for a in archs {
                    let spinner = Spinner::new(format!(
                        "Building {} ({}, polkavm)",
                        benchmark.name,
                        a.as_str()
                    ));
                    match polkavm::build_benchmark(&project_dir, benchmark.name, a.as_str()) {
                        Ok(path) => {
                            spinner.finish_with_success(&format!(
                                "{} ({}) → {}",
                                benchmark.name,
                                a.as_str(),
                                path.display()
                            ));
                        }
                        Err(e) => {
                            spinner.finish_with_failure(&format!(
                                "{} ({}): {}",
                                benchmark.name,
                                a.as_str(),
                                e
                            ));
                            return EXIT_FAILURE;
                        }
                    }
                }
            }
            BenchmarkSource::RiscvTests => {
                // Build C benchmark for each architecture using riscv-gcc
                let archs = match Arch::parse_list(arch_str) {
                    Ok(a) => a,
                    Err(e) => {
                        terminal::error(&e);
                        return EXIT_FAILURE;
                    }
                };

                for a in archs {
                    let spinner = Spinner::new(format!(
                        "Building {} ({}, riscv-tests)",
                        benchmark.name,
                        a.as_str()
                    ));
                    match build_riscv_tests_benchmark(&project_dir, benchmark.name, &a) {
                        Ok(path) => {
                            spinner.finish_with_success(&format!(
                                "{} ({}) → {}",
                                benchmark.name,
                                a.as_str(),
                                path.display()
                            ));
                        }
                        Err(e) => {
                            spinner.finish_with_failure(&format!(
                                "{} ({}): {}",
                                benchmark.name,
                                a.as_str(),
                                e
                            ));
                            return EXIT_FAILURE;
                        }
                    }
                }
            }
        }
    }

    terminal::success("Build complete");
    EXIT_SUCCESS
}

/// Compile benchmark ELF to native .so.
pub fn bench_compile(
    name: Option<&str>,
    arch: Option<&str>,
    fast: bool,
    cc: &str,
    linker: Option<&str>,
) -> i32 {
    let project_dir = find_project_root();

    // Determine which benchmarks to compile
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                terminal::error(&format!("Unknown benchmark '{}'", n));
                return EXIT_FAILURE;
            }
        },
        None => BENCHMARKS.iter().collect(),
    };

    let suffix = if fast { "fast" } else { "base" };

    let mut compiler: Compiler = cc.parse().unwrap_or_else(|e| {
        terminal::error(&format!("{}", e));
        std::process::exit(EXIT_FAILURE);
    });
    if let Some(ld) = linker {
        compiler = compiler.with_linker(ld);
    }

    for benchmark in &benchmarks {
        let arch_str = arch.unwrap_or(benchmark.default_archs);
        let archs = match Arch::parse_list(arch_str) {
            Ok(a) => a,
            Err(e) => {
                terminal::error(&e);
                return EXIT_FAILURE;
            }
        };

        for a in &archs {
            let elf_path = project_dir
                .join("bin")
                .join(a.as_str())
                .join(benchmark.name);

            if !elf_path.exists() {
                terminal::warning(&format!("{} not found, skipping", elf_path.display()));
                continue;
            }

            let out_dir = project_dir
                .join("target/benchmarks")
                .join(benchmark.name)
                .join(a.as_str())
                .join(suffix);

            let spinner = Spinner::new(format!("Compiling {} ({})", benchmark.name, a.as_str()));

            let mut options = CompileOptions::new()
                .with_compiler(compiler.clone())
                .with_export_functions(benchmark.uses_exports)
                .with_quiet(true);
            if fast {
                options = options.with_instret_mode(InstretMode::Off);
            }
            // Enable HTIF for riscv-tests benchmarks
            if matches!(benchmark.source, BenchmarkSource::RiscvTests) {
                options = options.with_htif(true);
            }

            if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
                spinner.finish_with_failure(&format!("compile failed: {}", e));
                return EXIT_FAILURE;
            }
            spinner.finish_with_success(&format!(
                "{} ({}) → {}",
                benchmark.name,
                a.as_str(),
                out_dir.display()
            ));
        }
    }

    terminal::success("Compile complete");
    EXIT_SUCCESS
}

/// Run compiled benchmark.
pub fn bench_run(
    name: Option<&str>,
    arch: Option<&str>,
    runs: usize,
    fast: bool,
    compare_host: bool,
    force: bool,
) -> i32 {
    let project_dir = find_project_root();

    // Determine which benchmarks to run
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                terminal::error(&format!("Unknown benchmark '{}'", n));
                return EXIT_FAILURE;
            }
        },
        None => BENCHMARKS.iter().collect(),
    };

    let suffix = if fast { "fast" } else { "base" };

    // Default compiler for auto-compilation
    let compiler: Compiler = "clang".parse().unwrap();

    for benchmark in &benchmarks {
        let arch_str = arch.unwrap_or(benchmark.default_archs);
        let archs = match Arch::parse_list(arch_str) {
            Ok(a) => a,
            Err(e) => {
                terminal::error(&e);
                return EXIT_FAILURE;
            }
        };

        // Collect all rows first, then sort by overhead/time
        let mut rows: Vec<bench::TableRow> = Vec::new();

        // Run host baseline if requested and available
        let mut host_time: Option<f64> = None;
        if compare_host {
            match &benchmark.source {
                BenchmarkSource::Rust { .. } => {
                    // Use prebuilt host binary if available
                    if let Some(host_path) = benchmark.host_binary {
                        let host_bin = project_dir.join(host_path);
                        if host_bin.exists() {
                            let spinner =
                                Spinner::new(format!("Running {} (host)", benchmark.name));
                            match bench::run_host(&host_bin, runs) {
                                Ok(result) => {
                                    spinner.finish_and_clear();
                                    tracing::debug!("host run complete");
                                    host_time = result.time_secs;
                                    rows.push(bench::TableRow::host("host", &result));
                                }
                                Err(e) => {
                                    spinner.finish_with_failure(&e);
                                    rows.push(bench::TableRow::error("host", e));
                                }
                            }
                        }
                    }
                }
                BenchmarkSource::Polkavm => {
                    // Build and run polkavm benchmark on host
                    let host_lib = project_dir
                        .join("bin/host")
                        .join(format!("{}.so", benchmark.name));
                    if !host_lib.exists() {
                        let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
                        if let Err(e) = polkavm::build_host_benchmark(&project_dir, benchmark.name)
                        {
                            spinner.finish_with_failure(&format!("build failed: {}", e));
                            rows.push(bench::TableRow::error("host", "build failed".to_string()));
                        } else {
                            spinner.finish_and_clear();
                            tracing::debug!("host build complete");
                        }
                    }
                    if host_lib.exists() {
                        let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
                        match polkavm::run_host_benchmark(&host_lib, runs) {
                            Ok(result) => {
                                spinner.finish_and_clear();
                                tracing::debug!("host run complete");
                                host_time = Some(result.time_secs);
                                // Create a HostResult-like row
                                let host_result = bench::HostResult {
                                    time_secs: Some(result.time_secs),
                                    perf: None,
                                };
                                rows.push(bench::TableRow::host("host", &host_result));
                            }
                            Err(e) => {
                                spinner.finish_with_failure(&e);
                                rows.push(bench::TableRow::error("host", e));
                            }
                        }
                    }
                }
                BenchmarkSource::RiscvTests => {
                    // No host comparison for riscv-tests benchmarks
                }
            }
        }

        // Run each architecture
        for a in &archs {
            if let Some(row) = run_single_arch(
                benchmark,
                a,
                &project_dir,
                suffix,
                fast,
                runs,
                &compiler,
                host_time,
                force,
            ) {
                rows.push(row);
            }
        }

        // Sort by overhead (ascending), then by time. Errors go last.
        rows.sort_by(|a, b| {
            match (&a.error, &b.error) {
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), Some(_)) => std::cmp::Ordering::Equal,
                (None, None) => {
                    // Sort by overhead if available, otherwise by time
                    let a_key = a.overhead.or(a.time_secs.map(|t| t * 1000.0));
                    let b_key = b.overhead.or(b.time_secs.map(|t| t * 1000.0));
                    a_key
                        .partial_cmp(&b_key)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            }
        });

        // Print header and sorted rows
        bench::print_bench_header(benchmark.name, benchmark.description, runs);
        for row in &rows {
            bench::print_table_row(row);
        }

        println!();
    }

    EXIT_SUCCESS
}

/// Check if the library needs recompilation (ELF newer than .so).
fn needs_recompile(elf_path: &std::path::Path, lib_path: &std::path::Path) -> bool {
    if !lib_path.exists() {
        return true;
    }
    // Check if ELF is newer than the compiled library
    let elf_time = elf_path.metadata().and_then(|m| m.modified()).ok();
    let lib_time = lib_path.metadata().and_then(|m| m.modified()).ok();
    match (elf_time, lib_time) {
        (Some(elf), Some(lib)) => elf > lib,
        _ => true, // If we can't determine, recompile to be safe
    }
}

/// Run benchmark for a single architecture, returning a table row.
fn run_single_arch(
    benchmark: &BenchmarkInfo,
    arch: &Arch,
    project_dir: &std::path::Path,
    suffix: &str,
    fast: bool,
    runs: usize,
    compiler: &Compiler,
    host_time: Option<f64>,
    force: bool,
) -> Option<bench::TableRow> {
    let elf_path = project_dir
        .join("bin")
        .join(arch.as_str())
        .join(benchmark.name);
    let out_dir = project_dir
        .join("target/benchmarks")
        .join(benchmark.name)
        .join(arch.as_str())
        .join(suffix);

    let backend_name = format!("rvr-{}", arch.as_str());

    // Check if ELF exists, try to build if missing
    if !elf_path.exists() {
        match &benchmark.source {
            BenchmarkSource::Rust { path } => {
                // Auto-build for Rust sources
                let spinner =
                    Spinner::new(format!("Building {} ({})", benchmark.name, arch.as_str()));
                let project_path = project_dir.join(path);
                let result = build_rust_project(
                    &project_path,
                    arch.as_str(),
                    None,
                    Some(benchmark.name),
                    "nightly",
                    None,
                    true,
                    false,
                    true, // quiet mode - spinner handles output
                );
                if result != EXIT_SUCCESS {
                    spinner.finish_with_failure("build failed");
                    return Some(bench::TableRow::error(
                        &backend_name,
                        "build failed".to_string(),
                    ));
                }
                spinner.finish_and_clear();
                tracing::debug!(arch = arch.as_str(), "build complete");
            }
            BenchmarkSource::Polkavm => {
                // Auto-build using polkavm module
                let spinner =
                    Spinner::new(format!("Building {} ({})", benchmark.name, arch.as_str()));
                if let Err(e) = polkavm::build_benchmark(project_dir, benchmark.name, arch.as_str())
                {
                    spinner.finish_with_failure(&format!("build failed: {}", e));
                    return Some(bench::TableRow::error(
                        &backend_name,
                        "build failed".to_string(),
                    ));
                }
                spinner.finish_and_clear();
                tracing::debug!(arch = arch.as_str(), "build complete");
            }
            BenchmarkSource::RiscvTests => {
                // Auto-build using riscv-gcc
                let spinner =
                    Spinner::new(format!("Building {} ({}, riscv-tests)", benchmark.name, arch.as_str()));
                if let Err(e) = build_riscv_tests_benchmark(project_dir, benchmark.name, arch) {
                    spinner.finish_with_failure(&format!("build failed: {}", e));
                    return Some(bench::TableRow::error(
                        &backend_name,
                        "build failed".to_string(),
                    ));
                }
                spinner.finish_and_clear();
                tracing::debug!(name = benchmark.name, arch = arch.as_str(), "riscv-tests build complete");
            }
        }
    }

    // Check if .so exists and is up-to-date, compile if missing/stale/forced
    let lib_path = out_dir.join(format!("lib{}.so", suffix));
    let should_compile = force || needs_recompile(&elf_path, &lib_path);

    if should_compile {
        // Delete old output directory if forcing recompile
        if force && out_dir.exists() {
            let _ = std::fs::remove_dir_all(&out_dir);
        }

        let spinner = Spinner::new(format!("Compiling {} ({})", benchmark.name, arch.as_str()));
        let mut options = CompileOptions::new()
            .with_compiler(compiler.clone())
            .with_export_functions(benchmark.uses_exports)
            .with_quiet(true);
        if fast {
            options = options.with_instret_mode(InstretMode::Off);
        }
        // Enable HTIF for riscv-tests benchmarks
        if matches!(benchmark.source, BenchmarkSource::RiscvTests) {
            options = options.with_htif(true);
        }

        if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
            spinner.finish_with_failure(&format!("compile failed: {}", e));
            return Some(bench::TableRow::error(
                &backend_name,
                format!("compile failed: {}", e),
            ));
        }
        spinner.finish_and_clear();
        tracing::debug!(arch = arch.as_str(), "compile complete");
    }

    // Run the benchmark
    let spinner = Spinner::new(format!("Running {} ({})", benchmark.name, arch.as_str()));
    match bench::run_bench_auto(&out_dir, &elf_path, runs) {
        Ok((result, _mode)) => {
            spinner.finish_and_clear();
            tracing::debug!(arch = arch.as_str(), "run complete");
            Some(bench::TableRow::backend(&backend_name, &result, host_time))
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some(bench::TableRow::error(&backend_name, e))
        }
    }
}
