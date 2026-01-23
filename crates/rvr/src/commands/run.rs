//! Run command.

use std::path::PathBuf;

use tracing::error;

use crate::cli::{OutputFormat, EXIT_FAILURE};
use crate::commands::{print_multi_result, print_single_result};

/// Handle the `run` command.
pub fn cmd_run(lib_dir: &PathBuf, elf_path: &PathBuf, format: OutputFormat, runs: usize) -> i32 {
    let mut runner = match rvr::Runner::load(lib_dir, elf_path) {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, path = %lib_dir.display(), "failed to load library");
            return EXIT_FAILURE;
        }
    };

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
