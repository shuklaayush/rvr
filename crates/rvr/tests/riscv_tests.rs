use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

use rvr_emit::Backend;

mod common;

fn run_case(path: &str, backend: Backend) {
    if !backend_enabled(backend) {
        return;
    }
    let _guard = concurrency_guard();
    maybe_rebuild_elfs();
    let timeout = Duration::from_secs(10);
    let compiler = rvr::Compiler::default();
    let root = workspace_root();
    let full_path = root.join(path);
    if !full_path.exists() {
        return;
    }
    let result = rvr::test_support::riscv_tests::run_test(
        full_path.as_path(),
        timeout,
        &compiler,
        backend,
    );
    match result.status {
        rvr::test_support::riscv_tests::TestStatus::Pass => {}
        rvr::test_support::riscv_tests::TestStatus::Skip => {}
        rvr::test_support::riscv_tests::TestStatus::Fail => {
            let msg = result.error.unwrap_or_else(|| "unknown failure".to_string());
            panic!("{} failed: {}", result.name, msg);
        }
    }
}

fn maybe_rebuild_elfs() {
    static ONCE: Once = Once::new();
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
        let project_root = root;
        let config = rvr::test_support::riscv_tests::BuildConfig::new(
            rvr::test_support::riscv_tests::TestCategory::ALL.to_vec(),
        )
        .with_src_dir(project_root.join("programs/riscv-tests/isa"))
        .with_out_dir(project_root.join("bin/riscv-tests"))
        .with_toolchain(toolchain);

        let _ = rvr::test_support::riscv_tests::build_tests(&config);
    });
}

fn backend_enabled(backend: Backend) -> bool {
    matches!(backend, Backend::C | Backend::ARM64Asm | Backend::X86Asm)
}

use common::concurrency_guard;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("missing workspace root")
        .to_path_buf()
}

#[allow(non_snake_case)]
mod backend_c {
    use super::*;

    macro_rules! test_case {
        ($name:ident, $path:literal) => {
            #[test]
            fn $name() {
                run_case($path, Backend::C);
            }
        };
    }

    include!(concat!(env!("OUT_DIR"), "/riscv_tests_cases.rs"));
}

#[cfg(target_arch = "aarch64")]
#[allow(non_snake_case)]
mod backend_arm64 {
    use super::*;

    macro_rules! test_case {
        ($name:ident, $path:literal) => {
            #[test]
            fn $name() {
                run_case($path, Backend::ARM64Asm);
            }
        };
    }

    include!(concat!(env!("OUT_DIR"), "/riscv_tests_cases.rs"));
}

#[cfg(target_arch = "x86_64")]
#[allow(non_snake_case)]
mod backend_x86 {
    use super::*;

    macro_rules! test_case {
        ($name:ident, $path:literal) => {
            #[test]
            fn $name() {
                run_case($path, Backend::X86Asm);
            }
        };
    }

    include!(concat!(env!("OUT_DIR"), "/riscv_tests_cases.rs"));
}
