use std::path::{Path, PathBuf};

use rvr_emit::Backend;
use rvr_ir::Xlen;

use rvr::test_support::diff;

use rvr::Compiler;

/// Run pure-C comparison: generates and runs a standalone C comparison program.
///
/// This eliminates all Rust FFI overhead by running the comparison entirely in C.
pub(super) fn run_pure_c_comparison(
    elf_path: &PathBuf,
    output_dir: &Path,
    ref_backend: Backend,
    test_backend: Backend,
    compiler: &Compiler,
    cc: &str,
    max_instrs: Option<u64>,
) -> diff::CompareResult {
    use rvr_elf::ElfImage;
    use rvr_ir::Rv64;

    eprintln!("Using pure-C comparison (no Rust FFI overhead)");

    // Load ELF to get segments
    let elf_data = match std::fs::read(elf_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error reading ELF: {}", e);
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
    };

    let image = match ElfImage::<Rv64>::parse(&elf_data) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Error parsing ELF: {}", e);
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
    };

    // Compile both backends
    let ref_dir = output_dir.join("ref");
    eprintln!("Compiling reference ({:?})...", ref_backend);
    if let Err(err) = diff::compile_for_checkpoint(elf_path, &ref_dir, ref_backend, compiler) {
        eprintln!("Error: {}", err);
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    let test_dir = output_dir.join("test");
    eprintln!("Compiling test ({:?})...", test_backend);
    if let Err(err) = diff::compile_for_checkpoint(elf_path, &test_dir, test_backend, compiler) {
        eprintln!("Error: {}", err);
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    // Generate pure-C comparison program
    let default_mem_bits = rvr_emit::EmitConfig::<Rv64>::default().memory_bits;
    let initial_sp = image.lookup_symbol("__stack_top").unwrap_or(0);
    let initial_gp = image.lookup_symbol("__global_pointer$").unwrap_or(0);
    let config = diff::CCompareConfig {
        entry_point: image.entry_point,
        max_instrs: max_instrs.unwrap_or(u64::MAX),
        checkpoint_interval: 1_000_000,
        memory_bits: default_mem_bits,
        num_regs: 32,
        instret_suspend: true,
        initial_brk: Rv64::to_u64(image.get_initial_program_break()),
        initial_sp,
        initial_gp,
    };

    eprintln!("Generating pure-C comparison program...");
    if let Err(e) = diff::generate_c_compare(output_dir, &image.memory_segments, &config) {
        eprintln!("Error generating C code: {}", e);
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    // Compile comparison program
    eprintln!("Compiling comparison program...");
    if !diff::compile_c_compare(output_dir, cc) {
        eprintln!("Error: Failed to compile comparison program");
        eprintln!("See {}/diff_compare.c for generated code", output_dir.display());
        return diff::CompareResult {
            matched: 0,
            divergence: None,
        };
    }

    // Find the compiled libraries (names vary by backend: libref.so, libtest.so, librv.so)
    let ref_lib = find_library_in_dir(&ref_dir);
    let test_lib = find_library_in_dir(&test_dir);

    let (ref_lib, test_lib) = match (ref_lib, test_lib) {
        (Some(r), Some(t)) => (r, t),
        (None, _) => {
            eprintln!("Error: Could not find compiled library in {}", ref_dir.display());
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
        (_, None) => {
            eprintln!("Error: Could not find compiled library in {}", test_dir.display());
            return diff::CompareResult {
                matched: 0,
                divergence: None,
            };
        }
    };

    eprintln!("Running pure-C comparison...");
    eprintln!("  Reference: {}", ref_lib.display());
    eprintln!("  Test: {}", test_lib.display());
    eprintln!();

    match diff::run_c_compare(output_dir, &ref_lib, &test_lib) {
        Ok(output) => {
            // Print output
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if !stdout.is_empty() {
                eprint!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }

            // Parse result from output
            // The C program prints "PASS: N instructions matched" on success
            // or "DIVERGENCE at instruction N" on failure
            if output.status.success() {
                // Extract matched count from output (stdout or stderr)
                let combined = format!("{stdout}\n{stderr}");
                let tokens: Vec<&str> = combined.split_whitespace().collect();
                let matched = tokens
                    .iter()
                    .position(|t| *t == "PASS:")
                    .and_then(|idx| tokens.get(idx + 1))
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);

                diff::CompareResult {
                    matched,
                    divergence: None,
                }
            } else {
                // Extract divergence info from stderr
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
        }
        Err(e) => {
            eprintln!("Error running comparison: {}", e);
            diff::CompareResult {
                matched: 0,
                divergence: None,
            }
        }
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
