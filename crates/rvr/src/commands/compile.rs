//! Compile and lift commands.

use std::path::Path;

use rvr::{CompileOptions, Compiler};
use tracing::{error, info};

use crate::cli::{
    EXIT_FAILURE, EXIT_SUCCESS, InstretModeArg, SyscallModeArg, TracerArgs, build_tracer_config,
    parse_fixed_addresses,
};

/// Handle the `compile` command.
#[allow(clippy::too_many_arguments)]
pub fn cmd_compile(
    input: &Path,
    output: &Path,
    addr_check: bool,
    htif: bool,
    instret: InstretModeArg,
    syscalls: SyscallModeArg,
    jobs: usize,
    cc: Option<&str>,
    linker: Option<&str>,
    fixed_addresses: Option<&str>,
    tracer: &TracerArgs,
) -> i32 {
    info!(input = %input.display(), output = %output.display(), "compiling");

    let tracer_config = match build_tracer_config(tracer) {
        Ok(config) => config,
        Err(err) => {
            error!(error = %err, "invalid tracer configuration");
            return EXIT_FAILURE;
        }
    };

    let mut options = CompileOptions::new()
        .with_addr_check(addr_check)
        .with_htif(htif)
        .with_instret_mode(instret.into())
        .with_syscall_mode(syscalls.into())
        .with_tracer_config(tracer_config)
        .with_jobs(jobs);

    if let Some(addrs) = fixed_addresses {
        match parse_fixed_addresses(addrs) {
            Ok(config) => {
                info!(
                    state_addr = format!("{:#x}", config.state_addr),
                    memory_addr = format!("{:#x}", config.memory_addr),
                    "using fixed addresses"
                );
                options = options.with_fixed_addresses(config);
            }
            Err(e) => {
                error!(error = %e, "invalid fixed addresses");
                return EXIT_FAILURE;
            }
        }
    }

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

/// Handle the `lift` command.
#[allow(clippy::too_many_arguments)]
pub fn cmd_lift(
    input: &Path,
    output: &Path,
    addr_check: bool,
    htif: bool,
    line_info: bool,
    instret: InstretModeArg,
    syscalls: SyscallModeArg,
    fixed_addresses: Option<&str>,
    tracer: &TracerArgs,
) -> i32 {
    info!(input = %input.display(), output = %output.display(), "lifting");

    let tracer_config = match build_tracer_config(tracer) {
        Ok(config) => config,
        Err(err) => {
            error!(error = %err, "invalid tracer configuration");
            return EXIT_FAILURE;
        }
    };

    let mut options = CompileOptions::new()
        .with_addr_check(addr_check)
        .with_htif(htif)
        .with_line_info(line_info)
        .with_instret_mode(instret.into())
        .with_syscall_mode(syscalls.into())
        .with_tracer_config(tracer_config);

    if let Some(addrs) = fixed_addresses {
        match parse_fixed_addresses(addrs) {
            Ok(config) => {
                info!(
                    state_addr = format!("{:#x}", config.state_addr),
                    memory_addr = format!("{:#x}", config.memory_addr),
                    "using fixed addresses"
                );
                options = options.with_fixed_addresses(config);
            }
            Err(e) => {
                error!(error = %e, "invalid fixed addresses");
                return EXIT_FAILURE;
            }
        }
    }

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
