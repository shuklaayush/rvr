//! Differential execution command.

mod pure_c;

use std::path::{Path, PathBuf};

use rvr_emit::Backend;

use crate::cli::{EXIT_FAILURE, EXIT_SUCCESS};
use pure_c::run_pure_c_comparison;
use rvr::test_support::{diff, trace};

#[derive(Clone, Copy, Debug)]
enum DiffBackend {
    Spike,
    Backend(Backend),
}

impl DiffBackend {
    const fn as_backend(self) -> Option<Backend> {
        match self {
            Self::Backend(b) => Some(b),
            Self::Spike => None,
        }
    }
}

const fn diff_backend_from_arg(arg: DiffBackendArg) -> DiffBackend {
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
        && !diff::backend_supports_diff(backend)
    {
        return Err(format!("backend {backend:?} does not support diff tracing"));
    }
    if let Some(backend) = test_backend.as_backend()
        && !diff::backend_supports_diff(backend)
    {
        return Err(format!("backend {backend:?} does not support diff tracing"));
    }

    Ok((ref_backend, test_backend))
}

fn should_skip_test(name: &str) -> bool {
    matches!(name, "rv32ui-p-fence_i" | "rv64ui-p-fence_i")
}

const CHECKPOINT_INTERVAL: u64 = 1_000_000;

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

const fn granularity_from_arg(arg: DiffGranularityArg) -> diff::DiffGranularity {
    match arg {
        DiffGranularityArg::Instruction => diff::DiffGranularity::Instruction,
        DiffGranularityArg::Block => diff::DiffGranularity::Block,
        DiffGranularityArg::Hybrid => diff::DiffGranularity::Hybrid,
        DiffGranularityArg::Checkpoint => diff::DiffGranularity::Checkpoint,
        DiffGranularityArg::PureC => diff::DiffGranularity::PureC,
    }
}

fn test_name_from_path(elf_path: &Path) -> &str {
    elf_path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

fn resolve_isa(elf_path: &Path, isa: Option<String>, test_name: &str) -> Result<String, String> {
    let detected = match isa {
        Some(i) => i,
        None => trace::elf_to_isa(elf_path).map_err(|e| format!("Error detecting ISA: {e}"))?,
    };
    Ok(trace::isa_from_test_name(test_name, &detected))
}

fn resolve_entry_point(elf_path: &Path) -> Result<u64, String> {
    trace::elf_entry_point(elf_path).map_err(|e| format!("Error reading ELF entry point: {e}"))
}

fn create_output_dir(output_dir: Option<PathBuf>) -> PathBuf {
    output_dir.unwrap_or_else(|| {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        temp.keep()
    })
}

fn log_header(
    elf_path: &Path,
    mode: DiffModeArg,
    granularity: diff::DiffGranularity,
    isa: &str,
    entry_point: u64,
    max_instrs: Option<u64>,
) {
    eprintln!("ELF: {}", elf_path.display());
    eprintln!("Mode: {mode:?}");
    eprintln!("Granularity: {granularity:?}");
    eprintln!("ISA: {isa}");
    eprintln!("Entry: 0x{entry_point:x}");
    if let Some(n) = max_instrs {
        eprintln!("Max instructions: {n}");
    }
    eprintln!();
}

fn resolve_compiler(cc: &str) -> Result<rvr::Compiler, String> {
    cc.parse()
        .map_err(|e| format!("Error: invalid compiler: {e}"))
}

#[derive(Clone, Copy)]
struct CompareModes {
    use_block_comparison: bool,
    use_checkpoint_comparison: bool,
    use_pure_c: bool,
}

fn determine_compare_modes(
    granularity: diff::DiffGranularity,
    ref_backend: DiffBackend,
    test_backend: DiffBackend,
) -> CompareModes {
    let checkpoint_requested = matches!(granularity, diff::DiffGranularity::Checkpoint);
    let block_requested = matches!(
        granularity,
        diff::DiffGranularity::Block | diff::DiffGranularity::Hybrid
    );
    let use_block_comparison = block_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_))
        && diff::backend_supports_buffered_diff(ref_backend.as_backend().unwrap())
        && diff::backend_supports_buffered_diff(test_backend.as_backend().unwrap());
    if block_requested && !use_block_comparison {
        eprintln!(
            "Warning: block/hybrid requested but buffered diff not supported for this pair; falling back to instruction mode."
        );
    }

    let use_checkpoint_comparison = checkpoint_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_));
    if checkpoint_requested && !use_checkpoint_comparison {
        eprintln!(
            "Warning: checkpoint mode requires two compiled backends (not Spike); falling back to instruction mode."
        );
    }

    let pure_c_requested = matches!(granularity, diff::DiffGranularity::PureC);
    let use_pure_c = pure_c_requested
        && matches!(ref_backend, DiffBackend::Backend(_))
        && matches!(test_backend, DiffBackend::Backend(_));
    if pure_c_requested && !use_pure_c {
        eprintln!(
            "Warning: pure-C mode requires two compiled backends (not Spike); falling back to instruction mode."
        );
    }

    CompareModes {
        use_block_comparison,
        use_checkpoint_comparison,
        use_pure_c,
    }
}

