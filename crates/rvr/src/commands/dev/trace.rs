use std::path::{Path, PathBuf};

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use rvr::test_support::trace;

/// Compare instruction traces between rvr and Spike.
pub fn trace_compare(
    elf_path: &Path,
    output_dir: Option<PathBuf>,
    cc: &str,
    isa: Option<String>,
    timeout: u64,
    stop_on_first: bool,
) -> i32 {
    let test_name = elf_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if should_skip_trace(test_name) {
        return EXIT_SUCCESS;
    }

    let spike_path = match resolve_spike_path() {
        Ok(path) => path,
        Err(code) => return code,
    };

    let isa = match resolve_isa(elf_path, isa) {
        Ok(i) => i,
        Err(code) => return code,
    };

    let entry_point = match trace::elf_entry_point(elf_path) {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("Error reading ELF entry point: {e}");
            return EXIT_FAILURE;
        }
    };

    let output_dir = prepare_output_dir(output_dir);
    log_trace_setup(elf_path, &isa, entry_point, &spike_path, &output_dir);

    if let Err(code) = compile_rvr_trace(elf_path, &output_dir, cc) {
        return code;
    }

    let spike_trace_path =
        match run_spike(&spike_path, elf_path, &isa, &output_dir, timeout, test_name) {
            Ok(path) => path,
            Err(code) => return code,
        };

    let rvr_trace_path = match run_rvr(elf_path, &output_dir, timeout) {
        Ok(path) => path,
        Err(code) => return code,
    };

    compare_and_report(
        &spike_trace_path,
        &rvr_trace_path,
        entry_point,
        stop_on_first,
        &output_dir,
    )
}

fn should_skip_trace(test_name: &str) -> bool {
    if should_skip(test_name) {
        eprintln!("SKIP: {test_name} (not compatible with static recompilation)");
        return true;
    }
    if test_name.contains("lrsc") {
        eprintln!("SKIP: {test_name} (LR/SC trace is nondeterministic)");
        return true;
    }
    false
}

fn resolve_spike_path() -> Result<PathBuf, i32> {
    let Some(spike_path) = trace::find_spike() else {
        eprintln!("Error: Spike not found in PATH");
        eprintln!("Install from https://github.com/riscv-software-src/riscv-isa-sim");
        return Err(EXIT_FAILURE);
    };
    Ok(spike_path)
}

fn resolve_isa(elf_path: &Path, isa: Option<String>) -> Result<String, i32> {
    let isa = match isa {
        Some(i) => i,
        None => match trace::elf_to_isa(elf_path) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("Error detecting ISA: {e}");
                return Err(EXIT_FAILURE);
            }
        },
    };
    let test_name = elf_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    Ok(trace::isa_from_test_name(test_name, &isa))
}

fn prepare_output_dir(output_dir: Option<PathBuf>) -> PathBuf {
    output_dir.unwrap_or_else(|| {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        temp.keep()
    })
}

fn log_trace_setup(
    elf_path: &Path,
    isa: &str,
    entry_point: u64,
    spike_path: &Path,
    output_dir: &Path,
) {
    eprintln!("ELF: {}", elf_path.display());
    eprintln!("ISA: {isa}");
    eprintln!("Entry: 0x{entry_point:x}");
    eprintln!("Spike: {}", spike_path.display());
    eprintln!("Output: {}", output_dir.display());
    eprintln!();
}

fn compile_rvr_trace(elf_path: &Path, output_dir: &Path, cc: &str) -> Result<(), i32> {
    use std::process::Command;

    eprintln!("Step 1: Compiling with rvr (spike tracer)...");
    let compile_status = Command::new("./target/release/rvr")
        .arg("compile")
        .arg(elf_path)
        .arg("-o")
        .arg(output_dir)
        .arg("--tracer")
        .arg("spike")
        .arg("--cc")
        .arg(cc)
        .status();

    match compile_status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            eprintln!(
                "Error: rvr compile failed with exit code {:?}",
                status.code()
            );
            Err(EXIT_FAILURE)
        }
        Err(e) => {
            eprintln!("Error: failed to run rvr compile: {e}");
            Err(EXIT_FAILURE)
        }
    }
}

