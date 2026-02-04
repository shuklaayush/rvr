use std::path::{Path, PathBuf};

use rvr_emit::Backend;
use rvr_ir::Xlen;

use rvr::test_support::diff;

use rvr::Compiler;

/// Run pure-C comparison: generates and runs a standalone C comparison program.
///
/// This eliminates all Rust FFI overhead by running the comparison entirely in C.
pub(super) fn run_pure_c_comparison(
    elf_path: &Path,
    output_dir: &Path,
    ref_backend: Backend,
    test_backend: Backend,
    compiler: &Compiler,
    cc: &str,
    max_instrs: Option<u64>,
) -> diff::CompareResult {
    use rvr_ir::Rv64;

    eprintln!("Using pure-C comparison (no Rust FFI overhead)");

    let Some(image) = load_image::<Rv64>(elf_path) else {
        return fail_result();
    };

    // Compile both backends
    let ref_dir = output_dir.join("ref");
    eprintln!("Compiling reference ({ref_backend:?})...");
    if !compile_checkpoint(elf_path, &ref_dir, ref_backend, compiler) {
        return fail_result();
    }

    let test_dir = output_dir.join("test");
    eprintln!("Compiling test ({test_backend:?})...");
    if !compile_checkpoint(elf_path, &test_dir, test_backend, compiler) {
        return fail_result();
    }

    // Generate pure-C comparison program
    let default_mem_bits = rvr_emit::EmitConfig::<Rv64>::default().memory_bits;
    let initial_stack = image.lookup_symbol("__stack_top").unwrap_or(0);
    let initial_global = image.lookup_symbol("__global_pointer$").unwrap_or(0);
    let config = diff::CCompareConfig {
        entry_point: image.entry_point,
        max_instrs: max_instrs.unwrap_or(u64::MAX),
        checkpoint_interval: 1_000_000,
        memory_bits: default_mem_bits,
        num_regs: 32,
        instret_suspend: true,
        initial_brk: Rv64::to_u64(image.get_initial_program_break()),
        initial_sp: initial_stack,
        initial_gp: initial_global,
    };

    eprintln!("Generating pure-C comparison program...");
    if let Err(e) = diff::generate_c_compare(output_dir, &image.memory_segments, &config) {
        eprintln!("Error generating C code: {e}");
        return fail_result();
    }

    // Compile comparison program
    eprintln!("Compiling comparison program...");
    if !diff::compile_c_compare(output_dir, cc) {
        eprintln!("Error: Failed to compile comparison program");
        eprintln!(
            "See {}/diff_compare.c for generated code",
            output_dir.display()
        );
        return fail_result();
    }

    let Some((ref_lib, test_lib)) = find_libraries(&ref_dir, &test_dir) else {
        return fail_result();
    };

    eprintln!("Running pure-C comparison...");
    eprintln!("  Reference: {}", ref_lib.display());
    eprintln!("  Test: {}", test_lib.display());
    eprintln!();

    match diff::run_c_compare(output_dir, &ref_lib, &test_lib) {
        Ok(output) => parse_compare_output(&output),
        Err(e) => {
            eprintln!("Error running comparison: {e}");
            fail_result()
        }
    }
}

fn load_image<X: Xlen>(elf_path: &Path) -> Option<rvr_elf::ElfImage<X>> {
    let elf_data = match std::fs::read(elf_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error reading ELF: {e}");
            return None;
        }
    };

    match rvr_elf::ElfImage::<X>::parse(&elf_data) {
        Ok(i) => Some(i),
        Err(e) => {
            eprintln!("Error parsing ELF: {e}");
            None
        }
    }
}

fn compile_checkpoint(
    elf_path: &Path,
    out_dir: &Path,
    backend: Backend,
    compiler: &Compiler,
) -> bool {
    if let Err(err) = diff::compile_for_checkpoint(elf_path, out_dir, backend, compiler) {
        eprintln!("Error: {err}");
        return false;
    }
    true
}

fn find_libraries(ref_dir: &Path, test_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let ref_lib = find_library_in_dir(ref_dir)?;
    let test_lib = find_library_in_dir(test_dir)?;
    Some((ref_lib, test_lib))
}

fn parse_compare_output(output: &std::process::Output) -> diff::CompareResult {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        eprint!("{stdout}");
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }

    if output.status.success() {
        let combined = format!("{stdout}\n{stderr}");
        let tokens: Vec<&str> = combined.split_whitespace().collect();
        let matched = tokens
            .iter()
            .position(|t| *t == "PASS:")
            .and_then(|idx| tokens.get(idx + 1))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0);
        return diff::CompareResult {
            matched,
            divergence: None,
        };
    }

    let matched = stderr
        .lines()
        .find(|l| l.contains("DIVERGENCE"))
        .and_then(|l| {
            l.split_whitespace()
                .last()
                .and_then(|n| n.parse::<usize>().ok())
        })
        .unwrap_or(0);

    diff::CompareResult {
        matched,
        divergence: Some(diff::Divergence {
            index: matched,
            expected: diff::DiffState::default(),
            actual: diff::DiffState::default(),
            kind: diff::DivergenceKind::Pc,
        }),
    }
}

const fn fail_result() -> diff::CompareResult {
    diff::CompareResult {
        matched: 0,
        divergence: None,
    }
}

/// Find a shared library (.so) file in the given directory.
fn find_library_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("so") {
            return Some(path);
        }
    }
    None
}
