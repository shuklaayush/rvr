//! RVR CLI - RISC-V Recompiler

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use rvr::bench::{self, Arch, TableRow};
use rvr::{CompileOptions, InstretMode, SyscallMode};
use rvr_emit::{PassedVar, TracerConfig, TracerKind};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "rvr")]
#[command(about = "RISC-V Recompiler - compiles ELF to native code via C")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Instruction retirement counting mode.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
enum InstretModeArg {
    /// No instruction counting
    Off,
    /// Count instructions
    #[default]
    Count,
    /// Count and suspend at limit
    Suspend,
}

impl From<InstretModeArg> for InstretMode {
    fn from(arg: InstretModeArg) -> Self {
        match arg {
            InstretModeArg::Off => InstretMode::Off,
            InstretModeArg::Count => InstretMode::Count,
            InstretModeArg::Suspend => InstretMode::Suspend,
        }
    }
}

/// Tracer kind argument.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum TracerKindArg {
    None,
    Preflight,
    Stats,
    Ffi,
    Dynamic,
    Debug,
}

impl From<TracerKindArg> for TracerKind {
    fn from(arg: TracerKindArg) -> Self {
        match arg {
            TracerKindArg::None => TracerKind::None,
            TracerKindArg::Preflight => TracerKind::Preflight,
            TracerKindArg::Stats => TracerKind::Stats,
            TracerKindArg::Ffi => TracerKind::Ffi,
            TracerKindArg::Dynamic => TracerKind::Dynamic,
            TracerKindArg::Debug => TracerKind::Debug,
        }
    }
}

/// Syscall handling mode.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
enum SyscallModeArg {
    /// Bare-metal syscalls (exit only).
    #[default]
    Baremetal,
    /// Linux-style syscalls (brk/mmap/read/write, etc).
    Linux,
}

impl From<SyscallModeArg> for SyscallMode {
    fn from(arg: SyscallModeArg) -> Self {
        match arg {
            SyscallModeArg::Baremetal => SyscallMode::BareMetal,
            SyscallModeArg::Linux => SyscallMode::Linux,
        }
    }
}

/// Tracer configuration arguments.
#[derive(clap::Args, Clone, Debug)]
struct TracerArgs {
    /// Tracer kind (built-in).
    #[arg(long, value_enum, default_value = "none")]
    tracer: TracerKindArg,

    /// Custom tracer header path (overrides --tracer).
    #[arg(long)]
    tracer_header: Option<PathBuf>,

    /// Inline custom tracer header content (overrides --tracer).
    #[arg(long)]
    tracer_inline: Option<String>,

    /// Passed vars for the tracer (e.g. ptr:data, index:data_idx).
    #[arg(long = "tracer-pass", value_name = "KIND:NAME", action = clap::ArgAction::Append)]
    tracer_pass: Vec<String>,
}

/// Output format for run command.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
enum OutputFormat {
    /// Human-readable output (default)
    #[default]
    Text,
    /// Mojo-compatible output format
    Mojo,
    /// JSON output
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile an ELF file to a shared library
    Compile {
        /// Input ELF file
        #[arg(value_name = "ELF")]
        input: PathBuf,

        /// Output directory
        #[arg(short, long, default_value = "output")]
        output: PathBuf,

        /// Enable address checking
        #[arg(long)]
        addr_check: bool,

        /// Enable tohost check (for riscv-tests)
        #[arg(long)]
        tohost: bool,

        /// Instruction retirement mode
        #[arg(long, value_enum, default_value = "count")]
        instret: InstretModeArg,

        /// Syscall handling mode
        #[arg(long, value_enum, default_value = "baremetal")]
        syscalls: SyscallModeArg,

        /// Number of parallel compile jobs (0 = auto)
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,

        /// C compiler to use (e.g. clang or gcc)
        #[arg(long)]
        cc: Option<String>,

        #[command(flatten)]
        tracer: TracerArgs,
    },
    /// Lift an ELF file to C source (without compiling)
    Lift {
        /// Input ELF file
        #[arg(value_name = "ELF")]
        input: PathBuf,

        /// Output directory
        #[arg(short, long, default_value = "output")]
        output: PathBuf,

        /// Enable address checking
        #[arg(long)]
        addr_check: bool,

        /// Enable tohost check (for riscv-tests)
        #[arg(long)]
        tohost: bool,

        /// Instruction retirement mode
        #[arg(long, value_enum, default_value = "count")]
        instret: InstretModeArg,

        /// Syscall handling mode
        #[arg(long, value_enum, default_value = "baremetal")]
        syscalls: SyscallModeArg,

        #[command(flatten)]
        tracer: TracerArgs,
    },
    /// Run a compiled shared library
    Run {
        /// Directory containing the compiled shared library
        #[arg(value_name = "LIB_DIR")]
        lib_dir: PathBuf,

        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,

        /// Number of runs (for averaging)
        #[arg(long, default_value = "1")]
        runs: usize,
    },
    /// Run benchmarks
    Bench {
        #[command(subcommand)]
        command: BenchCommands,
    },
}

