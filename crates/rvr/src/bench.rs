//! Benchmarking utilities.
//!
//! Provides functions to benchmark compiled RISC-V programs with optional
//! hardware performance counter collection.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::{PerfCounters, RunResultWithPerf, Runner};

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
                Self::parse(part.trim()).ok_or_else(|| {
                    format!("unknown arch '{}', expected rv32i/rv32e/rv64i/rv64e", part)
                })
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

/// Result of running the host (native) binary.
#[derive(Debug, Clone, Default)]
pub struct HostResult {
    /// Execution time in seconds.
    pub time_secs: Option<f64>,
    /// Hardware perf counters (if available).
    pub perf: Option<PerfCounters>,
}

/// Run a compiled library and return results with perf counters.
pub fn run_bench(
    lib_dir: &Path,
    elf_path: &Path,
    runs: usize,
) -> Result<RunResultWithPerf, String> {
    let mut runner =
        Runner::load(lib_dir, elf_path).map_err(|e| format!("failed to load library: {}", e))?;

    if runs <= 1 {
        runner
            .run_with_counters()
            .map_err(|e| format!("execution failed: {}", e))
    } else {
        runner
            .run_multiple_with_counters(runs)
            .map_err(|e| format!("execution failed: {}", e))
    }
}

/// Run host binary and time it (for baseline comparison).
pub fn run_host(host_bin: &Path) -> Result<HostResult, String> {
    if !host_bin.exists() {
        return Err("host binary not found".to_string());
    }

    let start = Instant::now();
    let status = Command::new(host_bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("failed to run host: {}", e))?;

    let elapsed = start.elapsed().as_secs_f64();

    if !status.success() {
        return Err(format!("host exited with code {:?}", status.code()));
    }

    Ok(HostResult {
        time_secs: Some(elapsed),
        perf: None,
    })
}

// ============================================================================
// Formatting utilities
// ============================================================================

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

/// Calculate overhead ratio (vm_time / host_time).
pub fn calc_overhead(vm_time: f64, host_time: f64) -> Option<f64> {
    if host_time > 0.0 {
        Some(vm_time / host_time)
    } else {
        None
    }
}

/// Format overhead as "X.Xx".
pub fn format_overhead(oh: Option<f64>) -> String {
    oh.map(|v| format!("{:.1}x", v))
        .unwrap_or_else(|| "-".to_string())
}

/// Format IPC value.
pub fn format_ipc(ipc: Option<f64>) -> String {
    ipc.map(|v| format!("{:.2}", v))
        .unwrap_or_else(|| "-".to_string())
}

/// Format branch miss rate as percentage.
pub fn format_branch_miss(rate: Option<f64>) -> String {
    rate.map(|v| format!("{:.2}%", v))
        .unwrap_or_else(|| "-".to_string())
}

/// Format speed value with appropriate unit (MIPS or BIPS).
/// Input is in MIPS (millions of instructions per second).
pub fn format_speed(mips: f64) -> String {
    if mips <= 0.0 {
        "-".to_string()
    } else if mips >= 1000.0 {
        // BIPS = billions of instructions per second
        format!("{:.2} BIPS", mips / 1000.0)
    } else if mips >= 1.0 {
        format!("{:.0} MIPS", mips)
    } else {
        // Sub-MIPS: show with decimals
        format!("{:.2} MIPS", mips)
    }
}

/// Format speed for shell parsing (underscore instead of space).
pub fn format_speed_shell(mips: f64) -> String {
    if mips <= 0.0 {
        "-".to_string()
    } else if mips >= 1000.0 {
        format!("{:.2}_BIPS", mips / 1000.0)
    } else if mips >= 1.0 {
        format!("{:.0}_MIPS", mips)
    } else {
        format!("{:.2}_MIPS", mips)
    }
}

// ============================================================================
// Table output
// ============================================================================

/// Row in a benchmark results table.
#[derive(Debug, Clone)]
pub struct TableRow {
    /// Row label (arch name or "host").
    pub label: String,
    /// Instruction count (guest instret), None for host.
    pub instret: Option<u64>,
    /// Host instructions executed.
    pub host_instrs: Option<u64>,
    /// Host instructions per guest instruction.
    pub instrs_per_guest: Option<f64>,
    /// Execution time in seconds.
    pub time_secs: Option<f64>,
    /// Overhead compared to host (vm_time / host_time).
    pub overhead: Option<f64>,
    /// Speed in MIPS (guest MIPS), None for host.
    pub mips: Option<f64>,
    /// Instructions per cycle (host IPC).
    pub ipc: Option<f64>,
    /// Branch miss rate as percentage.
    pub branch_miss_rate: Option<f64>,
    /// Error message if benchmark failed.
    pub error: Option<String>,
}

