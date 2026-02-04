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

    /// Enable verbose output (sets `RUST_LOG=debug`)
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

        /// Analysis mode (auto = CFG for C, linear for asm)
        #[arg(long, value_enum, default_value = "auto")]
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

        /// Disable superblock formation (keeps blocks at natural boundaries).
        /// Useful for differential testing where dispatch to all block entries is needed.
        #[arg(long)]
        no_superblock: bool,

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
        /// Format: "`STATE_ADDR,MEMORY_ADDR`" (hex) or "default" for default addresses.
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

        /// Analysis mode (auto = CFG for C, linear for asm)
        #[arg(long, value_enum, default_value = "auto")]
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
        /// Format: "`STATE_ADDR,MEMORY_ADDR`" (hex) or "default" for default addresses.
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
    /// Developer utilities
    Dev {
        #[command(subcommand)]
        command: DevCommands,
    },
}

#[derive(Subcommand)]
pub enum DevCommands {
    /// Trace comparison between rvr and Spike (differential testing)
    Trace {
        /// Path to ELF binary
        elf: PathBuf,

        /// Output directory for compiled rvr code (default: temp dir)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// C compiler command
        #[arg(long, default_value = "clang")]
        cc: String,

        /// Stop on first difference
        #[arg(long)]
        stop_on_first: bool,

        /// ISA string for Spike (auto-detected if not specified)
        #[arg(long)]
        isa: Option<String>,

        /// Timeout in seconds
        #[arg(long, default_value = "60")]
        timeout: u64,
    },
    /// Lockstep differential execution between backends
    Diff {
        /// Comparison mode
        #[arg(value_enum)]
        mode: DiffModeArg,

        /// Path to ELF binary
        elf: PathBuf,

        /// Reference backend (overrides mode)
        #[arg(long = "ref", value_enum)]
        ref_backend: Option<DiffBackendArg>,

        /// Test backend (overrides mode)
        #[arg(long = "test", value_enum)]
        test_backend: Option<DiffBackendArg>,

        /// Comparison granularity
        #[arg(short, long, value_enum, default_value = "instruction")]
        granularity: DiffGranularityArg,

        /// Maximum instructions to compare
        #[arg(short = 'n', long)]
        max_instrs: Option<u64>,

        /// Output directory for compiled code (default: temp dir)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Use pre-compiled reference from this directory
        #[arg(long)]
        ref_dir: Option<PathBuf>,

        /// Use pre-compiled test from this directory
        #[arg(long)]
        test_dir: Option<PathBuf>,

        /// C compiler command
        #[arg(long, default_value = "clang")]
        cc: String,

        /// ISA string for Spike (auto-detected if not specified)
        #[arg(long)]
        isa: Option<String>,

        /// Also compare memory values when available
        #[arg(long)]
        strict_mem: bool,
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
    /// Count and suspend at limit (checked at block boundaries)
    Suspend,
    /// Count and suspend at limit (checked after every instruction)
    PerInstruction,
}

impl From<InstretModeArg> for InstretMode {
    fn from(arg: InstretModeArg) -> Self {
        match arg {
            InstretModeArg::Off => Self::Off,
            InstretModeArg::Count => Self::Count,
            InstretModeArg::Suspend => Self::Suspend,
            InstretModeArg::PerInstruction => Self::PerInstruction,
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
    Spike,
    Diff,
    BufferedDiff,
}

impl From<TracerKindArg> for TracerKind {
    fn from(arg: TracerKindArg) -> Self {
        match arg {
            TracerKindArg::None => Self::None,
            TracerKindArg::Preflight => Self::Preflight,
            TracerKindArg::Stats => Self::Stats,
            TracerKindArg::Ffi => Self::Ffi,
            TracerKindArg::Dynamic => Self::Dynamic,
            TracerKindArg::Debug => Self::Debug,
            TracerKindArg::Spike => Self::Spike,
            TracerKindArg::Diff => Self::Diff,
            TracerKindArg::BufferedDiff => Self::BufferedDiff,
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
            SyscallModeArg::Baremetal => Self::BareMetal,
            SyscallModeArg::Linux => Self::Linux,
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
            AddressModeArg::Unchecked => Self::Unchecked,
            AddressModeArg::Wrap => Self::Wrap,
            AddressModeArg::Bounds => Self::Bounds,
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
            BackendArg::C => Self::C,
            BackendArg::X86 => Self::X86Asm,
            BackendArg::Arm64 => Self::ARM64Asm,
        }
    }
}

/// Differential execution mode.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum DiffModeArg {
    /// Spike (reference) vs C backend
    SpikeC,
    /// Spike (reference) vs ARM64 backend
    SpikeArm64,
    /// C backend vs ARM64 backend
    CArm64,
}

/// Differential execution backend.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum DiffBackendArg {
    /// Spike (reference only)
    Spike,
    /// C backend
    C,
    /// ARM64 backend
    Arm64,
    /// x86 backend
    X86,
}

/// Differential execution granularity.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum DiffGranularityArg {
    /// Compare after every instruction
    #[default]
    Instruction,
    /// Compare at block boundaries
    Block,
    /// Compare by block, drill down on divergence
    Hybrid,
    /// Fast checkpoint comparison (compare PC+registers every 1M instructions)
    Checkpoint,
    /// Pure C comparison (generates standalone C program, no Rust FFI)
    PureC,
}

/// Analysis mode for the compilation pipeline.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum AnalysisModeArg {
    /// Auto: CFG for C backend, linear scan for asm backends (default)
    #[default]
    Auto,
    /// Full CFG analysis with block merging and optimizations
    Cfg,
    /// Linear scan: decode instructions without block merging (faster)
    Linear,
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

    /// Passed vars for the tracer (e.g. ptr:data, `index:data_idx`).
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
            return Err(format!("invalid tracer var '{item}', expected KIND:NAME"));
        }
        let var = match kind {
            "ptr" => PassedVar::ptr(name),
            "index" => PassedVar::index(name),
            "value" => PassedVar::value(name),
            _ => {
                return Err(format!(
                    "invalid tracer var kind '{kind}', expected ptr/index/value"
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
/// - "`STATE_ADDR,MEMORY_ADDR`" - hex addresses (e.g., "0x1000000000,0x2000000000")
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
        u64::from_str_radix(s, 16).map_err(|e| format!("invalid hex address: {e}"))
    };

    let state_addr = parse_hex(parts[0])?;
    let memory_addr = parse_hex(parts[1])?;

    Ok(FixedAddressConfig {
        state_addr,
        memory_addr,
    })
}
