use std::path::PathBuf;

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use rvr::test_support::trace;

/// Compare instruction traces between rvr and Spike.
pub fn trace_compare(
    elf_path: &PathBuf,
    output_dir: Option<PathBuf>,
    cc: &str,
    isa: Option<String>,
    timeout: u64,
    stop_on_first: bool,
) -> i32 {
    use std::process::Command;
    use std::time::Duration;

    // Check if test should be skipped
    let test_name = elf_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if should_skip(test_name) {
        eprintln!(
            "SKIP: {} (not compatible with static recompilation)",
            test_name
        );
        return EXIT_SUCCESS;
    }
    if test_name.contains("lrsc") {
        eprintln!("SKIP: {} (LR/SC trace is nondeterministic)", test_name);
        return EXIT_SUCCESS;
    }

    // Check Spike is available
    let spike_path = match trace::find_spike() {
        Some(p) => p,
        None => {
            eprintln!("Error: Spike not found in PATH");
            eprintln!("Install from https://github.com/riscv-software-src/riscv-isa-sim");
            return EXIT_FAILURE;
        }
    };

    // Determine ISA
    let isa = match isa {
        Some(i) => i,
        None => match trace::elf_to_isa(elf_path) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("Error detecting ISA: {}", e);
                return EXIT_FAILURE;
            }
        },
    };
    let isa = trace::isa_from_test_name(
        elf_path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
        &isa,
    );

    // Get entry point for alignment
    let entry_point = match trace::elf_entry_point(elf_path) {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("Error reading ELF entry point: {}", e);
            return EXIT_FAILURE;
        }
    };

    eprintln!("ELF: {}", elf_path.display());
    eprintln!("ISA: {}", isa);
    eprintln!("Entry: 0x{:x}", entry_point);
    eprintln!("Spike: {}", spike_path.display());

    // Create output directory
    let output_dir = output_dir.unwrap_or_else(|| {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        temp.keep()
    });

    eprintln!("Output: {}", output_dir.display());
    eprintln!();

    // Step 1: Compile ELF with rvr using spike tracer
    eprintln!("Step 1: Compiling with rvr (spike tracer)...");
    let compile_status = Command::new("./target/release/rvr")
        .arg("compile")
        .arg(elf_path)
        .arg("-o")
        .arg(&output_dir)
        .arg("--tracer")
        .arg("spike")
        .arg("--cc")
        .arg(cc)
        .status();

    match compile_status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!(
                "Error: rvr compile failed with exit code {:?}",
                status.code()
            );
            return EXIT_FAILURE;
        }
        Err(e) => {
            eprintln!("Error: failed to run rvr compile: {}", e);
            return EXIT_FAILURE;
        }
    }

    // Step 2: Run Spike and capture trace
    eprintln!("Step 2: Running Spike...");
    let spike_trace_path = output_dir.join("spike_trace.log");
    let spike_timeout = Duration::from_secs(timeout);
    let mut spike_cmd = Command::new(&spike_path);
    spike_cmd
        .arg(format!("--isa={}", isa))
        .arg("--log-commits")
        .arg(format!("--log={}", spike_trace_path.display()))
        .arg(elf_path);
    let spike_status = trace::run_command_with_timeout(&mut spike_cmd, spike_timeout);

    match spike_status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            if test_name.contains("ma_data") {
                eprintln!("SKIP: Spike reference failed for {}", test_name);
                return EXIT_SUCCESS;
            }
            eprintln!("Error: Spike failed with exit code {:?}", status.code());
            return EXIT_FAILURE;
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            eprintln!("Error: Spike timed out after {}s", timeout);
            return EXIT_FAILURE;
        }
        Err(e) => {
            eprintln!("Error: failed to run Spike: {}", e);
            return EXIT_FAILURE;
        }
    }

    // Step 3: Run rvr and capture trace
    eprintln!("Step 3: Running rvr...");
    let rvr_trace_path = output_dir.join("rvr_trace.log");
    // SAFETY: We're single-threaded at this point and immediately remove the var
    unsafe { std::env::set_var("RVR_TRACE_FILE", &rvr_trace_path) };

    let mut rvr_cmd = Command::new("./target/release/rvr");
    rvr_cmd.arg("run").arg(&output_dir).arg(elf_path);
    let rvr_status = trace::run_command_with_timeout(&mut rvr_cmd, Duration::from_secs(timeout));

    // SAFETY: We're single-threaded and just cleaning up
    unsafe { std::env::remove_var("RVR_TRACE_FILE") };

    match rvr_status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("Error: rvr run failed with exit code {:?}", status.code());
            return EXIT_FAILURE;
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            eprintln!("Error: rvr run timed out after {}s", timeout);
            return EXIT_FAILURE;
        }
        Err(e) => {
            eprintln!("Error: failed to run rvr: {}", e);
            return EXIT_FAILURE;
        }
    }

    // Step 4: Parse and compare traces
    eprintln!("Step 4: Comparing traces...");

    let spike_trace = match trace::parse_trace_file(&spike_trace_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing Spike trace: {}", e);
            return EXIT_FAILURE;
        }
    };

    let rvr_trace = match trace::parse_trace_file(&rvr_trace_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing rvr trace: {}", e);
            return EXIT_FAILURE;
        }
    };

    eprintln!("Spike trace: {} entries", spike_trace.len());
    eprintln!("rvr trace: {} entries", rvr_trace.len());

    // Align traces (skip Spike's startup code)
    let (spike_aligned, rvr_aligned) =
        trace::align_traces_at(&spike_trace, &rvr_trace, entry_point);
    eprintln!(
        "After alignment: Spike={}, rvr={}",
        spike_aligned.len(),
        rvr_aligned.len()
    );

    // Compare with entry point for ECALL handling
    let config = trace::CompareConfig {
        entry_point,
        strict_reg_writes: true,
        strict_mem_access: false, // Spike doesn't always log mem for loads
        stop_on_first,
    };
    let result = trace::compare_traces_with_config(&spike_aligned, &rvr_aligned, &config);

    eprintln!();
    if let Some(div) = &result.divergence {
        eprintln!("DIVERGENCE at instruction {}: {}", div.index, div.kind);
        eprintln!();
        eprintln!("Expected (Spike):");
        eprintln!("  PC: 0x{:016x}", div.expected.pc);
        eprintln!("  Opcode: 0x{:08x}", div.expected.opcode);
        if let (Some(rd), Some(val)) = (div.expected.rd, div.expected.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.expected.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Actual (rvr):");
        eprintln!("  PC: 0x{:016x}", div.actual.pc);
        eprintln!("  Opcode: 0x{:08x}", div.actual.opcode);
        if let (Some(rd), Some(val)) = (div.actual.rd, div.actual.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.actual.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Traces saved to:");
        eprintln!("  Spike: {}", spike_trace_path.display());
        eprintln!("  rvr: {}", rvr_trace_path.display());
        EXIT_FAILURE
    } else {
        eprintln!("PASS: {} instructions matched", result.matched);
        EXIT_SUCCESS
    }
}

fn should_skip(name: &str) -> bool {
    matches!(name, "rv32ui-p-fence_i" | "rv64ui-p-fence_i")
}
