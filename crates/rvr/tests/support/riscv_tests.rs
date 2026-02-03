//! RISC-V test suite runner for integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use rvr_emit::Backend;
use rvr::{CompileOptions, Compiler, Runner, build_utils, compile_with_options};

/// Tests to skip (not compatible with static recompilation).
const SKIP_TESTS: &[&str] = &[
    // fence.i tests self-modifying code - incompatible with static recompilation
    "rv32ui-p-fence_i",
    "rv64ui-p-fence_i",
];

/// Check if a test should be skipped.
pub fn should_skip(name: &str) -> bool {
    if SKIP_TESTS.contains(&name) {
        return true;
    }
    // Skip machine/supervisor mode tests
    if name.contains("mi-p-") || name.contains("si-p-") {
        return true;
    }
    false
}

/// Run a single test.
pub fn run_test(
    elf_path: &Path,
    timeout: Duration,
    compiler: &Compiler,
    backend: Backend,
) -> Result<(), String> {
    let name = elf_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    if should_skip(&name) {
        return Ok(());
    }

    let temp_dir = tempfile::tempdir().map_err(|e| format!("temp dir failed: {e}"))?;
    let out_dir = temp_dir.path().join("out");

    let options = CompileOptions::new()
        .with_htif(true)
        .with_quiet(true)
        .with_compiler(compiler.clone())
        .with_backend(backend);

    compile_with_options(elf_path, &out_dir, options)
        .map_err(|e| format!("compile failed: {e}"))?;

    run_with_timeout(&out_dir, elf_path, timeout)
        .map_err(|e| format!("{name} failed: {e}"))
}

fn run_with_timeout(lib_dir: &Path, elf_path: &Path, timeout: Duration) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let lib_dir = lib_dir.to_path_buf();
    let elf_path = elf_path.to_path_buf();

    std::thread::spawn(move || {
        let mut runner = match Runner::load(&lib_dir, &elf_path) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Err(format!("load failed: {e}")));
                return;
            }
        };
        match runner.run() {
            Ok(result) => {
                if result.exit_code == 0 {
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err(format!("exit={}", result.exit_code)));
                }
            }
            Err(e) => {
                let _ = tx.send(Err(format!("run failed: {e}")));
            }
        }
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err("timeout".to_string()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err("crash".to_string()),
    }
}

/// RISC-V test categories (directory names under riscv-tests/isa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestCategory {
    Rv32ui,
    Rv32um,
    Rv32ua,
    Rv32uc,
    Rv32mi,
    Rv32si,
    Rv32uzba,
    Rv32uzbb,
    Rv32uzbs,
    Rv64ui,
    Rv64um,
    Rv64ua,
    Rv64uc,
    Rv64mi,
    Rv64si,
    Rv64uzba,
    Rv64uzbb,
    Rv64uzbs,
    Rv32e,
    Rv64e,
}

impl TestCategory {
    pub const ALL: &'static [TestCategory] = &[
        Self::Rv32ui,
        Self::Rv32um,
        Self::Rv32ua,
        Self::Rv32uc,
        Self::Rv32mi,
        Self::Rv32si,
        Self::Rv32uzba,
        Self::Rv32uzbb,
        Self::Rv32uzbs,
        Self::Rv64ui,
        Self::Rv64um,
        Self::Rv64ua,
        Self::Rv64uc,
        Self::Rv64mi,
        Self::Rv64si,
        Self::Rv64uzba,
        Self::Rv64uzbb,
        Self::Rv64uzbs,
        Self::Rv32e,
        Self::Rv64e,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rv32ui => "rv32ui",
            Self::Rv32um => "rv32um",
            Self::Rv32ua => "rv32ua",
            Self::Rv32uc => "rv32uc",
            Self::Rv32mi => "rv32mi",
            Self::Rv32si => "rv32si",
            Self::Rv32uzba => "rv32uzba",
            Self::Rv32uzbb => "rv32uzbb",
            Self::Rv32uzbs => "rv32uzbs",
            Self::Rv64ui => "rv64ui",
            Self::Rv64um => "rv64um",
            Self::Rv64ua => "rv64ua",
            Self::Rv64uc => "rv64uc",
            Self::Rv64mi => "rv64mi",
            Self::Rv64si => "rv64si",
            Self::Rv64uzba => "rv64uzba",
            Self::Rv64uzbb => "rv64uzbb",
            Self::Rv64uzbs => "rv64uzbs",
            Self::Rv32e => "rv32e",
            Self::Rv64e => "rv64e",
        }
    }

    pub fn march_mabi(&self) -> (&'static str, &'static str) {
        match self {
            Self::Rv32ui => ("rv32i", "ilp32"),
            Self::Rv32um => ("rv32im", "ilp32"),
            Self::Rv32ua => ("rv32ima", "ilp32"),
            Self::Rv32uc => ("rv32imac", "ilp32"),
            Self::Rv32mi => ("rv32im", "ilp32"),
            Self::Rv32si => ("rv32im", "ilp32"),
            Self::Rv32uzba => ("rv32im_zba", "ilp32"),
            Self::Rv32uzbb => ("rv32im_zbb", "ilp32"),
            Self::Rv32uzbs => ("rv32im_zbs", "ilp32"),
            Self::Rv64ui => ("rv64i", "lp64"),
            Self::Rv64um => ("rv64im", "lp64"),
            Self::Rv64ua => ("rv64ima", "lp64"),
            Self::Rv64uc => ("rv64imac", "lp64"),
            Self::Rv64mi => ("rv64im", "lp64"),
            Self::Rv64si => ("rv64im", "lp64"),
            Self::Rv64uzba => ("rv64im_zba", "lp64"),
            Self::Rv64uzbb => ("rv64im_zbb", "lp64"),
            Self::Rv64uzbs => ("rv64im_zbs", "lp64"),
            Self::Rv32e => ("rv32e", "ilp32e"),
            Self::Rv64e => ("rv64e", "lp64e"),
        }
    }
}

