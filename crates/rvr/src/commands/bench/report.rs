//! Benchmark report generation.

use std::path::Path;
use std::process::Command;

use rvr::Compiler;
use rvr::bench::{self, Arch};

use super::{
    BENCHMARKS, BenchmarkSource, coremark, find_project_root, libriscv, polkavm, riscv_tests,
    run_libriscv_benchmark, run_single_arch,
};
use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::terminal::{self, Spinner};

// ============================================================================
// System information collection
// ============================================================================

/// Collect system information for the report header.
/// Ordered by relevance: arch/CPU first, then compilers, then OS context.
fn collect_system_info() -> Vec<(String, String)> {
    let mut info = Vec::new();

    // Architecture (most relevant for performance comparison)
    if let Ok(output) = Command::new("uname").arg("-m").output()
        && output.status.success()
    {
        let arch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info.push(("Architecture".to_string(), arch));
    }

    // CPU model
    if let Some(model) = get_cpu_model() {
        info.push(("CPU".to_string(), model));
    }

    // Clang version (used for compiling generated C code)
    if let Ok(output) = Command::new("clang").arg("--version").output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        info.push(("Clang".to_string(), version));
    }

    // Rust version (recompiler toolchain)
    if let Ok(output) = Command::new("rustc").arg("--version").output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info.push(("Rust".to_string(), version));
    }

    // OS distribution
    if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
        for line in contents.lines() {
            if line.starts_with("PRETTY_NAME=") {
                let name = line
                    .trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string();
                info.push(("OS".to_string(), name));
                break;
            }
        }
    }

    // Date
    if let Ok(output) = Command::new("date").arg("+%Y-%m-%d %H:%M:%S").output()
        && output.status.success()
    {
        let date = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info.push(("Date".to_string(), date));
    }

    info
}

/// Get CPU model string.
fn get_cpu_model() -> Option<String> {
    // Try /proc/cpuinfo (Linux)
    if let Ok(contents) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in contents.lines() {
            // ARM64 uses "CPU part" or "model name", x86 uses "model name"
            if (line.starts_with("model name") || line.starts_with("Model"))
                && let Some(value) = line.split(':').nth(1)
            {
                return Some(value.trim().to_string());
            }
        }
        // ARM64 fallback: look for Hardware or CPU implementer
        for line in contents.lines() {
            if line.starts_with("Hardware")
                && let Some(value) = line.split(':').nth(1)
            {
                return Some(value.trim().to_string());
            }
        }
    }

    // Try sysctl (macOS)
    if let Ok(output) = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        && output.status.success()
    {
        let model = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !model.is_empty() {
            return Some(model);
        }
    }

    // Fallback for Apple Silicon
    if let Ok(output) = Command::new("sysctl").args(["-n", "hw.model"]).output()
        && output.status.success()
    {
        let model = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !model.is_empty() {
            return Some(model);
        }
    }

    None
}

// ============================================================================
// Formatting
// ============================================================================

/// Format a table row as markdown string.
fn format_table_row(row: &bench::TableRow) -> String {
    if let Some(ref err) = row.error {
        let err_display = if err.len() > 12 {
            format!("{}...", &err[..9])
        } else {
            err.clone()
        };
        return format!(
            "| {:<14} | {:>10} | {:>10} | {:>9} | {:>10} | {:>6} | {:>12} | {:>5} | {:>11} |",
            row.label, "-", "-", "-", "-", "-", err_display, "-", "-"
        );
    }

    let instret = row
        .instret
        .map(bench::format_num)
        .unwrap_or_else(|| "-".to_string());
    let host_instrs = row
        .host_instrs
        .map(bench::format_num)
        .unwrap_or_else(|| "-".to_string());
    let instrs_per_guest = bench::format_instrs_per_guest(row.instrs_per_guest);
    let time = row
        .time_secs
        .map(bench::format_time)
        .unwrap_or_else(|| "-".to_string());
    let overhead = bench::format_overhead(row.overhead);
    let speed = row
        .mips
        .map(bench::format_speed)
        .unwrap_or_else(|| "-".to_string());
    let ipc = bench::format_ipc(row.ipc);
    let branch_miss = bench::format_branch_miss(row.branch_miss_rate);

    format!(
        "| {:<14} | {:>10} | {:>10} | {:>9} | {:>10} | {:>6} | {:>12} | {:>5} | {:>11} |",
        row.label, instret, host_instrs, instrs_per_guest, time, overhead, speed, ipc, branch_miss
    )
}

