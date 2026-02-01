//! Command implementations.
//!
//! Each submodule handles a specific CLI command or group of commands.

mod bench;
mod build;
mod compile;
mod run;
mod test;

use crate::cli::{
    BenchCommands, Cli, Commands, EXIT_SUCCESS, OutputFormat, RiscvTestCommands, TestCommands,
};

/// Dispatch CLI command to the appropriate handler.
pub fn run_command(cli: &Cli) -> i32 {
    match &cli.command {
        Commands::Compile {
            input,
            output,
            backend,
            analysis,
            address_mode,
            htif,
            instret,
            syscalls,
            perf,
            jobs,
            cc,
            linker,
            fixed_addresses,
            tracer,
        } => compile::cmd_compile(
            input,
            output,
            *backend,
            *analysis,
            *address_mode,
            *htif,
            *instret,
            *syscalls,
            *perf,
            *jobs,
            cc.as_deref(),
            linker.as_deref(),
            fixed_addresses.as_deref(),
            tracer,
        ),
        Commands::Lift {
            input,
            output,
            backend,
            analysis,
            address_mode,
            htif,
            line_info,
            instret,
            syscalls,
            perf,
            fixed_addresses,
            tracer,
        } => compile::cmd_lift(
            input,
            output,
            *backend,
            *analysis,
            *address_mode,
            *htif,
            *line_info,
            *instret,
            *syscalls,
            *perf,
            fixed_addresses.as_deref(),
            tracer,
        ),
        Commands::Run {
            lib_dir,
            elf_path,
            format,
            runs,
            memory_bits,
            max_insns,
            call,
            gdb,
            load_state,
            save_state,
            debug,
        } => run::cmd_run(
            lib_dir,
            elf_path,
            *format,
            *runs,
            *memory_bits,
            *max_insns,
            call.as_deref(),
            gdb.as_deref(),
            load_state.as_ref(),
            save_state.as_ref(),
            *debug,
        ),
        Commands::Build {
            path,
            target,
            output,
            name,
            toolchain,
            features,
            release,
            verbose,
        } => build::build_rust_project(
            path,
            target,
            output.as_ref(),
            name.as_deref(),
            toolchain,
            features.as_deref(),
            *release,
            *verbose,
            false, // not quiet
        ),
        Commands::Bench { command } => match command {
            BenchCommands::List => {
                bench::bench_list();
                EXIT_SUCCESS
            }
            BenchCommands::Report {
                output,
                runs,
                no_libriscv,
                no_host,
                force,
                compile,
            } => bench::bench_report(output, *runs, *no_libriscv, *no_host, *force, compile),
            BenchCommands::Build {
                name,
                arch,
                no_host,
            } => bench::bench_build(name.as_deref(), arch.as_deref(), *no_host),
            BenchCommands::Compile {
                name,
                arch,
                compile,
            } => bench::bench_compile(name.as_deref(), arch.as_deref(), compile),
            BenchCommands::Run {
                name,
                arch,
                runs,
                compare_host,
                compare_libriscv,
                force,
                compile,
            } => bench::bench_run(
                name.as_deref(),
                arch.as_deref(),
                *runs,
                *compare_host,
                *compare_libriscv,
                *force,
                compile,
            ),
        },
        Commands::Test { command } => match command {
            TestCommands::Riscv { command } => match command {
                RiscvTestCommands::Build {
                    category,
                    output,
                    toolchain,
                } => test::riscv_tests_build(category, output.clone(), toolchain.clone()),
                RiscvTestCommands::Run {
                    filter,
                    verbose,
                    timeout,
                    cc,
                    linker,
                    backend,
                } => test::riscv_tests_run(
                    filter.clone(),
                    *verbose,
                    *timeout,
                    cc,
                    linker.as_deref(),
                    (*backend).into(),
                ),
            },
        },
    }
}

// ============================================================================
// Output formatting helpers
// ============================================================================

/// Print a single run result.
pub fn print_single_result(format: OutputFormat, result: &rvr::RunResult) {
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

/// Print averaged result from multiple runs.
pub fn print_multi_result(
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
