//! CoreMark benchmark building.
//!
//! CoreMark requires a platform port. We generate a minimal RISC-V bare-metal
//! port at build time to avoid modifying the upstream sources.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::Arch;

/// Host port header for CoreMark (64-bit compatible).
const HOST_PORTME_H: &str = include_str!("host_portme.h");

/// Host port implementation for CoreMark.
const HOST_PORTME_C: &str = include_str!("host_portme.c");

/// Minimal RISC-V port header for CoreMark.
const PORTME_H: &str = include_str!("riscv_portme.h");

/// Minimal RISC-V port implementation for CoreMark.
const PORTME_C: &str = include_str!("riscv_portme.c");

/// Build CoreMark benchmark for RISC-V.
pub fn build_benchmark(project_dir: &std::path::Path, arch: &Arch) -> Result<PathBuf, String> {
    let toolchain = rvr::build_utils::find_toolchain()
        .ok_or_else(|| "RISC-V toolchain not found (need riscv64-unknown-elf-gcc)".to_string())?;

    let gcc = format!("{}gcc", toolchain);
    let out_dir = project_dir.join("bin").join(arch.as_str());
    let coremark_dir = project_dir.join("programs/coremark");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

    // Create port files in temp directory (not in submodule or target)
    let port_dir = std::env::temp_dir().join("rvr_coremark_port");
    std::fs::create_dir_all(&port_dir).map_err(|e| format!("failed to create port dir: {}", e))?;

    std::fs::write(port_dir.join("core_portme.h"), PORTME_H)
        .map_err(|e| format!("failed to write portme.h: {}", e))?;
    std::fs::write(port_dir.join("core_portme.c"), PORTME_C)
        .map_err(|e| format!("failed to write portme.c: {}", e))?;

    let (march, mabi) = match arch {
        Arch::Rv32i | Arch::Rv32e => ("rv32im_zicsr", "ilp32"),
        Arch::Rv64i | Arch::Rv64e => ("rv64im_zicsr", "lp64"),
    };

    let out_path = out_dir.join("coremark");

    let core_files: Vec<PathBuf> = vec![
        coremark_dir.join("core_list_join.c"),
        coremark_dir.join("core_main.c"),
        coremark_dir.join("core_matrix.c"),
        coremark_dir.join("core_state.c"),
        coremark_dir.join("core_util.c"),
        port_dir.join("core_portme.c"),
    ];

    let mut cmd = Command::new(&gcc);
    cmd.arg(format!("-march={}", march))
        .arg(format!("-mabi={}", mabi))
        .args(["-O3", "-funroll-loops"])
        .args(["-static", "-nostdlib", "-nostartfiles", "-ffreestanding"])
        .args(["-DITERATIONS=400000", "-DPERFORMANCE_RUN=1"])
        .arg(format!("-I{}", coremark_dir.display()))
        .arg(format!("-I{}", port_dir.display()))
        .args(&core_files)
        .arg("-lgcc") // For 64-bit division on RV32
        .arg("-o")
        .arg(&out_path);

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

/// Build CoreMark as native executable for the host.
pub fn build_host_benchmark(project_dir: &std::path::Path) -> Result<PathBuf, String> {
    let out_dir = project_dir.join("bin/host");
    let coremark_dir = project_dir.join("programs/coremark");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

    // Create host port files in temp directory
    let port_dir = std::env::temp_dir().join("rvr_coremark_host_port");
    std::fs::create_dir_all(&port_dir).map_err(|e| format!("failed to create port dir: {}", e))?;

    std::fs::write(port_dir.join("core_portme.h"), HOST_PORTME_H)
        .map_err(|e| format!("failed to write host portme.h: {}", e))?;
    std::fs::write(port_dir.join("core_portme.c"), HOST_PORTME_C)
        .map_err(|e| format!("failed to write host portme.c: {}", e))?;

    let out_path = out_dir.join("coremark");

    let core_files: Vec<PathBuf> = vec![
        coremark_dir.join("core_list_join.c"),
        coremark_dir.join("core_main.c"),
        coremark_dir.join("core_matrix.c"),
        coremark_dir.join("core_state.c"),
        coremark_dir.join("core_util.c"),
        port_dir.join("core_portme.c"),
    ];

    let mut cmd = Command::new("cc");
    cmd.args(["-O3", "-funroll-loops"])
        .args(["-DITERATIONS=400000", "-DPERFORMANCE_RUN=1"])
        .arg(format!("-I{}", coremark_dir.display()))
        .arg(format!("-I{}", port_dir.display()))
        .args(&core_files)
        .arg("-o")
        .arg(&out_path);

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
    pub time_secs: f64,
    pub perf: Option<rvr::PerfCounters>,
}

/// Run CoreMark host benchmark.
pub fn run_host_benchmark(
    bin_path: &std::path::Path,
    runs: usize,
) -> Result<HostBenchResult, String> {
    use std::time::Instant;

    let runs = runs.max(1);

    // Warm up
    let _ = Command::new(bin_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let start = Instant::now();
    for i in 0..runs {
        let output = Command::new(bin_path)
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("failed to run benchmark: {}", e))?;

        if !output.status.success() {
            return Err("benchmark failed".to_string());
        }

        // Print output on last run to show "No errors detected"
        if i == runs - 1 {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Check for errors - CoreMark prints "Correct operation validated" or errors
            if stdout.contains("ERROR") {
                return Err(format!("CoreMark validation failed:\n{}", stdout));
            }
            // Print the output for user to see results
            print!("{}", stdout);
        }
    }
    let elapsed = start.elapsed();
    let time_secs = elapsed.as_secs_f64() / runs as f64;

    Ok(HostBenchResult {
        time_secs,
        perf: None,
    })
}