fn ensure_spike_available(ref_backend: DiffBackend) -> Result<(), String> {
    if matches!(ref_backend, DiffBackend::Spike) && diff::find_spike().is_none() {
        let mut message = String::from("Error: Spike not found in PATH\n");
        message.push_str("Install from https://github.com/riscv-software-src/riscv-isa-sim");
        return Err(message);
    }
    Ok(())
}

struct DiffContext<'a> {
    elf_path: &'a Path,
    output_dir: &'a Path,
    compiler: &'a rvr::Compiler,
    ref_backend: DiffBackend,
    test_backend: DiffBackend,
    ref_dir: Option<PathBuf>,
    test_dir: Option<PathBuf>,
    max_instrs: Option<u64>,
    strict_mem: bool,
    isa: &'a str,
    entry_point: u64,
}

fn run_pure_c(ctx: &DiffContext<'_>, cc: &str) -> diff::CompareResult {
    run_pure_c_comparison(
        ctx.elf_path,
        ctx.output_dir,
        ctx.ref_backend.as_backend().unwrap(),
        ctx.test_backend.as_backend().unwrap(),
        ctx.compiler,
        cc,
        ctx.max_instrs,
    )
}

fn run_block_comparison(ctx: &DiffContext<'_>) -> Result<diff::CompareResult, String> {
    eprintln!("Using block-level comparison (reference block vs test linear)");

    let block_dir = if let Some(dir) = ctx.ref_dir.clone() {
        dir
    } else {
        let dir = ctx.output_dir.join("block");
        let backend = ctx.ref_backend.as_backend().unwrap();
        eprintln!("Compiling block executor ({backend:?} with buffered-diff)...");
        diff::compile_for_diff_block(ctx.elf_path, &dir, backend, ctx.compiler)
            .map_err(|err| format!("Error: {err}"))?;
        dir
    };

    let linear_dir = if let Some(dir) = ctx.test_dir.clone() {
        dir
    } else {
        let dir = ctx.output_dir.join("linear");
        let backend = ctx.test_backend.as_backend().unwrap();
        eprintln!("Compiling linear executor ({backend:?} with diff)...");
        diff::compile_for_diff(ctx.elf_path, &dir, backend, ctx.compiler)
            .map_err(|err| format!("Error: {err}"))?;
        dir
    };

    eprintln!("Starting block-level differential execution...");
    let config = diff::CompareConfig {
        strict_reg_writes: true,
        strict_mem_access: ctx.strict_mem,
    };

    let mut block_exec = diff::BufferedInProcessExecutor::new(&block_dir, ctx.elf_path)
        .map_err(|e| format!("Error loading block executor: {e}"))?;
    let mut linear_exec = diff::InProcessExecutor::new(&linear_dir, ctx.elf_path)
        .map_err(|e| format!("Error loading linear executor: {e}"))?;

    Ok(diff::compare_block_vs_linear(
        &mut block_exec,
        &mut linear_exec,
        &config,
        ctx.max_instrs,
    ))
}

