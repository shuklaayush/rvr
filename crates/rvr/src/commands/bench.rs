//! Benchmark commands and registry.

use std::process::Command;

use rvr::bench::{self, Arch};
use rvr::{CompileOptions, Compiler, InstretMode};

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::commands::build::build_rust_project;

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
    /// Prebuilt ELF - already in bin/{arch}/{name}
    Prebuilt,
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
/// ELF binaries are at: bin/{arch}/{name}
const BENCHMARKS: &[BenchmarkInfo] = &[
    BenchmarkInfo {
        name: "minimal",
        description: "Minimal function call overhead",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "prime-sieve",
        description: "Prime number sieve algorithm",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "pinky",
        description: "NES emulator (cycle-accurate)",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "memset",
        description: "Memory set operations",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "reth",
        description: "Reth block validator",
        uses_exports: false,
        host_binary: Some("programs/reth-validator/target/release/reth-validator"),
        default_archs: "rv64i",
        source: BenchmarkSource::Rust {
            path: "programs/reth-validator",
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
            BenchmarkSource::Prebuilt => markers.push("prebuilt"),
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
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Determine which benchmarks to build
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                eprintln!("Error: unknown benchmark '{}'", n);
                eprintln!("Run 'rvr bench list' to see available benchmarks");
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
                eprintln!("Building {} from {}", benchmark.name, path);

                // Build host binary first (unless --no-host)
                if !no_host && benchmark.host_binary.is_some() {
                    eprintln!("  Building host binary...");
                    let status = Command::new("cargo")
                        .arg("build")
                        .arg("--release")
                        .arg("--manifest-path")
                        .arg(project_dir.join(path).join("Cargo.toml"))
                        .status()
                        .expect("failed to run cargo");

                    if !status.success() {
                        eprintln!("  Host build failed");
                        return EXIT_FAILURE;
                    }
                }

                // Build RISC-V ELFs using rvr build
                let project_path = project_dir.join(path);
                let result = build_rust_project(
                    &project_path,
                    arch_str,
                    None, // Use default output (bin/{arch}/)
                    Some(benchmark.name), // Use benchmark name as output name
                    "nightly",
                    None,
                    true,
                );

                if result != EXIT_SUCCESS {
                    return result;
                }
            }
            BenchmarkSource::Prebuilt => {
                // Check if prebuilt ELFs exist
                let archs: Vec<&str> = arch_str.split(',').map(|s| s.trim()).collect();
                let mut missing = false;

                for a in &archs {
                    let elf_path = project_dir.join("bin").join(a).join(benchmark.name);
                    if !elf_path.exists() {
                        eprintln!(
                            "  Warning: prebuilt ELF not found: {}",
                            elf_path.display()
                        );
                        missing = true;
                    }
                }

                if missing {
                    eprintln!(
                        "  Note: {} uses prebuilt ELFs. Place them in bin/<arch>/{}",
                        benchmark.name, benchmark.name
                    );
                } else {
                    eprintln!("  {} ELFs already present", benchmark.name);
                }
            }
        }
    }

    eprintln!();
    eprintln!("Build complete.");
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
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Determine which benchmarks to compile
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                eprintln!("Error: unknown benchmark '{}'", n);
                return EXIT_FAILURE;
            }
        },
        None => BENCHMARKS.iter().collect(),
    };

    let suffix = if fast { "fast" } else { "base" };

    let mut compiler: Compiler = cc.parse().unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
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
                eprintln!("Error: {}", e);
                return EXIT_FAILURE;
            }
        };

        for a in &archs {
            // ELF path: bin/{arch}/{name}
            let elf_path = project_dir.join("bin").join(a.as_str()).join(benchmark.name);

            if !elf_path.exists() {
                eprintln!(
                    "Warning: {} not found, skipping",
                    elf_path.display()
                );
                continue;
            }

            let out_dir = project_dir
                .join("target/benchmarks")
                .join(benchmark.name)
                .join(a.as_str())
                .join(suffix);

            eprintln!(
                "Compiling {} ({}) -> {}",
                benchmark.name,
                a.as_str(),
                out_dir.display()
            );

            let mut options = CompileOptions::new()
                .with_compiler(compiler.clone())
                .with_export_functions(benchmark.uses_exports);
            if fast {
                options = options.with_instret_mode(InstretMode::Off);
            }

            if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
                eprintln!("Error compiling {}: {}", a, e);
                return EXIT_FAILURE;
            }
        }
    }

    eprintln!("Compile complete.");
    EXIT_SUCCESS
}

/// Run compiled benchmark.
pub fn bench_run(
    name: Option<&str>,
    arch: Option<&str>,
    runs: usize,
    fast: bool,
    compare_host: bool,
) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Determine which benchmarks to run
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                eprintln!("Error: unknown benchmark '{}'", n);
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
                eprintln!("Error: {}", e);
                return EXIT_FAILURE;
            }
        };

        bench::print_bench_header(benchmark.name, benchmark.description, runs);

        // Collect all rows first, then sort by overhead/time
        let mut rows: Vec<bench::TableRow> = Vec::new();

        // Run host baseline if requested and available
        let mut host_time: Option<f64> = None;
        if compare_host {
            if let Some(host_path) = benchmark.host_binary {
                let host_bin = project_dir.join(host_path);
                if host_bin.exists() {
                    match bench::run_host(&host_bin, runs) {
                        Ok(result) => {
                            host_time = result.time_secs;
                            rows.push(bench::TableRow::host("host", &result));
                        }
                        Err(e) => {
                            rows.push(bench::TableRow::error("host", e));
                        }
                    }
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
                    a_key.partial_cmp(&b_key).unwrap_or(std::cmp::Ordering::Equal)
                }
            }
        });

        // Print sorted rows
        for row in &rows {
            bench::print_table_row(row);
        }

        println!();
    }

    EXIT_SUCCESS
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
) -> Option<bench::TableRow> {
    // ELF path: bin/{arch}/{name}
    let elf_path = project_dir.join("bin").join(arch.as_str()).join(benchmark.name);
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
                eprintln!("Building {} for {}...", benchmark.name, arch.as_str());
                let project_path = project_dir.join(path);
                let result = build_rust_project(
                    &project_path,
                    arch.as_str(),
                    None,
                    Some(benchmark.name),
                    "nightly",
                    None,
                    true,
                );
                if result != EXIT_SUCCESS {
                    return Some(bench::TableRow::error(&backend_name, "build failed".to_string()));
                }
            }
            BenchmarkSource::Prebuilt => {
                // No ELF available for this arch
                return Some(bench::TableRow::error(&backend_name, "no ELF".to_string()));
            }
        }
    }

    // Check if .so exists, compile if missing
    if !out_dir.exists() {
        eprintln!("Compiling {} ({})...", benchmark.name, arch.as_str());
        let mut options = CompileOptions::new()
            .with_compiler(compiler.clone())
            .with_export_functions(benchmark.uses_exports);
        if fast {
            options = options.with_instret_mode(InstretMode::Off);
        }

        if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
            return Some(bench::TableRow::error(&backend_name, format!("compile failed: {}", e)));
        }
    }

    match bench::run_bench_auto(&out_dir, &elf_path, runs) {
        Ok((result, _mode)) => {
            Some(bench::TableRow::backend(&backend_name, &result, host_time))
        }
        Err(e) => {
            Some(bench::TableRow::error(&backend_name, e))
        }
    }
}
