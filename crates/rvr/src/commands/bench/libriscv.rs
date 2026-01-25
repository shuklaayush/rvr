//! libriscv benchmark building.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::Arch;

/// Fib benchmark from libriscv - adapted for riscv-tests runtime.
const FIB_C: &str = r#"
// Fibonacci benchmark from libriscv, adapted for riscv-tests runtime.
// Original: programs/libriscv/binaries/measure_mips/fib.c

static long fib(long n, long acc, long prev)
{
    if (n == 0)
        return acc;
    else
        return fib(n - 1, prev + acc, acc);
}

int main(void)
{
    // Reduced from original 256M to run in reasonable time
    const volatile long n = 50000000;
    long result = fib(n, 0, 1);
    return (int)(result & 0xFF);
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

    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create output dir: {}", e))?;
    std::fs::create_dir_all(&build_dir)
        .map_err(|e| format!("failed to create build dir: {}", e))?;

    // Write the benchmark source to build directory
    let src_path = build_dir.join(format!("{}.c", name));
    let src_content = match name {
        "fib" => FIB_C,
        _ => return Err(format!("unknown libriscv benchmark: {}", name)),
    };
    std::fs::write(&src_path, src_content)
        .map_err(|e| format!("failed to write source: {}", e))?;

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
        .arg(format!("-T{}", link_ld.display()))
        .arg(&crt_s)
        .arg(&syscalls_c)
        .arg(&src_path)
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