#[derive(Subcommand)]
enum BenchCommands {
    /// Benchmark reth-validator across architectures
    Reth {
        /// Architectures to benchmark (comma-separated: rv32i,rv32e,rv64i,rv64e)
        #[arg(short, long, default_value = "rv32i,rv32e,rv64i,rv64e")]
        arch: String,

        /// Number of runs for averaging
        #[arg(short, long, default_value = "3")]
        runs: usize,

        /// Enable tracing
        #[arg(short, long)]
        trace: bool,

        /// Fast mode (no instret counting)
        #[arg(short, long)]
        fast: bool,

        /// Skip compilation (use existing .so)
        #[arg(short, long)]
        no_compile: bool,
    },
}

fn parse_passed_vars(items: &[String]) -> Result<Vec<PassedVar>, String> {
    let mut vars = Vec::new();
    for item in items {
        let mut parts = item.splitn(2, ':');
        let kind = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if name.is_empty() {
            return Err(format!("invalid tracer var '{}', expected KIND:NAME", item));
        }
        let var = match kind {
            "ptr" => PassedVar::ptr(name),
            "index" => PassedVar::index(name),
            "value" => PassedVar::value(name),
            _ => {
                return Err(format!(
                    "invalid tracer var kind '{}', expected ptr/index/value",
                    kind
                ))
            }
        };
        vars.push(var);
    }
    Ok(vars)
}

fn build_tracer_config(args: &TracerArgs) -> Result<TracerConfig, String> {
    let passed_vars = parse_passed_vars(&args.tracer_pass)?;

    if args.tracer_header.is_some() && args.tracer_inline.is_some() {
        return Err("only one of --tracer-header or --tracer-inline may be used".to_string());
    }

    if let Some(path) = &args.tracer_header {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom");
        return Ok(TracerConfig::custom_file(name, path, passed_vars));
    }

    if let Some(inline) = &args.tracer_inline {
        return Ok(TracerConfig::custom_inline("inline", inline, passed_vars));
    }

    let mut config = TracerConfig::builtin(args.tracer.into());
    if !passed_vars.is_empty() {
        config = config.with_passed_vars(passed_vars);
    }
    Ok(config)
}

fn print_single_result(format: OutputFormat, result: &rvr::RunResult) {
    match format {
        OutputFormat::Text => {
            println!("Exit code: {}", result.exit_code);
            println!("Instructions: {}", result.instret);
            println!("Time: {:.6}s", result.time_secs);
            println!("Speed: {:.2} MIPS", result.mips);
        }
        OutputFormat::Mojo => {
            result.print_mojo_format();
        }
        OutputFormat::Json => {
            result.print_json();
        }
    }
}

fn print_multi_result(
    format: OutputFormat,
    runs: usize,
    first: &rvr::RunResult,
    avg_time: f64,
    avg_mips: f64,
) {
    match format {
        OutputFormat::Text => {
            println!("Runs: {}", runs);
            println!("Exit code: {}", first.exit_code);
            println!("Instructions: {}", first.instret);
            println!("Avg time: {:.6}s", avg_time);
            println!("Avg speed: {:.2} MIPS", avg_mips);
        }
        OutputFormat::Mojo => {
            println!("instret: {}", first.instret);
            println!("time: {:.6}", avg_time);
            println!("speed: {:.2} MIPS", avg_mips);
        }
        OutputFormat::Json => {
            println!(
                r#"{{"runs":{},"instret":{},"avg_time":{:.6},"avg_mips":{:.2},"exit_code":{}}}"#,
                runs, first.instret, avg_time, avg_mips, first.exit_code
            );
        }
    }
}

fn exit_if_failed(code: u8) {
    if code != 0 {
        std::process::exit(code as i32);
    }
}

