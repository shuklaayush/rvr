//! Benchmarking utilities.
//!
//! Provides functions to benchmark compiled RISC-V programs with optional
//! hardware performance counter collection.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use rvr_isa::{REG_GP, REG_RA, REG_SP};

use crate::perf::HostPerfCounters;
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
    pub const ALL: &'static [Self] = &[Self::Rv32i, Self::Rv32e, Self::Rv64i, Self::Rv64e];

    /// Parse from string (e.g., "rv32i").
    #[must_use]
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
    ///
    /// # Errors
    /// Returns an error when an unknown architecture string is encountered.
    pub fn parse_list(s: &str) -> Result<Vec<Self>, String> {
        if s.eq_ignore_ascii_case("all") {
            return Ok(vec![Self::Rv32i, Self::Rv32e, Self::Rv64i, Self::Rv64e]);
        }
        s.split(',')
            .map(|part| {
                Self::parse(part.trim()).ok_or_else(|| {
                    format!("unknown arch '{part}', expected rv32i/rv32e/rv64i/rv64e/all")
                })
            })
            .collect()
    }

    /// Get string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
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
///
/// # Errors
/// Returns an error if the library fails to load or execution fails.
pub fn run_bench(
    lib_dir: &Path,
    elf_path: &Path,
    runs: usize,
) -> Result<RunResultWithPerf, String> {
    let mut runner =
        Runner::load(lib_dir, elf_path).map_err(|e| format!("failed to load library: {e}"))?;

    if runs <= 1 {
        runner
            .run_with_counters()
            .map_err(|e| format!("execution failed: {e}"))
    } else {
        runner
            .run_multiple_with_counters(runs)
            .map_err(|e| format!("execution failed: {e}"))
    }
}

/// Benchmark execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchMode {
    /// Executable mode: run from entry point
    Executable,
    /// Library mode: call `initialize()` then `run()` N times
    Library,
}

/// Run a benchmark with automatic mode detection.
///
/// Uses the `RV_EXPORT_FUNCTIONS` metadata from the compiled library to determine
/// whether to use library mode (call initialize/run) or executable mode (entry point).
///
/// # Errors
/// Returns an error if the library fails to load or execution fails.
pub fn run_bench_auto(
    lib_dir: &Path,
    elf_path: &Path,
    runs: usize,
) -> Result<(RunResultWithPerf, BenchMode), String> {
    let mut runner =
        Runner::load(lib_dir, elf_path).map_err(|e| format!("failed to load library: {e}"))?;

    if runner.has_export_functions() {
        // Library mode: call initialize() then run()
        let init_addr = runner
            .lookup_symbol("initialize")
            .ok_or("export_functions mode but 'initialize' symbol not found")?;
        let run_addr = runner
            .lookup_symbol("run")
            .ok_or("export_functions mode but 'run' symbol not found")?;

        let result = run_bench_library_inner(&mut runner, init_addr, run_addr, runs)?;
        Ok((result, BenchMode::Library))
    } else {
        // Executable mode: run from entry point
        let result = if runs <= 1 {
            runner
                .run_with_counters()
                .map_err(|e| format!("execution failed: {e}"))?
        } else {
            runner
                .run_multiple_with_counters(runs)
                .map_err(|e| format!("execution failed: {e}"))?
        };
        Ok((result, BenchMode::Executable))
    }
}

/// Run a library-mode benchmark with `initialize()` and `run()` exports.
///
/// The benchmark exports two symbols:
/// - `initialize`: Called once before timing (setup)
/// - `run`: Called N times with timing (the actual benchmark)
///
/// # Errors
/// Returns an error if the library fails to load or the benchmark fails to run.
pub fn run_bench_library(
    lib_dir: &Path,
    elf_path: &Path,
    runs: usize,
) -> Result<RunResultWithPerf, String> {
    let mut runner =
        Runner::load(lib_dir, elf_path).map_err(|e| format!("failed to load library: {e}"))?;

    // Look up required symbols
    let init_addr = runner
        .lookup_symbol("initialize")
        .ok_or("symbol 'initialize' not found in ELF")?;
    let run_addr = runner
        .lookup_symbol("run")
        .ok_or("symbol 'run' not found in ELF")?;

    run_bench_library_inner(&mut runner, init_addr, run_addr, runs)
}

