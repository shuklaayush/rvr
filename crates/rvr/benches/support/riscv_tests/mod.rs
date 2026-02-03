//! riscv-tests benchmark building.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::Arch;


/// Build a riscv-tests benchmark using riscv-gcc.
/// Returns the path to the built ELF on success.
pub fn build_benchmark(
    project_dir: &std::path::Path,
    name: &str,
    arch: &Arch,
) -> Result<PathBuf, String> {
    let toolchain = rvr::build_utils::find_toolchain()
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
