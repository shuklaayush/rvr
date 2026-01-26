//! Run command.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use tracing::{error, info, warn};

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS, OutputFormat};
use crate::commands::{print_multi_result, print_single_result};

/// Handle the `run` command.
#[allow(clippy::too_many_arguments)]
pub fn cmd_run(
    lib_dir: &PathBuf,
    elf_path: &PathBuf,
    format: OutputFormat,
    runs: usize,
    memory_bits: u8,
    max_insns: Option<u64>,
    call_func: Option<&str>,
    gdb_addr: Option<&str>,
    load_state_path: Option<&PathBuf>,
    save_state_path: Option<&PathBuf>,
    debug_mode: bool,
) -> i32 {
    let memory_size = 1usize << memory_bits;
    let mut runner = match rvr::Runner::load_with_memory(lib_dir, elf_path, memory_size) {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, path = %lib_dir.display(), "failed to load library");
            return EXIT_FAILURE;
        }
    };

    // Load state from file if specified
    if let Some(path) = load_state_path {
        match runner.load_state(path) {
            Ok(()) => {
                info!(path = %path.display(), "loaded state");
            }
            Err(e) => {
                error!(error = %e, path = %path.display(), "failed to load state");
                return EXIT_FAILURE;
            }
        }
    }

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

    // If --debug is specified, start interactive debugger
    if debug_mode {
        return cmd_run_debug(runner);
    }

    // If --call is specified, call the function instead of running from entry point
    let exit_code = if let Some(func_name) = call_func {
        if !runner.has_export_functions() {
            warn!("--call requires library compiled with --export-functions");
            return EXIT_FAILURE;
        }
        match runner.call(func_name, &[]) {
            Ok(result) => {
                println!("{}", result);
                EXIT_SUCCESS
            }
            Err(e) => {
                error!(error = %e, "call failed");
                EXIT_FAILURE
            }
        }
    }
    // Normal execution
    else if runs <= 1 {
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
    };

    // Save state to file if specified
    if let Some(path) = save_state_path {
        match runner.save_state(path) {
            Ok(()) => {
                info!(path = %path.display(), "saved state");
            }
            Err(e) => {
                error!(error = %e, path = %path.display(), "failed to save state");
                return EXIT_FAILURE;
            }
        }
    }

    exit_code
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

