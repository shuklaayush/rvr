//! RVR CLI - RISC-V Recompiler

use std::path::PathBuf;
use std::process::Command;

use clap::{Parser, Subcommand, ValueEnum};
use rvr::bench::{self, Arch, TableRow};
use rvr::tests::{self, TestConfig};
use rvr::{CompileOptions, Compiler, InstretMode, SyscallMode};
use rvr_emit::{PassedVar, TracerConfig, TracerKind};
use tracing::{error, info};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "rvr")]
#[command(about = "RISC-V Recompiler - compiles ELF to native code via C")]
#[command(version)]
struct Cli {
    /// Show metrics summary after execution
    #[arg(long, global = true)]
    metrics: bool,

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
    /// Raw key-value output (for scripting)
    Raw,
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

        /// Emit #line directives with source locations (requires debug info in ELF)
        #[arg(long)]
        line_info: bool,

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

        /// Path to the ELF file
        #[arg(value_name = "ELF_PATH")]
        elf_path: PathBuf,

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
    /// Run test suites
    Test {
        #[command(subcommand)]
        command: TestCommands,
    },
}

#[derive(Subcommand)]
enum BenchCommands {
    /// Benchmark reth-validator
    Reth {
        #[command(subcommand)]
        command: RethBenchCommands,
    },
}

#[derive(Subcommand)]
enum TestCommands {
    /// Run riscv-tests suite
    Riscv {
        #[command(subcommand)]
        command: RiscvTestCommands,
    },
}

#[derive(Subcommand)]
enum RiscvTestCommands {
    /// Build riscv-tests from source (requires riscv toolchain)
    Build {
        /// Test categories to build (comma-separated, or "all")
        #[arg(short, long, default_value = "all")]
        category: String,

        /// Output directory for built binaries
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Toolchain prefix (e.g., riscv64-unknown-elf-)
        #[arg(long)]
        toolchain: Option<String>,
    },
    /// Run riscv-tests
    Run {
        /// Filter pattern (e.g., "rv64" to only run rv64 tests)
        #[arg(short, long)]
        filter: Option<String>,

        /// Verbose output (show all tests, not just failures)
        #[arg(short, long)]
        verbose: bool,

        /// Timeout per test in seconds
        #[arg(short, long, default_value = "10")]
        timeout: u64,
    },
}

#[derive(Subcommand)]
enum RethBenchCommands {
    /// Build RISC-V ELF binaries and host binary from source
    Build {
        /// Architectures to build (comma-separated: rv32i,rv32e,rv64i,rv64e)
        #[arg(short, long, default_value = "rv32i,rv32e,rv64i,rv64e")]
        arch: String,

        /// Skip building host binary
        #[arg(long)]
        no_host: bool,
    },
    /// Compile ELF binaries to native code (via rvr)
    Compile {
        /// Architectures to compile (comma-separated: rv32i,rv32e,rv64i,rv64e)
        #[arg(short, long, default_value = "rv32i,rv32e,rv64i,rv64e")]
        arch: String,

        /// Enable tracing
        #[arg(short, long)]
        trace: bool,

        /// Fast mode (no instret counting)
        #[arg(short, long)]
        fast: bool,

        /// C compiler to use (clang or gcc)
        #[arg(long, default_value = "clang")]
        cc: String,
    },
    /// Run benchmarks (assumes already compiled)
    Run {
        /// Architectures to benchmark (comma-separated: rv32i,rv32e,rv64i,rv64e)
        #[arg(short, long, default_value = "rv32i,rv32e,rv64i,rv64e")]
        arch: String,

        /// Number of runs for averaging
        #[arg(short, long, default_value = "3")]
        runs: usize,

        /// Enable tracing (must match compile)
        #[arg(short, long)]
        trace: bool,

        /// Fast mode (must match compile)
        #[arg(short, long)]
        fast: bool,
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
            println!("Speed: {}", rvr::bench::format_speed(result.mips));
        }
        OutputFormat::Raw => {
            println!("instret: {}", result.instret);
            println!("time: {:.6}", result.time_secs);
            println!("speed: {}", rvr::bench::format_speed_shell(result.mips));
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
            println!("Avg speed: {}", rvr::bench::format_speed(avg_mips));
        }
        OutputFormat::Raw => {
            println!("instret: {}", first.instret);
            println!("time: {:.6}", avg_time);
            println!("speed: {}", rvr::bench::format_speed_shell(avg_mips));
        }
        OutputFormat::Json => {
            println!(
                r#"{{"runs":{},"instret":{},"avg_time":{:.6},"avg_mips":{:.2},"exit_code":{}}}"#,
                runs, first.instret, avg_time, avg_mips, first.exit_code
            );
        }
    }
}