// ============================================================================
// Report generation
// ============================================================================

/// Generate a benchmark report and write to file.
pub fn bench_report(
    output: &Path,
    runs: usize,
    no_libriscv: bool,
    no_host: bool,
    force: bool,
) -> i32 {
    use std::io::Write;

    let project_dir = find_project_root();
    let compare_host = !no_host;
    let compare_libriscv = !no_libriscv;

    println!("Generating benchmark report to {}", output.display());
    println!();

    // Collect system info
    let system_info = collect_system_info();

    // Build report content
    let mut report = String::new();

    // Header
    report.push_str("# Benchmark Results\n\n");

    // System info table
    report.push_str("## System Information\n\n");
    let max_value_len = system_info.iter().map(|(_, v)| v.len()).max().unwrap_or(5);
    report.push_str(&format!(
        "| {:<12} | {:<width$} |\n",
        "Property",
        "Value",
        width = max_value_len
    ));
    report.push_str(&format!(
        "|{:-<14}|{:-<width$}|\n",
        "",
        "",
        width = max_value_len + 2
    ));
    for (key, value) in &system_info {
        report.push_str(&format!(
            "| {:<12} | {:<width$} |\n",
            key,
            value,
            width = max_value_len
        ));
    }
    report.push('\n');

    // Run benchmarks
    let archs = Arch::ALL;
    let compiler = Compiler::default();

    for benchmark in BENCHMARKS.iter() {
        println!("Running {}...", benchmark.name);

        // Benchmark header
        report.push_str(&format!("## {}\n\n", benchmark.name));
        report.push_str(&format!("*{} | runs: {}*\n\n", benchmark.description, runs));

        // Table header
        report.push_str(&format!(
            "| {:<14} | {:>10} | {:>10} | {:>9} | {:>10} | {:>6} | {:>12} | {:>5} | {:>11} |\n",
            "Backend",
            "Instret",
            "Host Ops",
            "Ops/Guest",
            "Time",
            "OH",
            "Speed",
            "IPC",
            "Branch Miss"
        ));
        report.push_str(
            "|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|\n",
        );

        let mut rows = Vec::new();
        let mut host_time: Option<f64> = None;

        // Run host if available
        if compare_host
            && benchmark.has_host
            && let Some((row, time)) = run_host_benchmark(benchmark, &project_dir, runs, force)
        {
            host_time = time;
            rows.push(row);
        }

        // Run rvr for each architecture
        for arch in archs {
            let suffix = arch.as_str();
            if let Some(row) = run_single_arch(
                benchmark,
                arch,
                &project_dir,
                suffix,
                false, // not fast mode
                runs,
                &compiler,
                host_time,
                force,
            ) {
                rows.push(row);
            }
        }

        // Run libriscv if compatible
        let libriscv_compatible = matches!(
            benchmark.source,
            BenchmarkSource::Libriscv | BenchmarkSource::Coremark
        );
        if compare_libriscv && libriscv_compatible {
            for arch in archs {
                if let Some(row) = run_libriscv_arch(benchmark, arch, &project_dir, runs, host_time)
                {
                    rows.push(row);
                }
            }
        }

        // Sort rows by time
        rows.sort_by(|a, b| {
            let time_a = a.time_secs.unwrap_or(f64::MAX);
            let time_b = b.time_secs.unwrap_or(f64::MAX);
            time_a
                .partial_cmp(&time_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Write rows
        for row in &rows {
            report.push_str(&format_table_row(row));
            report.push('\n');
        }
        report.push('\n');
    }

    // Write to file
    match std::fs::File::create(output) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(report.as_bytes()) {
                terminal::error(&format!("Failed to write report: {}", e));
                return EXIT_FAILURE;
            }
            println!();
            terminal::success(&format!("Report written to {}", output.display()));
            EXIT_SUCCESS
        }
        Err(e) => {
            terminal::error(&format!("Failed to create file: {}", e));
            EXIT_FAILURE
        }
    }
}

/// Run host benchmark and return row with host time.
fn run_host_benchmark(
    benchmark: &super::BenchmarkInfo,
    project_dir: &Path,
    runs: usize,
    force: bool,
) -> Option<(bench::TableRow, Option<f64>)> {
    match &benchmark.source {
        BenchmarkSource::Rust { path, .. } => {
            run_rust_host_benchmark(benchmark, project_dir, path, runs, force)
        }
        BenchmarkSource::Polkavm => run_polkavm_host_benchmark(benchmark, project_dir, runs, force),
        BenchmarkSource::RiscvTests => {
            run_riscv_tests_host_benchmark(benchmark, project_dir, runs, force)
        }
        BenchmarkSource::Libriscv => {
            run_libriscv_host_benchmark(benchmark, project_dir, runs, force)
        }
        BenchmarkSource::Coremark => {
            run_coremark_host_benchmark(benchmark, project_dir, runs, force)
        }
    }
}

/// Run host benchmark for Rust projects.
fn run_rust_host_benchmark(
    benchmark: &super::BenchmarkInfo,
    project_dir: &Path,
    path: &str,
    runs: usize,
    force: bool,
) -> Option<(bench::TableRow, Option<f64>)> {
    // Use host_binary path if specified, otherwise construct from path
    let host_bin = if let Some(host_path) = benchmark.host_binary {
        project_dir.join(host_path)
    } else {
        project_dir
            .join(path)
            .join("target/release")
            .join(benchmark.name)
    };

    // Build with cargo if binary doesn't exist or force rebuild
    if !host_bin.exists() || force {
        let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
        let output = Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(project_dir.join(path))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output();

        match output {
            Ok(o) if o.status.success() => spinner.finish_and_clear(),
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let error_detail = stderr
                    .lines()
                    .filter(|l| l.starts_with("error"))
                    .last()
                    .unwrap_or("build failed");
                spinner.finish_with_failure(error_detail);
                return Some((
                    bench::TableRow::error("host", error_detail.to_string()),
                    None,
                ));
            }
            Err(e) => {
                let msg = format!("failed to run cargo: {}", e);
                spinner.finish_with_failure(&msg);
                return Some((bench::TableRow::error("host", msg), None));
            }
        }
    }

    if !host_bin.exists() {
        return None;
    }

    let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
    match bench::run_host(&host_bin, runs) {
        Ok(result) => {
            spinner.finish_and_clear();
            let host_time = result.time_secs;
            Some((bench::TableRow::host("host", &result), host_time))
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some((bench::TableRow::error("host", e), None))
        }
    }
}

