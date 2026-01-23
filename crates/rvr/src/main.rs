//! RVR CLI - RISC-V Recompiler

use std::path::PathBuf;
use std::process::Command;

use clap::{Parser, Subcommand, ValueEnum};
use rvr::bench::{self, Arch};
use rvr::tests::{self, TestConfig};
use rvr::{CompileOptions, Compiler, InstretMode, SyscallMode};
use rvr_emit::{PassedVar, TracerConfig, TracerKind};
use tracing::{error, info};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

/// Exit code for success.
const EXIT_SUCCESS: i32 = 0;
/// Exit code for failure.
const EXIT_FAILURE: i32 = 1;

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

        /// Enable HTIF (Host-Target Interface) for riscv-tests
        #[arg(long)]
        htif: bool,

        /// Instruction retirement mode
        #[arg(long, value_enum, default_value = "count")]
        instret: InstretModeArg,

        /// Syscall handling mode
        #[arg(long, value_enum, default_value = "baremetal")]
        syscalls: SyscallModeArg,

        /// Number of parallel compile jobs (0 = auto)
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,

        /// C compiler command (e.g., clang, clang-20, gcc-13)
        #[arg(long)]
        cc: Option<String>,

        /// Linker to use (e.g., lld, lld-20). Auto-derived from --cc if not specified.
        #[arg(long)]
        linker: Option<String>,

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

        /// Enable HTIF (Host-Target Interface) for riscv-tests
        #[arg(long)]
        htif: bool,

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
    /// Build Rust project to RISC-V ELF
    Build {
        /// Path to Rust project (directory with Cargo.toml)
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,

        /// Target architectures (comma-separated: rv32i,rv32e,rv64i,rv64e)
        #[arg(short, long, default_value = "rv64i")]
        target: String,

        /// Output directory for ELF binaries (default: bin/{arch}/{name})
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Rust toolchain to use (default: nightly)
        #[arg(long, default_value = "nightly")]
        toolchain: String,

        /// Additional features to enable
        #[arg(long)]
        features: Option<String>,

        /// Build in release mode (default: true)
        #[arg(long, default_value = "true")]
        release: bool,
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
    /// List available benchmarks
    List,
    /// Build benchmark ELF from source
    Build {
        /// Benchmark name (omit to build all)
        #[arg(value_name = "NAME")]
        name: Option<String>,

        /// Architectures to build (comma-separated: rv32i,rv64i)
        #[arg(short, long)]
        arch: Option<String>,

        /// Skip building host binary (for benchmarks that have one)
        #[arg(long)]
        no_host: bool,
    },
    /// Compile benchmark ELF to native .so
    Compile {
        /// Benchmark name (omit to compile all)
        #[arg(value_name = "NAME")]
        name: Option<String>,

        /// Architectures to compile (comma-separated: rv32i,rv64i)
        #[arg(short, long)]
        arch: Option<String>,

        /// Fast mode (no instret counting)
        #[arg(short, long)]
        fast: bool,

        /// C compiler command
        #[arg(long, default_value = "clang")]
        cc: String,

        /// Linker to use
        #[arg(long)]
        linker: Option<String>,
    },
    /// Run compiled benchmark
    Run {
        /// Benchmark name (omit to run all)
        #[arg(value_name = "NAME")]
        name: Option<String>,

        /// Architectures to run (comma-separated: rv32i,rv64i)
        #[arg(short, long)]
        arch: Option<String>,

        /// Number of runs for averaging
        #[arg(short, long, default_value = "3")]
        runs: usize,

        /// Fast mode (must match compile)
        #[arg(short, long)]
        fast: bool,

        /// Include host binary comparison (if available)
        #[arg(long)]
        compare_host: bool,
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
            htif,
            instret,
            syscalls,
            jobs,
            cc,
            linker,
            tracer,
        } => {
            info!(input = %input.display(), output = %output.display(), "compiling");
            let tracer_config = match build_tracer_config(tracer) {
                Ok(config) => config,
                Err(err) => {
                    error!(error = %err, "invalid tracer configuration");
                    return EXIT_FAILURE;
                }
            };
            let options = CompileOptions::new()
                .with_addr_check(*addr_check)
                .with_htif(*htif)
                .with_instret_mode((*instret).into())
                .with_syscall_mode((*syscalls).into())
                .with_tracer_config(tracer_config)
                .with_jobs(*jobs);
            let options = if let Some(cc) = cc {
                let mut compiler: Compiler = cc.parse().unwrap_or_else(|e| {
                    error!(error = %e, "invalid compiler");
                    std::process::exit(EXIT_FAILURE);
                });
                if let Some(ld) = linker {
                    compiler = compiler.with_linker(ld);
                }
                options.with_compiler(compiler)
            } else {
                options
            };
            match rvr::compile_with_options(input, output, options) {
                Ok(path) => {
                    info!(output = %path.display(), "done");
                    EXIT_SUCCESS
                }
                Err(e) => {
                    error!(error = %e, "compilation failed");
                    EXIT_FAILURE
                }
            }
        }
        Commands::Lift {
            input,
            output,
            addr_check,
            htif,
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
                    return EXIT_FAILURE;
                }
            };
            let options = CompileOptions::new()
                .with_addr_check(*addr_check)
                .with_htif(*htif)
                .with_line_info(*line_info)
                .with_instret_mode((*instret).into())
                .with_syscall_mode((*syscalls).into())
                .with_tracer_config(tracer_config);
            match rvr::lift_to_c_with_options(input, output, options) {
                Ok(path) => {
                    info!(output = %path.display(), "done");
                    EXIT_SUCCESS
                }
                Err(e) => {
                    error!(error = %e, "lift failed");
                    EXIT_FAILURE
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
                    return EXIT_FAILURE;
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
                        EXIT_FAILURE
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
                        EXIT_FAILURE
                    }
                }
            }
        }
        Commands::Build {
            path,
            target,
            output,
            toolchain,
            features,
            release,
        } => build_rust_project(path, target, output.as_ref(), toolchain, features.as_deref(), *release),
        Commands::Bench { command } => match command {
            BenchCommands::List => {
                bench_list();
                EXIT_SUCCESS
            }
            BenchCommands::Build { name, arch, no_host } => {
                bench_build(name.as_deref(), arch.as_deref(), *no_host)
            }
            BenchCommands::Compile {
                name,
                arch,
                fast,
                cc,
                linker,
            } => bench_compile(name.as_deref(), arch.as_deref(), *fast, cc, linker.as_deref()),
            BenchCommands::Run {
                name,
                arch,
                runs,
                fast,
                compare_host,
            } => bench_run(name.as_deref(), arch.as_deref(), *runs, *fast, *compare_host),
        },
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

