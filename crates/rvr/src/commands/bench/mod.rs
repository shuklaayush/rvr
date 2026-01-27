//! Benchmark commands and registry.

mod coremark;
mod libriscv;
mod polkavm;
mod report;
mod riscv_tests;

pub use report::bench_report;

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::{self, Arch};
use rvr::{CompileOptions, Compiler, InstretMode, SyscallMode};

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::commands::build::build_rust_project;
use crate::terminal::{self, Spinner};

// ============================================================================
// Helpers
// ============================================================================

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
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        && output.status.success()
        && let Ok(path) = String::from_utf8(output.stdout)
    {
        return PathBuf::from(path.trim());
    }
    std::env::current_dir().expect("failed to get current directory")
}

/// Result from running libriscv emulator.
pub struct LibriscvResult {
    pub time_secs: f64,
    pub perf: Option<rvr::PerfCounters>,
}

/// Build the libriscv emulator if it doesn't exist.
fn build_libriscv_emulator(project_dir: &std::path::Path) -> Result<PathBuf, String> {
    let emulator_dir = project_dir.join("programs/libriscv/emulator");
    let emulator = emulator_dir.join(".build/rvlinux");

    if emulator.exists() {
        return Ok(emulator);
    }

    if !emulator_dir.exists() {
        return Err(format!(
            "libriscv emulator directory not found: {}",
            emulator_dir.display()
        ));
    }

    let build_script = emulator_dir.join("build.sh");
    if !build_script.exists() {
        return Err(format!(
            "libriscv build script not found: {}",
            build_script.display()
        ));
    }

    let spinner = Spinner::new("Building libriscv emulator".to_string());

    let output = Command::new("bash")
        .arg("build.sh")
        .arg("--bintr")
        .current_dir(&emulator_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run build.sh: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        spinner.finish_with_failure("build failed");
        return Err(format!("libriscv build failed: {}", stderr));
    }

    if !emulator.exists() {
        spinner.finish_with_failure("emulator not found after build");
        return Err("libriscv emulator not found after build".to_string());
    }

    spinner.finish_with_success("libriscv emulator built");
    Ok(emulator)
}

/// Run a benchmark using the libriscv emulator.
/// Returns the runtime parsed from libriscv output plus perf counters.
fn run_libriscv_benchmark(
    project_dir: &std::path::Path,
    elf_path: &std::path::Path,
    runs: usize,
) -> Result<LibriscvResult, String> {
    use rvr::perf::HostPerfCounters;
    use std::time::Instant;

    let emulator = build_libriscv_emulator(project_dir)?;

    let runs = runs.max(1);
    let mut perf_counters = HostPerfCounters::new();
    let mut total_time = 0.0;
    let mut total_cycles = 0u64;
    let mut total_instructions = 0u64;
    let mut total_branches = 0u64;
    let mut total_branch_misses = 0u64;

    // Get initial snapshot for delta tracking
    let mut prev_snapshot = perf_counters.as_mut().map(|c| c.read()).unwrap_or_default();

    for _ in 0..runs {
        let start = Instant::now();
        if let Some(ref mut counters) = perf_counters {
            let _ = counters.enable();
        }

        let output = Command::new(&emulator)
            .args(["-f", "0"]) // No instruction limit
            .arg(elf_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("failed to run libriscv: {}", e))?;

        if let Some(ref mut counters) = perf_counters {
            let _ = counters.disable();
        }
        let elapsed = start.elapsed().as_secs_f64();
        total_time += elapsed;

        // Read delta since last snapshot
        if let Some(ref mut counters) = perf_counters {
            let delta = counters.read_delta(&prev_snapshot);
            total_cycles += delta.cycles.unwrap_or(0);
            total_instructions += delta.instructions.unwrap_or(0);
            total_branches += delta.branches.unwrap_or(0);
            total_branch_misses += delta.branch_misses.unwrap_or(0);
            prev_snapshot = counters.read();
        }

        // On first run, verify we can parse the output
        if runs == 1 || total_time == elapsed {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}{}", stdout, stderr);

            let mut found_runtime = false;
            for line in combined.lines() {
                if line.starts_with("Runtime:") {
                    found_runtime = true;
                    break;
                }
            }
            if !found_runtime {
                return Err(format!(
                    "could not find Runtime in libriscv output:\n{}",
                    combined
                ));
            }
        }
    }

    let avg_time = total_time / runs as f64;
    let perf = perf_counters.map(|_| rvr::PerfCounters {
        cycles: Some(total_cycles / runs as u64),
        instructions: Some(total_instructions / runs as u64),
        branches: Some(total_branches / runs as u64),
        branch_misses: Some(total_branch_misses / runs as u64),
    });

    Ok(LibriscvResult {
        time_secs: avg_time,
        perf,
    })
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
    /// C benchmark from libriscv - build with riscv-gcc using riscv-tests runtime
    Libriscv,
    /// CoreMark benchmark from EEMBC
    Coremark,
}

