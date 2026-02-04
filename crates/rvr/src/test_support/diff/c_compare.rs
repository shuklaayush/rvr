//! Pure C differential comparison generator.
//!
//! Generates a C program that compares two backends without any Rust FFI overhead.
//! Uses dlopen to load compiled backends and compares execution in pure C.

use std::fmt::Write as FmtWrite;
use std::path::Path;
use std::process::Command;

use rvr_elf::MemorySegment;
use rvr_ir::Xlen;

/// Configuration for pure C comparison.
pub struct CCompareConfig {
    /// Entry point address.
    pub entry_point: u64,
    /// Maximum instructions to compare.
    pub max_instrs: u64,
    /// Checkpoint interval (compare state every N instructions).
    pub checkpoint_interval: u64,
    /// Memory size in bits (e.g., 28 for 256MB).
    pub memory_bits: u8,
    /// Number of registers (32 for RV32/64).
    pub num_regs: usize,
    /// Whether backends use instret suspension.
    pub instret_suspend: bool,
    /// Initial program break (`brk/start_brk`).
    pub initial_brk: u64,
    /// Initial stack pointer (x2).
    pub initial_sp: u64,
    /// Initial global pointer (x3).
    pub initial_gp: u64,
}

impl Default for CCompareConfig {
    fn default() -> Self {
        Self {
            entry_point: 0,
            max_instrs: u64::MAX,
            checkpoint_interval: 1_000_000,
            memory_bits: 28,
            num_regs: 32,
            instret_suspend: true,
            initial_brk: 0,
            initial_sp: 0,
            initial_gp: 0,
        }
    }
}

fn compare_source_preamble(
    config: &CCompareConfig,
    memory_size: u64,
    reg_type: &str,
    target_instret_field: &str,
) -> String {
    format!(
        r"// Auto-generated pure C differential comparison program
// Compares two backends instruction by instruction without Rust FFI overhead

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <dlfcn.h>
#include <sys/mman.h>
#include <time.h>

// Configuration
static const uint64_t kEntryPoint = 0x{entry_point:x}ULL;
static const uint64_t kMaxInstrs = {max_instrs}ULL;
static const uint64_t kCheckpointInterval = {checkpoint_interval}ULL;
static const uint64_t kMemorySize = {memory_size}ULL;
static const uint64_t kInitialBrk = 0x{initial_brk:x}ULL;
static const uint64_t kInitialSp = 0x{initial_sp:x}ULL;
static const uint64_t kInitialGp = 0x{initial_gp:x}ULL;
static const int kClockMonotonic = 1;
enum {{ kNumRegs = {num_regs}, kNumCsrs = 4096 }};

// RvState struct - must match the generated code layout exactly
// This is the ABI contract between the comparison and the backends
typedef struct {{
    {reg_type} regs[kNumRegs];
    {reg_type} pc;
    uint64_t instret;
{target_instret_field}
    {reg_type} reservation_addr;
    uint8_t reservation_valid;
    uint8_t has_exited;
    uint8_t exit_code;
    uint8_t _pad0;
    {reg_type} brk;
    {reg_type} start_brk;
    uint8_t* memory;
    // Note: CSRs and tracer fields follow but we don't access them
    {reg_type} csrs[kNumCsrs];
}} RvState;

// Function pointer type for rv_execute_from
typedef int (*execute_fn)(RvState* state, uint64_t start_pc);

// Backend handle
typedef struct {{
    void* handle;
    execute_fn execute_from;
    RvState state;
    uint8_t* memory;
    const char* name;
}} Backend;

// Segment data (embedded in binary)
",
        entry_point = config.entry_point,
        max_instrs = config.max_instrs,
        checkpoint_interval = config.checkpoint_interval,
        memory_size = memory_size,
        num_regs = config.num_regs,
        initial_brk = config.initial_brk,
        initial_sp = config.initial_sp,
        initial_gp = config.initial_gp,
        reg_type = reg_type,
        target_instret_field = target_instret_field,
    )
}