fn run_checkpoint_comparison(ctx: &DiffContext<'_>) -> Result<diff::CompareResult, String> {
    eprintln!("Using checkpoint comparison (1M instruction intervals)");

    let ref_dir = if let Some(dir) = ctx.ref_dir.clone() {
        dir
    } else {
        let dir = ctx.output_dir.join("ref");
        let backend = ctx.ref_backend.as_backend().unwrap();
        eprintln!("Compiling reference ({backend:?} for checkpoint)...");
        diff::compile_for_checkpoint(ctx.elf_path, &dir, backend, ctx.compiler)
            .map_err(|err| format!("Error: {err}"))?;
        dir
    };

    let test_dir = if let Some(dir) = ctx.test_dir.clone() {
        dir
    } else {
        let dir = ctx.output_dir.join("test");
        let backend = ctx.test_backend.as_backend().unwrap();
        eprintln!("Compiling test ({backend:?} for checkpoint)...");
        diff::compile_for_checkpoint(ctx.elf_path, &dir, backend, ctx.compiler)
            .map_err(|err| format!("Error: {err}"))?;
        dir
    };

    eprintln!("Starting checkpoint comparison...");
    let mut ref_runner = rvr::Runner::load(&ref_dir, ctx.elf_path)
        .map_err(|e| format!("Error loading reference runner: {e}"))?;
    ref_runner.prepare();
    let entry = ref_runner.entry_point();
    ref_runner.set_pc(entry);

    let mut test_runner = rvr::Runner::load(&test_dir, ctx.elf_path)
        .map_err(|e| format!("Error loading test runner: {e}"))?;
    test_runner.prepare();
    test_runner.set_pc(entry);

    Ok(diff::compare_checkpoint(
        &mut ref_runner,
        &mut test_runner,
        CHECKPOINT_INTERVAL,
        ctx.max_instrs,
    ))
}

fn run_instruction_comparison(ctx: &DiffContext<'_>) -> Result<diff::CompareResult, String> {
    let ref_compiled_dir = if let Some(dir) = ctx.ref_dir.clone() {
        dir
    } else if let Some(backend) = ctx.ref_backend.as_backend() {
        let dir = ctx.output_dir.join("ref");
        eprintln!("Compiling reference ({backend:?})...");
        diff::compile_for_diff(ctx.elf_path, &dir, backend, ctx.compiler)
            .map_err(|err| format!("Error: {err}"))?;
        dir
    } else {
        PathBuf::new()
    };

    let test_compiled_dir = if let Some(dir) = ctx.test_dir.clone() {
        dir
    } else if let Some(backend) = ctx.test_backend.as_backend() {
        let dir = ctx.output_dir.join("test");
        eprintln!("Compiling test ({backend:?})...");
        diff::compile_for_diff(ctx.elf_path, &dir, backend, ctx.compiler)
            .map_err(|err| format!("Error: {err}"))?;
        dir
    } else {
        return Err("test backend always required".to_string());
    };

    eprintln!("Starting differential execution...");
    let config = diff::CompareConfig {
        strict_reg_writes: true,
        strict_mem_access: ctx.strict_mem,
    };

    match ctx.ref_backend {
        DiffBackend::Spike => {
            let mut spike = diff::SpikeExecutor::start(ctx.elf_path, ctx.isa, ctx.entry_point)
                .map_err(|e| format!("Error starting Spike: {e}"))?;
            let mut test = diff::InProcessExecutor::new(&test_compiled_dir, ctx.elf_path)
                .map_err(|e| format!("Error loading test executor: {e}"))?;
            Ok(diff::compare_lockstep(
                &mut spike,
                &mut test,
                &config,
                ctx.max_instrs,
            ))
        }
        DiffBackend::Backend(_) => {
            let mut reference = diff::InProcessExecutor::new(&ref_compiled_dir, ctx.elf_path)
                .map_err(|e| format!("Error loading reference executor: {e}"))?;
            let mut test = diff::InProcessExecutor::new(&test_compiled_dir, ctx.elf_path)
                .map_err(|e| format!("Error loading test executor: {e}"))?;
            Ok(diff::compare_lockstep(
                &mut reference,
                &mut test,
                &config,
                ctx.max_instrs,
            ))
        }
    }
}

