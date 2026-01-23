//! CLI definitions and argument types.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use rvr::{InstretMode, SyscallMode};
use rvr_emit::{PassedVar, TracerConfig, TracerKind};

/// Exit code for success.
pub const EXIT_SUCCESS: i32 = 0;
/// Exit code for failure.
pub const EXIT_FAILURE: i32 = 1;

#[derive(Parser)]
#[command(name = "rvr")]
#[command(about = "RISC-V Recompiler - compiles ELF to native code via C")]
#[command(version)]
pub struct Cli {
    /// Show metrics summary after execution
    #[arg(long, global = true)]
    pub metrics: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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

        /// Start GDB server on specified address (e.g., :1234 or 127.0.0.1:1234)
        #[arg(long)]
        gdb: Option<String>,
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

        /// Output binary name (default: crate name from Cargo.toml)
        #[arg(short, long)]
        name: Option<String>,

        /// Rust toolchain to use (default: nightly)
        #[arg(long, default_value = "nightly")]
        toolchain: String,

        /// Additional features to enable
        #[arg(long)]
        features: Option<String>,

        /// Build in release mode (default: true)
        #[arg(long, default_value = "true")]
        release: bool,

        /// Show the exact cargo command being run
        #[arg(short, long)]
        verbose: bool,
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
pub enum BenchCommands {
    /// List available benchmarks
    List,
    /// Build benchmark ELF from source
    Build {
        /// Benchmark name (omit to build all)
        #[arg(value_name = "NAME")]
        name: Option<String>,

        /// Architectures (comma-separated: rv32i,rv64i or "all")
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

        /// Architectures (comma-separated: rv32i,rv64i or "all")
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

        /// Architectures (comma-separated: rv32i,rv64i or "all")
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

        /// Force recompilation (delete and rebuild .so files)
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum TestCommands {
    /// Run riscv-tests suite
    Riscv {
        #[command(subcommand)]
        command: RiscvTestCommands,
    },
}

#[derive(Subcommand)]
pub enum RiscvTestCommands {
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

// ============================================================================
// Argument types with conversions
// ============================================================================

/// Instruction retirement counting mode.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum InstretModeArg {
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
pub enum TracerKindArg {
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
pub enum SyscallModeArg {
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
pub struct TracerArgs {
    /// Tracer kind (built-in).
    #[arg(long, value_enum, default_value = "none")]
    pub tracer: TracerKindArg,

    /// Custom tracer header path (overrides --tracer).
    #[arg(long)]
    pub tracer_header: Option<PathBuf>,

    /// Inline custom tracer header content (overrides --tracer).
    #[arg(long)]
    pub tracer_inline: Option<String>,

    /// Passed vars for the tracer (e.g. ptr:data, index:data_idx).
    #[arg(long = "tracer-pass", value_name = "KIND:NAME", action = clap::ArgAction::Append)]
    pub tracer_pass: Vec<String>,
}

/// Output format for run command.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum OutputFormat {
    /// Human-readable output (default)
    #[default]
    Text,
    /// Raw key-value output (for scripting)
    Raw,
    /// JSON output
    Json,
}

// ============================================================================
// Tracer configuration helpers
// ============================================================================

/// Parse passed vars from CLI arguments.
pub fn parse_passed_vars(items: &[String]) -> Result<Vec<PassedVar>, String> {
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
                ));
            }
        };
        vars.push(var);
    }
    Ok(vars)
}

/// Build tracer configuration from CLI arguments.
pub fn build_tracer_config(args: &TracerArgs) -> Result<TracerConfig, String> {
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