fn run_spike(
    spike_path: &Path,
    elf_path: &Path,
    isa: &str,
    output_dir: &Path,
    timeout: u64,
    test_name: &str,
) -> Result<PathBuf, i32> {
    use std::process::Command;
    use std::time::Duration;

    eprintln!("Step 2: Running Spike...");
    let spike_trace_path = output_dir.join("spike_trace.log");
    let spike_timeout = Duration::from_secs(timeout);
    let mut spike_cmd = Command::new(spike_path);
    spike_cmd
        .arg(format!("--isa={isa}"))
        .arg("--log-commits")
        .arg(format!("--log={}", spike_trace_path.display()))
        .arg(elf_path);
    let spike_status = trace::run_command_with_timeout(&mut spike_cmd, spike_timeout);

    match spike_status {
        Ok(status) if status.success() => Ok(spike_trace_path),
        Ok(status) => {
            if test_name.contains("ma_data") {
                eprintln!("SKIP: Spike reference failed for {test_name}");
                return Err(EXIT_SUCCESS);
            }
            eprintln!("Error: Spike failed with exit code {:?}", status.code());
            Err(EXIT_FAILURE)
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            eprintln!("Error: Spike timed out after {timeout}s");
            Err(EXIT_FAILURE)
        }
        Err(e) => {
            eprintln!("Error: failed to run Spike: {e}");
            Err(EXIT_FAILURE)
        }
    }
}

fn run_rvr(elf_path: &Path, output_dir: &Path, timeout: u64) -> Result<PathBuf, i32> {
    use std::process::Command;
    use std::time::Duration;

    eprintln!("Step 3: Running rvr...");
    let rvr_trace_path = output_dir.join("rvr_trace.log");
    unsafe { std::env::set_var("RVR_TRACE_FILE", &rvr_trace_path) };

    let mut rvr_cmd = Command::new("./target/release/rvr");
    rvr_cmd.arg("run").arg(output_dir).arg(elf_path);
    let rvr_status = trace::run_command_with_timeout(&mut rvr_cmd, Duration::from_secs(timeout));

    unsafe { std::env::remove_var("RVR_TRACE_FILE") };

    match rvr_status {
        Ok(status) if status.success() => Ok(rvr_trace_path),
        Ok(status) => {
            eprintln!("Error: rvr run failed with exit code {:?}", status.code());
            Err(EXIT_FAILURE)
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            eprintln!("Error: rvr run timed out after {timeout}s");
            Err(EXIT_FAILURE)
        }
        Err(e) => {
            eprintln!("Error: failed to run rvr: {e}");
            Err(EXIT_FAILURE)
        }
    }
}

fn compare_and_report(
    spike_trace_path: &Path,
    rvr_trace_path: &Path,
    entry_point: u64,
    stop_on_first: bool,
    output_dir: &Path,
) -> i32 {
    eprintln!("Step 4: Comparing traces...");

    let spike_trace = match trace::parse_trace_file(spike_trace_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing Spike trace: {e}");
            return EXIT_FAILURE;
        }
    };

    let rvr_trace = match trace::parse_trace_file(rvr_trace_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing rvr trace: {e}");
            return EXIT_FAILURE;
        }
    };

    eprintln!("Spike trace: {} entries", spike_trace.len());
    eprintln!("rvr trace: {} entries", rvr_trace.len());

    let (spike_aligned, rvr_aligned) =
        trace::align_traces_at(&spike_trace, &rvr_trace, entry_point);
    eprintln!(
        "After alignment: Spike={}, rvr={}",
        spike_aligned.len(),
        rvr_aligned.len()
    );

    let config = trace::CompareConfig {
        entry_point,
        strict_reg_writes: true,
        strict_mem_access: false,
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
            eprintln!("  x{rd} = 0x{val:016x}");
        }
        if let Some(addr) = div.expected.mem_addr {
            eprintln!("  mem 0x{addr:016x}");
        }
        eprintln!();
        eprintln!("Actual (rvr):");
        eprintln!("  PC: 0x{:016x}", div.actual.pc);
        eprintln!("  Opcode: 0x{:08x}", div.actual.opcode);
        if let (Some(rd), Some(val)) = (div.actual.rd, div.actual.rd_value) {
            eprintln!("  x{rd} = 0x{val:016x}");
        }
        if let Some(addr) = div.actual.mem_addr {
            eprintln!("  mem 0x{addr:016x}");
        }
        eprintln!();
        eprintln!("Output: {}", output_dir.display());
        EXIT_FAILURE
    } else {
        eprintln!("PASS: {} instructions matched", result.matched);
        EXIT_SUCCESS
    }
}

fn should_skip(name: &str) -> bool {
    matches!(name, "rv32ui-p-fence_i" | "rv64ui-p-fence_i")
}
