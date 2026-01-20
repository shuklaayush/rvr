//! RVR CLI - RISC-V Recompiler

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use rvr::{CompileOptions, InstretMode};

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

        /// Number of parallel compile jobs (0 = auto)
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,
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
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            addr_check,
            tohost,
            instret,
            jobs: _,
        } => {
            println!("Compiling {} to {}", input.display(), output.display());
            let options = CompileOptions::new()
                .with_addr_check(addr_check)
                .with_tohost(tohost)
                .with_instret_mode(instret.into());
            match rvr::compile_with_options(&input, &output, options) {
                Ok(path) => println!("Output: {}", path.display()),
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
        } => {
            println!("Lifting {} to {}", input.display(), output.display());
            let options = CompileOptions::new()
                .with_addr_check(addr_check)
                .with_tohost(tohost)
                .with_instret_mode(instret.into());
            match rvr::lift_to_c_with_options(&input, &output, options) {
                Ok(path) => println!("Output: {}", path.display()),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