pub struct BuildConfig {
    pub categories: Vec<TestCategory>,
    pub src_dir: PathBuf,
    pub out_dir: PathBuf,
    pub toolchain: String,
}

impl BuildConfig {
    pub fn new(categories: Vec<TestCategory>) -> Self {
        Self {
            categories,
            src_dir: PathBuf::new(),
            out_dir: PathBuf::new(),
            toolchain: String::new(),
        }
    }

    pub fn with_src_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.src_dir = dir.into();
        self
    }

    pub fn with_out_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.out_dir = dir.into();
        self
    }

    pub fn with_toolchain(mut self, toolchain: impl Into<String>) -> Self {
        self.toolchain = toolchain.into();
        self
    }
}

pub fn find_toolchain() -> Option<String> {
    build_utils::find_toolchain()
}

pub fn build_tests(config: &BuildConfig) -> Result<(), String> {
    if !config.src_dir.exists() {
        return Err(format!(
            "source directory not found: {}\nMake sure riscv-tests submodule is initialized",
            config.src_dir.display()
        ));
    }

    let mut failures = 0usize;
    for &category in &config.categories {
        if let Err(e) = build_category(category, config) {
            eprintln!("  {}: {}", category.as_str(), e);
            failures += 1;
        }
    }

    if failures > 0 {
        Err(format!("{} categories failed", failures))
    } else {
        Ok(())
    }
}

fn build_category(category: TestCategory, config: &BuildConfig) -> Result<(), String> {
    let cat_name = category.as_str();
    let src_dir = config.src_dir.join(cat_name);
    let out_dir = config.out_dir.join(cat_name);

    if !src_dir.exists() {
        return Err(format!("source directory not found: {}", src_dir.display()));
    }

    fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {e}"))?;

    let (march, mabi) = category.march_mabi();
    let gcc = format!("{}gcc", config.toolchain);

    let env_p = config.src_dir.join("../env/p");
    let macros = config.src_dir.join("macros/scalar");
    let link_ld = config.src_dir.join("../env/p/link.ld");

    let entries = fs::read_dir(&src_dir).map_err(|e| format!("failed to read dir: {e}"))?;

    let mut failed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("S") {
            continue;
        }

        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };

        let out_name = format!("{}-p-{}", cat_name, stem);
        let out_path = out_dir.join(&out_name);

        let status = Command::new(&gcc)
            .arg(format!("-march={}", march))
            .arg(format!("-mabi={}", mabi))
            .args(["-static", "-mcmodel=medany", "-fvisibility=hidden"])
            .args(["-nostdlib", "-nostartfiles"])
            .arg(format!("-I{}", env_p.display()))
            .arg(format!("-I{}", macros.display()))
            .arg(format!("-T{}", link_ld.display()))
            .arg(&path)
            .arg("-o")
            .arg(&out_path)
            .stderr(std::process::Stdio::null())
            .status();

        if !matches!(status, Ok(s) if s.success()) {
            failed += 1;
        }
    }

    if failed > 0 {
        Err(format!("{failed} files failed"))
    } else {
        Ok(())
    }
}
