//! RISC-V architecture test runner for integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use rvr::{CompileOptions, Compiler, Runner, build_utils, compile_with_options};
use rvr_emit::Backend;

/// Maximum signature region size (64KB should be enough for any test).
const MAX_SIG_SIZE: usize = 0x10000;

/// Tests to skip (not compatible with static recompilation).
const SKIP_TESTS: &[&str] = &[
    // fence.i tests self-modifying code
    "fence_i", "fence-01",
];

/// Harness files location.
const HARNESS_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/support/riscv_arch_test_harness"
);

/// RISC-V architecture test category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchTestCategory {
    Rv64iI,
    Rv64iM,
    Rv64iA,
    Rv64iC,
    Rv64iB,
    Rv64iZicond,
    Rv32iI,
    Rv32iM,
    Rv32iA,
    Rv32iC,
    Rv32iB,
    Rv32iZicond,
}

impl ArchTestCategory {
    pub const ALL: &'static [Self] = &[
        Self::Rv64iI,
        Self::Rv64iM,
        Self::Rv64iA,
        Self::Rv64iC,
        Self::Rv64iB,
        Self::Rv64iZicond,
        Self::Rv32iI,
        Self::Rv32iM,
        Self::Rv32iA,
        Self::Rv32iC,
        Self::Rv32iB,
        Self::Rv32iZicond,
    ];

    pub const fn src_subdir(self) -> &'static str {
        match self {
            Self::Rv64iI => "rv64i_m/I",
            Self::Rv64iM => "rv64i_m/M",
            Self::Rv64iA => "rv64i_m/A",
            Self::Rv64iC => "rv64i_m/C",
            Self::Rv64iB => "rv64i_m/B",
            Self::Rv64iZicond => "rv64i_m/Zicond",
            Self::Rv32iI => "rv32i_m/I",
            Self::Rv32iM => "rv32i_m/M",
            Self::Rv32iA => "rv32i_m/A",
            Self::Rv32iC => "rv32i_m/C",
            Self::Rv32iB => "rv32i_m/B",
            Self::Rv32iZicond => "rv32i_m/Zicond",
        }
    }

    pub const fn out_subdir(self) -> &'static str {
        match self {
            Self::Rv64iI => "rv64i_m-I",
            Self::Rv64iM => "rv64i_m-M",
            Self::Rv64iA => "rv64i_m-A",
            Self::Rv64iC => "rv64i_m-C",
            Self::Rv64iB => "rv64i_m-B",
            Self::Rv64iZicond => "rv64i_m-Zicond",
            Self::Rv32iI => "rv32i_m-I",
            Self::Rv32iM => "rv32i_m-M",
            Self::Rv32iA => "rv32i_m-A",
            Self::Rv32iC => "rv32i_m-C",
            Self::Rv32iB => "rv32i_m-B",
            Self::Rv32iZicond => "rv32i_m-Zicond",
        }
    }

    pub const fn march_mabi(self) -> (&'static str, &'static str) {
        match self {
            Self::Rv64iB => ("rv64imac_zicsr_zicond_zba_zbb_zbs", "lp64"),
            Self::Rv64iI | Self::Rv64iM | Self::Rv64iA | Self::Rv64iC | Self::Rv64iZicond => {
                ("rv64imac_zicsr_zicond", "lp64")
            }
            Self::Rv32iB => ("rv32imac_zicsr_zicond_zba_zbb_zbs", "ilp32"),
            Self::Rv32iI | Self::Rv32iM | Self::Rv32iA | Self::Rv32iC | Self::Rv32iZicond => {
                ("rv32imac_zicsr_zicond", "ilp32")
            }
        }
    }
}

pub struct ArchBuildConfig {
    pub categories: Vec<ArchTestCategory>,
    pub src_dir: PathBuf,
    pub out_dir: PathBuf,
    pub refs_dir: PathBuf,
    pub toolchain: String,
    pub gen_refs: bool,
}