/// Internal implementation for library-mode benchmarks.
///
/// Calls `initialize()` once (not timed), then `run()` N times (timed).
/// Uses 0 as return address - `rv_trap` handles it and saves state properly.
fn run_bench_library_inner(
    runner: &mut Runner,
    init_addr: u64,
    run_addr: u64,
    runs: usize,
) -> Result<RunResultWithPerf, String> {
    let runs = runs.max(1);

    // Look up gp and sp from ELF symbols (standard linker-defined symbols)
    let gp = runner.lookup_symbol("__global_pointer$");
    let sp = runner.lookup_symbol("__stack_top");

    // Set up perf counters
    let mut perf_group = crate::perf::PerfGroup::new();

    // Run benchmark N times
    let mut total_time = 0.0;
    let mut total_instret = 0u64;

    for _ in 0..runs {
        // Load segments and reset state for each run
        runner.prepare();

        // Set gp and sp from ELF symbols instead of running entry point
        if let Some(gp_val) = gp {
            runner.set_register(REG_GP as usize, gp_val);
        }
        if let Some(sp_val) = sp {
            runner.set_register(REG_SP as usize, sp_val);
        }

        // Set return address to 0 - rv_trap handles it
        runner.set_register(REG_RA as usize, 0);

        // Run initialize() (not timed)
        runner
            .execute_from(init_addr)
            .map_err(|e| format!("initialize() failed: {e}"))?;

        // Clear exit flag and reset ra for run()
        runner.clear_exit();
        runner.set_register(REG_RA as usize, 0);

        // Record instret before run() to calculate delta
        let instret_before = runner.instret();

        if let Some(ref mut group) = perf_group {
            let _ = group.reset();
            let _ = group.enable();
        }

        let (elapsed, instret_after) = runner
            .execute_from(run_addr)
            .map_err(|e| format!("run() failed: {e}"))?;

        if let Some(ref mut group) = perf_group {
            let _ = group.disable();
        }

        total_time += elapsed.as_secs_f64();
        total_instret += instret_after - instret_before;

        // Clear exit flag for next iteration
        runner.clear_exit();
    }

    let runs_u64 = u64::try_from(runs).unwrap_or(u64::MAX);
    let avg_time = total_time / u64_to_f64(runs_u64);
    let avg_instret = total_instret / runs_u64;
    let mips = (u64_to_f64(avg_instret) / avg_time) / 1_000_000.0;

    let perf = perf_group.as_mut().and_then(crate::perf::PerfGroup::read);

    let result = crate::RunResult {
        exit_code: 0,
        instret: avg_instret,
        time_secs: avg_time,
        mips,
    };

    Ok(RunResultWithPerf { result, perf })
}

/// Run host binary and time it (for baseline comparison).
/// Collects perf counters and supports multiple runs for averaging.
///
/// # Errors
/// Returns an error if the host binary is missing or execution fails.
pub fn run_host(host_bin: &Path, runs: usize) -> Result<HostResult, String> {
    if !host_bin.exists() {
        return Err("host binary not found".to_string());
    }

    let runs = runs.max(1);
    let mut perf_counters = HostPerfCounters::new();
    let mut total_time = 0.0;
    let mut total_cycles = 0u64;
    let mut total_instructions = 0u64;
    let mut total_branches = 0u64;
    let mut total_branch_misses = 0u64;

    // Get initial snapshot for delta tracking
    let mut prev_snapshot = perf_counters
        .as_mut()
        .map_or_else(Default::default, crate::perf::HostPerfCounters::read);

    for _ in 0..runs {
        let start = Instant::now();
        if let Some(ref mut counters) = perf_counters {
            let _ = counters.enable();
        }

        let status = Command::new(host_bin)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| format!("failed to run host: {e}"))?;

        if let Some(ref mut counters) = perf_counters {
            let _ = counters.disable();
        }
        let elapsed = start.elapsed().as_secs_f64();

        if !status.success() {
            return Err(format!("host exited with code {:?}", status.code()));
        }

        total_time += elapsed;

        // Read delta since last snapshot (works around reset() issues with inherit)
        if let Some(ref mut counters) = perf_counters {
            let delta = counters.read_delta(&prev_snapshot);
            total_cycles += delta.cycles.unwrap_or(0);
            total_instructions += delta.instructions.unwrap_or(0);
            total_branches += delta.branches.unwrap_or(0);
            total_branch_misses += delta.branch_misses.unwrap_or(0);
            prev_snapshot = counters.read();
        }
    }

    let runs_u64 = u64::try_from(runs).unwrap_or(u64::MAX);
    let avg_time = total_time / u64_to_f64(runs_u64);
    let perf = perf_counters.map(|_| PerfCounters {
        cycles: Some(total_cycles / runs_u64),
        instructions: Some(total_instructions / runs_u64),
        branches: Some(total_branches / runs_u64),
        branch_misses: Some(total_branch_misses / runs_u64),
    });

    Ok(HostResult {
        time_secs: Some(avg_time),
        perf,
    })
}

