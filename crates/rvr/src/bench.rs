//! Benchmarking utilities with optional perf integration.

use std::path::Path;
use std::process::Command;

use crate::{RunResult, Runner};

/// System-level performance statistics from `perf stat`.
#[derive(Debug, Clone, Default)]
pub struct PerfStats {
    /// CPU cycles consumed.
    pub cycles: Option<u64>,
    /// Instructions per cycle.
    pub ipc: Option<f64>,
    /// Total branches executed.
    pub branches: Option<u64>,
    /// Branch misses.
    pub branch_misses: Option<u64>,
    /// Branch miss rate as percentage.
    pub branch_miss_rate: Option<f64>,
}

/// Result of a benchmark run, combining execution metrics with perf stats.
#[derive(Debug, Clone)]
pub struct BenchResult {
    /// Core execution result.
    pub run: RunResult,
    /// Optional perf statistics.
    pub perf: Option<PerfStats>,
}

impl BenchResult {
    /// Create from a run result without perf stats.
    pub fn from_run(run: RunResult) -> Self {
        Self { run, perf: None }
    }

    /// Create from run result with perf stats.
    pub fn with_perf(run: RunResult, perf: PerfStats) -> Self {
        Self {
            run,
            perf: Some(perf),
        }
    }
}

/// RISC-V architecture variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    Rv32i,
    Rv32e,
    Rv64i,
    Rv64e,
}

impl Arch {
    /// All supported architectures.
    pub const ALL: &'static [Arch] = &[Arch::Rv32i, Arch::Rv32e, Arch::Rv64i, Arch::Rv64e];

    /// Parse from string (e.g., "rv32i").
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rv32i" => Some(Self::Rv32i),
            "rv32e" => Some(Self::Rv32e),
            "rv64i" => Some(Self::Rv64i),
            "rv64e" => Some(Self::Rv64e),
            _ => None,
        }
    }

    /// Parse comma-separated list of architectures.
    pub fn parse_list(s: &str) -> Result<Vec<Self>, String> {
        s.split(',')
            .map(|part| {
                Self::parse(part.trim())
                    .ok_or_else(|| format!("unknown arch '{}', expected rv32i/rv32e/rv64i/rv64e", part))
            })
            .collect()
    }

    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rv32i => "rv32i",
            Self::Rv32e => "rv32e",
            Self::Rv64i => "rv64i",
            Self::Rv64e => "rv64e",
        }
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Benchmark configuration.
#[derive(Debug, Clone)]
pub struct BenchConfig {
    /// Architectures to benchmark.
    pub archs: Vec<Arch>,
    /// Number of runs for averaging.
    pub runs: usize,
    /// Enable tracing.
    pub trace: bool,
    /// Fast mode (no instret counting).
    pub fast: bool,
    /// Use perf for system metrics.
    pub perf: bool,
    /// Skip compilation (use existing .so).
    pub no_compile: bool,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            archs: Arch::ALL.to_vec(),
            runs: 3,
            trace: false,
            fast: false,
            perf: true,
            no_compile: false,
        }
    }
}

/// Check if perf is available and working.
pub fn perf_available() -> bool {
    Command::new("perf")
        .args(["stat", "-e", "instructions", "true"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a compiled library with perf stat and parse the results.
pub fn run_with_perf(lib_dir: &Path, runs: usize) -> Result<BenchResult, String> {
    // Get the executable path for rvr
    let rvr_path = std::env::current_exe()
        .map_err(|e| format!("failed to get rvr path: {}", e))?;

    // Run under perf stat
    let output = Command::new("perf")
        .args([
            "stat",
            "-e", "instructions,cycles,branches,branch-misses",
        ])
        .arg(&rvr_path)
        .arg("run")
        .arg(lib_dir)
        .arg("--format")
        .arg("mojo")
        .arg("--runs")
        .arg(runs.to_string())
        .output()
        .map_err(|e| format!("failed to run perf: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse mojo format from stdout
    let instret = parse_mojo_field(&stdout, "instret").unwrap_or(0);
    let time_secs = parse_mojo_field_f64(&stdout, "time").unwrap_or(0.0);
    let mips = parse_mojo_speed(&stdout).unwrap_or(0.0);

    let run = RunResult {
        exit_code: if output.status.success() { 0 } else { 1 },
        instret,
        time_secs,
        mips,
    };

    // Parse perf output from stderr
    let perf = parse_perf_output(&stderr);

    Ok(BenchResult::with_perf(run, perf))
}

/// Run without perf, just using the Runner directly.
pub fn run_without_perf(lib_dir: &Path, runs: usize) -> Result<BenchResult, String> {
    let runner = Runner::load(lib_dir)
        .map_err(|e| format!("failed to load library: {}", e))?;

    if runs <= 1 {
        let result = runner.run()
            .map_err(|e| format!("execution failed: {}", e))?;
        Ok(BenchResult::from_run(result))
    } else {
        let results = runner.run_multiple(runs)
            .map_err(|e| format!("execution failed: {}", e))?;

        let avg_time = results.iter().map(|r| r.time_secs).sum::<f64>() / runs as f64;
        let avg_mips = results.iter().map(|r| r.mips).sum::<f64>() / runs as f64;
        let first = &results[0];

        Ok(BenchResult::from_run(RunResult {
            exit_code: first.exit_code,
            instret: first.instret,
            time_secs: avg_time,
            mips: avg_mips,
        }))
    }
}

fn parse_mojo_field(output: &str, field: &str) -> Option<u64> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix(field) {
            if let Some(rest) = rest.strip_prefix(':') {
                return rest.trim().parse().ok();
            }
        }
    }
    None
}

fn parse_mojo_field_f64(output: &str, field: &str) -> Option<f64> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix(field) {
            if let Some(rest) = rest.strip_prefix(':') {
                return rest.trim().parse().ok();
            }
        }
    }
    None
}

fn parse_mojo_speed(output: &str) -> Option<f64> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("speed:") {
            // Format: "123.45 MIPS"
            let rest = rest.trim();
            if let Some(num) = rest.strip_suffix(" MIPS") {
                return num.trim().parse().ok();
            }
        }
    }
    None
}