fn report_result(result: &diff::CompareResult, output_dir: &Path) -> i32 {
    eprintln!();
    result.divergence.as_ref().map_or_else(
        || {
            eprintln!("PASS: {} instructions matched", result.matched);
            EXIT_SUCCESS
        },
        |div| {
            eprintln!("DIVERGENCE at instruction {}: {}", div.index, div.kind);
            eprintln!();
            eprintln!("Expected:");
            eprintln!("  PC: 0x{:016x}", div.expected.pc);
            eprintln!("  Opcode: 0x{:08x}", div.expected.opcode);
            if let (Some(rd), Some(val)) = (div.expected.rd, div.expected.rd_value) {
                eprintln!("  x{rd} = 0x{val:016x}");
            }
            if let Some(addr) = div.expected.mem_addr {
                eprintln!("  mem 0x{addr:016x}");
            }
            eprintln!();
            eprintln!("Actual:");
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
        },
    )
}

fn run_comparison(
    ctx: &DiffContext<'_>,
    cc: &str,
    modes: CompareModes,
) -> Result<diff::CompareResult, String> {
    if modes.use_pure_c {
        Ok(run_pure_c(ctx, cc))
    } else if modes.use_block_comparison {
        run_block_comparison(ctx)
    } else if modes.use_checkpoint_comparison {
        run_checkpoint_comparison(ctx)
    } else {
        run_instruction_comparison(ctx)
    }
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
    let granularity = granularity_from_arg(granularity_arg);

    let test_name = test_name_from_path(elf_path);
    if should_skip_test(test_name) {
        eprintln!("SKIP: {test_name} (not compatible with static recompilation)");
        return EXIT_SUCCESS;
    }

    let isa = match resolve_isa(elf_path, isa, test_name) {
        Ok(isa) => isa,
        Err(message) => {
            eprintln!("{message}");
            return EXIT_FAILURE;
        }
    };

    let entry_point = match resolve_entry_point(elf_path) {
        Ok(entry_point) => entry_point,
        Err(message) => {
            eprintln!("{message}");
            return EXIT_FAILURE;
        }
    };

    log_header(elf_path, mode, granularity, &isa, entry_point, max_instrs);

    let output_dir = create_output_dir(output_dir);

    let (ref_backend, test_backend) = match resolve_diff_backends(mode, ref_backend, test_backend) {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("Error: {msg}");
            return EXIT_FAILURE;
        }
    };

    eprintln!("Reference: {ref_backend:?}");
    eprintln!("Test: {test_backend:?}");

    if let Err(message) = ensure_spike_available(ref_backend) {
        eprintln!("{message}");
        return EXIT_FAILURE;
    }

    let modes = determine_compare_modes(granularity, ref_backend, test_backend);
    let compiler = match resolve_compiler(cc) {
        Ok(compiler) => compiler,
        Err(message) => {
            eprintln!("{message}");
            return EXIT_FAILURE;
        }
    };

    let ctx = DiffContext {
        elf_path,
        output_dir: &output_dir,
        compiler: &compiler,
        ref_backend,
        test_backend,
        ref_dir,
        test_dir,
        max_instrs,
        strict_mem,
        isa: &isa,
        entry_point,
    };

    let result = match run_comparison(&ctx, cc, modes) {
        Ok(result) => result,
        Err(message) => {
            eprintln!("{message}");
            return EXIT_FAILURE;
        }
    };

    report_result(&result, &output_dir)
}