// ============================================================================
// Formatting utilities
// ============================================================================

fn u64_to_f64(value: u64) -> f64 {
    let hi = u32::try_from(value >> 32).unwrap_or(u32::MAX);
    let lo = u32::try_from(value & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    f64::from(hi) * 4_294_967_296.0 + f64::from(lo)
}

/// Format a number with SI suffix (K, M, B).
#[must_use]
pub fn format_num(n: u64) -> String {
    if n >= 1_000_000_000 {
        let whole = n / 1_000_000_000;
        let frac = (n % 1_000_000_000) / 10_000_000;
        format!("{whole}.{frac:02}B")
    } else if n >= 1_000_000 {
        let whole = n / 1_000_000;
        let frac = (n % 1_000_000) / 10_000;
        format!("{whole}.{frac:02}M")
    } else if n >= 1_000 {
        let whole = n / 1_000;
        let frac = (n % 1_000) / 10;
        format!("{whole}.{frac:02}K")
    } else {
        n.to_string()
    }
}

/// Calculate overhead ratio (`vm_time` / `host_time`).
#[must_use]
pub fn calc_overhead(vm_time: f64, host_time: f64) -> Option<f64> {
    if host_time > 0.0 {
        Some(vm_time / host_time)
    } else {
        None
    }
}

/// Format overhead as "X.Xx".
#[must_use]
pub fn format_overhead(oh: Option<f64>) -> String {
    oh.map_or_else(|| "-".to_string(), |v| format!("{v:.1}x"))
}

/// Format IPC value.
#[must_use]
pub fn format_ipc(ipc: Option<f64>) -> String {
    ipc.map_or_else(|| "-".to_string(), |v| format!("{v:.2}"))
}

/// Format branch miss rate as percentage.
#[must_use]
pub fn format_branch_miss(rate: Option<f64>) -> String {
    rate.map_or_else(|| "-".to_string(), |v| format!("{v:.2}%"))
}

/// Format speed value with appropriate unit (`MIPS` or `BIPS`).
/// Input is in `MIPS` (millions of instructions per second).
#[must_use]
pub fn format_speed(mips: f64) -> String {
    if mips <= 0.0 {
        "-".to_string()
    } else if mips >= 1000.0 {
        // BIPS = billions of instructions per second
        format!("{:.2} BIPS", mips / 1000.0)
    } else if mips >= 1.0 {
        format!("{mips:.0} MIPS")
    } else {
        // Sub-MIPS: show with decimals
        format!("{mips:.2} MIPS")
    }
}

/// Format speed for shell parsing (underscore instead of space).
#[must_use]
pub fn format_speed_shell(mips: f64) -> String {
    if mips <= 0.0 {
        "-".to_string()
    } else if mips >= 1000.0 {
        format!("{:.2}_BIPS", mips / 1000.0)
    } else if mips >= 1.0 {
        format!("{mips:.0}_MIPS")
    } else {
        format!("{mips:.2}_MIPS")
    }
}

/// Format time value with appropriate unit (s, ms, us, ns).
/// Input is in seconds.
#[must_use]
pub fn format_time(secs: f64) -> String {
    if secs <= 0.0 {
        "-".to_string()
    } else if secs >= 1.0 {
        format!("{secs:.2}s")
    } else if secs >= 0.001 {
        format!("{:.2}ms", secs * 1000.0)
    } else if secs >= 0.000_001 {
        format!("{:.2}us", secs * 1_000_000.0)
    } else {
        format!("{:.2}ns", secs * 1_000_000_000.0)
    }
}

// ============================================================================
// Table output
// ============================================================================

/// Row in a benchmark results table.
#[derive(Debug, Clone)]
pub struct TableRow {
    /// Row label (arch name or `host`).
    pub label: String,
    /// Instruction count (guest instret), None for host.
    pub instret: Option<u64>,
    /// Host instructions executed.
    pub host_instrs: Option<u64>,
    /// Host instructions per guest instruction.
    pub instrs_per_guest: Option<f64>,
    /// Execution time in seconds.
    pub time_secs: Option<f64>,
    /// Overhead compared to host (`vm_time` / `host_time`).
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
    #[must_use]
    pub fn host(label: &str, result: &HostResult) -> Self {
        let (ipc, branch_miss_rate, host_instrs) =
            result.perf.as_ref().map_or((None, None, None), |p| {
                (p.ipc(), p.branch_miss_rate(), p.instructions)
            });

        Self {
            label: label.to_string(),
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

    /// Create a row for a VM backend.
    #[must_use]
    pub fn backend(label: &str, result: &RunResultWithPerf, host_time: Option<f64>) -> Self {
        let overhead = host_time.and_then(|ht| calc_overhead(result.result.time_secs, ht));
        let (ipc, branch_miss_rate, host_instrs) =
            result.perf.as_ref().map_or((None, None, None), |p| {
                (p.ipc(), p.branch_miss_rate(), p.instructions)
            });

        // Calculate host instructions per guest instruction
        let instrs_per_guest =
            host_instrs.map(|hi| u64_to_f64(hi) / u64_to_f64(result.result.instret));

        Self {
            label: label.to_string(),
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
    #[must_use]
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
#[must_use]
pub fn format_instrs_per_guest(ipg: Option<f64>) -> String {
    ipg.map_or_else(|| "-".to_string(), |v| format!("{v:.1}x"))
}

/// Print markdown table header for benchmark results.
pub fn print_bench_header(name: &str, description: &str, runs: usize) {
    println!("## {name}");
    println!();
    println!("*{description} | runs: {runs}*");
    println!();
    println!(
        "| {:<14} | {:>10} | {:>10} | {:>9} | {:>10} | {:>6} | {:>12} | {:>5} | {:>11} |",
        "Backend", "Instret", "Host Ops", "Ops/Guest", "Time", "OH", "Speed", "IPC", "Branch Miss"
    );
    println!(
        "|{:-<16}|{:-<12}|{:-<12}|{:-<11}|{:-<12}|{:-<8}|{:-<14}|{:-<7}|{:-<13}|",
        "", "", "", "", "", "", "", "", ""
    );
}

/// Print a table row.
pub fn print_table_row(row: &TableRow) {
    if let Some(ref err) = row.error {
        // Truncate error to fit in Speed column (12 chars)
        let err_display = if err.len() > 12 {
            format!("{err}...", err = &err[..9])
        } else {
            err.clone()
        };
        println!(
            "| {:<14} | {:>10} | {:>10} | {:>9} | {:>10} | {:>6} | {:>12} | {:>5} | {:>11} |",
            row.label, "-", "-", "-", "-", "-", err_display, "-", "-"
        );
        return;
    }

    let instret = row.instret.map_or_else(|| "-".to_string(), format_num);
    let host_instrs = row.host_instrs.map_or_else(|| "-".to_string(), format_num);
    let instrs_per_guest = format_instrs_per_guest(row.instrs_per_guest);
    let time = row.time_secs.map_or_else(|| "-".to_string(), format_time);
    let overhead = format_overhead(row.overhead);
    let speed = row.mips.map_or_else(|| "-".to_string(), format_speed);
    let ipc = format_ipc(row.ipc);
    let branch_miss = format_branch_miss(row.branch_miss_rate);

    println!(
        "| {:<14} | {:>10} | {:>10} | {:>9} | {:>10} | {:>6} | {:>12} | {:>5} | {:>11} |",
        row.label, instret, host_instrs, instrs_per_guest, time, overhead, speed, ipc, branch_miss
    );
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
