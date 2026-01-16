//! RVR CLI - RISC-V Recompiler

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rvr")]
#[command(about = "RISC-V Recompiler - compiles ELF to native code via C")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

        /// Number of parallel jobs
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
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            addr_check: _,
            jobs: _,
        } => {
            println!("Compiling {} to {}", input.display(), output.display());
            match rvr::compile(&input, &output) {
                Ok(path) => println!("Output: {}", path.display()),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Lift { input, output } => {
            println!("Lifting {} to {}", input.display(), output.display());
            // TODO: Implement
            eprintln!("Not yet implemented");
            std::process::exit(1);
        }
    }
}
