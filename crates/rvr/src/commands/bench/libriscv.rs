//! libriscv benchmark building.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

use libloading::os::unix::{Library, RTLD_NOW, Symbol};

use rvr::PerfCounters;
use rvr::bench::Arch;
use rvr::perf::HostPerfCounters;

use super::host_lib_ext;

/// Header to make RISC-V code compile on host.
/// - Renames syscall to _host_syscall (so function definition compiles)
/// - Stubs out asm/register bindings (so the function body compiles)
/// - Stores result in global to prevent optimizer removing fib call
const HOST_COMPAT_H: &str = r#"long __host_result;
#define syscall _host_syscall
#define asm(x) /* removed */
"#;

/// Build a libriscv benchmark directly from source.
/// These benchmarks use Linux syscall conventions (syscall 93 for exit).
pub fn build_benchmark(
    project_dir: &std::path::Path,
    name: &str,
    arch: &Arch,
) -> Result<PathBuf, String> {
    let toolchain = rvr::build_utils::find_toolchain()
        .ok_or_else(|| "RISC-V toolchain not found (need riscv64-unknown-elf-gcc)".to_string())?;

    let gcc = format!("{}gcc", toolchain);
    let out_dir = project_dir.join("bin").join(arch.as_str());
    let libriscv_dir = project_dir.join("programs/libriscv/binaries");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

    // Get the source file path and check arch compatibility
    let (src_path, is_asm) = match name {
        "fib" => (libriscv_dir.join("measure_mips/fib.c"), false),
        "fib-asm" => {
            // Assembly version is RV64 only (uses lui for 256M constant)
            if !matches!(arch, Arch::Rv64i | Arch::Rv64e) {
                return Err("fib-asm only supports rv64i/rv64e".to_string());
            }
            (libriscv_dir.join("measure_mips/fib64.S"), true)
        }
        _ => return Err(format!("unknown libriscv benchmark: {}", name)),
    };

    if !src_path.exists() {
        return Err(format!("source not found: {}", src_path.display()));
    }

    let (march, mabi) = match arch {
        Arch::Rv32i | Arch::Rv32e => ("rv32im_zicsr", "ilp32"),
        Arch::Rv64i | Arch::Rv64e => ("rv64im_zicsr", "lp64"),
    };

    let out_path = out_dir.join(name);

    // Build - sources have their own _start and use syscall 93 (Linux exit)
    let mut cmd = Command::new(&gcc);
    cmd.arg(format!("-march={}", march))
        .arg(format!("-mabi={}", mabi))
        .args(["-static", "-nostdlib", "-nostartfiles"]);
    if !is_asm {
        cmd.args(["-O3", "-fno-builtin"]);
    }
    cmd.arg(&src_path).arg("-o").arg(&out_path);

    let output = cmd
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run gcc: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gcc failed: {}", stderr));
    }

    Ok(out_path)
}

/// Build a libriscv benchmark as a shared library for the host.
pub fn build_host_benchmark(project_dir: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    let libriscv_dir = project_dir.join("programs/libriscv/binaries");
    let out_dir = project_dir.join("bin/host");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

    let src_path = match name {
        "fib" => libriscv_dir.join("measure_mips/fib.c"),
        // fib-asm is RISC-V assembly, can't compile for host
        "fib-asm" => return Err("fib-asm has no host version (RISC-V assembly)".to_string()),
        _ => return Err(format!("unknown libriscv benchmark: {}", name)),
    };

    if !src_path.exists() {
        return Err(format!("source not found: {}", src_path.display()));
    }

    // Write compat header
    let compat_h = out_dir.join("_host_compat.h");
    std::fs::write(&compat_h, HOST_COMPAT_H)
        .map_err(|e| format!("failed to write compat header: {}", e))?;

    let out_path = out_dir.join(format!("{}.{}", name, host_lib_ext()));

    // Use sed to remove __asm__ volatile lines, then compile with compat header
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            r#"sed 's/__asm__ volatile[^;]*;/__host_result = a0;/' {} | cc -shared -fPIC -O3 -include{} -x c - -o {}"#,
            src_path.display(),
            compat_h.display(),
            out_path.display()
        ))
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

/// Run a libriscv host benchmark by calling _start via dlopen.
pub fn run_host_benchmark(
    lib_path: &std::path::Path,
    runs: usize,
) -> Result<HostBenchResult, String> {
    let lib = unsafe {
        Library::open(Some(lib_path), RTLD_NOW)
            .map_err(|e| format!("failed to load {}: {}", lib_path.display(), e))?
    };

    type StartFn = unsafe extern "C" fn();

    let start_fn: Symbol<StartFn> = unsafe {
        lib.get(b"_start")
            .map_err(|e| format!("_start symbol not found: {}", e))?
    };

    let runs = runs.max(1);

    // Warm up
    for _ in 0..10 {
        unsafe { start_fn() };
    }

    // Set up perf counters
    let mut perf_counters = HostPerfCounters::new();
    let snapshot_before = perf_counters.as_mut().map(|c| c.read()).unwrap_or_default();

    // Timed runs with perf (enable once for all runs to avoid per-call overhead)
    if let Some(ref mut counters) = perf_counters {
        let _ = counters.enable();
    }
    let start = Instant::now();
    for _ in 0..runs {
        unsafe { start_fn() };
    }
    let elapsed = start.elapsed();
    if let Some(ref mut counters) = perf_counters {
        let _ = counters.disable();
    }

    let time_secs = elapsed.as_secs_f64() / runs as f64;
    let perf = perf_counters.map(|mut c| {
        let delta = c.read_delta(&snapshot_before);
        PerfCounters {
            cycles: delta.cycles.map(|v| v / runs as u64),
            instructions: delta.instructions.map(|v| v / runs as u64),
            branches: delta.branches.map(|v| v / runs as u64),
            branch_misses: delta.branch_misses.map(|v| v / runs as u64),
        }
    });

    Ok(HostBenchResult { time_secs, perf })
}
