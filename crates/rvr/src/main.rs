//! RVR CLI - RISC-V Recompiler

mod cli;
mod commands;
mod terminal;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;

use cli::{Cli, Commands};

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

    let exit_code = commands::run_command(&cli);

    // Print metrics summary if enabled
    if let Some(handle) = metrics_handle {
        handle.print_summary();
    }

    std::process::exit(exit_code);
}