impl ArchBuildConfig {
    pub const fn new(categories: Vec<ArchTestCategory>) -> Self {
        Self {
            categories,
            src_dir: PathBuf::new(),
            out_dir: PathBuf::new(),
            refs_dir: PathBuf::new(),
            toolchain: String::new(),
            gen_refs: false,
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

    pub fn with_refs_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.refs_dir = dir.into();
        self
    }

    pub fn with_toolchain(mut self, toolchain: impl Into<String>) -> Self {
        self.toolchain = toolchain.into();
        self
    }

    pub const fn with_gen_refs(mut self, gen_refs: bool) -> Self {
        self.gen_refs = gen_refs;
        self
    }
}

pub fn find_toolchain() -> Option<String> {
    build_utils::find_toolchain()
}

pub fn find_spike() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("spike");
        if candidate.is_file() {
            return candidate.to_str().map(std::string::ToString::to_string);
        }
    }
    None
}

pub fn run_test(
    elf_path: &Path,
    ref_path: &Path,
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

    if !ref_path.exists() {
        return Err("missing reference signature".to_string());
    }

    let temp_dir = tempfile::tempdir().map_err(|e| format!("temp dir failed: {e}"))?;
    let out_dir = temp_dir.path().join("out");

    let options = CompileOptions::new()
        .with_htif(true)
        .with_quiet(true)
        .with_compiler(compiler.clone())
        .with_backend(backend);

    compile_with_options(elf_path, &out_dir, &options)
        .map_err(|e| format!("compile failed: {e}"))?;

    let signature = run_and_extract_signature(&out_dir, elf_path, timeout)?;
    let reference =
        fs::read_to_string(ref_path).map_err(|e| format!("failed to read reference: {e}"))?;

    if compare_signatures(&signature, &reference) {
        Ok(())
    } else {
        Err(format!("{name} signature mismatch"))
    }
}

fn should_skip(name: &str) -> bool {
    SKIP_TESTS.iter().any(|&skip| name.contains(skip))
}