fn main() {
    let cli = Cli::parse();

    // Initialize metrics recorder if enabled
    let metrics_handle = if cli.metrics {
        let recorder = rvr::metrics::CliRecorder::new();
        recorder.install()
    } else {
        None
    };

    // Initialize metric descriptions
    rvr::metrics::init();

    // Initialize tracing with appropriate level based on command
    let default_level = match &cli.command {
        Commands::Bench { .. } | Commands::Test { .. } => "rvr=warn",
        _ => "rvr=info",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(default_level.parse().unwrap()),
        )
        .with_target(false)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let exit_code = run_command(&cli);

    // Print metrics summary if enabled
    if let Some(handle) = metrics_handle {
        handle.print_summary();
    }

    std::process::exit(exit_code);
}

fn run_command(cli: &Cli) -> i32 {
    match &cli.command {
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
            let tracer_config = match build_tracer_config(tracer) {
                Ok(config) => config,
                Err(err) => {
                    error!(error = %err, "invalid tracer configuration");
                    return 1;
                }
            };
            let options = CompileOptions::new()
                .with_addr_check(*addr_check)
                .with_tohost(*tohost)
                .with_instret_mode((*instret).into())
                .with_syscall_mode((*syscalls).into())
                .with_tracer_config(tracer_config)
                .with_jobs(*jobs);
            let options = if let Some(cc) = cc {
                let compiler: Compiler = cc.parse().unwrap_or_else(|e| {
                    error!(error = %e, "invalid compiler");
                    std::process::exit(1);
                });
                options.with_compiler(compiler)
            } else {
                options
            };
            match rvr::compile_with_options(input, output, options) {
                Ok(path) => {
                    info!(output = %path.display(), "done");
                    0
                }
                Err(e) => {
                    error!(error = %e, "compilation failed");
                    1
                }
            }
        }
        Commands::Lift {
            input,
            output,
            addr_check,
            tohost,
            line_info,
            instret,
            syscalls,
            tracer,
        } => {
            info!(input = %input.display(), output = %output.display(), "lifting");
            let tracer_config = match build_tracer_config(tracer) {
                Ok(config) => config,
                Err(err) => {
                    error!(error = %err, "invalid tracer configuration");
                    return 1;
                }
            };
            let options = CompileOptions::new()
                .with_addr_check(*addr_check)
                .with_tohost(*tohost)
                .with_line_info(*line_info)
                .with_instret_mode((*instret).into())
                .with_syscall_mode((*syscalls).into())
                .with_tracer_config(tracer_config);
            match rvr::lift_to_c_with_options(input, output, options) {
                Ok(path) => {
                    info!(output = %path.display(), "done");
                    0
                }
                Err(e) => {
                    error!(error = %e, "lift failed");
                    1
                }
            }
        }
        Commands::Run {
            lib_dir,
            elf_path,
            format,
            runs,
        } => {
            let mut runner = match rvr::Runner::load(lib_dir, elf_path) {
                Ok(r) => r,
                Err(e) => {
                    error!(error = %e, path = %lib_dir.display(), "failed to load library");
                    return 1;
                }
            };

            if *runs <= 1 {
                match runner.run() {
                    Ok(result) => {
                        print_single_result(*format, &result);
                        result.exit_code as i32
                    }
                    Err(e) => {
                        error!(error = %e, "execution failed");
                        1
                    }
                }
            } else {
                match runner.run_multiple(*runs) {
                    Ok(results) => {
                        let avg_time: f64 =
                            results.iter().map(|r| r.time_secs).sum::<f64>() / *runs as f64;
                        let avg_mips: f64 =
                            results.iter().map(|r| r.mips).sum::<f64>() / *runs as f64;
                        let first = &results[0];

                        print_multi_result(*format, *runs, first, avg_time, avg_mips);
                        first.exit_code as i32
                    }
                    Err(e) => {
                        error!(error = %e, "execution failed");
                        1
                    }
                }
            }
        }
        Commands::Bench { command } => {
            match command {
                BenchCommands::Reth { command } => match command {
                    RethBenchCommands::Build { arch, no_host } => {
                        reth_build(arch, *no_host);
                    }
                    RethBenchCommands::Compile {
                        arch,
                        trace,
                        fast,
                        cc,
                    } => {
                        reth_compile(arch, *trace, *fast, cc);
                    }
                    RethBenchCommands::Run {
                        arch,
                        runs,
                        trace,
                        fast,
                    } => {
                        reth_run(arch, *runs, *trace, *fast);
                    }
                },
            }
            0
        }
        Commands::Test { command } => match command {
            TestCommands::Riscv { command } => match command {
                RiscvTestCommands::Build {
                    category,
                    output,
                    toolchain,
                } => riscv_tests_build(category, output.clone(), toolchain.clone()),
                RiscvTestCommands::Run {
                    filter,
                    verbose,
                    timeout,
                } => riscv_tests_run(filter.clone(), *verbose, *timeout),
            },
        },
    }
}

