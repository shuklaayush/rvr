//! RISC-V test suite runner.
//!
//! Runs riscv-tests and reports pass/fail/skip results.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{compile_with_options, CompileOptions, Runner};

/// Tests to skip (not compatible with static recompilation).
const SKIP_TESTS: &[&str] = &[
    // fence.i tests self-modifying code - incompatible with static recompilation
    "rv32ui-p-fence_i",
    "rv64ui-p-fence_i",
];

/// Test result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Pass,
    Fail,
    Skip,
}

/// Result of running a single test.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test name (e.g., "rv64ui-p-add").
    pub name: String,
    /// Test status.
    pub status: TestStatus,
    /// Error message if failed.
    pub error: Option<String>,
}

impl TestResult {
    /// Create a passing result.
    pub fn pass(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Pass,
            error: None,
        }
    }

    /// Create a failing result.
    pub fn fail(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Fail,
            error: Some(error.into()),
        }
    }

    /// Create a skipped result.
    pub fn skip(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Skip,
            error: None,
        }
    }
}

/// Summary of test run results.
#[derive(Debug, Clone, Default)]
pub struct TestSummary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub failures: Vec<TestResult>,
}

impl TestSummary {
    /// Total number of tests.
    pub fn total(&self) -> usize {
        self.passed + self.failed + self.skipped
    }

    /// Whether all tests passed (ignoring skips).
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Add a result to the summary.
    pub fn add(&mut self, result: TestResult) {
        // Record metric for this test
        crate::metrics::record_test(&result.name, result.status);

        match result.status {
            TestStatus::Pass => self.passed += 1,
            TestStatus::Fail => {
                self.failed += 1;
                self.failures.push(result);
            }
            TestStatus::Skip => self.skipped += 1,
        }
    }

    /// Record summary totals to metrics.
    pub fn record_metrics(&self) {
        crate::metrics::record_test_summary(
            self.passed as u64,
            self.failed as u64,
            self.skipped as u64,
        );
    }
}

/// Configuration for running tests.
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Test directory (default: bin/riscv/tests).
    pub test_dir: PathBuf,
    /// Filter pattern (e.g., "rv64" to only run rv64 tests).
    pub filter: Option<String>,
    /// Verbose output (show all tests, not just failures).
    pub verbose: bool,
    /// Timeout for each test in seconds.
    pub timeout_secs: u64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            test_dir: PathBuf::from("bin/riscv/tests"),
            filter: None,
            verbose: false,
            timeout_secs: 5,
        }
    }
}

impl TestConfig {
    /// Set the test directory.
    pub fn with_test_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.test_dir = dir.into();
        self
    }

    /// Set a filter pattern.
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Enable verbose output.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Set timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// Check if a test should be skipped.
pub fn should_skip(name: &str) -> bool {
    // Skip explicit skip list
    if SKIP_TESTS.contains(&name) {
        return true;
    }
    // Skip machine/supervisor mode tests
    if name.contains("mi-p-") || name.contains("si-p-") {
        return true;
    }
    false
}

/// Discover test files in the given directory (recursively).
pub fn discover_tests(test_dir: &Path, filter: Option<&str>) -> Vec<PathBuf> {
    let mut tests = Vec::new();

    if !test_dir.exists() {
        return tests;
    }

    discover_tests_recursive(test_dir, filter, &mut tests);
    tests.sort();
    tests
}

fn discover_tests_recursive(dir: &Path, filter: Option<&str>, tests: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Recurse into subdirectories
            discover_tests_recursive(&path, filter, tests);
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        // Only match rv*-p-* pattern (user-level tests)
        if !name.starts_with("rv") || !name.contains("-p-") {
            continue;
        }

        // Apply filter if specified
        if let Some(filter) = filter {
            if !name.contains(filter) {
                continue;
            }
        }

        tests.push(path);
    }
}

/// Run a single test.
pub fn run_test(elf_path: &Path, timeout: Duration) -> TestResult {
    let name = elf_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Check skip list
    if should_skip(&name) {
        return TestResult::skip(name);
    }

    // Create temp directory for compilation output
    let temp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => return TestResult::fail(name, format!("temp dir failed: {}", e)),
    };

    let out_dir = temp_dir.path().join("out");

    // Compile with tohost enabled
    let options = CompileOptions::new().with_tohost(true).with_quiet(true);

    if let Err(e) = compile_with_options(elf_path, &out_dir, options) {
        return TestResult::fail(name, format!("compile failed: {}", e));
    }

    // Run with timeout
    let result = run_with_timeout(&out_dir, elf_path, timeout);

    match result {
        Ok(exit_code) => {
            if exit_code == 0 {
                TestResult::pass(name)
            } else {
                TestResult::fail(name, format!("exit={}", exit_code))
            }
        }
        Err(e) => TestResult::fail(name, e),
    }
}

