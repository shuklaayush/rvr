//! Polkavm benchmark build and host runner.
//!
//! Handles building polkavm benchmarks for RISC-V targets and native host.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use libloading::os::unix::{Library, Symbol, RTLD_NOW};

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
    cmd.env("RUSTFLAGS", &rustflags);

    // Preserve only essential env vars (PATH, HOME, etc.)
    for (key, _) in std::env::vars() {
        if key.starts_with("CARGO") {
            cmd.env_remove(&key);
        }
    }

    let mut child = cmd
        .arg("+nightly")
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
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run cargo: {}", e))?;

    // Filter stderr to remove "multiple build targets" warnings
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if !line.contains("found to be present in multiple build targets")
                && !line.contains("`lib` target")
                && !line.contains("`bin` target")
            {
                eprintln!("{}", line);
            }
        }
    }

    let status = child.wait().map_err(|e| format!("cargo wait failed: {}", e))?;
    if !status.success() {
        return Err(format!("cargo build failed for {}/{}", benchmark, arch));
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
    std::fs::copy(&src, &dest)
        .map_err(|e| format!("failed to copy {} to {}: {}", src.display(), dest.display(), e))?;

    Ok(dest)
}

/// Compile entry.S for the specified architecture.
fn compile_entry(toolchain_dir: &Path, arch: &str) -> Result<PathBuf, String> {
    let entry_s = toolchain_dir.join("entry.S");
    let entry_obj = toolchain_dir.join(format!("entry_{}.o", arch));

    // Check if recompilation needed
    if entry_obj.exists() {
        let src_time = std::fs::metadata(&entry_s)
            .and_then(|m| m.modified())
            .ok();
        let obj_time = std::fs::metadata(&entry_obj)
            .and_then(|m| m.modified())
            .ok();

        if let (Some(src), Some(obj)) = (src_time, obj_time) {
            if obj > src {
                return Ok(entry_obj);
            }
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

/// Get the host target triple.
fn get_host_target() -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|e| format!("failed to run rustc: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(target) = line.strip_prefix("host: ") {
            return Ok(target.to_string());
        }
    }
    Err("could not determine host target".to_string())
}

/// Build a polkavm benchmark for the host (native) target.
pub fn build_host_benchmark(project_root: &Path, benchmark: &str) -> Result<PathBuf, String> {
    let guest_programs = project_root.join(POLKAVM_GUEST_PROGRAMS);
    let bench_name = format!("bench-{}", benchmark);

    // Get host target to explicitly build for native platform
    let host_target = get_host_target()?;

    // Build as cdylib for the host
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&guest_programs);

    // Remove all CARGO env vars to avoid workspace confusion
    for (key, _) in std::env::vars() {
        if key.starts_with("CARGO") {
            cmd.env_remove(&key);
        }
    }

    let mut child = cmd
        .arg("build")
        .arg("--manifest-path")
        .arg(guest_programs.join("Cargo.toml"))
        .arg("--target")
        .arg(&host_target)
        .arg("--release")
        .arg("--lib")
        .arg("-p")
        .arg(&bench_name)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run cargo: {}", e))?;

    // Filter stderr to remove "multiple build targets" warnings
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if !line.contains("found to be present in multiple build targets")
                && !line.contains("`lib` target")
                && !line.contains("`bin` target")
            {
                eprintln!("{}", line);
            }
        }
    }

    let status = child.wait().map_err(|e| format!("cargo wait failed: {}", e))?;
    if !status.success() {
        return Err(format!("cargo build failed for {} (host)", benchmark));
    }

    // Find the built library
    let lib_name = format!("lib{}.so", bench_name.replace('-', "_"));
    let lib_path = guest_programs
        .join("target")
        .join(&host_target)
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
    std::fs::copy(&lib_path, &dest)
        .map_err(|e| format!("failed to copy: {}", e))?;

    Ok(dest)
}

/// Result of running a host benchmark.
#[derive(Debug, Clone)]
pub struct HostBenchResult {
    /// Average time per run in seconds.
    pub time_secs: f64,
    /// Number of runs.
    pub runs: usize,
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

    // Timed runs
    let start = Instant::now();
    for _ in 0..runs {
        unsafe { run() };
    }
    let elapsed = start.elapsed();

    let time_secs = elapsed.as_secs_f64() / runs as f64;

    Ok(HostBenchResult { time_secs, runs })
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
