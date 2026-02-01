//! CLI definitions and argument types.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use rvr::{AddressMode, FixedAddressConfig, InstretMode, SyscallMode};
use rvr_emit::c::{PassedVar, TracerConfig, TracerKind};

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

    /// Enable verbose output (sets RUST_LOG=debug)
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress output (only show errors)
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub silent: bool,

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

        /// Code generation backend
        #[arg(long, value_enum, default_value = "c")]
        backend: BackendArg,

        /// Analysis mode (cfg = full CFG with block merging, linear = no merging)
        #[arg(long, value_enum, default_value = "cfg")]
        analysis: AnalysisModeArg,

        /// Address translation mode
        #[arg(long, value_enum, default_value = "wrap")]
        address_mode: AddressModeArg,

        /// Enable HTIF (Host-Target Interface) for riscv-tests
        #[arg(long)]
        htif: bool,

        /// Instruction retirement mode
        #[arg(long, value_enum, default_value = "count")]
        instret: InstretModeArg,

        /// Syscall handling mode
        #[arg(long, value_enum, default_value = "baremetal")]
        syscalls: SyscallModeArg,

        /// Perf mode (disable instret and CSR reads)
        #[arg(long)]
        perf: bool,

        /// Number of parallel compile jobs (0 = auto)
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,

        /// C compiler command (e.g., clang, clang-20, gcc-13)
        #[arg(long)]
        cc: Option<String>,

        /// Linker to use (e.g., lld, lld-20). Auto-derived from --cc if not specified.
        #[arg(long)]
        linker: Option<String>,

        /// Use fixed addresses for state and memory (experimental).
        /// Format: "STATE_ADDR,MEMORY_ADDR" (hex) or "default" for default addresses.
        /// Requires runtime to map memory at these addresses.
        #[arg(long, value_name = "ADDRS")]
        fixed_addresses: Option<String>,

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

        /// Code generation backend
        #[arg(long, value_enum, default_value = "c")]
        backend: BackendArg,

        /// Analysis mode (cfg = full CFG with block merging, linear = no merging)
        #[arg(long, value_enum, default_value = "cfg")]
        analysis: AnalysisModeArg,

        /// Address translation mode
        #[arg(long, value_enum, default_value = "wrap")]
        address_mode: AddressModeArg,

        /// Enable HTIF (Host-Target Interface) for riscv-tests
        #[arg(long)]
        htif: bool,

        /// Emit #line directives with source locations (requires debug info in ELF)
        #[arg(long, default_value = "true")]
        line_info: bool,

        /// Instruction retirement mode
        #[arg(long, value_enum, default_value = "count")]
        instret: InstretModeArg,

        /// Syscall handling mode
        #[arg(long, value_enum, default_value = "baremetal")]
        syscalls: SyscallModeArg,

        /// Perf mode (disable instret and CSR reads)
        #[arg(long)]
        perf: bool,

        /// Use fixed addresses for state and memory (experimental).
        /// Format: "STATE_ADDR,MEMORY_ADDR" (hex) or "default" for default addresses.
        /// Requires runtime to map memory at these addresses.
        #[arg(long, value_name = "ADDRS")]
        fixed_addresses: Option<String>,

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

        /// Memory size as power of 2 (e.g., 30 = 1 GiB, 32 = 4 GiB)
        #[arg(long, default_value = "32")]
        memory_bits: u8,

        /// Maximum instructions to execute before stopping (requires --instret suspend at compile time)
        #[arg(long)]
        max_insns: Option<u64>,

        /// Call a function by name instead of running from entry point (requires --export-functions at compile time)
        #[arg(long)]
        call: Option<String>,

        /// Start GDB server on specified address (e.g., :1234 or 127.0.0.1:1234)
        #[arg(long)]
        gdb: Option<String>,

        /// Load state from file before execution
        #[arg(long)]
        load_state: Option<PathBuf>,

        /// Save state to file after execution
        #[arg(long)]
        save_state: Option<PathBuf>,

        /// Interactive debugger mode (requires --instret suspend at compile time)
        #[arg(long, conflicts_with_all = ["gdb", "runs"])]
        debug: bool,
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

/// Shared compilation options for benchmark commands.
#[derive(clap::Args, Clone, Debug)]
pub struct BenchCompileArgs {
    /// C compiler command
    #[arg(long, default_value = "clang")]
    pub cc: String,

    /// Linker to use
    #[arg(long)]
    pub linker: Option<String>,

    /// Code generation backend
    #[arg(long, value_enum, default_value = "c")]
    pub backend: BackendArg,

    /// Address translation mode
    #[arg(long, value_enum, default_value = "wrap")]
    pub address_mode: AddressModeArg,

