//! Benchmark suite helpers used by benches.

pub mod coremark;
pub mod libriscv;
pub mod polkavm;
pub mod registry;
pub mod riscv_tests;
pub mod rust_bench;

pub use registry::{BenchmarkInfo, BenchmarkSource};

use std::path::PathBuf;
use std::process::Command;

/// Find the project root directory (git root or cwd).
pub fn find_project_root() -> PathBuf {
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        && output.status.success()
        && let Ok(path) = String::from_utf8(output.stdout)
    {
        return PathBuf::from(path.trim());
    }
    std::env::current_dir().expect("failed to get current directory")
}
