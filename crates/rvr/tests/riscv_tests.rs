use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use libtest_mimic::{Arguments, Failed, Trial};
use rvr_emit::Backend;

mod common;

fn main() {
    let mut args = Arguments::from_args();
    common::cap_threads(&mut args);

    let cases = collect_riscv_tests();
    let backends = enabled_backends();

    let mut trials = Vec::new();
    for backend in backends {
        let backend_name = backend_label(backend);
        for path in &cases {
            let name = format!("{}::{}", backend_name, ident_from_path(path));
            let path = path.clone();
            trials.push(Trial::test(name, move || run_case(&path, backend)));
        }
    }

    libtest_mimic::run(&args, trials).exit();
}

fn run_case(path: &Path, backend: Backend) -> Result<(), Failed> {
    let _ = maybe_rebuild_elfs();
    let timeout = Duration::from_secs(10);
    let compiler = rvr::Compiler::default();
    let root = workspace_root();
    let full_path = root.join(path);
    if !full_path.exists() {
        return Ok(());
    }
    let result = rvr::test_support::riscv_tests::run_test(
        full_path.as_path(),
        timeout,
        &compiler,
        backend,
    );
    match result.status {
        rvr::test_support::riscv_tests::TestStatus::Pass => Ok(()),
        rvr::test_support::riscv_tests::TestStatus::Skip => Ok(()),
        rvr::test_support::riscv_tests::TestStatus::Fail => {
            let msg = result.error.unwrap_or_else(|| "unknown failure".to_string());
            Err(Failed::from(format!("{} failed: {}", result.name, msg)))
        }
    }
}

fn enabled_backends() -> Vec<Backend> {
    let mut backends = vec![Backend::C];
    #[cfg(target_arch = "aarch64")]
    {
        backends.push(Backend::ARM64Asm);
    }
    #[cfg(target_arch = "x86_64")]
    {
        backends.push(Backend::X86Asm);
    }
    backends
}

fn backend_label(backend: Backend) -> &'static str {
    match backend {
        Backend::C => "backend_c",
        Backend::ARM64Asm => "backend_arm64",
        Backend::X86Asm => "backend_x86",
    }
}

fn maybe_rebuild_elfs() -> Result<(), Failed> {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    let mut status = Ok(());
    ONCE.call_once(|| {
        let root = workspace_root();
        let bins = root.join("bin/riscv-tests");
        if bins.exists() {
            return;
        }
        let toolchain = rvr::test_support::riscv_tests::find_toolchain()
            .unwrap_or_default();
        if toolchain.is_empty() {
            return;
        }
        let config = rvr::test_support::riscv_tests::BuildConfig::new(
            rvr::test_support::riscv_tests::TestCategory::ALL.to_vec(),
        )
        .with_src_dir(root.join("programs/riscv-tests/isa"))
        .with_out_dir(root.join("bin/riscv-tests"))
        .with_toolchain(toolchain);

        if let Err(err) = rvr::test_support::riscv_tests::build_tests(&config) {
            status = Err(Failed::from(format!("failed to build riscv-tests: {err}")));
        }
    });
    status
}

fn collect_riscv_tests() -> Vec<PathBuf> {
    let root = workspace_root();
    let dir = root.join("bin/riscv-tests");
    let mut cases = Vec::new();
    if dir.exists() {
        let _ = collect_files(&dir, &mut cases);
    }
    cases.sort();
    cases
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("missing workspace root")
        .to_path_buf()
}

fn ident_from_path(path: &Path) -> String {
    let root = workspace_root();
    let rel = path.strip_prefix(&root).unwrap_or(path);
    let mut s = String::new();
    for ch in rel.to_string_lossy().chars() {
        if ch.is_ascii_alphanumeric() {
            s.push(ch);
        } else {
            s.push('_');
        }
    }
    if s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        s.insert(0, '_');
    }
    s
}