/// Run host benchmark for Polkavm benchmarks (compiled as shared library).
fn run_polkavm_host_benchmark(
    benchmark: &super::BenchmarkInfo,
    project_dir: &Path,
    runs: usize,
    force: bool,
) -> Option<(bench::TableRow, Option<f64>)> {
    let host_lib = project_dir
        .join("bin/host")
        .join(format!("{}.so", benchmark.name));

    if !host_lib.exists() || force {
        let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
        if let Err(e) = polkavm::build_host_benchmark(project_dir, benchmark.name) {
            spinner.finish_with_failure(&e);
            return Some((bench::TableRow::error("host", e), None));
        }
        spinner.finish_and_clear();
    }

    if !host_lib.exists() {
        return None;
    }

    let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
    match polkavm::run_host_benchmark(&host_lib, runs) {
        Ok(result) => {
            spinner.finish_and_clear();
            let host_time = Some(result.time_secs);
            let host_result = bench::HostResult {
                time_secs: Some(result.time_secs),
                perf: result.perf,
            };
            Some((bench::TableRow::host("host", &host_result), host_time))
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some((bench::TableRow::error("host", e), None))
        }
    }
}

/// Run host benchmark for riscv-tests benchmarks.
fn run_riscv_tests_host_benchmark(
    benchmark: &super::BenchmarkInfo,
    project_dir: &Path,
    runs: usize,
    force: bool,
) -> Option<(bench::TableRow, Option<f64>)> {
    let host_bin = project_dir.join("bin/host").join(benchmark.name);

    if !host_bin.exists() || force {
        let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
        if let Err(e) = riscv_tests::build_host_benchmark(project_dir, benchmark.name) {
            spinner.finish_with_failure(&format!("build failed: {}", e));
            return Some((
                bench::TableRow::error("host", "build failed".to_string()),
                None,
            ));
        }
        spinner.finish_and_clear();
    }

    if !host_bin.exists() {
        return None;
    }

    let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
    match riscv_tests::run_host_benchmark(&host_bin, runs) {
        Ok(result) => {
            spinner.finish_and_clear();
            let host_time = Some(result.time_secs);
            let host_result = bench::HostResult {
                time_secs: Some(result.time_secs),
                perf: result.perf,
            };
            Some((bench::TableRow::host("host", &host_result), host_time))
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some((bench::TableRow::error("host", e), None))
        }
    }
}

