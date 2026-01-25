//! libriscv benchmark building.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::Arch;

/// Wrapper for fib.c - includes original source and calls fib() directly.
const FIB_WRAPPER_C: &str = r#"
// Wrapper for libriscv fib benchmark.
// Renames _start to avoid conflict with CRT, then calls fib() directly.

#define _start _unused_original_start
#include "fib.c"
#undef _start

int main(void) {
    // Use 50M iterations (reduced from original 256M for practical runtime)
    const volatile long n = 50000000;
    return (int)(fib(n, 0, 1) & 0xFF);
}
"#;

/// Build a libriscv benchmark using riscv-gcc with riscv-tests runtime.
pub fn build_benchmark(
    project_dir: &std::path::Path,
    name: &str,
    arch: &Arch,
) -> Result<PathBuf, String> {
    let toolchain = rvr::tests::find_toolchain()
        .ok_or_else(|| "RISC-V toolchain not found (need riscv64-unknown-elf-gcc)".to_string())?;

    let gcc = format!("{}gcc", toolchain);
    let out_dir = project_dir.join("bin").join(arch.as_str());
    let build_dir = project_dir.join("target/libriscv-build").join(name);

    // Use riscv-tests common infrastructure
    let bench_dir = project_dir.join("programs/riscv-tests/benchmarks");
    let common_dir = bench_dir.join("common");
    let env_dir = project_dir.join("programs/riscv-tests/env");
    let link_ld = common_dir.join("test.ld");
    let crt_s = common_dir.join("crt.S");
    let syscalls_c = common_dir.join("syscalls.c");

    // libriscv source directories
    let libriscv_dir = project_dir.join("programs/libriscv/binaries");

    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create output dir: {}", e))?;
    std::fs::create_dir_all(&build_dir)
        .map_err(|e| format!("failed to create build dir: {}", e))?;

    // Get wrapper content and source include path for this benchmark
    let (wrapper_content, src_include_dir) = match name {
        "fib" => (FIB_WRAPPER_C, libriscv_dir.join("measure_mips")),
        _ => return Err(format!("unknown libriscv benchmark: {}", name)),
    };

    // Write wrapper to build directory
    let wrapper_path = build_dir.join(format!("{}_wrapper.c", name));
    std::fs::write(&wrapper_path, wrapper_content)
        .map_err(|e| format!("failed to write wrapper: {}", e))?;

    let (march, mabi) = match arch {
        Arch::Rv32i | Arch::Rv32e => ("rv32im_zicsr", "ilp32"),
        Arch::Rv64i | Arch::Rv64e => ("rv64im_zicsr", "lp64"),
    };

    let out_path = out_dir.join(name);

    let mut cmd = Command::new(&gcc);
    cmd.arg(format!("-march={}", march))
        .arg(format!("-mabi={}", mabi))
        .args(["-static", "-mcmodel=medany", "-fvisibility=hidden"])
        .args(["-nostdlib", "-nostartfiles"])
        .args(["-std=gnu99", "-O3", "-fno-common", "-fno-builtin-printf"])
        .arg(format!("-I{}", common_dir.display()))
        .arg(format!("-I{}", env_dir.display()))
        .arg(format!("-I{}", src_include_dir.display()))
        .arg(format!("-T{}", link_ld.display()))
        .arg(&crt_s)
        .arg(&syscalls_c)
        .arg(&wrapper_path)
        .arg("-lgcc")
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