/// Interactive debugger.
fn cmd_run_debug(mut runner: rvr::Runner) -> i32 {
    if !runner.supports_suspend() {
        warn!("--debug requires library compiled with --instret suspend");
        return EXIT_FAILURE;
    }

    println!("Interactive debugger. Type 'help' for commands.");
    println!("Entry point: 0x{:x}", runner.entry_point());
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Set up for single-stepping by default
    let mut breakpoints: Vec<u64> = Vec::new();

    loop {
        // Show current state
        print_debug_state(&runner);

        // Prompt
        print!("(rvr) ");
        let _ = stdout.flush();

        // Read command
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() || line.is_empty() {
            println!();
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            // Default: step one instruction
            if !execute_step(&mut runner, 1, &breakpoints) {
                break;
            }
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts[0];

        match cmd {
            "help" | "h" | "?" => {
                println!("Commands:");
                println!("  step [N], s [N]    - Execute N instructions (default: 1)");
                println!("  continue, c        - Continue until breakpoint or exit");
                println!("  regs, r            - Show all registers");
                println!("  reg <N>            - Show register N");
                println!("  mem <addr> [len]   - Show memory at address (hex)");
                println!("  break <addr>, b    - Set breakpoint at address (hex)");
                println!("  delete <addr>, d   - Delete breakpoint");
                println!("  list, l            - List breakpoints");
                println!("  pc                 - Show program counter");
                println!("  quit, q            - Exit debugger");
                println!();
            }
            "step" | "s" => {
                let count = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                if !execute_step(&mut runner, count, &breakpoints) {
                    break;
                }
            }
            "continue" | "c" => {
                // Run until breakpoint or exit
                loop {
                    if !execute_step(&mut runner, 1, &breakpoints) {
                        break;
                    }
                    let pc = runner.get_pc();
                    if breakpoints.contains(&pc) {
                        println!("Breakpoint hit at 0x{:x}", pc);
                        break;
                    }
                }
            }
            "regs" | "r" => {
                print_all_registers(&runner);
            }
            "reg" => {
                if let Some(n) = parts.get(1).and_then(|s| s.parse::<usize>().ok()) {
                    if n < runner.num_regs() {
                        println!(
                            "x{} = 0x{:x} ({})",
                            n,
                            runner.get_register(n),
                            runner.get_register(n)
                        );
                    } else {
                        println!("Invalid register number");
                    }
                } else {
                    println!("Usage: reg <N>");
                }
            }
            "mem" | "m" => {
                if let Some(addr_str) = parts.get(1) {
                    let addr = parse_hex(addr_str);
                    let len = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(64);
                    print_memory(&runner, addr, len);
                } else {
                    println!("Usage: mem <addr> [len]");
                }
            }
            "break" | "b" => {
                if let Some(addr_str) = parts.get(1) {
                    let addr = parse_hex(addr_str);
                    if !breakpoints.contains(&addr) {
                        breakpoints.push(addr);
                        println!("Breakpoint set at 0x{:x}", addr);
                    } else {
                        println!("Breakpoint already exists at 0x{:x}", addr);
                    }
                } else {
                    println!("Usage: break <addr>");
                }
            }
            "delete" | "d" => {
                if let Some(addr_str) = parts.get(1) {
                    let addr = parse_hex(addr_str);
                    if let Some(pos) = breakpoints.iter().position(|&a| a == addr) {
                        breakpoints.remove(pos);
                        println!("Breakpoint deleted at 0x{:x}", addr);
                    } else {
                        println!("No breakpoint at 0x{:x}", addr);
                    }
                } else {
                    println!("Usage: delete <addr>");
                }
            }
            "list" | "l" => {
                if breakpoints.is_empty() {
                    println!("No breakpoints set");
                } else {
                    println!("Breakpoints:");
                    for (i, addr) in breakpoints.iter().enumerate() {
                        println!("  {}: 0x{:x}", i, addr);
                    }
                }
            }
            "pc" => {
                println!("PC = 0x{:x}", runner.get_pc());
            }
            "quit" | "q" => {
                break;
            }
            _ => {
                println!("Unknown command: {}. Type 'help' for commands.", cmd);
            }
        }
    }

    runner.exit_code() as i32
}

/// Execute N instructions, returns false if program exited.
fn execute_step(runner: &mut rvr::Runner, count: u64, _breakpoints: &[u64]) -> bool {
    let current_instret = runner.instret();
    runner.set_target_instret(current_instret + count);
    runner.clear_exit();

    match runner.execute_from(runner.get_pc()) {
        Ok(_) => true,
        Err(e) => {
            // Check if it's a normal exit
            if runner.exit_code() == 0 {
                println!("Program exited with code 0");
            } else {
                println!(
                    "Execution stopped: {} (exit code: {})",
                    e,
                    runner.exit_code()
                );
            }
            false
        }
    }
}

/// Print current debug state (PC and next instruction hint).
fn print_debug_state(runner: &rvr::Runner) {
    let pc = runner.get_pc();
    let instret = runner.instret();
    println!("[{:>8}] PC=0x{:08x}", instret, pc);
}

/// Print all registers.
fn print_all_registers(runner: &rvr::Runner) {
    let num_regs = runner.num_regs();
    for i in 0..num_regs {
        let val = runner.get_register(i);
        print!("x{:<2} = 0x{:016x}  ", i, val);
        if (i + 1) % 4 == 0 {
            println!();
        }
    }
    if !num_regs.is_multiple_of(4) {
        println!();
    }
    println!("pc  = 0x{:016x}", runner.get_pc());
}

/// Print memory contents.
fn print_memory(runner: &rvr::Runner, addr: u64, len: usize) {
    let mut buf = vec![0u8; len];
    let read = runner.read_memory(addr, &mut buf);
    if read == 0 {
        println!("Cannot read memory at 0x{:x}", addr);
        return;
    }

    for (i, chunk) in buf[..read].chunks(16).enumerate() {
        print!("0x{:08x}:  ", addr + (i * 16) as u64);
        for byte in chunk {
            print!("{:02x} ", byte);
        }
        // Pad if less than 16 bytes
        for _ in chunk.len()..16 {
            print!("   ");
        }
        print!(" |");
        for byte in chunk {
            let c = *byte as char;
            if c.is_ascii_graphic() || c == ' ' {
                print!("{}", c);
            } else {
                print!(".");
            }
        }
        println!("|");
    }
}

/// Parse hex address (with or without 0x prefix).
fn parse_hex(s: &str) -> u64 {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(s, 16).unwrap_or(0)
}
