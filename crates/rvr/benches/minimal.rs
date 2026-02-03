#![feature(test)]

extern crate test;

use std::path::{Path, PathBuf};

use rvr::{bench, CompileOptions};
use rvr_emit::Backend;
use test::Bencher;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("missing workspace root")
        .to_path_buf()
}

fn bench_elf(path: &str) -> PathBuf {
    workspace_root().join(path)
}

fn run_bench(b: &mut Bencher, elf: &Path, backend: Backend) {
    if !elf.exists() {
        return;
    }
    let tempdir = tempfile::tempdir().expect("tempdir");
    let options = CompileOptions::default()
        .with_backend(backend)
        .with_quiet(true);
    rvr::compile_with_options(elf, tempdir.path(), options).expect("compile");
    b.iter(|| {
        let _ = bench::run_bench_auto(tempdir.path(), elf, 1).expect("run");
    });
}

#[bench]
fn minimal_c(b: &mut Bencher) {
    let elf = bench_elf("bin/rv64i/minimal");
    run_bench(b, &elf, Backend::C);
}

#[cfg(target_arch = "aarch64")]
#[bench]
fn minimal_arm64(b: &mut Bencher) {
    let elf = bench_elf("bin/rv64i/minimal");
    run_bench(b, &elf, Backend::ARM64Asm);
}
