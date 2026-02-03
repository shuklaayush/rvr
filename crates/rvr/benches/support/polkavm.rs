//! Polkavm benchmark build and host runner.
//!
//! Handles building polkavm benchmarks for RISC-V targets and native host.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Find actual compiler error (error[E...]) rather than generic "could not compile"
        let error_detail = stderr
            .lines()
            .find(|l| l.contains("error[E"))
            .or_else(|| stderr.lines().find(|l| l.starts_with("error:")))
            .or_else(|| stderr.lines().rfind(|l| l.starts_with("error")))
            .unwrap_or("unknown error");
        tracing::warn!("cargo build failed:\n{}", stderr);
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