/// Run host benchmark for libriscv benchmarks (compiled as shared library).
fn run_libriscv_host_benchmark(
    benchmark: &super::BenchmarkInfo,
    project_dir: &Path,
    runs: usize,
    force: bool,
) -> Option<(bench::TableRow, Option<f64>)> {
    let host_lib = project_dir
        .join("bin/host")
        .join(format!("{}.so", benchmark.name));

    if !host_lib.exists() || force {
        let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
        if let Err(e) = libriscv::build_host_benchmark(project_dir, benchmark.name) {
            spinner.finish_with_failure(&format!("build failed: {}", e));
            return Some((
                bench::TableRow::error("host", "build failed".to_string()),
                None,
            ));
        }
        spinner.finish_and_clear();
    }

    if !host_lib.exists() {
        return None;
    }

    let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
    match libriscv::run_host_benchmark(&host_lib, runs) {
        Ok(result) => {
            spinner.finish_and_clear();
            let host_time = Some(result.time_secs);
            let host_result = bench::HostResult {
                time_secs: Some(result.time_secs),
                perf: result.perf,
            };
            Some((bench::TableRow::host("host", &host_result), host_time))
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some((bench::TableRow::error("host", e), None))
        }
    }
}

/// Run host benchmark for CoreMark.
fn run_coremark_host_benchmark(
    benchmark: &super::BenchmarkInfo,
    project_dir: &Path,
    runs: usize,
    force: bool,
) -> Option<(bench::TableRow, Option<f64>)> {
    let host_bin = project_dir.join("bin/host/coremark");

    if !host_bin.exists() || force {
        let spinner = Spinner::new(format!("Building {} (host)", benchmark.name));
        if let Err(e) = coremark::build_host_benchmark(project_dir) {
            spinner.finish_with_failure(&format!("build failed: {}", e));
            return Some((
                bench::TableRow::error("host", "build failed".to_string()),
                None,
            ));
        }
        spinner.finish_and_clear();
    }

    if !host_bin.exists() {
        return None;
    }

    let spinner = Spinner::new(format!("Running {} (host)", benchmark.name));
    match bench::run_host(&host_bin, runs) {
        Ok(result) => {
            spinner.finish_and_clear();
            let host_time = result.time_secs;
            Some((bench::TableRow::host("host", &result), host_time))
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some((bench::TableRow::error("host", e), None))
        }
    }
}

/// Run libriscv benchmark for a specific architecture.
fn run_libriscv_arch(
    benchmark: &super::BenchmarkInfo,
    arch: &Arch,
    project_dir: &Path,
    runs: usize,
    host_time: Option<f64>,
) -> Option<bench::TableRow> {
    let elf_path = project_dir
        .join("bin")
        .join(arch.as_str())
        .join(benchmark.name);
    if !elf_path.exists() {
        return None;
    }

    let backend_name = format!("libriscv-{}", arch.as_str());
    let spinner = Spinner::new(format!(
        "Running {} (libriscv-{})",
        benchmark.name,
        arch.as_str()
    ));

    match run_libriscv_benchmark(project_dir, &elf_path, runs) {
        Ok(result) => {
            spinner.finish_and_clear();
            let (ipc, branch_miss_rate, host_instrs) = result
                .perf
                .as_ref()
                .map(|p| (p.ipc(), p.branch_miss_rate(), p.instructions))
                .unwrap_or((None, None, None));

            Some(bench::TableRow {
                label: backend_name,
                instret: None,
                host_instrs,
                instrs_per_guest: None,
                time_secs: Some(result.time_secs),
                overhead: host_time.map(|ht| result.time_secs / ht),
                mips: None,
                ipc,
                branch_miss_rate,
                error: None,
            })
        }
        Err(e) => {
            spinner.finish_with_failure(&e);
            Some(bench::TableRow::error(&backend_name, e))
        }
    }
}