fn parse_perf_output(stderr: &str) -> PerfStats {
    let mut stats = PerfStats::default();

    for line in stderr.lines() {
        let line = line.trim();

        // Parse "1,234,567 instructions" or similar
        // Note: This is host instructions, not guest instret - we skip it

        // Parse cycles
        if line.contains("cycles") && !line.contains("stalled") {
            if let Some(num) = extract_perf_number(line) {
                stats.cycles = Some(num);
            }
        }

        // Parse branches (not branch-misses)
        if line.contains("branches") && !line.contains("branch-misses") {
            if let Some(num) = extract_perf_number(line) {
                stats.branches = Some(num);
            }
        }

        // Parse branch-misses
        if line.contains("branch-misses") {
            if let Some(num) = extract_perf_number(line) {
                stats.branch_misses = Some(num);
            }
        }
    }

    // Calculate derived metrics
    if let (Some(branches), Some(misses)) = (stats.branches, stats.branch_misses) {
        if branches > 0 {
            stats.branch_miss_rate = Some((misses as f64 / branches as f64) * 100.0);
        }
    }

    // IPC from perf output (look for pattern like "(0.85 insn per cycle)")
    for line in stderr.lines() {
        if line.contains("insn per cycle") || line.contains("IPC") {
            if let Some(ipc) = extract_ipc(line) {
                stats.ipc = Some(ipc);
                break;
            }
        }
    }

    // Fallback: calculate IPC if we have cycles but perf didn't report it
    // Note: We'd need host instructions for this, which perf reports

    stats
}

fn extract_perf_number(line: &str) -> Option<u64> {
    // perf output format: "  1,234,567      instructions"
    let first_word = line.split_whitespace().next()?;
    let cleaned: String = first_word.chars().filter(|c| c.is_ascii_digit()).collect();
    cleaned.parse().ok()
}

fn extract_ipc(line: &str) -> Option<f64> {
    // Look for patterns like "(0.85 insn per cycle)" or "# 0.85 IPC"
    for part in line.split_whitespace() {
        if let Ok(val) = part.trim_matches(|c: char| !c.is_ascii_digit() && c != '.').parse::<f64>() {
            if val > 0.0 && val < 100.0 {
                return Some(val);
            }
        }
    }
    None
}

/// Format a number with SI suffix (K, M, B).
pub fn format_num(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Row in a benchmark results table.
#[derive(Debug, Clone)]
pub struct TableRow {
    pub arch: Arch,
    pub result: Option<BenchResult>,
    pub error: Option<String>,
}

impl TableRow {
    pub fn success(arch: Arch, result: BenchResult) -> Self {
        Self { arch, result: Some(result), error: None }
    }

    pub fn error(arch: Arch, error: String) -> Self {
        Self { arch, result: None, error: Some(error) }
    }
}

/// Print markdown table header.
pub fn print_table_header(trace: bool, fast: bool, runs: usize) {
    println!();
    println!("## Benchmark Results");
    println!();
    let mode = if trace {
        "Trace"
    } else if fast {
        "Fast (no instret)"
    } else {
        "Base"
    };
    println!("Mode: **{}** | Runs: **{}**", mode, runs);
    println!();
    println!("| Arch | Instret | Time | Speed | IPC | Branch Miss |");
    println!("|------|---------|------|-------|-----|-------------|");
}

/// Print a table row.
pub fn print_table_row(row: &TableRow) {
    match &row.result {
        Some(result) => {
            let instret = format_num(result.run.instret);
            let time = format!("{:.3}s", result.run.time_secs);
            let speed = format!("{:.2} MIPS", result.run.mips);

            let ipc = result.perf.as_ref()
                .and_then(|p| p.ipc)
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "-".to_string());

            let branch_miss = result.perf.as_ref()
                .and_then(|p| p.branch_miss_rate)
                .map(|v| format!("{:.2}%", v))
                .unwrap_or_else(|| "-".to_string());

            println!("| {} | {} | {} | {} | {} | {} |",
                row.arch, instret, time, speed, ipc, branch_miss);
        }
        None => {
            let err = row.error.as_deref().unwrap_or("-");
            println!("| {} | {} | - | - | - | - |", row.arch, err);
        }
    }
}

/// Print the full table.
pub fn print_table(rows: &[TableRow], trace: bool, fast: bool, runs: usize) {
    print_table_header(trace, fast, runs);
    for row in rows {
        print_table_row(row);
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arch_parse() {
        assert_eq!(Arch::parse("rv32i"), Some(Arch::Rv32i));
        assert_eq!(Arch::parse("RV64E"), Some(Arch::Rv64e));
        assert_eq!(Arch::parse("invalid"), None);
    }

    #[test]
    fn test_arch_list_parse() {
        let archs = Arch::parse_list("rv32i,rv64e").unwrap();
        assert_eq!(archs, vec![Arch::Rv32i, Arch::Rv64e]);
    }

    #[test]
    fn test_format_num() {
        assert_eq!(format_num(500), "500");
        assert_eq!(format_num(1500), "1.50K");
        assert_eq!(format_num(1_500_000), "1.50M");
        assert_eq!(format_num(7_920_000_000), "7.92B");
    }
}
