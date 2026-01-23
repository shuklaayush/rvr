//! Rust build command for cross-compiling to RISC-V.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::terminal::{self, Spinner};

/// Embedded target specifications.
pub mod targets {
    pub const RV32I: &str = include_str!("../../../../toolchain/rv32i.json");
    pub const RV32E: &str = include_str!("../../../../toolchain/rv32e.json");
    pub const RV64I: &str = include_str!("../../../../toolchain/rv64i.json");
    pub const RV64E: &str = include_str!("../../../../toolchain/rv64e.json");
    pub const LINK_X: &str = include_str!("../../../../toolchain/link.x");

    /// Get target spec JSON for the given architecture.
    pub fn get_target_spec(arch: &str) -> Option<&'static str> {
        match arch {
            "rv32i" => Some(RV32I),
            "rv32e" => Some(RV32E),
            "rv64i" => Some(RV64I),
            "rv64e" => Some(RV64E),
            _ => None,
        }
    }
}

/// Parameters for building a single architecture.
struct BuildParams<'a> {
    arch: &'a str,
    project_path: &'a PathBuf,
    target_dir: &'a PathBuf,
    link_x_path: &'a PathBuf,
    crate_name: &'a str,
    bin_name: &'a str,
    toolchain: &'a str,
    features: Option<&'a str>,
    release: bool,
    output: Option<&'a PathBuf>,
    project_dir: &'a PathBuf,
    verbose: bool,
    quiet: bool,
}

/// Build a Rust project to RISC-V ELF.
#[allow(clippy::too_many_arguments)]
pub fn build_rust_project(
    path: &PathBuf,
    target_str: &str,
    output: Option<&PathBuf>,
    output_name: Option<&str>,
    toolchain: &str,
    features: Option<&str>,
    release: bool,
    verbose: bool,
    quiet: bool,
) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Resolve project path
    let project_path = if path.is_absolute() {
        path.clone()
    } else {
        project_dir.join(path)
    };

    // Check Cargo.toml exists
    let cargo_toml = project_path.join("Cargo.toml");
    if !cargo_toml.exists() {
        terminal::error(&format!("{} not found", cargo_toml.display()));
        return EXIT_FAILURE;
    }

    // Get project name from Cargo.toml
    let cargo_content = match std::fs::read_to_string(&cargo_toml) {
        Ok(c) => c,
        Err(e) => {
            terminal::error(&format!("Reading {}: {}", cargo_toml.display(), e));
            return EXIT_FAILURE;
        }
    };

    let crate_name = cargo_content
        .lines()
        .find(|line| line.starts_with("name"))
        .and_then(|line| line.split('=').nth(1))
        .map(|s| s.trim().trim_matches('"'))
        .unwrap_or("unknown");

    // Output name defaults to crate name
    let bin_name = output_name.unwrap_or(crate_name);

    // Parse target architectures
    let targets: Vec<&str> = target_str.split(',').map(|s| s.trim()).collect();

    // Create temp directory for target specs
    let target_dir = project_path.join("target/.rvr");
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        terminal::error(&format!("Creating {}: {}", target_dir.display(), e));
        return EXIT_FAILURE;
    }

    // Write linker script
    let link_x_path = target_dir.join("link.x");
    if let Err(e) = std::fs::write(&link_x_path, targets::LINK_X) {
        terminal::error(&format!("Writing link.x: {}", e));
        return EXIT_FAILURE;
    }

    for arch in &targets {
        if let Err(code) = build_for_arch(BuildParams {
            arch,
            project_path: &project_path,
            target_dir: &target_dir,
            link_x_path: &link_x_path,
            crate_name,
            bin_name,
            toolchain,
            features,
            release,
            output,
            project_dir: &project_dir,
            verbose,
            quiet,
        }) {
            return code;
        }
    }

    if !quiet {
        terminal::success("Build complete");
    }
    EXIT_SUCCESS
}

