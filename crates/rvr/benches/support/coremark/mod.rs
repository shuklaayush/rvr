//! CoreMark benchmark building.
//!
//! CoreMark requires a platform port. We generate a minimal RISC-V bare-metal
//! port at build time to avoid modifying the upstream sources.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::Arch;

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