/// Run a compiled test with timeout.
fn run_with_timeout(lib_dir: &Path, elf_path: &Path, timeout: Duration) -> Result<u8, String> {
    // Use std::thread to implement timeout
    let (tx, rx) = std::sync::mpsc::channel();
    let lib_dir_clone = lib_dir.to_path_buf();
    let elf_path_clone = elf_path.to_path_buf();

    std::thread::spawn(move || {
        let mut runner = match Runner::load(&lib_dir_clone, &elf_path_clone) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Err(format!("load failed: {}", e)));
                return;
            }
        };
        match runner.run() {
            Ok(result) => {
                let _ = tx.send(Ok(result.exit_code));
            }
            Err(e) => {
                let _ = tx.send(Err(format!("run failed: {}", e)));
            }
        }
    });

    // Wait with timeout - if timeout exceeded, we just return timeout error
    // Note: The thread will continue running but we won't wait for it
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err("timeout".to_string()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err("crash".to_string()),
    }
}

/// ANSI color codes.
pub mod colors {
    pub const RED: &str = "\x1b[0;31m";
    pub const GREEN: &str = "\x1b[0;32m";
    pub const YELLOW: &str = "\x1b[0;33m";
    pub const RESET: &str = "\x1b[0m";
}

/// Print a test result line.
pub fn print_result(result: &TestResult, index: usize, total: usize, verbose: bool) {
    match result.status {
        TestStatus::Pass => {
            if verbose {
                println!(
                    "[{}/{}] {}PASS{} {}",
                    index,
                    total,
                    colors::GREEN,
                    colors::RESET,
                    result.name
                );
            }
        }
        TestStatus::Fail => {
            let error = result.error.as_deref().unwrap_or("unknown");
            println!(
                "[{}/{}] {}FAIL{} {} ({})",
                index,
                total,
                colors::RED,
                colors::RESET,
                result.name,
                error
            );
        }
        TestStatus::Skip => {
            if verbose {
                println!(
                    "[{}/{}] {}SKIP{} {}",
                    index,
                    total,
                    colors::YELLOW,
                    colors::RESET,
                    result.name
                );
            }
        }
    }
}

/// Print test summary.
pub fn print_summary(summary: &TestSummary) {
    println!();
    println!("================================");
    println!(
        "{}PASSED{}: {}",
        colors::GREEN,
        colors::RESET,
        summary.passed
    );
    println!("{}FAILED{}: {}", colors::RED, colors::RESET, summary.failed);
    println!(
        "{}SKIPPED{}: {}",
        colors::YELLOW,
        colors::RESET,
        summary.skipped
    );
    println!();

    if !summary.failures.is_empty() {
        println!("Failures:");
        for failure in &summary.failures {
            let error = failure.error.as_deref().unwrap_or("unknown");
            println!("  {} ({})", failure.name, error);
        }
        println!();
    }
}

/// Run all tests with the given configuration.
pub fn run_all(config: &TestConfig) -> TestSummary {
    let tests = discover_tests(&config.test_dir, config.filter.as_deref());
    let total = tests.len();
    let timeout = Duration::from_secs(config.timeout_secs);

    let mut summary = TestSummary::default();

    for (i, test_path) in tests.iter().enumerate() {
        let result = run_test(test_path, timeout);
        print_result(&result, i + 1, total, config.verbose);
        summary.add(result);
    }

    // Record summary totals to metrics
    summary.record_metrics();

    summary
}

#[cfg(test)]
mod test_tests {
    use super::*;

    #[test]
    fn test_should_skip() {
        assert!(should_skip("rv32ui-p-fence_i"));
        assert!(should_skip("rv64ui-p-fence_i"));
        assert!(should_skip("rv64mi-p-csr"));
        assert!(should_skip("rv64si-p-csr"));
        assert!(!should_skip("rv64ui-p-add"));
        assert!(!should_skip("rv32um-p-mul"));
    }

    #[test]
    fn test_result_creation() {
        let pass = TestResult::pass("test1");
        assert_eq!(pass.status, TestStatus::Pass);
        assert!(pass.error.is_none());

        let fail = TestResult::fail("test2", "bad exit");
        assert_eq!(fail.status, TestStatus::Fail);
        assert_eq!(fail.error.as_deref(), Some("bad exit"));

        let skip = TestResult::skip("test3");
        assert_eq!(skip.status, TestStatus::Skip);
    }

    #[test]
    fn test_summary() {
        let mut summary = TestSummary::default();
        summary.add(TestResult::pass("t1"));
        summary.add(TestResult::pass("t2"));
        summary.add(TestResult::fail("t3", "err"));
        summary.add(TestResult::skip("t4"));

        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.total(), 4);
        assert!(!summary.all_passed());
        assert_eq!(summary.failures.len(), 1);
    }
}