// --- Rust build support ---

/// Embedded target specs
mod targets {
    pub const RV32I: &str = include_str!("../../../toolchain/rv32i.json");
    pub const RV32E: &str = include_str!("../../../toolchain/rv32e.json");
    pub const RV64I: &str = include_str!("../../../toolchain/rv64i.json");
    pub const RV64E: &str = include_str!("../../../toolchain/rv64e.json");
    pub const LINK_X: &str = include_str!("../../../toolchain/link.x");

    pub fn get_target_spec(arch: &str) -> Option<&'static str> {
        match arch {
            "rv32i" => Some(RV32I),
            "rv32e" => Some(RV32E),
            "rv64i" => Some(RV64I),
            "rv64e" => Some(RV64E),
            _ => None,
        }
    }
}

/// Build a Rust project to RISC-V ELF.
fn build_rust_project(
    path: &PathBuf,
    target_str: &str,
    output: Option<&PathBuf>,
    toolchain: &str,
    features: Option<&str>,
    release: bool,
) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Resolve project path
    let project_path = if path.is_absolute() {
        path.clone()
    } else {
        project_dir.join(path)
    };

    // Check Cargo.toml exists
    let cargo_toml = project_path.join("Cargo.toml");
    if !cargo_toml.exists() {
        eprintln!("Error: {} not found", cargo_toml.display());
        return EXIT_FAILURE;
    }

    // Get project name from Cargo.toml
    let cargo_content = match std::fs::read_to_string(&cargo_toml) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", cargo_toml.display(), e);
            return EXIT_FAILURE;
        }
    };

    let project_name = cargo_content
        .lines()
        .find(|line| line.starts_with("name"))
        .and_then(|line| line.split('=').nth(1))
        .map(|s| s.trim().trim_matches('"'))
        .unwrap_or("unknown");

    // Parse target architectures
    let targets: Vec<&str> = target_str.split(',').map(|s| s.trim()).collect();

    // Create temp directory for target specs
    let target_dir = project_path.join("target/.rvr");
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!("Error creating {}: {}", target_dir.display(), e);
        return EXIT_FAILURE;
    }

    // Write linker script
    let link_x_path = target_dir.join("link.x");
    if let Err(e) = std::fs::write(&link_x_path, targets::LINK_X) {
        eprintln!("Error writing link.x: {}", e);
        return EXIT_FAILURE;
    }

    for arch in &targets {
        // Get and write target spec
        let spec = match targets::get_target_spec(arch) {
            Some(s) => s,
            None => {
                eprintln!("Error: unknown target '{}'", arch);
                eprintln!("Supported targets: rv32i, rv32e, rv64i, rv64e");
                return EXIT_FAILURE;
            }
        };

        let spec_path = target_dir.join(format!("{}.json", arch));
        if let Err(e) = std::fs::write(&spec_path, spec) {
            eprintln!("Error writing {}: {}", spec_path.display(), e);
            return EXIT_FAILURE;
        }

        eprintln!("Building {} for {}", project_name, arch);

        // Determine RUSTFLAGS
        let cpu = if arch.starts_with("rv64") {
            "generic-rv64"
        } else {
            "generic-rv32"
        };

        let rustflags = format!(
            "-Clink-arg=-T{} -Clink-arg=--gc-sections -Ctarget-cpu={} -Ccode-model=medium",
            link_x_path.display(),
            cpu
        );

        // Build cargo command
        let mut cmd = Command::new("cargo");
        cmd.arg(format!("+{}", toolchain))
            .arg("build")
            .arg("--target")
            .arg(&spec_path)
            .arg("-Zbuild-std=core,alloc")
            .arg("-Zbuild-std-features=compiler-builtins-mem")
            .current_dir(&project_path)
            .env("RUSTFLAGS", &rustflags);

        if release {
            cmd.arg("--release");
        }

        if let Some(feats) = features {
            cmd.arg("--features").arg(feats);
        }

        let status = match cmd.status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error running cargo: {}", e);
                return EXIT_FAILURE;
            }
        };

        if !status.success() {
            eprintln!("Build failed for {}", arch);
            return EXIT_FAILURE;
        }

        // Copy output to destination
        let profile = if release { "release" } else { "debug" };
        let build_output = project_path
            .join("target")
            .join(arch)
            .join(profile)
            .join(project_name);

        let dest_dir = match output {
            Some(o) => o.join(arch),
            None => project_dir.join("bin").join(arch),
        };

        if let Err(e) = std::fs::create_dir_all(&dest_dir) {
            eprintln!("Error creating {}: {}", dest_dir.display(), e);
            return EXIT_FAILURE;
        }

        let dest_path = dest_dir.join(project_name);
        if let Err(e) = std::fs::copy(&build_output, &dest_path) {
            eprintln!(
                "Error copying {} to {}: {}",
                build_output.display(),
                dest_path.display(),
                e
            );
            return EXIT_FAILURE;
        }

        eprintln!("  -> {}", dest_path.display());
    }

    eprintln!("Build complete.");
    EXIT_SUCCESS
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
            return EXIT_FAILURE;
        }
    };

    // Find toolchain
    let toolchain = match toolchain.or_else(rvr::tests::find_toolchain) {
        Some(t) => t,
        None => {
            eprintln!("Error: RISC-V toolchain not found");
            eprintln!("Install riscv64-unknown-elf-gcc or specify --toolchain");
            return EXIT_FAILURE;
        }
    };

    let project_dir = std::env::current_dir().expect("failed to get current directory");

    let mut config = BuildConfig::new(categories)
        .with_src_dir(project_dir.join("tests/riscv-tests/isa"))
        .with_toolchain(&toolchain);

    if let Some(out) = output {
        config = config.with_out_dir(out);
    } else {
        config = config.with_out_dir(project_dir.join("bin/riscv-tests"));
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
            return EXIT_FAILURE;
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
    EXIT_SUCCESS
}