/// Build RISC-V ELF binaries and host binary from source.
fn reth_build(arch_str: &str, no_host: bool) {
    let project_dir = std::env::current_dir().expect("failed to get current directory");
    let reth_dir = project_dir.join("programs/reth-validator");

    if !reth_dir.exists() {
        eprintln!("Error: {} not found", reth_dir.display());
        std::process::exit(1);
    }

    let archs: Vec<&str> = arch_str.split(',').map(|s| s.trim()).collect();

    // Build host binary first (unless --no-host)
    if !no_host {
        eprintln!("Building host binary...");
        let status = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("--manifest-path")
            .arg(reth_dir.join("Cargo.toml"))
            .status()
            .expect("failed to run cargo");

        if !status.success() {
            eprintln!("Host build failed");
            std::process::exit(1);
        }
    }

    // Build RISC-V ELFs via make
    eprintln!("Building RISC-V ELFs: {}", archs.join(", "));

    let status = Command::new("make")
        .arg("-C")
        .arg(&reth_dir)
        .args(&archs)
        .status()
        .expect("failed to run make");

    if !status.success() {
        eprintln!("ELF build failed");
        std::process::exit(1);
    }

    // List output binaries
    eprintln!();
    eprintln!("Build complete. Output binaries:");

    // Host binary
    if !no_host {
        let host_bin = reth_dir.join("target/release/reth-validator");
        if host_bin.exists() {
            if let Ok(meta) = std::fs::metadata(&host_bin) {
                eprintln!("  {} ({} bytes)", host_bin.display(), meta.len());
            }
        }
    }

    // RISC-V ELFs
    for arch in &archs {
        let bin_path = project_dir.join(format!("bin/reth/{}/reth-validator", arch));
        if bin_path.exists() {
            if let Ok(meta) = std::fs::metadata(&bin_path) {
                eprintln!("  {} ({} bytes)", bin_path.display(), meta.len());
            }
        }
    }
}

/// Compile reth-validator ELFs to native code for all specified architectures.
fn reth_compile(arch_str: &str, trace: bool, fast: bool, cc: &str) {
    let archs = match Arch::parse_list(arch_str) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let project_dir = std::env::current_dir().expect("failed to get current directory");
    let bin_dir = project_dir.join("bin/reth");

    let suffix = get_bench_suffix(trace, fast);

    for arch in &archs {
        let elf_path = bin_dir.join(arch.as_str()).join("reth-validator");
        if !elf_path.exists() {
            eprintln!("Warning: {} not found, skipping", elf_path.display());
            continue;
        }

        let out_dir = project_dir
            .join("target")
            .join(arch.as_str())
            .join(format!("reth-{}", suffix));

        let compiler: Compiler = cc.parse().unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        });

        eprintln!("Compiling {} ({}) -> {}", arch, compiler, out_dir.display());

        let mut options = CompileOptions::new().with_compiler(compiler);
        if trace {
            options = options.with_tracer_config(TracerConfig::builtin(TracerKind::Stats));
        }
        if fast {
            options = options.with_instret_mode(InstretMode::Off);
        }

        if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
            eprintln!("Error compiling {}: {}", arch, e);
            std::process::exit(1);
        }
    }

    eprintln!("Compile complete.");
}

