//! riscv-tests benchmark building.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

use rvr::PerfCounters;
use rvr::bench::Arch;
use rvr::perf::HostPerfCounters;

/// Host-compatible setStats() for riscv-tests benchmarks.
/// Uses clock_gettime instead of CSRs, prints timing in parseable format.
const HOST_SYSCALLS_C: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <time.h>

static uint64_t start_nanos;
static uint64_t elapsed_nanos;

static uint64_t get_nanos(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

void setStats(int enable) {
    if (enable) {
        start_nanos = get_nanos();
    } else {
        elapsed_nanos = get_nanos() - start_nanos;
        printf("host_nanos = %lu\n", (unsigned long)elapsed_nanos);
    }
}
"#;

/// Fix header for dhrystone - uses TIME code path to avoid multiple definition issues.
const DHRYSTONE_FIX_H: &str = "#define TIME 1\n";

/// Build a riscv-tests benchmark using riscv-gcc.
/// Returns the path to the built ELF on success.
pub fn build_benchmark(
    project_dir: &std::path::Path,
    name: &str,
    arch: &Arch,
) -> Result<PathBuf, String> {
    let toolchain = rvr::tests::find_toolchain()
        .ok_or_else(|| "RISC-V toolchain not found (need riscv64-unknown-elf-gcc)".to_string())?;

    let gcc = format!("{}gcc", toolchain);
    let bench_dir = project_dir.join("programs/riscv-tests/benchmarks");
    let common_dir = bench_dir.join("common");
    let out_dir = project_dir.join("bin").join(arch.as_str());

    let src_dir = bench_dir.join(name);
    if !src_dir.exists() {
        return Err(format!("benchmark source not found: {}", src_dir.display()));
    }

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

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
    let link_ld = common_dir.join("test.ld");
    let crt_s = common_dir.join("crt.S");
    let syscalls_c = common_dir.join("syscalls.c");
    let env_dir = project_dir.join("programs/riscv-tests/env");

    let (march, mabi) = match arch {
        Arch::Rv32i | Arch::Rv32e => ("rv32im_zicsr", "ilp32"),
        Arch::Rv64i | Arch::Rv64e => ("rv64im_zicsr", "lp64"),
    };

    let mut cmd = Command::new(&gcc);
    cmd.arg(format!("-march={}", march))
        .arg(format!("-mabi={}", mabi))
        .args(["-static", "-mcmodel=medany", "-fvisibility=hidden"])
        .args(["-nostdlib", "-nostartfiles"])
        .args([
            "-std=gnu99",
            "-O2",
            "-ffast-math",
            "-fno-common",
            "-fno-builtin-printf",
        ])
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

    cmd.arg("-lgcc").arg("-o").arg(&out_path);

    let status = cmd
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| format!("failed to run gcc: {}", e))?;

    if !status.success() {
        return Err(format!("gcc failed with exit code {:?}", status.code()));
    }

    Ok(out_path)
}

/// Build a riscv-tests benchmark for the host using the system compiler.
pub fn build_host_benchmark(project_dir: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    let bench_dir = project_dir.join("programs/riscv-tests/benchmarks");
    let common_dir = bench_dir.join("common");
    let out_dir = project_dir.join("bin/host");

    let src_dir = bench_dir.join(name);
    if !src_dir.exists() {
        return Err(format!("benchmark source not found: {}", src_dir.display()));
    }

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

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

    // Write embedded support files to out_dir
    let host_syscalls = out_dir.join("_host_syscalls.c");
    std::fs::write(&host_syscalls, HOST_SYSCALLS_C)
        .map_err(|e| format!("failed to write host syscalls: {}", e))?;

    let out_path = out_dir.join(name);

    let mut cmd = Command::new("cc");
    cmd.args(["-O3", "-std=gnu99", "-DPREALLOCATE=1"])
        .args(["-Wno-implicit-int", "-Wno-implicit-function-declaration"])
        .arg(format!("-I{}", common_dir.display()));

    // For dhrystone, include fix header to avoid times() code path issues
    if name == "dhrystone" {
        let fix_header = out_dir.join("_dhrystone_fix.h");
        std::fs::write(&fix_header, DHRYSTONE_FIX_H)
            .map_err(|e| format!("failed to write dhrystone fix: {}", e))?;
        cmd.arg(format!("-include{}", fix_header.display()));
    }

    cmd.arg(&host_syscalls);

    for f in &c_files {
        cmd.arg(f);
    }

    cmd.arg("-o").arg(&out_path);

    let output = cmd
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run cc: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cc failed: {}", stderr));
    }

    Ok(out_path)
}

/// Result of running a host benchmark.
#[derive(Debug, Clone)]
pub struct HostBenchResult {
    /// Average time per run in seconds.
    pub time_secs: f64,
    /// Hardware perf counters (if available).
    pub perf: Option<PerfCounters>,
}

/// Run a riscv-tests host benchmark and collect timing + perf counters.
pub fn run_host_benchmark(
    host_bin: &std::path::Path,
    runs: usize,
) -> Result<HostBenchResult, String> {
    let runs = runs.max(1);

    // Set up perf counters
    let mut perf_counters = HostPerfCounters::new();
    let mut total_cycles = 0u64;
    let mut total_instructions = 0u64;
    let mut total_branches = 0u64;
    let mut total_branch_misses = 0u64;
    let mut prev_snapshot = perf_counters.as_mut().map(|c| c.read()).unwrap_or_default();

    let start = Instant::now();
    for _ in 0..runs {
        if let Some(ref mut counters) = perf_counters {
            let _ = counters.enable();
        }

        let output = Command::new(host_bin)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("failed to run host: {}", e))?;

        if let Some(ref mut counters) = perf_counters {
            let _ = counters.disable();
            let delta = counters.read_delta(&prev_snapshot);
            total_cycles += delta.cycles.unwrap_or(0);
            total_instructions += delta.instructions.unwrap_or(0);
            total_branches += delta.branches.unwrap_or(0);
            total_branch_misses += delta.branch_misses.unwrap_or(0);
            prev_snapshot = counters.read();
        }

        if !output.status.success() {
            return Err(format!("host exited with code {:?}", output.status.code()));
        }
    }
    let elapsed = start.elapsed();

    let time_secs = elapsed.as_secs_f64() / runs as f64;
    let perf = perf_counters.map(|_| PerfCounters {
        cycles: Some(total_cycles / runs as u64),
        instructions: Some(total_instructions / runs as u64),
        branches: Some(total_branches / runs as u64),
        branch_misses: Some(total_branch_misses / runs as u64),
    });

    Ok(HostBenchResult { time_secs, perf })
}
