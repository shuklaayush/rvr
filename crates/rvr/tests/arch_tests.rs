use std::path::PathBuf;
use std::sync::{Condvar, Mutex, Once, OnceLock};
use std::time::Duration;

use rvr_emit::Backend;

fn run_case(elf: &str, reference: &str, backend: Backend) {
    if !backend_enabled(backend) {
        return;
    }
    let _guard = concurrency_guard();
    maybe_rebuild_elfs();
    let timeout = Duration::from_secs(10);
    let compiler = rvr::Compiler::default();
    let root = workspace_root();
    let elf_path = root.join(elf);
    let ref_path = root.join(reference);
    if !ref_path.exists() || !elf_path.exists() {
        return;
    }
    let result = rvr::test_support::arch_tests::run_test(
        elf_path.as_path(),
        ref_path.as_path(),
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
        let bins = root.join("bin/riscv-arch-test");
        if bins.exists() {
            return;
        }
        let toolchain = rvr::test_support::arch_tests::find_toolchain()
            .unwrap_or_default();
        if toolchain.is_empty() {
            return;
        }
        let project_root = root;
        let config = rvr::test_support::arch_tests::ArchBuildConfig::new(
            rvr::test_support::arch_tests::ArchTestCategory::ALL.to_vec(),
        )
        .with_src_dir(project_root.join("programs/riscv-arch-test/riscv-test-suite"))
        .with_out_dir(project_root.join("bin/riscv-arch-test"))
        .with_refs_dir(project_root.join("bin/riscv-arch-test/references"))
        .with_toolchain(toolchain)
        .with_gen_refs(true);

        let _ = rvr::test_support::arch_tests::build_tests(&config);
    });
}

fn backend_enabled(backend: Backend) -> bool {
    matches!(backend, Backend::C | Backend::ARM64Asm | Backend::X86Asm)
}

const DEFAULT_MAX_TEST_THREADS: usize = 5;

struct Semaphore {
    available: Mutex<usize>,
    cvar: Condvar,
}

impl Semaphore {
    fn new(limit: usize) -> Self {
        Self {
            available: Mutex::new(limit),
            cvar: Condvar::new(),
        }
    }

    fn acquire(&self) -> SemaphoreGuard<'_> {
        let mut available = self.available.lock().unwrap();
        while *available == 0 {
            available = self.cvar.wait(available).unwrap();
        }
        *available -= 1;
        SemaphoreGuard { sem: self }
    }

    fn release(&self) {
        let mut available = self.available.lock().unwrap();
        *available += 1;
        self.cvar.notify_one();
    }
}

struct SemaphoreGuard<'a> {
    sem: &'a Semaphore,
}

impl Drop for SemaphoreGuard<'_> {
    fn drop(&mut self) {
        self.sem.release();
    }
}

fn concurrency_guard() -> SemaphoreGuard<'static> {
    static SEM: OnceLock<Semaphore> = OnceLock::new();
    SEM.get_or_init(|| Semaphore::new(DEFAULT_MAX_TEST_THREADS))
        .acquire()
}

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

    macro_rules! arch_case {
        ($name:ident, $elf:literal, $ref:literal) => {
            #[test]
            fn $name() {
                run_case($elf, $ref, Backend::C);
            }
        };
    }

    include!(concat!(env!("OUT_DIR"), "/arch_tests_cases.rs"));
}

#[cfg(target_arch = "aarch64")]
#[allow(non_snake_case)]
mod backend_arm64 {
    use super::*;

    macro_rules! arch_case {
        ($name:ident, $elf:literal, $ref:literal) => {
            #[test]
            fn $name() {
                run_case($elf, $ref, Backend::ARM64Asm);
            }
        };
    }

    include!(concat!(env!("OUT_DIR"), "/arch_tests_cases.rs"));
}

#[cfg(target_arch = "x86_64")]
#[allow(non_snake_case)]
mod backend_x86 {
    use super::*;

    macro_rules! arch_case {
        ($name:ident, $elf:literal, $ref:literal) => {
            #[test]
            fn $name() {
                run_case($elf, $ref, Backend::X86Asm);
            }
        };
    }

    include!(concat!(env!("OUT_DIR"), "/arch_tests_cases.rs"));
}