    /// Enable instruction counting (use --instret=false to disable)
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub instret: bool,

    /// Perf mode (disable instret and CSR reads)
    #[arg(long)]
    pub perf: bool,

    /// Use fixed addresses for state and memory (experimental)
    #[arg(long)]
    pub fixed_addresses: bool,
}

#[derive(Subcommand)]
pub enum BenchCommands {
    /// List available benchmarks
    List,
    /// Generate benchmark report with system info
    Report {
        /// Output file (default: BENCHMARKS.md)
        #[arg(short, long, default_value = "BENCHMARKS.md")]
        output: PathBuf,

        /// Number of runs for averaging
        #[arg(short, long, default_value = "3")]
        runs: usize,

        /// Skip libriscv comparison
        #[arg(long)]
        no_libriscv: bool,

        /// Skip host comparison
        #[arg(long)]
        no_host: bool,

        /// Force rebuild of all ELFs and recompilation
        #[arg(long)]
        force: bool,

        #[command(flatten)]
        compile: BenchCompileArgs,
    },
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

        #[command(flatten)]
        compile: BenchCompileArgs,
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

        /// Include host binary comparison (if available)
        #[arg(long)]
        compare_host: bool,

        /// Include libriscv emulator comparison
        #[arg(long)]
        compare_libriscv: bool,

        /// Force recompilation (delete and rebuild .so files)
        #[arg(long)]
        force: bool,

        #[command(flatten)]
        compile: BenchCompileArgs,
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

        /// C compiler command
        #[arg(long, default_value = "clang")]
        cc: String,

        /// Linker to use
        #[arg(long)]
        linker: Option<String>,

        /// Code generation backend
        #[arg(long, value_enum, default_value = "c")]
        backend: BackendArg,
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

/// Address translation mode for memory accesses.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum AddressModeArg {
    /// Assume valid + passthrough (guard pages catch OOB)
    Unchecked,
    /// Mask to memory size (matches sv39)
    #[default]
    Wrap,
    /// Bounds check + trap (explicit errors)
    Bounds,
}

impl From<AddressModeArg> for AddressMode {
    fn from(arg: AddressModeArg) -> Self {
        match arg {
            AddressModeArg::Unchecked => AddressMode::Unchecked,
            AddressModeArg::Wrap => AddressMode::Wrap,
            AddressModeArg::Bounds => AddressMode::Bounds,
        }
    }
}

/// Code generation backend.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum BackendArg {
    /// Emit C code, compile with clang/gcc (default)
    #[default]
    C,
    /// Emit x86-64 assembly, compile with gcc/as (experimental)
    X86,
    /// Emit ARM64 assembly, compile with gcc/as (experimental)
    Arm64,
}

impl From<BackendArg> for rvr_emit::Backend {
    fn from(arg: BackendArg) -> Self {
        match arg {
            BackendArg::C => rvr_emit::Backend::C,
            BackendArg::X86 => rvr_emit::Backend::X86Asm,
            BackendArg::Arm64 => rvr_emit::Backend::ARM64Asm,
        }
    }
}

/// Analysis mode for the compilation pipeline.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum AnalysisModeArg {
    /// Full CFG analysis with block merging and optimizations (default)
    #[default]
    Cfg,
    /// Linear scan: decode instructions without block merging (faster)
    Linear,
}

impl From<AnalysisModeArg> for rvr_emit::AnalysisMode {
    fn from(arg: AnalysisModeArg) -> Self {
        match arg {
            AnalysisModeArg::Cfg => rvr_emit::AnalysisMode::FullCfg,
            AnalysisModeArg::Linear => rvr_emit::AnalysisMode::Basic,
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

/// Parse fixed addresses from CLI argument.
///
/// Accepts:
/// - "default" - use default addresses (64GB, 128GB)
/// - "STATE_ADDR,MEMORY_ADDR" - hex addresses (e.g., "0x1000000000,0x2000000000")
pub fn parse_fixed_addresses(arg: &str) -> Result<FixedAddressConfig, String> {
    let arg = arg.trim();

    if arg.eq_ignore_ascii_case("default") {
        return Ok(FixedAddressConfig::default());
    }

    let parts: Vec<&str> = arg.split(',').collect();
    if parts.len() != 2 {
        return Err("expected format: STATE_ADDR,MEMORY_ADDR (hex) or 'default'".to_string());
    }

    let parse_hex = |s: &str| -> Result<u64, String> {
        let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
        u64::from_str_radix(s, 16).map_err(|e| format!("invalid hex address: {}", e))
    };

    let state_addr = parse_hex(parts[0])?;
    let memory_addr = parse_hex(parts[1])?;

    Ok(FixedAddressConfig {
        state_addr,
        memory_addr,
    })
}
