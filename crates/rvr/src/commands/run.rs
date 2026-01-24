//! Run command.

use std::path::PathBuf;

use tracing::{error, warn};

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS, OutputFormat};
use crate::commands::{print_multi_result, print_single_result};

/// Handle the `run` command.
pub fn cmd_run(
    lib_dir: &PathBuf,
    elf_path: &PathBuf,
    format: OutputFormat,
    runs: usize,
    memory_bits: u8,
    max_insns: Option<u64>,
    gdb_addr: Option<&str>,
) -> i32 {
    let memory_size = 1usize << memory_bits;
    let mut runner = match rvr::Runner::load_with_memory(lib_dir, elf_path, memory_size) {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, path = %lib_dir.display(), "failed to load library");
            return EXIT_FAILURE;
        }
    };

    // Set up instruction limit if specified
    if let Some(limit) = max_insns {
        if runner.supports_suspend() {
            runner.set_target_instret(limit);
        } else {
            warn!("--max-insns requires library compiled with --instret suspend");
            return EXIT_FAILURE;
        }
    }

    // If --gdb is specified, start GDB server instead of running normally
    if let Some(addr) = gdb_addr {
        return cmd_run_gdb(runner, addr);
    }

    // Normal execution
    if runs <= 1 {
        match runner.run() {
            Ok(result) => {
                print_single_result(format, &result);
                result.exit_code as i32
            }
            Err(e) => {
                error!(error = %e, "execution failed");
                EXIT_FAILURE
            }
        }
    } else {
        match runner.run_multiple(runs) {
            Ok(results) => {
                let avg_time: f64 = results.iter().map(|r| r.time_secs).sum::<f64>() / runs as f64;
                let avg_mips: f64 = results.iter().map(|r| r.mips).sum::<f64>() / runs as f64;
                let first = &results[0];

                print_multi_result(format, runs, first, avg_time, avg_mips);
                first.exit_code as i32
            }
            Err(e) => {
                error!(error = %e, "execution failed");
                EXIT_FAILURE
            }
        }
    }
}

/// Run with GDB server.
fn cmd_run_gdb(runner: rvr::Runner, addr: &str) -> i32 {
    use rvr::gdb::GdbServer;

    let server = GdbServer::new(runner);
    match server.run(addr) {
        Ok(()) => EXIT_SUCCESS,
        Err(e) => {
            error!(error = %e, "GDB server error");
            EXIT_FAILURE
        }
    }
}
