//! Differential execution command.

use std::path::PathBuf;

use rvr_emit::Backend;

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use rvr::test_support::{diff, trace};

mod compile;
mod pure_c;

use compile::{compile_for_checkpoint, compile_for_diff, compile_for_diff_block};
use pure_c::run_pure_c_comparison;

#[derive(Clone, Copy, Debug)]
enum DiffBackend {
    Spike,
    Backend(Backend),
}

impl DiffBackend {
    fn as_backend(&self) -> Option<Backend> {
        match self {
            DiffBackend::Backend(b) => Some(*b),
            DiffBackend::Spike => None,
        }
    }
}

fn diff_backend_from_arg(arg: DiffBackendArg) -> DiffBackend {
    match arg {
        DiffBackendArg::Spike => DiffBackend::Spike,
        DiffBackendArg::C => DiffBackend::Backend(Backend::C),
        DiffBackendArg::Arm64 => DiffBackend::Backend(Backend::ARM64Asm),
        DiffBackendArg::X86 => DiffBackend::Backend(Backend::X86Asm),
    }
}

fn resolve_diff_backends(
    mode: DiffModeArg,
    ref_backend: Option<DiffBackendArg>,
    test_backend: Option<DiffBackendArg>,
) -> Result<(DiffBackend, DiffBackend), String> {
    let (ref_backend, test_backend) = match (ref_backend, test_backend) {
        (Some(r), Some(t)) => (diff_backend_from_arg(r), diff_backend_from_arg(t)),
        (None, None) => match mode {
            DiffModeArg::SpikeC => (DiffBackend::Spike, DiffBackend::Backend(Backend::C)),
            DiffModeArg::SpikeArm64 => {
                (DiffBackend::Spike, DiffBackend::Backend(Backend::ARM64Asm))
            }
            DiffModeArg::CArm64 => (
                DiffBackend::Backend(Backend::C),
                DiffBackend::Backend(Backend::ARM64Asm),
            ),
        },
        _ => {
            return Err("both --ref and --test must be provided together".to_string());
        }
    };

    if matches!(test_backend, DiffBackend::Spike) {
        return Err("Spike can only be used as reference backend".to_string());
    }

    if matches!(ref_backend, DiffBackend::Spike) && matches!(test_backend, DiffBackend::Spike) {
        return Err("cannot compare Spike against Spike".to_string());
    }

    if let Some(backend) = ref_backend.as_backend()
        && !backend_supports_diff(backend)
    {
        return Err(format!(
            "backend {:?} does not support diff tracing",
            backend
        ));
    }
    if let Some(backend) = test_backend.as_backend()
        && !backend_supports_diff(backend)
    {
        return Err(format!(
            "backend {:?} does not support diff tracing",
            backend
        ));
    }

    Ok((ref_backend, test_backend))
}

fn should_skip_test(name: &str) -> bool {
    matches!(name, "rv32ui-p-fence_i" | "rv64ui-p-fence_i")
}

fn backend_supports_diff(backend: Backend) -> bool {
    matches!(backend, Backend::C | Backend::ARM64Asm | Backend::X86Asm)
}

fn backend_supports_buffered_diff(backend: Backend) -> bool {
    matches!(backend, Backend::C)
}

// ============================================================================
// Differential Execution Command
// ============================================================================

use crate::cli::{DiffBackendArg, DiffGranularityArg, DiffModeArg};

pub struct DiffCompareArgs<'a> {
    pub mode: DiffModeArg,
    pub ref_backend: Option<DiffBackendArg>,
    pub test_backend: Option<DiffBackendArg>,
    pub elf_path: &'a PathBuf,
    pub granularity_arg: DiffGranularityArg,
    pub max_instrs: Option<u64>,
    pub output_dir: Option<PathBuf>,
    pub ref_dir: Option<PathBuf>,
    pub test_dir: Option<PathBuf>,
    pub cc: &'a str,
    pub isa: Option<String>,
    pub strict_mem: bool,
}