/// Build for a single architecture.
fn build_for_arch(p: BuildParams<'_>) -> Result<(), i32> {
    let BuildParams {
        arch,
        project_path,
        target_dir,
        link_x_path,
        crate_name,
        bin_name,
        toolchain,
        features,
        release,
        output,
        project_dir,
        verbose,
        quiet,
    } = p;
    // Get and write target spec
    let spec = match targets::get_target_spec(arch) {
        Some(s) => s,
        None => {
            terminal::error(&format!("Unknown target '{}'", arch));
            terminal::info("Supported targets: rv32i, rv32e, rv64i, rv64e");
            return Err(EXIT_FAILURE);
        }
    };

    let spec_path = target_dir.join(format!("{}.json", arch));
    if let Err(e) = std::fs::write(&spec_path, spec) {
        terminal::error(&format!("Writing {}: {}", spec_path.display(), e));
        return Err(EXIT_FAILURE);
    }

    // Create spinner for build (unless quiet or verbose)
    let spinner = if !quiet && !verbose {
        Some(Spinner::new(format!(
            "Building {} for {}",
            crate_name, arch
        )))
    } else if !quiet {
        eprintln!("Building {} for {}", crate_name, arch);
        None
    } else {
        None
    };

    // Determine RUSTFLAGS
    let cpu = if arch.starts_with("rv64") {
        "generic-rv64"
    } else {
        "generic-rv32"
    };

    let rustflags = format!(
        "-Clink-arg=-T{} -Clink-arg=--gc-sections -Ctarget-cpu={} -Ccode-model=medium",
        link_x_path.display(),
        cpu
    );

    // Build cargo command
    let mut cmd = Command::new("cargo");
    cmd.arg(format!("+{}", toolchain))
        .arg("build")
        .arg("--target")
        .arg(&spec_path)
        .arg("-Zbuild-std=core,alloc")
        .arg("-Zbuild-std-features=compiler-builtins-mem")
        .current_dir(project_path)
        .env("RUSTFLAGS", &rustflags);

    if release {
        cmd.arg("--release");
    }

    if let Some(feats) = features {
        cmd.arg("--features").arg(feats);
    }

    // Print the command if verbose mode is enabled
    if verbose {
        eprintln!();
        eprintln!("RUSTFLAGS=\"{}\" \\", rustflags);
        eprint!(
            "  cargo +{} build --target {}",
            toolchain,
            spec_path.display()
        );
        eprint!(" -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem");
        if release {
            eprint!(" --release");
        }
        if let Some(feats) = features {
            eprint!(" --features {}", feats);
        }
        eprintln!();
        eprintln!();
    }

    // Run with suppressed output if we have a spinner
    if spinner.is_some() {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            if let Some(s) = &spinner {
                s.finish_with_failure(&format!("Running cargo: {}", e));
            } else {
                terminal::error(&format!("Running cargo: {}", e));
            }
            return Err(EXIT_FAILURE);
        }
    };

    if !status.success() {
        if let Some(s) = &spinner {
            s.finish_with_failure(&format!("Build failed for {}", arch));
        } else if !quiet {
            terminal::error(&format!("Build failed for {}", arch));
        }
        return Err(EXIT_FAILURE);
    }

    // Copy output to destination
    let profile = if release { "release" } else { "debug" };
    let build_output = project_path
        .join("target")
        .join(arch)
        .join(profile)
        .join(crate_name);

    let dest_dir = match output {
        Some(o) => o.join(arch),
        None => project_dir.join("bin").join(arch),
    };

    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        if let Some(s) = &spinner {
            s.finish_with_failure(&format!("Creating {}: {}", dest_dir.display(), e));
        } else {
            terminal::error(&format!("Creating {}: {}", dest_dir.display(), e));
        }
        return Err(EXIT_FAILURE);
    }

    let dest_path = dest_dir.join(bin_name);
    if let Err(e) = std::fs::copy(&build_output, &dest_path) {
        if let Some(s) = &spinner {
            s.finish_with_failure(&format!(
                "Copying {} to {}: {}",
                build_output.display(),
                dest_path.display(),
                e
            ));
        } else {
            terminal::error(&format!(
                "Copying {} to {}: {}",
                build_output.display(),
                dest_path.display(),
                e
            ));
        }
        return Err(EXIT_FAILURE);
    }

    if let Some(s) = spinner {
        s.finish_with_success(&format!(
            "{} ({}) → {}",
            crate_name,
            arch,
            dest_path.display()
        ));
    } else if !quiet {
        terminal::path_output(&dest_path);
    }
    Ok(())
}
