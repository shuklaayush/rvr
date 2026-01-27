//! Polkavm benchmark build and host runner.
//!
//! Handles building polkavm benchmarks for RISC-V targets and native host.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use libloading::os::unix::{Library, RTLD_NOW, Symbol};

use rvr::PerfCounters;
use rvr::perf::HostPerfCounters;

/// Polkavm guest programs directory (relative to project root).
const POLKAVM_GUEST_PROGRAMS: &str = "programs/polkavm/guest-programs";

/// Build a polkavm benchmark for the specified architecture.
pub fn build_benchmark(
    project_root: &Path,
    benchmark: &str,
    arch: &str,
) -> Result<PathBuf, String> {
    let toolchain_dir = project_root.join("toolchain");
    let guest_programs = project_root.join(POLKAVM_GUEST_PROGRAMS);

    // Validate paths
    if !guest_programs.exists() {
        return Err(format!(
            "polkavm guest-programs not found: {}",
            guest_programs.display()
        ));
    }

    let target_spec = toolchain_dir.join(format!("{}.json", arch));
    if !target_spec.exists() {
        return Err(format!("target spec not found: {}", target_spec.display()));
    }

    // Compile entry.S for this architecture
    let entry_obj = compile_entry(&toolchain_dir, arch)?;

    // Build with cargo
    let link_script = toolchain_dir.join("link.x");
    let bench_name = format!("bench-{}", benchmark);

    let rustflags = format!(
        "-C target-feature=+zba,+zbb,+zbs \
         -C link-arg={} \
         -C link-arg=-T{} \
         -C link-arg=--undefined=initialize \
         -C link-arg=--undefined=run",
        entry_obj.display(),
        link_script.display()
    );

    // Start with fresh env, keeping only essential vars
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&guest_programs);

    // Preserve only essential env vars (PATH, HOME, etc.)
    for (key, _) in std::env::vars() {
        if key.starts_with("CARGO") {
            cmd.env_remove(&key);
        }
    }

    cmd.env("RUSTFLAGS", &rustflags);

    let output = cmd
        .arg("+nightly-2025-05-10")
        .arg("build")
        .arg("--manifest-path")
        .arg(guest_programs.join("Cargo.toml"))
        .arg("-Z")
        .arg("build-std=core,alloc")
        .arg("-Z")
        .arg("build-std-features=compiler-builtins-mem")
        .arg("--target")
        .arg(&target_spec)
        .arg("--release")
        .arg("--bin")
        .arg(&bench_name)
        .arg("-p")
        .arg(&bench_name)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let error_detail = stderr
            .lines()
            .filter(|l| l.starts_with("error"))
            .last()
            .unwrap_or("unknown error");
        return Err(format!(
            "cargo build failed for {}/{}: {}",
            benchmark, arch, error_detail
        ));
    }

    // Copy to output directory
    let src = guest_programs
        .join("target")
        .join(arch)
        .join("release")
        .join(&bench_name);
    let dest_dir = project_root.join("bin").join(arch);
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("failed to create {}: {}", dest_dir.display(), e))?;

    let dest = dest_dir.join(benchmark);
    std::fs::copy(&src, &dest).map_err(|e| {
        format!(
            "failed to copy {} to {}: {}",
            src.display(),
            dest.display(),
            e
        )
    })?;

    Ok(dest)
}

/// Compile entry.S for the specified architecture.
fn compile_entry(toolchain_dir: &Path, arch: &str) -> Result<PathBuf, String> {
    let entry_s = toolchain_dir.join("entry.S");
    let entry_obj = toolchain_dir.join(format!("entry_{}.o", arch));

    // Check if recompilation needed
    if entry_obj.exists() {
        let src_time = std::fs::metadata(&entry_s).and_then(|m| m.modified()).ok();
        let obj_time = std::fs::metadata(&entry_obj)
            .and_then(|m| m.modified())
            .ok();

        if let (Some(src), Some(obj)) = (src_time, obj_time)
            && obj > src
        {
            return Ok(entry_obj);
        }
    }

    let (target, march) = match arch {
        "rv32i" => ("riscv32", "rv32imac"),
        "rv32e" => ("riscv32", "rv32emac"),
        "rv64i" => ("riscv64", "rv64imac"),
        "rv64e" => ("riscv64", "rv64emac"),
        _ => return Err(format!("unknown architecture: {}", arch)),
    };

    let status = Command::new("clang")
        .arg(format!("--target={}", target))
        .arg(format!("-march={}", march))
        .arg("-c")
        .arg(&entry_s)
        .arg("-o")
        .arg(&entry_obj)
        .status()
        .map_err(|e| format!("failed to run clang: {}", e))?;

    if !status.success() {
        return Err(format!("clang failed to compile entry.S for {}", arch));
    }

    Ok(entry_obj)
}