/// Run riscv-tests suite.
fn riscv_tests_run(filter: Option<String>, verbose: bool, timeout: u64) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");
    let test_dir = project_dir.join("bin/riscv-tests");

    if !test_dir.exists() {
        eprintln!("Error: test directory not found: {}", test_dir.display());
        eprintln!("Place riscv-tests ELF binaries in bin/riscv-tests/");
        return EXIT_FAILURE;
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
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    }
}

// --- Benchmark registry ---

/// How to build a benchmark.
#[derive(Clone, Copy)]
enum BenchmarkSource {
    /// Rust project - build with `rvr build`
    Rust {
        /// Path to project directory (relative to repo root)
        path: &'static str,
    },
    /// Prebuilt ELF - already in bin/{arch}/{name}
    Prebuilt,
}

/// Benchmark metadata.
struct BenchmarkInfo {
    /// Benchmark name (used in CLI and paths).
    name: &'static str,
    /// Short description.
    description: &'static str,
    /// Whether benchmark uses export_functions mode (initialize/run pattern).
    /// If false, runs from ELF entry point.
    uses_exports: bool,
    /// Path to host binary relative to project root (for comparison).
    /// None if no host binary available.
    host_binary: Option<&'static str>,
    /// Default architectures for this benchmark.
    default_archs: &'static str,
    /// How to build this benchmark.
    source: BenchmarkSource,
}

