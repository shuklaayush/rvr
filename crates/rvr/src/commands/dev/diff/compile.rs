use std::path::PathBuf;

use rvr_emit::Backend;

use super::{backend_supports_buffered_diff, backend_supports_diff};

/// Compilation mode for differential execution.
#[derive(Clone, Copy, Debug)]
enum DiffCompileMode {
    /// Linear mode: per-instruction stepping with single diff tracer.
    /// Used for linear executor that steps one instruction at a time.
    Linear,
    /// Block mode: run blocks with buffered diff tracer.
    /// Used for block executor that captures N instructions per block.
    Block,
    /// Checkpoint mode: suspend mode only, no tracer needed.
    /// Used for fast checkpoint comparison at intervals.
    Checkpoint,
}

/// Compile an ELF for differential execution with the specified mode.
fn compile_for_diff_mode(
    elf_path: &PathBuf,
    output_dir: &PathBuf,
    backend: Backend,
    cc: &str,
    mode: DiffCompileMode,
) -> bool {
    use std::process::Command;

    std::fs::create_dir_all(output_dir).ok();

    if matches!(mode, DiffCompileMode::Block) && !backend_supports_buffered_diff(backend) {
        eprintln!(
            "Error: buffered diff tracer not supported for backend {:?}",
            backend
        );
        return false;
    }
    if matches!(mode, DiffCompileMode::Linear) && !backend_supports_diff(backend) {
        eprintln!("Error: diff tracer not supported for backend {:?}", backend);
        return false;
    }

    let mut cmd = Command::new("./target/release/rvr");
    cmd.arg("compile")
        .arg(elf_path)
        .arg("-o")
        .arg(output_dir)
        .arg("--backend")
        .arg(match backend {
            Backend::C => "c",
            Backend::ARM64Asm => "arm64",
            Backend::X86Asm => "x86",
        })
        .arg("--cc")
        .arg(cc);

    match mode {
        DiffCompileMode::Linear => {
            // Linear mode: step per instruction with single diff tracer
            cmd.arg("--instret")
                .arg("per-instruction")
                .arg("--tracer")
                .arg("diff")
                .arg("--no-superblock");
        }
        DiffCompileMode::Block => {
            // Block mode: run blocks with buffered diff tracer
            // Use suspend mode (not per-instruction) so we can run full blocks
            cmd.arg("--instret")
                .arg("suspend")
                .arg("--tracer")
                .arg("buffered-diff");
            // Note: no --no-superblock so blocks can be optimized
        }
        DiffCompileMode::Checkpoint => {
            // Checkpoint mode: suspend only, no tracer needed
            // Use --no-superblock to ensure consistent instruction sequences
            // This makes the code paths more similar to instruction mode
            cmd.arg("--instret")
                .arg("per-instruction")
                .arg("--no-superblock");
        }
    }

    let status = cmd.status();
    matches!(status, Ok(s) if s.success())
}

/// Compile an ELF for linear differential execution (per-instruction stepping).
pub(super) fn compile_for_diff(
    elf_path: &PathBuf,
    output_dir: &PathBuf,
    backend: Backend,
    cc: &str,
) -> bool {
    compile_for_diff_mode(elf_path, output_dir, backend, cc, DiffCompileMode::Linear)
}

/// Compile an ELF for block-level differential execution (buffered tracer).
pub(super) fn compile_for_diff_block(
    elf_path: &PathBuf,
    output_dir: &PathBuf,
    backend: Backend,
    cc: &str,
) -> bool {
    compile_for_diff_mode(elf_path, output_dir, backend, cc, DiffCompileMode::Block)
}

/// Compile an ELF for checkpoint-based differential execution (suspend mode only).
pub(super) fn compile_for_checkpoint(
    elf_path: &PathBuf,
    output_dir: &PathBuf,
    backend: Backend,
    cc: &str,
) -> bool {
    compile_for_diff_mode(elf_path, output_dir, backend, cc, DiffCompileMode::Checkpoint)
}