/// Patch polkavm target spec files to use integer target-pointer-width.
/// Required for nightly >= 2025-09-01 (rust-lang/rust#144443).
fn patch_polkavm_target_specs(project_root: &Path) -> Result<(), String> {
    let targets_dir = project_root.join("programs/polkavm/crates/polkavm-linker/targets");

    // Known target spec locations
    let spec_files = [
        "legacy/riscv32emac-unknown-none-polkavm.json",
        "legacy/riscv64emac-unknown-none-polkavm.json",
        "1_91/riscv32emac-unknown-none-polkavm.json",
        "1_91/riscv64emac-unknown-none-polkavm.json",
    ];

    for spec in spec_files {
        let path = targets_dir.join(spec);
        if !path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

        // Replace string "32" or "64" with integer for target-pointer-width
        let patched = content
            .replace("\"target-pointer-width\": \"32\"", "\"target-pointer-width\": 32")
            .replace("\"target-pointer-width\": \"64\"", "\"target-pointer-width\": 64");

        if patched != content {
            std::fs::write(&path, patched)
                .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

/// Patch bench-common.rs to add missing architecture support for host builds.
/// This is needed because upstream polkavm doesn't support all host architectures.
fn patch_bench_common(project_root: &Path) -> Result<(), String> {
    let bench_common = project_root
        .join(POLKAVM_GUEST_PROGRAMS)
        .join("bench-common.rs");

    let content =
        std::fs::read_to_string(&bench_common).map_err(|e| format!("failed to read: {}", e))?;

    // Check if aarch64 support is already present
    if content.contains("target_arch = \"aarch64\"") {
        return Ok(());
    }

    // Add aarch64 panic handler after x86_64
    let patched = content.replace(
        r#"#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        core::arch::asm!("ud2", options(noreturn));
    }

    #[cfg(target_os = "solana")]"#,
        r#"#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        core::arch::asm!("ud2", options(noreturn));
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("brk #0x1", options(noreturn));
    }

    #[cfg(target_os = "solana")]"#,
    );

    std::fs::write(&bench_common, patched).map_err(|e| format!("failed to write: {}", e))?;
    Ok(())
}

/// Build a polkavm benchmark for the host (native) target.
pub fn build_host_benchmark(project_root: &Path, benchmark: &str) -> Result<PathBuf, String> {
    let guest_programs = project_root.join(POLKAVM_GUEST_PROGRAMS);
    let bench_name = format!("bench-{}", benchmark);

    // Patch polkavm files for compatibility with newer Rust toolchains
    patch_polkavm_target_specs(project_root)?;
    patch_bench_common(project_root)?;

    // Build as cdylib for the host (don't pass --target to avoid rebuilding std)
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&guest_programs);

    // Remove workspace-related CARGO env vars to avoid confusion
    cmd.env_remove("CARGO_TARGET_DIR");
    cmd.env_remove("CARGO_MANIFEST_DIR");
    cmd.env_remove("CARGO_PKG_NAME");

    let output = cmd
        .arg("build")
        .arg("--manifest-path")
        .arg(guest_programs.join("Cargo.toml"))
        .arg("--release")
        .arg("--lib")
        .arg("-p")
        .arg(&bench_name)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Extract last meaningful error line for the error message
        let error_detail = stderr
            .lines()
            .filter(|l| l.starts_with("error"))
            .last()
            .unwrap_or("unknown error");
        // Include more context in verbose mode via RUST_LOG
        tracing::debug!("cargo build stderr:\n{}", stderr);
        return Err(format!(
            "cargo build failed for {} (host): {}",
            benchmark, error_detail
        ));
    }

    // Find the built library (without --target, it's in target/release/)
    let lib_name = format!("lib{}.so", bench_name.replace('-', "_"));
    let lib_path = guest_programs
        .join("target")
        .join("release")
        .join(&lib_name);

    if !lib_path.exists() {
        return Err(format!("host library not found: {}", lib_path.display()));
    }

    // Copy to bin/host/
    let dest_dir = project_root.join("bin/host");
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("failed to create {}: {}", dest_dir.display(), e))?;

    let dest = dest_dir.join(format!("{}.so", benchmark));
    std::fs::copy(&lib_path, &dest).map_err(|e| format!("failed to copy: {}", e))?;

    Ok(dest)
}

/// Result of running a host benchmark.
#[derive(Debug, Clone)]
pub struct HostBenchResult {
    /// Average time per run in seconds.
    pub time_secs: f64,
    /// Hardware perf counters (if available).
    pub perf: Option<PerfCounters>,
}

/// Run a polkavm benchmark on the host.
///
/// The library must export `initialize` and `run` symbols.
pub fn run_host_benchmark(lib_path: &Path, runs: usize) -> Result<HostBenchResult, String> {
    // Load the library
    let lib = unsafe {
        Library::open(Some(lib_path), RTLD_NOW)
            .map_err(|e| format!("failed to load {}: {}", lib_path.display(), e))?
    };

    // Get function pointers
    type InitFn = unsafe extern "C" fn();
    type RunFn = unsafe extern "C" fn();

    let initialize: Symbol<InitFn> = unsafe {
        lib.get(b"initialize")
            .map_err(|e| format!("initialize symbol not found: {}", e))?
    };

    let run: Symbol<RunFn> = unsafe {
        lib.get(b"run")
            .map_err(|e| format!("run symbol not found: {}", e))?
    };

    // Initialize
    unsafe { initialize() };

    // Warm up
    for _ in 0..10 {
        unsafe { run() };
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
        unsafe { run() };
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_compile_entry_arch_mapping() {
        // Just verify the arch mapping doesn't panic
        let archs = ["rv32i", "rv32e", "rv64i", "rv64e"];
        for arch in archs {
            let (target, march) = match arch {
                "rv32i" => ("riscv32", "rv32imac"),
                "rv32e" => ("riscv32", "rv32emac"),
                "rv64i" => ("riscv64", "rv64imac"),
                "rv64e" => ("riscv64", "rv64emac"),
                _ => panic!("unknown arch"),
            };
            assert!(!target.is_empty());
            assert!(!march.is_empty());
        }
    }
}