/// All registered benchmarks.
/// ELF binaries are at: bin/{arch}/{name}
const BENCHMARKS: &[BenchmarkInfo] = &[
    BenchmarkInfo {
        name: "minimal",
        description: "Minimal function call overhead",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "prime-sieve",
        description: "Prime number sieve algorithm",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "pinky",
        description: "NES emulator (cycle-accurate)",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "memset",
        description: "Memory set operations",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        source: BenchmarkSource::Prebuilt,
    },
    BenchmarkInfo {
        name: "reth",
        description: "Reth block validator",
        uses_exports: false,
        host_binary: Some("programs/reth-validator/target/release/reth-validator"),
        default_archs: "rv64i",
        source: BenchmarkSource::Rust {
            path: "programs/reth-validator",
        },
    },
];

/// Find benchmark by name.
fn find_benchmark(name: &str) -> Option<&'static BenchmarkInfo> {
    BENCHMARKS.iter().find(|b| b.name == name)
}

/// List available benchmarks.
fn bench_list() {
    println!("Available benchmarks:");
    println!();
    for b in BENCHMARKS {
        let mut markers = Vec::new();
        match b.source {
            BenchmarkSource::Rust { .. } => markers.push("rust"),
            BenchmarkSource::Prebuilt => markers.push("prebuilt"),
        }
        if b.host_binary.is_some() {
            markers.push("has host");
        }
        let marker_str = if markers.is_empty() {
            String::new()
        } else {
            format!(" [{}]", markers.join(", "))
        };
        println!("  {:<20} {}{}", b.name, b.description, marker_str);
    }
    println!();
    println!("Commands:");
    println!("  rvr bench build [name]     Build ELF from source");
    println!("  rvr bench compile [name]   Compile ELF to native .so");
    println!("  rvr bench run [name]       Run benchmark");
    println!();
    println!("Omit [name] to operate on all benchmarks.");
}