impl TableRow {
    /// Create a row for the host baseline.
    pub fn host(result: &HostResult) -> Self {
        let (ipc, branch_miss_rate, host_instrs) = result
            .perf
            .as_ref()
            .map(|p| (p.ipc(), p.branch_miss_rate(), p.instructions))
            .unwrap_or((None, None, None));

        Self {
            label: "host".to_string(),
            instret: None,
            host_instrs,
            instrs_per_guest: None,
            time_secs: result.time_secs,
            overhead: Some(1.0),
            mips: None,
            ipc,
            branch_miss_rate,
            error: None,
        }
    }

    /// Create a row for a VM architecture.
    pub fn arch(arch: Arch, result: &RunResultWithPerf, host_time: Option<f64>) -> Self {
        let overhead = host_time.and_then(|ht| calc_overhead(result.result.time_secs, ht));
        let (ipc, branch_miss_rate, host_instrs) = result
            .perf
            .as_ref()
            .map(|p| (p.ipc(), p.branch_miss_rate(), p.instructions))
            .unwrap_or((None, None, None));

        // Calculate host instructions per guest instruction
        let instrs_per_guest = host_instrs.map(|hi| hi as f64 / result.result.instret as f64);

        Self {
            label: arch.as_str().to_string(),
            instret: Some(result.result.instret),
            host_instrs,
            instrs_per_guest,
            time_secs: Some(result.result.time_secs),
            overhead,
            mips: Some(result.result.mips),
            ipc,
            branch_miss_rate,
            error: None,
        }
    }

    /// Create an error row.
    pub fn error(label: &str, error: String) -> Self {
        Self {
            label: label.to_string(),
            instret: None,
            host_instrs: None,
            instrs_per_guest: None,
            time_secs: None,
            overhead: None,
            mips: None,
            ipc: None,
            branch_miss_rate: None,
            error: Some(error),
        }
    }
}

/// Format host instructions per guest instruction.
pub fn format_instrs_per_guest(ipg: Option<f64>) -> String {
    ipg.map(|v| format!("{:.1}x", v))
        .unwrap_or_else(|| "-".to_string())
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
    println!("*Mode: **{}** | Runs: **{}***", mode, runs);
    println!();
    println!(
        "| {:<6} | {:>10} | {:>10} | {:>9} | {:>8} | {:>6} | {:>12} | {:>5} | {:>11} |",
        "Arch", "Instret", "Host Ops", "Ops/Guest", "Time", "OH", "Speed", "IPC", "Branch Miss"
    );
    println!(
        "|{:-<8}|{:-<12}|{:-<12}|{:-<11}|{:-<10}|{:-<8}|{:-<14}|{:-<7}|{:-<13}|",
        "", "", "", "", "", "", "", "", ""
    );
}

/// Print a table row.
pub fn print_table_row(row: &TableRow) {
    if row.error.is_some() {
        println!(
            "| {:<6} | {:>10} | {:>10} | {:>9} | {:>8} | {:>6} | {:>12} | {:>5} | {:>11} |",
            row.label, "-", "-", "-", "-", "-", "-", "-", "-"
        );
        return;
    }

    let instret = row
        .instret
        .map(format_num)
        .unwrap_or_else(|| "-".to_string());
    let host_instrs = row
        .host_instrs
        .map(format_num)
        .unwrap_or_else(|| "-".to_string());
    let instrs_per_guest = format_instrs_per_guest(row.instrs_per_guest);
    let time = row
        .time_secs
        .map(|t| format!("{:.3}s", t))
        .unwrap_or_else(|| "-".to_string());
    let overhead = format_overhead(row.overhead);
    let speed = row
        .mips
        .map(format_speed)
        .unwrap_or_else(|| "-".to_string());
    let ipc = format_ipc(row.ipc);
    let branch_miss = format_branch_miss(row.branch_miss_rate);

    println!(
        "| {:<6} | {:>10} | {:>10} | {:>9} | {:>8} | {:>6} | {:>12} | {:>5} | {:>11} |",
        row.label, instret, host_instrs, instrs_per_guest, time, overhead, speed, ipc, branch_miss
    );
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

    #[test]
    fn test_calc_overhead() {
        assert_eq!(calc_overhead(2.0, 1.0), Some(2.0));
        assert_eq!(calc_overhead(1.5, 0.5), Some(3.0));
        assert_eq!(calc_overhead(1.0, 0.0), None);
    }

    #[test]
    fn test_format_overhead() {
        assert_eq!(format_overhead(Some(2.5)), "2.5x");
        assert_eq!(format_overhead(Some(10.0)), "10.0x");
        assert_eq!(format_overhead(None), "-");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(0.0), "-");
        assert_eq!(format_speed(-1.0), "-");
        assert_eq!(format_speed(0.5), "0.50 MIPS");
        assert_eq!(format_speed(100.0), "100 MIPS");
        assert_eq!(format_speed(999.0), "999 MIPS");
        assert_eq!(format_speed(1000.0), "1.00 BIPS");
        assert_eq!(format_speed(3861.0), "3.86 BIPS");
        assert_eq!(format_speed(8609.0), "8.61 BIPS");
    }
}