/// Benchmark metadata.
pub struct BenchmarkInfo {
    /// Benchmark name (used in CLI and paths).
    pub name: &'static str,
    /// Short description.
    pub description: &'static str,
    /// Whether benchmark uses export_functions mode (initialize/run pattern).
    pub uses_exports: bool,
    /// Path to host binary relative to project root (for comparison).
    pub host_binary: Option<&'static str>,
    /// Default architectures for this benchmark.
    pub default_archs: &'static str,
    /// Supported architectures (None = all, Some = only those listed).
    pub supported_archs: Option<&'static str>,
    /// Whether a host version can be built.
    pub has_host: bool,
    /// How to build this benchmark.
    pub source: BenchmarkSource,
}

/// All registered benchmarks.
/// ELF binaries are at: bin/{arch}/{name}
const BENCHMARKS: &[BenchmarkInfo] = &[
    // riscv-tests benchmarks (C-based)
    BenchmarkInfo {
        name: "towers",
        description: "Towers of Hanoi (recursive)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "qsort",
        description: "Quick sort algorithm",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "rsort",
        description: "Radix sort algorithm",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "median",
        description: "Median filter",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "multiply",
        description: "Software multiply",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "vvadd",
        description: "Vector-vector addition",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "memcpy",
        description: "Memory copy operations",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "dhrystone",
        description: "Classic Dhrystone benchmark",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    // libriscv benchmarks (use Linux syscalls, not HTIF)
    BenchmarkInfo {
        name: "fib",
        description: "Fibonacci (recursive tail-call)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Libriscv,
    },
    BenchmarkInfo {
        name: "fib-asm",
        description: "Fibonacci (hand-written assembly)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: Some("rv64i,rv64e"), // RV64 assembly only
        has_host: false,                      // Can't compile assembly for host
        source: BenchmarkSource::Libriscv,
    },
    // coremark benchmark
    BenchmarkInfo {
        name: "coremark",
        description: "CoreMark CPU benchmark (EEMBC)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Coremark,
    },
    // polkavm benchmarks
    BenchmarkInfo {
        name: "minimal",
        description: "Minimal function call overhead",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "prime-sieve",
        description: "Prime number sieve algorithm",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "pinky",
        description: "NES emulator (cycle-accurate)",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "memset",
        description: "Memory set operations",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    // rust benchmarks
    BenchmarkInfo {
        name: "reth",
        description: "Reth block validator",
        uses_exports: false,
        host_binary: Some("programs/reth/target/release/reth"),
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Rust {
            path: "programs/reth",
        },
    },
];

impl BenchmarkInfo {
    /// Check if this benchmark supports the given architecture.
    fn supports_arch(&self, arch: &Arch) -> bool {
        match self.supported_archs {
            None => true,
            Some(archs) => Arch::parse_list(archs)
                .map(|list| list.contains(arch))
                .unwrap_or(false),
        }
    }
}

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
            BenchmarkSource::Libriscv => markers.push("libriscv"),
            BenchmarkSource::Coremark => markers.push("coremark"),
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
        let arch_str = arch.unwrap_or(benchmark.default_archs);

        match benchmark.source {
            BenchmarkSource::Rust { path } => {
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
                        true,
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
                    match riscv_tests::build_benchmark(&project_dir, benchmark.name, &a) {
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
            BenchmarkSource::Libriscv => {
                let archs = match Arch::parse_list(arch_str) {
                    Ok(a) => a,
                    Err(e) => {
                        terminal::error(&e);
                        return EXIT_FAILURE;
                    }
                };

                for a in archs {
                    // Skip unsupported architectures
                    if !benchmark.supports_arch(&a) {
                        terminal::warning(&format!(
                            "{} does not support {}, skipping",
                            benchmark.name,
                            a.as_str()
                        ));
                        continue;
                    }
                    let spinner = Spinner::new(format!(
                        "Building {} ({}, libriscv)",
                        benchmark.name,
                        a.as_str()
                    ));
                    match libriscv::build_benchmark(&project_dir, benchmark.name, &a) {
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
            BenchmarkSource::Coremark => {
                let archs = match Arch::parse_list(arch_str) {
                    Ok(a) => a,
                    Err(e) => {
                        terminal::error(&e);
                        return EXIT_FAILURE;
                    }
                };

                for a in archs {
                    if !benchmark.supports_arch(&a) {
                        terminal::warning(&format!(
                            "{} does not support {}, skipping",
                            benchmark.name,
                            a.as_str()
                        ));
                        continue;
                    }
                    let spinner = Spinner::new(format!(
                        "Building {} ({}, coremark)",
                        benchmark.name,
                        a.as_str()
                    ));
                    match coremark::build_benchmark(&project_dir, &a) {
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

    let mut compiler: Compiler = cc.parse().unwrap_or_else(|e: String| {
        terminal::error(&e);
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
            match &benchmark.source {
                BenchmarkSource::RiscvTests => {
                    options = options.with_htif(true);
                }
                BenchmarkSource::Libriscv => {
                    options = options.with_syscall_mode(SyscallMode::Linux);
                }
                _ => {}
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
#[allow(clippy::too_many_arguments)]
pub fn bench_run(
    name: Option<&str>,
    arch: Option<&str>,
    runs: usize,
    fast: bool,
    compare_host: bool,
    compare_libriscv: bool,
    force: bool,
    cc: &str,
    linker: Option<&str>,
) -> i32 {
    let project_dir = find_project_root();

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
    let mut compiler: Compiler = cc.parse().unwrap_or_else(|e: String| {
        terminal::error(&format!("invalid compiler: {}", e));
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

        let mut rows: Vec<bench::TableRow> = Vec::new();
        let mut host_time: Option<f64> = None;

        if compare_host {
            match &benchmark.source {
                BenchmarkSource::Rust { path } => {
                    if let Some(host_path) = benchmark.host_binary {
                        let host_bin = project_dir.join(host_path);
                        // Auto-build host binary if it doesn't exist
                        if !host_bin.exists() || force {
                            let spinner =
                                Spinner::new(format!("Building {} (host)", benchmark.name));
                            let mut cmd = Command::new("cargo");
                            cmd.arg("build")
                                .arg("--release")
                                .arg("--manifest-path")
                                .arg(project_dir.join(path).join("Cargo.toml"));

                            if run_silent(&mut cmd) {
                                spinner.finish_and_clear();
                            } else {
                                spinner.finish_with_failure(&format!(
                                    "{} (host) build failed",
                                    benchmark.name
                                ));
                                rows.push(bench::TableRow::error(
                                    "host",
                                    "build failed".to_string(),
                                ));
                            }
                        }
                        if host_bin.exists() {
                            let spinner =
                                Spinner::new(format!("Running {} (host)", benchmark.name));
                            match bench::run_host(&host_bin, runs) {
                                Ok(result) => {
                                    spinner.finish_and_clear();
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
                        }
                    }
                    if host_lib.exists() {
                        let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
                        match polkavm::run_host_benchmark(&host_lib, runs) {
                            Ok(result) => {
                                spinner.finish_and_clear();
                                host_time = Some(result.time_secs);
                                let host_result = bench::HostResult {
                                    time_secs: Some(result.time_secs),
                                    perf: result.perf,
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
                    let host_bin = project_dir.join("bin/host").join(benchmark.name);
                    if !host_bin.exists() || force {
                        let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
                        if let Err(e) =
                            riscv_tests::build_host_benchmark(&project_dir, benchmark.name)
                        {
                            spinner.finish_with_failure(&format!("build failed: {}", e));
                            rows.push(bench::TableRow::error("host", "build failed".to_string()));
                        } else {
                            spinner.finish_and_clear();
                        }
                    }
                    if host_bin.exists() {
                        let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
                        match riscv_tests::run_host_benchmark(&host_bin, runs) {
                            Ok(result) => {
                                spinner.finish_and_clear();
                                host_time = Some(result.time_secs);
                                let host_result = bench::HostResult {
                                    time_secs: Some(result.time_secs),
                                    perf: result.perf,
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
                BenchmarkSource::Libriscv => {
                    if !benchmark.has_host {
                        terminal::info(&format!(
                            "{} has no host version (e.g., RISC-V assembly only)",
                            benchmark.name
                        ));
                    } else {
                        let host_lib = project_dir
                            .join("bin/host")
                            .join(format!("{}.so", benchmark.name));
                        if !host_lib.exists() || force {
                            let spinner =
                                Spinner::new(format!("Building {} (host)", benchmark.name));
                            if let Err(e) =
                                libriscv::build_host_benchmark(&project_dir, benchmark.name)
                            {
                                spinner.finish_with_failure(&format!("build failed: {}", e));
                                rows.push(bench::TableRow::error(
                                    "host",
                                    "build failed".to_string(),
                                ));
                            } else {
                                spinner.finish_and_clear();
                            }
                        }
                        if host_lib.exists() {
                            let spinner =
                                Spinner::new(format!("Running {} (host)", benchmark.name));
                            match libriscv::run_host_benchmark(&host_lib, runs) {
                                Ok(result) => {
                                    spinner.finish_and_clear();
                                    host_time = Some(result.time_secs);
                                    let host_result = bench::HostResult {
                                        time_secs: Some(result.time_secs),
                                        perf: result.perf,
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
                }
                BenchmarkSource::Coremark => {
                    if !benchmark.has_host {
                        terminal::info(&format!(
                            "{} has no host version available",
                            benchmark.name
                        ));
                    } else {
                        let host_bin = project_dir.join("bin/host").join("coremark");
                        if !host_bin.exists() || force {
                            let spinner =
                                Spinner::new(format!("Building {} (host)", benchmark.name));
                            if let Err(e) = coremark::build_host_benchmark(&project_dir) {
                                spinner.finish_with_failure(&format!("build failed: {}", e));
                                rows.push(bench::TableRow::error(
                                    "host",
                                    "build failed".to_string(),
                                ));
                            } else {
                                spinner.finish_and_clear();
                            }
                        }
                        if host_bin.exists() {
                            let spinner =
                                Spinner::new(format!("Running {} (host)", benchmark.name));
                            match coremark::run_host_benchmark(&host_bin, runs) {
                                Ok(result) => {
                                    spinner.finish_and_clear();
                                    host_time = Some(result.time_secs);
                                    let host_result = bench::HostResult {
                                        time_secs: Some(result.time_secs),
                                        perf: result.perf,
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
                }
            }
        }

        // Run libriscv comparison for each architecture if requested
        // Only Libriscv and Coremark benchmarks are compatible (use Linux syscalls)
        let libriscv_compatible = matches!(
            benchmark.source,
            BenchmarkSource::Libriscv | BenchmarkSource::Coremark
        );
        if compare_libriscv && libriscv_compatible {
            for a in &archs {
                let elf_path = project_dir.join(format!("bin/{}/{}", a.as_str(), benchmark.name));
                if !elf_path.exists() {
                    terminal::warning(&format!(
                        "ELF not found for libriscv comparison: {}",
                        elf_path.display()
                    ));
                    continue;
                }
                let label = format!("libriscv-{}", a.as_str());
                let spinner = Spinner::new(format!("Running {} ({})", benchmark.name, label));
                match run_libriscv_benchmark(&project_dir, &elf_path, runs) {
                    Ok(result) => {
                        spinner.finish_and_clear();
                        let overhead = host_time.map(|ht| result.time_secs / ht);
                        let (ipc, branch_miss_rate, host_instrs) = result
                            .perf
                            .as_ref()
                            .map(|p| (p.ipc(), p.branch_miss_rate(), p.instructions))
                            .unwrap_or((None, None, None));
                        rows.push(bench::TableRow {
                            label,
                            instret: None,
                            host_instrs,
                            instrs_per_guest: None,
                            time_secs: Some(result.time_secs),
                            overhead,
                            mips: None,
                            ipc,
                            branch_miss_rate,
                            error: None,
                        });
                    }
                    Err(e) => {
                        spinner.finish_with_failure(&e);
                        rows.push(bench::TableRow::error(&label, e));
                    }
                }
            }
        }

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

        rows.sort_by(|a, b| match (&a.error, &b.error) {
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), Some(_)) => std::cmp::Ordering::Equal,
            (None, None) => {
                let a_key = a.overhead.or(a.time_secs.map(|t| t * 1000.0));
                let b_key = b.overhead.or(b.time_secs.map(|t| t * 1000.0));
                a_key
                    .partial_cmp(&b_key)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        });

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
    let elf_time = elf_path.metadata().and_then(|m| m.modified()).ok();
    let lib_time = lib_path.metadata().and_then(|m| m.modified()).ok();
    match (elf_time, lib_time) {
        (Some(elf), Some(lib)) => elf > lib,
        _ => true,
    }
}

/// Run benchmark for a single architecture, returning a table row.
#[allow(clippy::too_many_arguments)]
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
    // Skip unsupported architectures
    if !benchmark.supports_arch(arch) {
        return None;
    }

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

    // Auto-build if ELF missing
    if !elf_path.exists() {
        match &benchmark.source {
            BenchmarkSource::Rust { path } => {
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
                    true,
                );
                if result != EXIT_SUCCESS {
                    spinner.finish_with_failure("build failed");
                    return Some(bench::TableRow::error(
                        &backend_name,
                        "build failed".to_string(),
                    ));
                }
                spinner.finish_and_clear();
            }
            BenchmarkSource::Polkavm => {
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
            }
            BenchmarkSource::RiscvTests => {
                let spinner = Spinner::new(format!(
                    "Building {} ({}, riscv-tests)",
                    benchmark.name,
                    arch.as_str()
                ));
                if let Err(e) = riscv_tests::build_benchmark(project_dir, benchmark.name, arch) {
                    spinner.finish_with_failure(&format!("build failed: {}", e));
                    return Some(bench::TableRow::error(
                        &backend_name,
                        "build failed".to_string(),
                    ));
                }
                spinner.finish_and_clear();
            }
            BenchmarkSource::Libriscv => {
                let spinner = Spinner::new(format!(
                    "Building {} ({}, libriscv)",
                    benchmark.name,
                    arch.as_str()
                ));
                if let Err(e) = libriscv::build_benchmark(project_dir, benchmark.name, arch) {
                    spinner.finish_with_failure(&format!("build failed: {}", e));
                    return Some(bench::TableRow::error(
                        &backend_name,
                        "build failed".to_string(),
                    ));
                }
                spinner.finish_and_clear();
            }
            BenchmarkSource::Coremark => {
                let spinner = Spinner::new(format!(
                    "Building {} ({}, coremark)",
                    benchmark.name,
                    arch.as_str()
                ));
                if let Err(e) = coremark::build_benchmark(project_dir, arch) {
                    spinner.finish_with_failure(&format!("build failed: {}", e));
                    return Some(bench::TableRow::error(
                        &backend_name,
                        "build failed".to_string(),
                    ));
                }
                spinner.finish_and_clear();
            }
        }
    }

    // Compile if needed
    let lib_path = out_dir.join(format!("lib{}.so", suffix));
    let should_compile = force || needs_recompile(&elf_path, &lib_path);

    if should_compile {
        if force
            && out_dir.exists()
            && let Err(e) = std::fs::remove_dir_all(&out_dir)
        {
            terminal::warning(&format!(
                "Failed to clean output directory {}: {}",
                out_dir.display(),
                e
            ));
        }

        let spinner = Spinner::new(format!("Compiling {} ({})", benchmark.name, arch.as_str()));
        let mut options = CompileOptions::new()
            .with_compiler(compiler.clone())
            .with_export_functions(benchmark.uses_exports)
            .with_quiet(true);
        if fast {
            options = options.with_instret_mode(InstretMode::Off);
        }
        match &benchmark.source {
            BenchmarkSource::RiscvTests => {
                options = options.with_htif(true);
            }
            BenchmarkSource::Libriscv | BenchmarkSource::Coremark => {
                // libriscv and coremark benchmarks use Linux syscall conventions
                options = options.with_syscall_mode(SyscallMode::Linux);
            }
            _ => {}
        }

        if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
            spinner.finish_with_failure(&format!("compile failed: {}", e));
            return Some(bench::TableRow::error(
                &backend_name,
                format!("compile failed: {}", e),
            ));
        }
        spinner.finish_and_clear();
    }

    // Run
    let spinner = Spinner::new(format!("Running {} ({})", benchmark.name, arch.as_str()));
    match bench::run_bench_auto(&out_dir, &elf_path, runs) {
        Ok((result, _mode)) => {
            spinner.finish_and_clear();
            Some(bench::TableRow::backend(&backend_name, &result, host_time))
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some(bench::TableRow::error(&backend_name, e))
        }
    }
}