/// Run benchmarks for all specified architectures.
fn reth_run(arch_str: &str, runs: usize, trace: bool, fast: bool) {
    let archs = match Arch::parse_list(arch_str) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Run host binary first to get baseline (optional - may not exist on all platforms)
    let host_bin = project_dir.join("programs/reth-validator/target/release/reth-validator");
    let (host_result, host_time) = match bench::run_host(&host_bin, runs) {
        Ok(result) => {
            let time = result.time_secs;
            (Some(result), time)
        }
        Err(_) => (None, None),
    };

    let suffix = get_bench_suffix(trace, fast);

    // Collect all rows (host row only if we have host results)
    let mut rows: Vec<TableRow> = Vec::new();
    if let Some(ref result) = host_result {
        rows.push(TableRow::host(result));
    }

    let bin_dir = project_dir.join("bin/reth");

    for arch in &archs {
        let elf_path = bin_dir.join(arch.as_str()).join("reth-validator");
        let out_dir = project_dir
            .join("target")
            .join(arch.as_str())
            .join(format!("reth-{}", suffix));

        let row = if !out_dir.exists() {
            TableRow::error(
                arch.as_str(),
                "not compiled (run `rvr bench reth compile` first)".to_string(),
            )
        } else {
            match bench::run_bench(&out_dir, &elf_path, runs) {
                Ok(result) => TableRow::arch(*arch, &result, host_time),
                Err(e) => TableRow::error(arch.as_str(), e),
            }
        };

        rows.push(row);
    }

    // Sort by time (fastest first), errors go last
    rows.sort_by(|a, b| match (a.time_secs, b.time_secs) {
        (Some(ta), Some(tb)) => ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    // Print header and sorted rows
    bench::print_table_header(trace, fast, runs);
    for row in &rows {
        bench::print_table_row(row);
    }

    println!();
}

fn get_bench_suffix(trace: bool, fast: bool) -> &'static str {
    match (trace, fast) {
        (true, true) => "trace-fast",
        (true, false) => "trace",
        (false, true) => "fast",
        (false, false) => "base",
    }
}

/// Build riscv-tests from source.
fn riscv_tests_build(
    category_str: &str,
    output: Option<PathBuf>,
    toolchain: Option<String>,
) -> i32 {
    use rvr::tests::{BuildConfig, TestCategory};

    // Parse categories
    let categories = match TestCategory::parse_list(category_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    // Find toolchain
    let toolchain = match toolchain.or_else(rvr::tests::find_toolchain) {
        Some(t) => t,
        None => {
            eprintln!("Error: RISC-V toolchain not found");
            eprintln!("Install riscv64-unknown-elf-gcc or specify --toolchain");
            return 1;
        }
    };

    let project_dir = std::env::current_dir().expect("failed to get current directory");

    let mut config = BuildConfig::new(categories)
        .with_src_dir(project_dir.join("tests/riscv-tests/isa"))
        .with_toolchain(&toolchain);

    if let Some(out) = output {
        config = config.with_out_dir(out);
    } else {
        config = config.with_out_dir(project_dir.join("bin/riscv/tests"));
    }

    eprintln!("Using toolchain: {}gcc", toolchain);
    eprintln!("Source: {}", config.src_dir.display());
    eprintln!("Output: {}", config.out_dir.display());
    eprintln!();

    eprintln!("Building {} categories...", config.categories.len());

    let results = match rvr::tests::build_tests(&config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    // Print per-category results
    for result in &results {
        if result.failed > 0 {
            eprintln!(
                "  {}: {} built, {} failed",
                result.category, result.built, result.failed
            );
        } else {
            eprintln!("  {}: {} tests", result.category, result.built);
        }
    }

    rvr::tests::print_build_summary(&results);
    0
}

/// Run riscv-tests suite.
fn riscv_tests_run(filter: Option<String>, verbose: bool, timeout: u64) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");
    let test_dir = project_dir.join("bin/riscv/tests");

    if !test_dir.exists() {
        eprintln!("Error: test directory not found: {}", test_dir.display());
        eprintln!("Place riscv-tests ELF binaries in bin/riscv/tests/");
        return 1;
    }

    let config = TestConfig::default()
        .with_test_dir(test_dir)
        .with_verbose(verbose)
        .with_timeout(timeout);
    let config = if let Some(f) = filter {
        config.with_filter(f)
    } else {
        config
    };

    let summary = tests::run_all(&config);
    tests::print_summary(&summary);

    if summary.all_passed() {
        0
    } else {
        1
    }
}
