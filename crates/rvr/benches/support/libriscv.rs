//! libriscv benchmark building.

use rvr::bench::Arch;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Build a libriscv benchmark directly from source.
/// These benchmarks use Linux syscall conventions (syscall 93 for exit).
pub fn build_benchmark(
    project_dir: &std::path::Path,
    name: &str,
    arch: Arch,
) -> Result<PathBuf, String> {
    let toolchain = rvr::build_utils::find_toolchain()
        .ok_or_else(|| "RISC-V toolchain not found (need riscv64-unknown-elf-gcc)".to_string())?;

    let gcc = format!("{toolchain}gcc");
    let out_dir = project_dir.join("bin").join(arch.as_str());
    let libriscv_dir = project_dir.join("programs/libriscv/binaries");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {e}"))?;

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
        _ => return Err(format!("unknown libriscv benchmark: {name}")),
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
    cmd.arg(format!("-march={march}"))
        .arg(format!("-mabi={mabi}"))
        .args(["-static", "-nostdlib", "-nostartfiles"]);
    if !is_asm {
        cmd.args(["-O3", "-fno-builtin"]);
    }
    cmd.arg(&src_path).arg("-o").arg(&out_path);

    let output = cmd
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run gcc: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gcc failed: {stderr}"));
    }

    Ok(out_path)
}
