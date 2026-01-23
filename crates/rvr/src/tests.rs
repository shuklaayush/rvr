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
    /// Test directory (default: bin/riscv-tests).
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
            test_dir: PathBuf::from("bin/riscv-tests"),
            filter: None,
            verbose: false,
            timeout_secs: 10,
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

    // Compile with HTIF enabled
    let options = CompileOptions::new().with_htif(true).with_quiet(true);

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

// ============================================================================
// Test Building
// ============================================================================

use std::process::Command;

/// RISC-V test category (maps to riscv-tests subdirectories).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestCategory {
    // RV32 categories
    Rv32ui,
    Rv32uc,
    Rv32um,
    Rv32ua,
    Rv32mi,
    Rv32si,
    Rv32uzba,
    Rv32uzbb,
    Rv32uzbs,
    // RV64 categories
    Rv64ui,
    Rv64uc,
    Rv64um,
    Rv64ua,
    Rv64mi,
    Rv64si,
    Rv64uzba,
    Rv64uzbb,
    Rv64uzbs,
}

impl TestCategory {
    /// All supported test categories.
    pub const ALL: &'static [TestCategory] = &[
        Self::Rv32ui,
        Self::Rv32uc,
        Self::Rv32um,
        Self::Rv32ua,
        Self::Rv32mi,
        Self::Rv32si,
        Self::Rv32uzba,
        Self::Rv32uzbb,
        Self::Rv32uzbs,
        Self::Rv64ui,
        Self::Rv64uc,
        Self::Rv64um,
        Self::Rv64ua,
        Self::Rv64mi,
        Self::Rv64si,
        Self::Rv64uzba,
        Self::Rv64uzbb,
        Self::Rv64uzbs,
    ];

    /// Parse from string (e.g., "rv32ui").
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rv32ui" => Some(Self::Rv32ui),
            "rv32uc" => Some(Self::Rv32uc),
            "rv32um" => Some(Self::Rv32um),
            "rv32ua" => Some(Self::Rv32ua),
            "rv32mi" => Some(Self::Rv32mi),
            "rv32si" => Some(Self::Rv32si),
            "rv32uzba" => Some(Self::Rv32uzba),
            "rv32uzbb" => Some(Self::Rv32uzbb),
            "rv32uzbs" => Some(Self::Rv32uzbs),
            "rv64ui" => Some(Self::Rv64ui),
            "rv64uc" => Some(Self::Rv64uc),
            "rv64um" => Some(Self::Rv64um),
            "rv64ua" => Some(Self::Rv64ua),
            "rv64mi" => Some(Self::Rv64mi),
            "rv64si" => Some(Self::Rv64si),
            "rv64uzba" => Some(Self::Rv64uzba),
            "rv64uzbb" => Some(Self::Rv64uzbb),
            "rv64uzbs" => Some(Self::Rv64uzbs),
            _ => None,
        }
    }

    /// Parse comma-separated list of categories (or "all").
    pub fn parse_list(s: &str) -> Result<Vec<Self>, String> {
        if s.eq_ignore_ascii_case("all") {
            return Ok(Self::ALL.to_vec());
        }
        s.split(',')
            .map(|part| {
                Self::parse(part.trim()).ok_or_else(|| {
                    format!(
                        "unknown category '{}', expected one of: {}",
                        part,
                        Self::ALL
                            .iter()
                            .map(|c| c.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
            })
            .collect()
    }

    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rv32ui => "rv32ui",
            Self::Rv32uc => "rv32uc",
            Self::Rv32um => "rv32um",
            Self::Rv32ua => "rv32ua",
            Self::Rv32mi => "rv32mi",
            Self::Rv32si => "rv32si",
            Self::Rv32uzba => "rv32uzba",
            Self::Rv32uzbb => "rv32uzbb",
            Self::Rv32uzbs => "rv32uzbs",
            Self::Rv64ui => "rv64ui",
            Self::Rv64uc => "rv64uc",
            Self::Rv64um => "rv64um",
            Self::Rv64ua => "rv64ua",
            Self::Rv64mi => "rv64mi",
            Self::Rv64si => "rv64si",
            Self::Rv64uzba => "rv64uzba",
            Self::Rv64uzbb => "rv64uzbb",
            Self::Rv64uzbs => "rv64uzbs",
        }
    }

    /// Get -march and -mabi flags for this category.
    pub fn march_mabi(&self) -> (&'static str, &'static str) {
        match self {
            // RV32 base
            Self::Rv32ui
            | Self::Rv32uc
            | Self::Rv32um
            | Self::Rv32ua
            | Self::Rv32mi
            | Self::Rv32si => ("rv32g", "ilp32"),
            // RV32 extensions
            Self::Rv32uzba => ("rv32g_zba", "ilp32"),
            Self::Rv32uzbb => ("rv32g_zbb", "ilp32"),
            Self::Rv32uzbs => ("rv32g_zbs", "ilp32"),
            // RV64 base
            Self::Rv64ui
            | Self::Rv64uc
            | Self::Rv64um
            | Self::Rv64ua
            | Self::Rv64mi
            | Self::Rv64si => ("rv64g", "lp64d"),
            // RV64 extensions
            Self::Rv64uzba => ("rv64g_zba", "lp64d"),
            Self::Rv64uzbb => ("rv64g_zbb", "lp64d"),
            Self::Rv64uzbs => ("rv64g_zbs", "lp64d"),
        }
    }
}

impl std::fmt::Display for TestCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration for building tests.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    /// Test source directory (contains isa/ subdirectory).
    pub src_dir: PathBuf,
    /// Output directory for built binaries.
    pub out_dir: PathBuf,
    /// Toolchain prefix (e.g., "riscv64-unknown-elf-").
    pub toolchain: String,
    /// Categories to build.
    pub categories: Vec<TestCategory>,
}

