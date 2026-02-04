//! Rust benchmark builder (RISC-V ELF via cargo).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rvr::bench::Arch;

const fn target_spec_for(arch: Arch) -> &'static str {
    match arch {
        Arch::Rv32i => "rv32i",
        Arch::Rv32e => "rv32e",
        Arch::Rv64i => "rv64i",
        Arch::Rv64e => "rv64e",
    }
}

fn read_project_name(cargo_toml: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(cargo_toml)
        .map_err(|e| format!("failed to read {}: {}", cargo_toml.display(), e))?;
    let name = content
        .lines()
        .find(|line| line.trim_start().starts_with("name"))
        .and_then(|line| line.split('=').nth(1))
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty());
    name.map(std::string::ToString::to_string)
        .ok_or_else(|| "failed to find package name".to_string())
}

/// Build a Rust benchmark as a RISC-V ELF.
pub fn build_benchmark(
    project_dir: &Path,
    project_path: &str,
    arch: Arch,
    bin_name: Option<&str>,
) -> Result<PathBuf, String> {
    let project_root = project_dir.join(project_path);
    let cargo_toml = project_root.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(format!("missing Cargo.toml at {}", cargo_toml.display()));
    }

    let bin_name = match bin_name {
        Some(name) => name.to_string(),
        None => read_project_name(&cargo_toml)?,
    };

    let target = target_spec_for(arch);
    let target_dir = project_dir.join("target/.rvr_bench");
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("failed to create {}: {}", target_dir.display(), e))?;

    let spec_src = project_dir.join("toolchain").join(format!("{target}.json"));
    if !spec_src.exists() {
        return Err(format!("missing target spec: {}", spec_src.display()));
    }

    let spec_path = target_dir.join(format!("{target}.json"));
    std::fs::copy(&spec_src, &spec_path).map_err(|e| format!("failed to copy target spec: {e}"))?;

    let link_x = project_dir.join("toolchain/link.x");
    let link_out = target_dir.join("link.x");
    std::fs::copy(&link_x, &link_out).map_err(|e| format!("failed to copy link.x: {e}"))?;

    let cpu = if matches!(arch, Arch::Rv64i | Arch::Rv64e) {
        "generic-rv64"
    } else {
        "generic-rv32"
    };

    let rustflags = format!(
        "-Clink-arg=-T{} -Clink-arg=--gc-sections -Ctarget-cpu={} -Ccode-model=medium",
        link_out.display(),
        cpu
    );

    let mut cmd = Command::new("cargo");
    cmd.arg("+nightly")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg(&spec_path)
        .arg("--bin")
        .arg(&bin_name)
        .arg("--manifest-path")
        .arg(&cargo_toml)
        .env("RUSTFLAGS", rustflags)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run cargo: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo build failed: {stderr}"));
    }

    let elf_path = project_root
        .join("target")
        .join(target)
        .join("release")
        .join(&bin_name);

    if !elf_path.exists() {
        return Err(format!("missing output ELF: {}", elf_path.display()));
    }

    let out_dir = project_dir.join("bin").join(arch.as_str());
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create {}: {}", out_dir.display(), e))?;
    let dest = out_dir.join(&bin_name);
    std::fs::copy(&elf_path, &dest)
        .map_err(|e| format!("failed to copy {}: {}", elf_path.display(), e))?;

    Ok(dest)
}