/// Run lockstep differential execution between two backends.
pub fn diff_compare(args: DiffCompareArgs<'_>) -> i32 {
    let DiffCompareArgs {
        mode,
        ref_backend,
        test_backend,
        elf_path,
        granularity_arg,
        max_instrs,
        output_dir,
        ref_dir,
        test_dir,
        cc,
        isa,
        strict_mem,
    } = args;
    // Convert granularity argument
    let granularity = match granularity_arg {
        DiffGranularityArg::Instruction => diff::DiffGranularity::Instruction,
        DiffGranularityArg::Block => diff::DiffGranularity::Block,
        DiffGranularityArg::Hybrid => diff::DiffGranularity::Hybrid,
        DiffGranularityArg::Checkpoint => diff::DiffGranularity::Checkpoint,
        DiffGranularityArg::PureC => diff::DiffGranularity::PureC,
    };

    // Check if test should be skipped
    let test_name = elf_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if should_skip_test(test_name) {
        eprintln!(
            "SKIP: {} (not compatible with static recompilation)",
            test_name
        );
        return EXIT_SUCCESS;
    }

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
    let isa = trace::isa_from_test_name(test_name, &isa);

    // Get entry point for alignment
    let entry_point = match trace::elf_entry_point(elf_path) {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("Error reading ELF entry point: {}", e);
            return EXIT_FAILURE;
        }
    };

    eprintln!("ELF: {}", elf_path.display());
    eprintln!("Mode: {:?}", mode);
    eprintln!("Granularity: {:?}", granularity);
    eprintln!("ISA: {}", isa);
    eprintln!("Entry: 0x{:x}", entry_point);
    if let Some(n) = max_instrs {
        eprintln!("Max instructions: {}", n);
    }
    eprintln!();

    // Create output directory
    let output_dir = output_dir.unwrap_or_else(|| {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        temp.keep()
    });

    // Resolve reference/test backends
    let (ref_backend, test_backend) = match resolve_diff_backends(mode, ref_backend, test_backend) {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("Error: {}", msg);
            return EXIT_FAILURE;
        }
    };

    eprintln!("Reference: {:?}", ref_backend);
    eprintln!("Test: {:?}", test_backend);

    // Check Spike is available if needed
    let needs_spike = matches!(ref_backend, DiffBackend::Spike);
    if needs_spike && diff::find_spike().is_none() {
        eprintln!("Error: Spike not found in PATH");
        eprintln!("Install from https://github.com/riscv-software-src/riscv-isa-sim");
        return EXIT_FAILURE;
    }

    let checkpoint_requested = matches!(granularity, diff::DiffGranularity::Checkpoint);
    let block_requested = matches!(
        granularity,
        diff::DiffGranularity::Block | diff::DiffGranularity::Hybrid
    );
    let use_block_comparison = block_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_))
        && backend_supports_buffered_diff(ref_backend.as_backend().unwrap())
        && backend_supports_buffered_diff(test_backend.as_backend().unwrap());
    if block_requested && !use_block_comparison {
        eprintln!(
            "Warning: block/hybrid requested but buffered diff not supported for this pair; falling back to instruction mode."
        );
    }

    // Check if checkpoint mode is possible (requires two compiled backends)
    let use_checkpoint_comparison = checkpoint_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_));
    if checkpoint_requested && !use_checkpoint_comparison {
        eprintln!(
            "Warning: checkpoint mode requires two compiled backends (not Spike); falling back to instruction mode."
        );
    }

    // Check if pure-C mode is possible (requires two compiled backends)
    let pure_c_requested = matches!(granularity, diff::DiffGranularity::PureC);
    let use_pure_c = pure_c_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_));
    if pure_c_requested && !use_pure_c {
        eprintln!(
            "Warning: pure-C mode requires two compiled backends (not Spike); falling back to instruction mode."
        );
    }

    // Determine what to compile based on mode and granularity
    let result = if use_pure_c {
        // Pure-C comparison: generates a standalone C program
        run_pure_c_comparison(
            elf_path,
            &output_dir,
            ref_backend.as_backend().unwrap(),
            test_backend.as_backend().unwrap(),
            cc,
            max_instrs,
        )
    } else if use_block_comparison {
        // Block-level comparison: reference as block executor, test as linear executor
        eprintln!("Using block-level comparison (reference block vs test linear)");

        // Compile C with buffered-diff tracer for block execution
        let block_dir = if let Some(dir) = ref_dir {
            dir
        } else {
            let dir = output_dir.join("block");
            let backend = ref_backend.as_backend().unwrap();
            eprintln!(
                "Compiling block executor ({:?} with buffered-diff)...",
                backend
            );
            if !compile_for_diff_block(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile block executor");
                return EXIT_FAILURE;
            }
            dir
        };

        // Compile ARM64 with diff tracer for linear execution
        let linear_dir = if let Some(dir) = test_dir {
            dir
        } else {
            let dir = output_dir.join("linear");
            let backend = test_backend.as_backend().unwrap();
            eprintln!("Compiling linear executor ({:?} with diff)...", backend);
            if !compile_for_diff(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile linear executor");
                return EXIT_FAILURE;
            }
            dir
        };

        // Create executors
        eprintln!("Starting block-level differential execution...");

        let config = diff::CompareConfig {
            strict_reg_writes: true,
            strict_mem_access: strict_mem,
        };

        let mut block_exec = match diff::BufferedInProcessExecutor::new(&block_dir, elf_path) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Error loading block executor: {}", e);
                return EXIT_FAILURE;
            }
        };

        let mut linear_exec = match diff::InProcessExecutor::new(&linear_dir, elf_path) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Error loading linear executor: {}", e);
                return EXIT_FAILURE;
            }
        };

        diff::compare_block_vs_linear(&mut block_exec, &mut linear_exec, &config, max_instrs)
    } else if use_checkpoint_comparison {
        // Fast checkpoint comparison: compare PC+registers every N instructions
        eprintln!("Using checkpoint comparison (1M instruction intervals)");

        // Compile both backends with suspend mode (no tracer needed)
        let ref_dir = if let Some(dir) = ref_dir {
            dir
        } else {
            let dir = output_dir.join("ref");
            let backend = ref_backend.as_backend().unwrap();
            eprintln!("Compiling reference ({:?} for checkpoint)...", backend);
            if !compile_for_checkpoint(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile reference");
                return EXIT_FAILURE;
            }
            dir
        };

        let test_dir = if let Some(dir) = test_dir {
            dir
        } else {
            let dir = output_dir.join("test");
            let backend = test_backend.as_backend().unwrap();
            eprintln!("Compiling test ({:?} for checkpoint)...", backend);
            if !compile_for_checkpoint(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile test");
                return EXIT_FAILURE;
            }
            dir
        };

        eprintln!("Starting checkpoint comparison...");

        // Load runners
        let mut ref_runner = match rvr::Runner::load(&ref_dir, elf_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error loading reference runner: {}", e);
                return EXIT_FAILURE;
            }
        };
        ref_runner.prepare();
        let entry = ref_runner.entry_point();
        ref_runner.set_pc(entry);

        let mut test_runner = match rvr::Runner::load(&test_dir, elf_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error loading test runner: {}", e);
                return EXIT_FAILURE;
            }
        };
        test_runner.prepare();
        test_runner.set_pc(entry);

        // Checkpoint interval: 1M instructions
        const CHECKPOINT_INTERVAL: u64 = 1_000_000;

        diff::compare_checkpoint(&mut ref_runner, &mut test_runner, CHECKPOINT_INTERVAL, max_instrs)
    } else {
        // Instruction-level comparison (original behavior)
        // Compile reference if needed
        let ref_compiled_dir = if let Some(dir) = ref_dir {
            dir
        } else if let Some(backend) = ref_backend.as_backend() {
            let dir = output_dir.join("ref");
            eprintln!("Compiling reference ({:?})...", backend);
            if !compile_for_diff(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile reference");
                return EXIT_FAILURE;
            }
            dir
        } else {
            PathBuf::new() // Not used for Spike mode
        };

        // Compile test
        let test_compiled_dir = if let Some(dir) = test_dir {
            dir
        } else if let Some(backend) = test_backend.as_backend() {
            let dir = output_dir.join("test");
            eprintln!("Compiling test ({:?})...", backend);
            if !compile_for_diff(elf_path, &dir, backend, cc) {
                eprintln!("Error: Failed to compile test");
                return EXIT_FAILURE;
            }
            dir
        } else {
            unreachable!("test backend always required")
        };

        // Create executors
        eprintln!("Starting differential execution...");

        let config = diff::CompareConfig {
            strict_reg_writes: true,
            strict_mem_access: strict_mem,
        };

        // Run comparison based on mode
        match ref_backend {
            DiffBackend::Spike => {
                // Spike as reference
                let mut spike = match diff::SpikeExecutor::start(elf_path, &isa, entry_point) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error starting Spike: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                let mut test = match diff::InProcessExecutor::new(&test_compiled_dir, elf_path) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Error loading test executor: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                diff::compare_lockstep(&mut spike, &mut test, &config, max_instrs)
            }
            DiffBackend::Backend(_) => {
                let mut reference = match diff::InProcessExecutor::new(&ref_compiled_dir, elf_path)
                {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Error loading reference executor: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                let mut test = match diff::InProcessExecutor::new(&test_compiled_dir, elf_path) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Error loading test executor: {}", e);
                        return EXIT_FAILURE;
                    }
                };

                diff::compare_lockstep(&mut reference, &mut test, &config, max_instrs)
            }
        }
    };

    // Report result
    eprintln!();
    if let Some(div) = &result.divergence {
        eprintln!("DIVERGENCE at instruction {}: {}", div.index, div.kind);
        eprintln!();
        eprintln!("Expected:");
        eprintln!("  PC: 0x{:016x}", div.expected.pc);
        eprintln!("  Opcode: 0x{:08x}", div.expected.opcode);
        if let (Some(rd), Some(val)) = (div.expected.rd, div.expected.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.expected.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Actual:");
        eprintln!("  PC: 0x{:016x}", div.actual.pc);
        eprintln!("  Opcode: 0x{:08x}", div.actual.opcode);
        if let (Some(rd), Some(val)) = (div.actual.rd, div.actual.rd_value) {
            eprintln!("  x{} = 0x{:016x}", rd, val);
        }
        if let Some(addr) = div.actual.mem_addr {
            eprintln!("  mem 0x{:016x}", addr);
        }
        eprintln!();
        eprintln!("Output: {}", output_dir.display());
        EXIT_FAILURE
    } else {
        eprintln!("PASS: {} instructions matched", result.matched);
        EXIT_SUCCESS
    }
}