impl BuildConfig {
    /// Create a new build config with defaults.
    pub fn new(categories: Vec<TestCategory>) -> Self {
        Self {
            src_dir: PathBuf::from("tests/riscv-tests/isa"),
            out_dir: PathBuf::from("bin/riscv-tests"),
            toolchain: String::new(), // Will be auto-detected
            categories,
        }
    }

    /// Set the source directory.
    pub fn with_src_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.src_dir = dir.into();
        self
    }

    /// Set the output directory.
    pub fn with_out_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.out_dir = dir.into();
        self
    }

    /// Set the toolchain prefix.
    pub fn with_toolchain(mut self, toolchain: impl Into<String>) -> Self {
        self.toolchain = toolchain.into();
        self
    }
}

/// Find RISC-V GCC toolchain prefix.
pub fn find_toolchain() -> Option<String> {
    const PREFIXES: &[&str] = &[
        "riscv64-unknown-elf-",
        "riscv32-unknown-elf-",
        "riscv64-linux-gnu-",
        "riscv32-linux-gnu-",
    ];

    for prefix in PREFIXES {
        let gcc = format!("{}gcc", prefix);
        if Command::new("which")
            .arg(&gcc)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(prefix.to_string());
        }
    }
    None
}

/// Result of building a category.
#[derive(Debug)]
pub struct BuildResult {
    pub category: TestCategory,
    pub built: usize,
    pub failed: usize,
}

/// Build tests for a single category.
pub fn build_category(category: TestCategory, config: &BuildConfig) -> Result<BuildResult, String> {
    let cat_name = category.as_str();
    let src_dir = config.src_dir.join(cat_name);
    let out_dir = config.out_dir.join(cat_name);

    if !src_dir.exists() {
        return Err(format!("source directory not found: {}", src_dir.display()));
    }

    fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

    let (march, mabi) = category.march_mabi();
    let gcc = format!("{}gcc", config.toolchain);

    // Include paths relative to src_dir
    let env_p = config.src_dir.join("../env/p");
    let macros = config.src_dir.join("macros/scalar");
    let link_ld = config.src_dir.join("../env/p/link.ld");

    let mut built = 0;
    let mut failed = 0;

    // Find all .S files
    let entries = fs::read_dir(&src_dir).map_err(|e| format!("failed to read dir: {}", e))?;

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

        match status {
            Ok(s) if s.success() => built += 1,
            _ => failed += 1,
        }
    }

    Ok(BuildResult {
        category,
        built,
        failed,
    })
}

/// Build all specified test categories.
pub fn build_tests(config: &BuildConfig) -> Result<Vec<BuildResult>, String> {
    // Validate source directory
    if !config.src_dir.exists() {
        return Err(format!(
            "source directory not found: {}\nMake sure riscv-tests submodule is initialized",
            config.src_dir.display()
        ));
    }

    let mut results = Vec::new();

    for &category in &config.categories {
        match build_category(category, config) {
            Ok(result) => results.push(result),
            Err(e) => {
                eprintln!("  {}: {}", category, e);
            }
        }
    }

    Ok(results)
}

/// Print build results summary.
pub fn print_build_summary(results: &[BuildResult]) {
    let total_built: usize = results.iter().map(|r| r.built).sum();
    let total_failed: usize = results.iter().map(|r| r.failed).sum();

    println!();
    println!(
        "Build complete: {} tests built, {} failed",
        total_built, total_failed
    );
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