/// Build benchmark ELF from source.
fn bench_build(name: Option<&str>, arch: Option<&str>, no_host: bool) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Determine which benchmarks to build
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                eprintln!("Error: unknown benchmark '{}'", n);
                eprintln!("Run 'rvr bench list' to see available benchmarks");
                return EXIT_FAILURE;
            }
        },
        None => BENCHMARKS.iter().collect(),
    };

    for benchmark in &benchmarks {
        // Determine architectures
        let arch_str = arch.unwrap_or(benchmark.default_archs);

        match benchmark.source {
            BenchmarkSource::Rust { path } => {
                eprintln!("Building {} from {}", benchmark.name, path);

                // Build host binary first (unless --no-host)
                if !no_host && benchmark.host_binary.is_some() {
                    eprintln!("  Building host binary...");
                    let status = Command::new("cargo")
                        .arg("build")
                        .arg("--release")
                        .arg("--manifest-path")
                        .arg(project_dir.join(path).join("Cargo.toml"))
                        .status()
                        .expect("failed to run cargo");

                    if !status.success() {
                        eprintln!("  Host build failed");
                        return EXIT_FAILURE;
                    }
                }

                // Build RISC-V ELFs using rvr build
                let project_path = project_dir.join(path);
                let result = build_rust_project(
                    &project_path,
                    arch_str,
                    None, // Use default output (bin/{arch}/)
                    "nightly",
                    None,
                    true,
                );

                if result != EXIT_SUCCESS {
                    return result;
                }
            }
            BenchmarkSource::Prebuilt => {
                // Check if prebuilt ELFs exist
                let archs: Vec<&str> = arch_str.split(',').map(|s| s.trim()).collect();
                let mut missing = false;

                for a in &archs {
                    let elf_path = project_dir.join("bin").join(a).join(benchmark.name);
                    if !elf_path.exists() {
                        eprintln!(
                            "  Warning: prebuilt ELF not found: {}",
                            elf_path.display()
                        );
                        missing = true;
                    }
                }

                if missing {
                    eprintln!(
                        "  Note: {} uses prebuilt ELFs. Place them in bin/<arch>/{}",
                        benchmark.name, benchmark.name
                    );
                } else {
                    eprintln!("  {} ELFs already present", benchmark.name);
                }
            }
        }
    }

    eprintln!();
    eprintln!("Build complete.");
    EXIT_SUCCESS
}

/// Compile benchmark ELF to native .so.
fn bench_compile(
    name: Option<&str>,
    arch: Option<&str>,
    fast: bool,
    cc: &str,
    linker: Option<&str>,
) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Determine which benchmarks to compile
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                eprintln!("Error: unknown benchmark '{}'", n);
                return EXIT_FAILURE;
            }
        },
        None => BENCHMARKS.iter().collect(),
    };

    let suffix = if fast { "fast" } else { "base" };

    let mut compiler: Compiler = cc.parse().unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(EXIT_FAILURE);
    });
    if let Some(ld) = linker {
        compiler = compiler.with_linker(ld);
    }

    for benchmark in &benchmarks {
        let arch_str = arch.unwrap_or(benchmark.default_archs);
        let archs = match Arch::parse_list(arch_str) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Error: {}", e);
                return EXIT_FAILURE;
            }
        };

        for a in &archs {
            // ELF path: bin/{arch}/{name}
            let elf_path = project_dir.join("bin").join(a.as_str()).join(benchmark.name);

            if !elf_path.exists() {
                eprintln!(
                    "Warning: {} not found, skipping",
                    elf_path.display()
                );
                continue;
            }

            let out_dir = project_dir
                .join("target/benchmarks")
                .join(benchmark.name)
                .join(a.as_str())
                .join(suffix);

            eprintln!(
                "Compiling {} ({}) -> {}",
                benchmark.name,
                a.as_str(),
                out_dir.display()
            );

            let mut options = CompileOptions::new()
                .with_compiler(compiler.clone())
                .with_export_functions(benchmark.uses_exports);
            if fast {
                options = options.with_instret_mode(InstretMode::Off);
            }

            if let Err(e) = rvr::compile_with_options(&elf_path, &out_dir, options) {
                eprintln!("Error compiling {}: {}", a, e);
                return EXIT_FAILURE;
            }
        }
    }

    eprintln!("Compile complete.");
    EXIT_SUCCESS
}

