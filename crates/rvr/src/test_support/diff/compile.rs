use std::path::{Path, PathBuf};

use rvr_emit::Backend;
use rvr_emit::c::TracerKind;

use crate::{CompileOptions, Compiler, InstretMode, TracerConfig, compile_with_options};

/// Compilation mode for differential execution.
#[derive(Clone, Copy, Debug)]
enum DiffCompileMode {
    /// Linear mode: per-instruction stepping with single diff tracer.
    Linear,
    /// Block mode: run blocks with buffered diff tracer.
    Block,
    /// Checkpoint mode: suspend mode only, no tracer needed.
    Checkpoint,
}

fn compile_for_diff_mode(
    elf_path: &Path,
    output_dir: &Path,
    backend: Backend,
    compiler: &Compiler,
    mode: DiffCompileMode,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("failed to create output dir: {e}"))?;

    if matches!(mode, DiffCompileMode::Block) && !super::backend_supports_buffered_diff(backend) {
        return Err(format!(
            "buffered diff tracer not supported for backend {:?}",
            backend
        ));
    }
    if matches!(mode, DiffCompileMode::Linear) && !super::backend_supports_diff(backend) {
        return Err(format!(
            "diff tracer not supported for backend {:?}",
            backend
        ));
    }

    let (instret_mode, tracer_config, superblock) = match mode {
        DiffCompileMode::Linear => (
            InstretMode::PerInstruction,
            TracerConfig::builtin(TracerKind::Diff),
            false,
        ),
        DiffCompileMode::Block => (
            InstretMode::Suspend,
            TracerConfig::builtin(TracerKind::BufferedDiff),
            true,
        ),
        DiffCompileMode::Checkpoint => (InstretMode::PerInstruction, TracerConfig::none(), false),
    };

    let mut options = CompileOptions::new()
        .with_backend(backend)
        .with_instret_mode(instret_mode)
        .with_tracer_config(tracer_config)
        .with_compiler(compiler.clone())
        .with_quiet(true);

    if !superblock {
        options = options.with_superblock(false);
    }

    compile_with_options(elf_path, output_dir, options)
        .map_err(|e| format!("compile failed: {e}"))
}

/// Compile an ELF for linear differential execution (per-instruction stepping).
pub fn compile_for_diff(
    elf_path: &Path,
    output_dir: &Path,
    backend: Backend,
    compiler: &Compiler,
) -> Result<PathBuf, String> {
    compile_for_diff_mode(elf_path, output_dir, backend, compiler, DiffCompileMode::Linear)
}

/// Compile an ELF for block-level differential execution (buffered tracer).
pub fn compile_for_diff_block(
    elf_path: &Path,
    output_dir: &Path,
    backend: Backend,
    compiler: &Compiler,
) -> Result<PathBuf, String> {
    compile_for_diff_mode(elf_path, output_dir, backend, compiler, DiffCompileMode::Block)
}

/// Compile an ELF for checkpoint-based differential execution (suspend mode only).
pub fn compile_for_checkpoint(
    elf_path: &Path,
    output_dir: &Path,
    backend: Backend,
    compiler: &Compiler,
) -> Result<PathBuf, String> {
    compile_for_diff_mode(
        elf_path,
        output_dir,
        backend,
        compiler,
        DiffCompileMode::Checkpoint,
    )
}