fn compare_source_backend(segment_load: &str) -> String {
    format!(
        r#"

// Load a backend from .so file
static bool load_backend(Backend* b, const char* path, const char* name) {{
    b->name = name;
    b->handle = dlopen(path, RTLD_NOW);
    if (!b->handle) {{
        fprintf(stderr, "Failed to load %s: %s\n", path, dlerror());
        return false;
    }}

    b->execute_from = (execute_fn)dlsym(b->handle, "rv_execute_from");
    if (!b->execute_from) {{
        fprintf(stderr, "Failed to find rv_execute_from in %s\n", path);
        dlclose(b->handle);
        return false;
    }}

    // Allocate and zero state
    memset(&b->state, 0, sizeof(RvState));

    // Allocate memory
    b->memory = (uint8_t*)mmap(NULL, kMemorySize, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (b->memory == MAP_FAILED) {{
        fprintf(stderr, "Failed to allocate memory\n");
        dlclose(b->handle);
        return false;
    }}
    memset(b->memory, 0, kMemorySize);
    b->state.memory = b->memory;

    // Initialize registers
    b->state.pc = kEntryPoint;
    b->state.regs[2] = kInitialSp;
    b->state.regs[3] = kInitialGp;
    b->state.brk = kInitialBrk;
    b->state.start_brk = kInitialBrk;

    return true;
}}

// Load ELF segments into backend memory
static void load_segments(Backend* b) {{
{segment_load}
}}

// Compare two backend states
static bool compare_states(const Backend* a, const Backend* b) {{
    if (a->state.pc != b->state.pc) {{
        printf("PC mismatch: %s=0x%lx %s=0x%lx\n",
               a->name, a->state.pc, b->name, b->state.pc);
        return false;
    }}

    for (int i = 0; i < kNumRegs; i++) {{
        if (a->state.regs[i] != b->state.regs[i]) {{
            printf("Reg x%d mismatch: %s=0x%lx %s=0x%lx\n",
                   i, a->name, (uint64_t)a->state.regs[i],
                   b->name, (uint64_t)b->state.regs[i]);
            return false;
        }}
    }}

    return true;
}}
"#,
    )
}

fn compare_source_runtime(instret_suspend: u8) -> String {
    format!(
        r#"

// Run backend for N instructions
static bool run_backend(Backend* b, uint64_t num_instrs) {{
    if (kCheckpointInterval == 0) {{
        return false;
    }}

    if ({instret_suspend} == 0) {{
        // No suspension support - just run once
        b->execute_from(&b->state, b->state.pc);
        return true;
    }}

    // Set target instret
    b->state.target_instret = b->state.instret + num_instrs;
    b->execute_from(&b->state, b->state.pc);
    return true;
}}

// Find exact divergence using binary search
static uint64_t find_divergence(Backend* a, Backend* b, uint64_t start, uint64_t end) {{
    while (start < end) {{
        uint64_t mid = (start + end + 1) / 2;

        a->state.target_instret = a->state.instret + mid;
        b->state.target_instret = b->state.instret + mid;
        a->execute_from(&a->state, a->state.pc);
        b->execute_from(&b->state, b->state.pc);

        if (compare_states(a, b)) {{
            start = mid;
        }} else {{
            end = mid - 1;
        }}
    }}
    return start + 1;
}}

int main(int argc, char** argv) {{
    if (argc < 3) {{
        fprintf(stderr, "Usage: %s <ref_backend.so> <test_backend.so>\n", argv[0]);
        return 1;
    }}

    Backend ref_backend;
    Backend test_backend;

    if (!load_backend(&ref_backend, argv[1], "ref") ||
        !load_backend(&test_backend, argv[2], "test")) {{
        return 1;
    }}

    load_segments(&ref_backend);
    load_segments(&test_backend);

    // Run comparison loop
    uint64_t instrs = 0;
    while (instrs < kMaxInstrs) {{
        if (!run_backend(&ref_backend, kCheckpointInterval) ||
            !run_backend(&test_backend, kCheckpointInterval)) {{
            fprintf(stderr, "Failed to run backends\n");
            return 1;
        }}

        if (!compare_states(&ref_backend, &test_backend)) {{
            uint64_t diverge_at = find_divergence(&ref_backend, &test_backend, 0,
                                                  kCheckpointInterval);
            printf("Divergence at instruction %lu\n", instrs + diverge_at);
            return 1;
        }}

        instrs += kCheckpointInterval;
    }}

    printf("No divergence after %lu instructions\n", instrs);
    return 0;
}}
"#,
    )
}

/// Generate a C program that compares two compiled backends using dlopen.
///
/// The generated program will:
/// 1. dlopen both compiled .so files
/// 2. Create two `RvState` structs with matching layout
/// 3. Load ELF segments into each backend's memory
/// 4. Run both backends for `checkpoint_interval` instructions
/// 5. Compare PC and registers at each checkpoint
/// 6. Binary search to find exact divergence if checkpoint fails
///
/// # Errors
///
/// Returns errors from writing the generated source file.
pub fn generate_c_compare<X: Xlen>(
    output_dir: &Path,
    segments: &[MemorySegment<X>],
    config: &CCompareConfig,
) -> std::io::Result<()> {
    let segment_data = generate_segment_data::<X>(segments);
    let segment_load = generate_segment_load::<X>(segments);

    let target_instret_field = if config.instret_suspend {
        "    uint64_t target_instret;"
    } else {
        ""
    };
    let reg_type = if X::REG_BYTES == 4 {
        "uint32_t"
    } else {
        "uint64_t"
    };
    let instret_suspend = u8::from(config.instret_suspend);

    let memory_size = 1u64 << config.memory_bits;
    let mut code = compare_source_preamble(config, memory_size, reg_type, target_instret_field);
    code.push_str(&segment_data);
    code.push_str(&compare_source_backend(&segment_load));
    code.push_str(&compare_source_runtime(instret_suspend));

    std::fs::write(output_dir.join("diff_compare.c"), code)?;
    Ok(())
}

/// Generate C code for embedded segment data.
fn generate_segment_data<X: Xlen>(segments: &[MemorySegment<X>]) -> String {
    let mut code = String::new();

    for (i, seg) in segments.iter().enumerate() {
        // Skip pure BSS segments (no file data)
        if seg.data.is_empty() {
            continue;
        }

        let vaddr = X::to_u64(seg.virtual_start);

        let _ = write!(code, "static const uint8_t segment_{i}_data[] = {{\n    ");

        for (j, byte) in seg.data.iter().enumerate() {
            if j > 0 && j % 16 == 0 {
                code.push_str("\n    ");
            }
            let _ = write!(code, "0x{byte:02x},");
        }

        let _ = write!(
            code,
            "\n}};\nstatic const uint64_t segment_{i}_addr = 0x{vaddr:x}ULL;\n"
        );
        let _ = write!(
            code,
            "static const size_t segment_{i}_size = {};\n\n",
            seg.data.len()
        );
    }

    code
}

/// Generate C code to load segments into backend memory.
fn generate_segment_load<X: Xlen>(segments: &[MemorySegment<X>]) -> String {
    let mut code = String::new();

    for (i, seg) in segments.iter().enumerate() {
        let vaddr = X::to_u64(seg.virtual_start);
        let filesz = seg.filesz();
        let memsz = seg.memsz();

        if !seg.data.is_empty() {
            // Copy file data - mask address to fit within memory
            let _ = writeln!(
                code,
                "    memcpy(b->memory + (segment_{i}_addr & (kMemorySize - 1)), segment_{i}_data, segment_{i}_size);"
            );
        }

        // Zero-fill BSS portion (memsz > filesz) - mask address
        if memsz > filesz {
            let bss_start = vaddr + filesz;
            let bss_size = memsz - filesz;
            let _ = writeln!(
                code,
                "    memset(b->memory + (0x{bss_start:x}ULL & (kMemorySize - 1)), 0, {bss_size});"
            );
        }
    }

    code
}

/// Compile the generated comparison program.
#[must_use]
pub fn compile_c_compare(output_dir: &Path, cc: &str) -> bool {
    let compare_src = output_dir.join("diff_compare.c");
    let compare_bin = output_dir.join("diff_compare");

    let status = Command::new(cc)
        .arg("-O3")
        .arg("-march=native")
        .arg("-o")
        .arg(&compare_bin)
        .arg(&compare_src)
        .arg("-ldl")
        .status();

    matches!(status, Ok(s) if s.success())
}

/// Run the comparison program.
///
/// # Errors
///
/// Returns errors from launching the comparison program.
pub fn run_c_compare(
    output_dir: &Path,
    ref_lib: &Path,
    test_lib: &Path,
) -> std::io::Result<std::process::Output> {
    let compare_bin = output_dir.join("diff_compare");
    Command::new(&compare_bin)
        .arg(ref_lib)
        .arg(test_lib)
        .output()
}