/// Run compiled benchmark.
fn bench_run(
    name: Option<&str>,
    arch: Option<&str>,
    runs: usize,
    fast: bool,
    compare_host: bool,
) -> i32 {
    let project_dir = std::env::current_dir().expect("failed to get current directory");

    // Determine which benchmarks to run
    let benchmarks: Vec<&BenchmarkInfo> = match name {
        Some(n) => match find_benchmark(n) {
            Some(b) => vec![b],
            None => {
                eprintln!("Error: unknown benchmark '{}'", n);
                return EXIT_FAILURE;
            }
        },
        None => BENCHMARKS.iter().collect(),
    };

    let suffix = if fast { "fast" } else { "base" };

    for benchmark in &benchmarks {
        let arch_str = arch.unwrap_or(benchmark.default_archs);
        let archs = match Arch::parse_list(arch_str) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Error: {}", e);
                return EXIT_FAILURE;
            }
        };

        println!("## {}", benchmark.name);
        println!();
        println!("*{} | runs: {}*", benchmark.description, runs);
        println!();
        println!(
            "| {:<8} | {:>12} | {:>10} | {:>12} |",
            "Backend", "Instructions", "Time", "Speed"
        );
        println!("|----------|--------------|------------|--------------|");

        // Run host baseline if requested and available
        if compare_host {
            if let Some(host_path) = benchmark.host_binary {
                let host_bin = project_dir.join(host_path);
                if host_bin.exists() {
                    match bench::run_host(&host_bin, runs) {
                        Ok(result) => {
                            let time_str = result
                                .time_secs
                                .map(|t| format!("{:.3}s", t))
                                .unwrap_or_else(|| "-".to_string());
                            println!(
                                "| {:<8} | {:>12} | {:>10} | {:>12} |",
                                "native", "-", time_str, "-"
                            );
                        }
                        Err(e) => {
                            println!(
                                "| {:<8} | {:>12} | {:>10} | {:>12} |",
                                "native", "-", "-", format!("err: {}", e)
                            );
                        }
                    }
                }
            }
        }

        // Run each architecture
        for a in &archs {
            // ELF path: bin/{arch}/{name}
            let elf_path = project_dir.join("bin").join(a.as_str()).join(benchmark.name);
            let out_dir = project_dir
                .join("target/benchmarks")
                .join(benchmark.name)
                .join(a.as_str())
                .join(suffix);

            let backend_name = format!("rvr-{}", a.as_str());

            if !out_dir.exists() {
                println!(
                    "| {:<8} | {:>12} | {:>10} | {:>12} |",
                    backend_name, "-", "-", "not compiled"
                );
                continue;
            }

            match bench::run_bench_auto(&out_dir, &elf_path, runs) {
                Ok((result, _mode)) => {
                    println!(
                        "| {:<8} | {:>12} | {:>10.3}s | {:>12} |",
                        backend_name,
                        bench::format_num(result.result.instret),
                        result.result.time_secs,
                        bench::format_speed(result.result.mips),
                    );
                }
                Err(e) => {
                    println!(
                        "| {:<8} | {:>12} | {:>10} | {:>12} |",
                        backend_name, "-", "-", format!("err: {}", e)
                    );
                }
            }
        }

        println!();
    }

    EXIT_SUCCESS
}
