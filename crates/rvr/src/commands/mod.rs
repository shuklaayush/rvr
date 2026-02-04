//! Command implementations.
//!
//! Each submodule handles a specific CLI command or group of commands.

mod build;
mod compile;
mod dev;
mod run;

use crate::cli::{Cli, Commands, DevCommands, OutputFormat};

/// Dispatch CLI command to the appropriate handler.
pub fn run_command(cli: &Cli) -> i32 {
    match &cli.command {
        Commands::Compile { .. } => handle_compile(cli),
        Commands::Lift { .. } => handle_lift(cli),
        Commands::Run { .. } => handle_run(cli),
        Commands::Build { .. } => handle_build(cli),
        Commands::Dev { command } => handle_dev(command),
    }
}

fn handle_compile(cli: &Cli) -> i32 {
    let Commands::Compile {
        input,
        output,
        backend,
        analysis,
        address_mode,
        htif,
        instret,
        syscalls,
        perf,
        no_superblock,
        jobs,
        cc,
        linker,
        fixed_addresses,
        tracer,
    } = &cli.command
    else {
        unreachable!("compile command variant mismatch");
    };

    compile::cmd_compile(
        input,
        output,
        *backend,
        *analysis,
        *address_mode,
        *htif,
        *instret,
        *syscalls,
        *perf,
        *no_superblock,
        *jobs,
        cc.as_deref(),
        linker.as_deref(),
        fixed_addresses.as_deref(),
        tracer,
    )
}

fn handle_lift(cli: &Cli) -> i32 {
    let Commands::Lift {
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
    } = &cli.command
    else {
        unreachable!("lift command variant mismatch");
    };

    compile::cmd_lift(
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
    )
}

fn handle_run(cli: &Cli) -> i32 {
    let Commands::Run {
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
    } = &cli.command
    else {
        unreachable!("run command variant mismatch");
    };

    run::cmd_run(
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
    )
}

fn handle_build(cli: &Cli) -> i32 {
    let Commands::Build {
        path,
        target,
        output,
        name,
        toolchain,
        features,
        release,
        verbose,
    } = &cli.command
    else {
        unreachable!("build command variant mismatch");
    };

    build::build_rust_project(
        path,
        target,
        output.as_ref(),
        name.as_deref(),
        toolchain,
        features.as_deref(),
        *release,
        *verbose,
        false,
    )
}

fn handle_dev(command: &DevCommands) -> i32 {
    match command {
        DevCommands::Trace {
            elf,
            output,
            cc,
            stop_on_first,
            isa,
            timeout,
        } => dev::trace_compare(
            elf,
            output.clone(),
            cc,
            isa.clone(),
            *timeout,
            *stop_on_first,
        ),
        DevCommands::Diff {
            mode,
            elf,
            ref_backend,
            test_backend,
            granularity,
            max_instrs,
            output,
            ref_dir,
            test_dir,
            cc,
            isa,
            strict_mem,
        } => dev::diff_compare(dev::DiffCompareArgs {
            mode: *mode,
            ref_backend: *ref_backend,
            test_backend: *test_backend,
            elf_path: elf,
            granularity_arg: *granularity,
            max_instrs: *max_instrs,
            output_dir: output.clone(),
            ref_dir: ref_dir.clone(),
            test_dir: test_dir.clone(),
            cc,
            isa: isa.clone(),
            strict_mem: *strict_mem,
        }),
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
            println!("Runs: {runs}");
            println!("Exit code: {}", first.exit_code);
            println!("Instructions: {}", first.instret);
            println!("Avg time: {avg_time:.6}s");
            println!("Avg speed: {}", rvr::bench::format_speed(avg_mips));
        }
        OutputFormat::Raw => {
            println!("instret: {}", first.instret);
            println!("time: {avg_time:.6}");
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