fn run_and_extract_signature(
    lib_dir: &Path,
    elf_path: &Path,
    timeout: Duration,
) -> Result<String, String> {
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
            Ok(_) => {
                let _ = tx.send(extract_signature_from_runner(&runner));
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

fn extract_signature_from_runner(runner: &Runner) -> Result<String, String> {
    let sig_start = runner
        .lookup_symbol("begin_signature")
        .ok_or("begin_signature symbol not found")?;
    let sig_end = runner
        .lookup_symbol("end_signature")
        .ok_or("end_signature symbol not found")?;

    if sig_end <= sig_start {
        return Err("invalid signature bounds".to_string());
    }

    let sig_size = usize::try_from(sig_end - sig_start).unwrap_or(usize::MAX);
    if sig_size > MAX_SIG_SIZE {
        return Err(format!("signature too large: {sig_size} bytes"));
    }

    let num_words = sig_size / 4;
    let mut lines = Vec::with_capacity(num_words);
    let mut buf = [0u8; 4];
    for i in 0..num_words {
        let addr = sig_start + (i as u64 * 4);
        let _ = runner.read_memory(addr, &mut buf);
        let word = u32::from_le_bytes(buf);
        lines.push(format!("{word:08x}"));
    }

    Ok(lines.join("\n"))
}

fn compare_signatures(actual: &str, reference: &str) -> bool {
    let actual_lines: Vec<&str> = actual
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let reference_lines: Vec<&str> = reference
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    actual_lines == reference_lines
}

pub fn build_tests(config: &ArchBuildConfig) -> Result<(), String> {
    if !config.src_dir.exists() {
        return Err(format!(
            "source directory not found: {}\nMake sure riscv-arch-test submodule is initialized",
            config.src_dir.display()
        ));
    }

    if config.gen_refs && find_spike().is_none() {
        return Err("Spike not found".to_string());
    }

    let mut failures = 0usize;
    for &category in &config.categories {
        if let Err(e) = build_category(category, config) {
            eprintln!("  {}: {}", category.out_subdir(), e);
            failures += 1;
        }
    }

    if failures > 0 {
        Err(format!("{failures} categories failed"))
    } else {
        Ok(())
    }
}

fn build_category(category: ArchTestCategory, config: &ArchBuildConfig) -> Result<(), String> {
    let src_dir = config.src_dir.join(category.src_subdir());
    let out_dir = config.out_dir.join(category.out_subdir());
    let refs_dir = config.refs_dir.join(category.out_subdir());

    if !src_dir.exists() {
        return Err(format!("source directory not found: {}", src_dir.display()));
    }

    fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {e}"))?;
    if config.gen_refs {
        fs::create_dir_all(&refs_dir).map_err(|e| format!("failed to create refs dir: {e}"))?;
    }

    let (march, mabi) = category.march_mabi();
    let gcc = format!("{}gcc", config.toolchain);

    let harness_dir = PathBuf::from(HARNESS_DIR);
    let model_test = harness_dir.join("model_test.h");
    let link_ld = harness_dir.join("link.ld");

    let entries = fs::read_dir(&src_dir).map_err(|e| format!("failed to read dir: {e}"))?;
    let mut failed = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("S") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        let out_name = format!("{}-{}", category.out_subdir(), stem);
        let out_path = out_dir.join(&out_name);

        let status = Command::new(&gcc)
            .arg(format!("-march={march}"))
            .arg(format!("-mabi={mabi}"))
            .args(["-static", "-mcmodel=medany", "-fvisibility=hidden"])
            .args(["-nostdlib", "-nostartfiles"])
            .arg(format!("-I{}", harness_dir.display()))
            .arg(format!("-I{}", src_dir.display()))
            .arg(format!("-T{}", link_ld.display()))
            .arg(&path)
            .arg("-o")
            .arg(&out_path)
            .arg(format!("-DRVMODEL_H=\\\"{}\\\"", model_test.display()))
            .stderr(std::process::Stdio::null())
            .status();

        if !matches!(status, Ok(s) if s.success()) {
            failed += 1;
            continue;
        }

        if config.gen_refs {
            let ref_path = refs_dir.join(format!("{out_name}.sig"));
            if let Err(e) = generate_reference(&out_path, &ref_path, category) {
                eprintln!("  {out_name}: {e}");
                failed += 1;
            }
        }
    }

    if failed > 0 {
        Err(format!("{failed} files failed"))
    } else {
        Ok(())
    }
}

fn generate_reference(
    elf_path: &Path,
    ref_path: &Path,
    category: ArchTestCategory,
) -> Result<(), String> {
    let spike = find_spike().ok_or("Spike not found")?;
    let isa = category_to_spike_isa(category);

    let output = Command::new(spike)
        .arg(format!("--isa={isa}"))
        .arg("--signature")
        .arg(ref_path)
        .arg(elf_path)
        .output()
        .map_err(|e| format!("failed to run spike: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err("spike failed".to_string())
    }
}

const fn category_to_spike_isa(category: ArchTestCategory) -> &'static str {
    match category {
        ArchTestCategory::Rv64iI
        | ArchTestCategory::Rv64iM
        | ArchTestCategory::Rv64iA
        | ArchTestCategory::Rv64iC
        | ArchTestCategory::Rv64iZicond => "rv64imac_zicsr_zicond",
        ArchTestCategory::Rv64iB => "rv64imac_zicsr_zicond_zba_zbb_zbs",
        ArchTestCategory::Rv32iI
        | ArchTestCategory::Rv32iM
        | ArchTestCategory::Rv32iA
        | ArchTestCategory::Rv32iC
        | ArchTestCategory::Rv32iZicond => "rv32imac_zicsr_zicond",
        ArchTestCategory::Rv32iB => "rv32imac_zicsr_zicond_zba_zbb_zbs",
    }
}