fn main() {
    // Initialize tracing with env filter (RUST_LOG=debug for debug output)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("rvr=info".parse().unwrap()))
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            addr_check,
            tohost,
            instret,
            syscalls,
            jobs,
            cc,
            tracer,
        } => {
            info!(input = %input.display(), output = %output.display(), "compiling");
            let tracer_config = match build_tracer_config(&tracer) {
                Ok(config) => config,
                Err(err) => {
                    eprintln!("Error: {}", err);
                    std::process::exit(1);
                }
            };
            let options = CompileOptions::new()
                .with_addr_check(addr_check)
                .with_tohost(tohost)
                .with_instret_mode(instret.into())
                .with_syscall_mode(syscalls.into())
                .with_tracer_config(tracer_config)
                .with_jobs(jobs);
            let options = if let Some(cc) = cc {
                options.with_compiler(cc)
            } else {
                options
            };
            match rvr::compile_with_options(&input, &output, options) {
                Ok(path) => info!(output = %path.display(), "done"),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Lift {
            input,
            output,
            addr_check,
            tohost,
            instret,
            syscalls,
            tracer,
        } => {
            info!(input = %input.display(), output = %output.display(), "lifting");
            let tracer_config = match build_tracer_config(&tracer) {
                Ok(config) => config,
                Err(err) => {
                    eprintln!("Error: {}", err);
                    std::process::exit(1);
                }
            };
            let options = CompileOptions::new()
                .with_addr_check(addr_check)
                .with_tohost(tohost)
                .with_instret_mode(instret.into())
                .with_syscall_mode(syscalls.into())
                .with_tracer_config(tracer_config);
            match rvr::lift_to_c_with_options(&input, &output, options) {
                Ok(path) => info!(output = %path.display(), "done"),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Run {
            lib_dir,
            format,
            runs,
        } => {
            let runner = match rvr::Runner::load(&lib_dir) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error loading library: {}", e);
                    std::process::exit(1);
                }
            };

            if runs <= 1 {
                match runner.run() {
                    Ok(result) => {
                        print_single_result(format, &result);
                        exit_if_failed(result.exit_code);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match runner.run_multiple(runs) {
                    Ok(results) => {
                        let avg_time: f64 =
                            results.iter().map(|r| r.time_secs).sum::<f64>() / runs as f64;
                        let avg_mips: f64 =
                            results.iter().map(|r| r.mips).sum::<f64>() / runs as f64;
                        let first = &results[0];

                        print_multi_result(format, runs, first, avg_time, avg_mips);
                        exit_if_failed(first.exit_code);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Bench { command } => match command {
            BenchCommands::Reth {
                arch,
                runs,
                trace,
                fast,
                no_compile,
            } => {
                run_reth_benchmark(&arch, runs, trace, fast, no_compile);
            }
        },
    }
}

fn run_reth_benchmark(arch_str: &str, runs: usize, trace: bool, fast: bool, no_compile: bool) {
    // Parse architectures
    let archs = match Arch::parse_list(arch_str) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Get project directory (assuming we're running from project root or rvr is installed)
    let project_dir = std::env::current_dir().expect("failed to get current directory");
    let bin_dir = project_dir.join("bin/reth");

    // Check if perf is available
    let use_perf = bench::perf_available();

    // Collect results
    let mut rows = Vec::new();

    bench::print_table_header(trace, fast, runs);

    for arch in &archs {
        let row = run_single_arch(&project_dir, &bin_dir, *arch, runs, trace, fast, no_compile, use_perf);
        bench::print_table_row(&row);
        rows.push(row);
    }

    println!();
}

fn run_single_arch(
    project_dir: &std::path::Path,
    bin_dir: &std::path::Path,
    arch: Arch,
    runs: usize,
    trace: bool,
    fast: bool,
    no_compile: bool,
    use_perf: bool,
) -> TableRow {
    let elf_path = bin_dir.join(arch.as_str()).join("reth-validator");

    // Check if ELF exists
    if !elf_path.exists() {
        return TableRow::error(
            arch,
            format!("ELF not found (run scripts/reth-build.sh {})", arch),
        );
    }

    // Determine output directory
    let suffix = match (trace, fast) {
        (true, true) => "trace-fast",
        (true, false) => "trace",
        (false, true) => "fast",
        (false, false) => "base",
    };
    let out_dir = project_dir
        .join("target")
        .join(arch.as_str())
        .join(format!("reth-{}", suffix));

    // Compile if needed
    if !no_compile || !out_dir.exists() {
        let mut options = CompileOptions::new();
        if trace {
            options = options.with_tracer_config(rvr_emit::TracerConfig::builtin(
                rvr_emit::TracerKind::Stats,
            ));
        }
        if fast {
            options = options.with_instret_mode(InstretMode::Off);
        }

        if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
            return TableRow::error(arch, format!("compile failed: {}", e));
        }
    }

    // Run benchmark
    let result = if use_perf {
        bench::run_with_perf(&out_dir, runs)
    } else {
        bench::run_without_perf(&out_dir, runs)
    };

    match result {
        Ok(bench_result) => TableRow::success(arch, bench_result),
        Err(e) => TableRow::error(arch, e),
    }
}
