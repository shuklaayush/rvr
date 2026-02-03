//! Pure C differential comparison generator.
//!
//! Generates a C program that compares two backends without any Rust FFI overhead.
//! Uses dlopen to load compiled backends and compares execution in pure C.

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
    /// Initial program break (brk/start_brk).
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

/// Generate a C program that compares two compiled backends using dlopen.
///
/// The generated program will:
/// 1. dlopen both compiled .so files
/// 2. Create two RvState structs with matching layout
/// 3. Load ELF segments into each backend's memory
/// 4. Run both backends for checkpoint_interval instructions
/// 5. Compare PC and registers at each checkpoint
/// 6. Binary search to find exact divergence if checkpoint fails
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

    let memory_size = 1u64 << config.memory_bits;
    let code = format!(
        r##"// Auto-generated pure C differential comparison program
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
{segment_data}

// Load a backend from .so file
static bool load_backend(Backend* b, const char* path, const char* name) {{
    b->name = name;
    b->handle = dlopen(path, RTLD_NOW);
    if (!b->handle) {{
        fprintf(stderr, "Failed to load %s: %s\\n", path, dlerror());
        return false;
    }}

    b->execute_from = (execute_fn)dlsym(b->handle, "rv_execute_from");
    if (!b->execute_from) {{
        fprintf(stderr, "Failed to find rv_execute_from in %s: %s\\n", path, dlerror());
        dlclose(b->handle);
        return false;
    }}

    // Allocate memory
    b->memory = mmap(NULL, kMemorySize, PROT_READ | PROT_WRITE,
                     MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (b->memory == MAP_FAILED) {{
        fprintf(stderr, "Failed to allocate memory for %s\\n", name);
        dlclose(b->handle);
        return false;
    }}

    // Initialize state
    memset(&b->state, 0, sizeof(b->state));
    b->state.pc = kEntryPoint;
    b->state.brk = kInitialBrk;
    b->state.start_brk = kInitialBrk;
    if (kInitialSp) b->state.regs[2] = kInitialSp;
    if (kInitialGp) b->state.regs[3] = kInitialGp;
    b->state.memory = b->memory;

    return true;
}}

// Load segments into backend memory
static void load_segments(Backend* b) {{
{segment_load}
}}

// Unload backend
static void unload_backend(Backend* b) {{
    if (b->memory && b->memory != MAP_FAILED) {{
        munmap(b->memory, kMemorySize);
    }}
    if (b->handle) {{
        dlclose(b->handle);
    }}
}}

// Compare two states, return true if they match
static bool states_match(const RvState* ref, const RvState* test) {{
    if (ref->pc != test->pc) return false;
    for (int i = 0; i < kNumRegs; i++) {{
        if (ref->regs[i] != test->regs[i]) return false;
    }}
    return true;
}}

// Print divergence details
static void print_divergence(uint64_t instr, const RvState* ref, const RvState* test) {{
    fprintf(stderr, "\\nDIVERGENCE at instruction %llu\\n", (unsigned long long)instr);
    fprintf(stderr, "Reference PC: 0x%016llx\\n", (unsigned long long)ref->pc);
    fprintf(stderr, "Test PC:      0x%016llx\\n", (unsigned long long)test->pc);

    for (int i = 0; i < kNumRegs; i++) {{
        if (ref->regs[i] != test->regs[i]) {{
            fprintf(stderr, "  x%d: ref=0x%016llx test=0x%016llx\\n",
                    i, (unsigned long long)ref->regs[i],
                    (unsigned long long)test->regs[i]);
        }}
    }}
}}

// Reset backend state and memory to initial image
static bool reset_backend(Backend* b) {{
    if (b->memory && b->memory != MAP_FAILED) {{
        munmap(b->memory, kMemorySize);
    }}
    b->memory = mmap(NULL, kMemorySize, PROT_READ | PROT_WRITE,
                     MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (b->memory == MAP_FAILED) {{
        fprintf(stderr, "Failed to allocate memory for %s\\n", b->name);
        return false;
    }}
    memset(&b->state, 0, sizeof(b->state));
    b->state.pc = kEntryPoint;
    b->state.brk = kInitialBrk;
    b->state.start_brk = kInitialBrk;
    if (kInitialSp) b->state.regs[2] = kInitialSp;
    if (kInitialGp) b->state.regs[3] = kInitialGp;
    b->state.memory = b->memory;
    load_segments(b);
    return true;
}}

// Run backend until target instret (suspend mode required)
static int run_to_instret(Backend* b, uint64_t target) {{
    while (b->state.instret < target && !b->state.has_exited) {{
        b->state.target_instret = target;
        b->execute_from(&b->state, b->state.pc);
    }}
    return b->state.has_exited ? 1 : 0;
}}

// Step one instruction on both backends and compare
static int step_and_compare(Backend* ref, Backend* test, uint64_t* matched) {{
    ref->state.target_instret = ref->state.instret + 1;
    test->state.target_instret = test->state.instret + 1;
    ref->execute_from(&ref->state, ref->state.pc);
    test->execute_from(&test->state, test->state.pc);
    if (!states_match(&ref->state, &test->state)) {{
        print_divergence(*matched, &ref->state, &test->state);
        return 1;
    }}
    *matched = ref->state.instret;
    return 0;
}}

// Fast checkpoint + binary search to first mismatch
static int compare_checkpoint_find_first(Backend* ref, Backend* test, uint64_t* matched) {{
    uint64_t limit = kMaxInstrs;
    uint64_t last_match = 0;
    uint64_t mismatch_hi = 0;
    *matched = 0;

    while (last_match < limit) {{
        uint64_t remaining = limit - last_match;
        uint64_t batch = (remaining < kCheckpointInterval) ? remaining : kCheckpointInterval;

        ref->state.target_instret = ref->state.instret + batch;
        test->state.target_instret = test->state.instret + batch;
        ref->execute_from(&ref->state, ref->state.pc);
        test->execute_from(&test->state, test->state.pc);

        if (!states_match(&ref->state, &test->state)) {{
            mismatch_hi = ref->state.instret;
            break;
        }}

        last_match = ref->state.instret;
        *matched = last_match;

        if (ref->state.has_exited || test->state.has_exited) {{
            if (ref->state.has_exited != test->state.has_exited) {{
                fprintf(stderr, "Exit status mismatch: ref=%d test=%d\\n",
                        ref->state.has_exited, test->state.has_exited);
                return 1;
            }}
            return 0;
        }}
    }}

    if (mismatch_hi == 0) {{
        return 0;
    }}

    uint64_t low = last_match;
    uint64_t high = mismatch_hi;

    while (high - low > 1) {{
        uint64_t mid = low + (high - low) / 2;

        if (!reset_backend(ref) || !reset_backend(test)) {{
            return 1;
        }}
        if (run_to_instret(ref, mid) || run_to_instret(test, mid)) {{
            fprintf(stderr, "Exited before reaching target instret %llu\\n",
                    (unsigned long long)mid);
            return 1;
        }}

        if (states_match(&ref->state, &test->state)) {{
            low = mid;
        }} else {{
            high = mid;
        }}
    }}

    if (!reset_backend(ref) || !reset_backend(test)) {{
        return 1;
    }}
    if (run_to_instret(ref, low) || run_to_instret(test, low)) {{
        fprintf(stderr, "Exited before reaching target instret %llu\\n",
                (unsigned long long)low);
        return 1;
    }}
    *matched = low;
    return step_and_compare(ref, test, matched);
}}

// Get current time in seconds
static double get_time(void) {{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec * 1e-9;
}}

int main(int argc, char** argv) {{
    if (argc != 3) {{
        fprintf(stderr, "Usage: %s <ref.so> <test.so>\\n", argv[0]);
        return 1;
    }}

    const char* ref_path = argv[1];
    const char* test_path = argv[2];

    Backend ref = {{0}};
    Backend test = {{0}};

    // Load backends
    printf("Loading reference: %s\\n", ref_path);
    if (!load_backend(&ref, ref_path, "reference")) {{
        return 1;
    }}

    printf("Loading test: %s\\n", test_path);
    if (!load_backend(&test, test_path, "test")) {{
        unload_backend(&ref);
        return 1;
    }}

    // Load segments into both
    load_segments(&ref);
    load_segments(&test);

    printf("Starting comparison at PC=0x%llx\\n", (unsigned long long)kEntryPoint);
    printf("Checkpoint: %llu instructions\\n", (unsigned long long)kCheckpointInterval);

    // Run comparison
    double start = get_time();
    uint64_t matched = 0;
    int result = compare_checkpoint_find_first(&ref, &test, &matched);
    double elapsed = get_time() - start;

    if (result == 0) {{
        printf("\\nPASS: %llu instructions matched in %.3fs (%.2fM instr/s)\\n",
               (unsigned long long)matched, elapsed,
               matched / elapsed / 1e6);
        if (ref.state.exit_code != 0) {{
            printf("Exit code: %d\\n", ref.state.exit_code);
            result = ref.state.exit_code;
        }}
    }} else {{
        printf("\\nFAIL: Divergence after %llu instructions\\n",
               (unsigned long long)matched);
    }}

    unload_backend(&ref);
    unload_backend(&test);

    return result;
}}
"##,
        entry_point = config.entry_point,
        max_instrs = config.max_instrs,
        checkpoint_interval = config.checkpoint_interval,
        memory_size = memory_size,
        num_regs = config.num_regs,
        initial_brk = config.initial_brk,
        initial_sp = config.initial_sp,
        initial_gp = config.initial_gp,
        target_instret_field = target_instret_field,
        segment_data = segment_data,
        segment_load = segment_load,
        reg_type = reg_type,
    );

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

        code.push_str(&format!(
            "static const uint8_t segment_{}_data[] = {{\n    ",
            i
        ));

        for (j, byte) in seg.data.iter().enumerate() {
            if j > 0 && j % 16 == 0 {
                code.push_str("\n    ");
            }
            code.push_str(&format!("0x{:02x},", byte));
        }

        code.push_str(&format!(
            "\n}};\nstatic const uint64_t segment_{}_addr = 0x{:x}ULL;\n",
            i, vaddr
        ));
        code.push_str(&format!(
            "static const size_t segment_{}_size = {};\n\n",
            i,
            seg.data.len()
        ));
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
            code.push_str(&format!(
                "    memcpy(b->memory + (segment_{}_addr & (kMemorySize - 1)), segment_{}_data, segment_{}_size);\n",
                i, i, i
            ));
        }

        // Zero-fill BSS portion (memsz > filesz) - mask address
        if memsz > filesz {
            let bss_start = vaddr + filesz;
            let bss_size = memsz - filesz;
            code.push_str(&format!(
                "    memset(b->memory + (0x{:x}ULL & (kMemorySize - 1)), 0, {});\n",
                bss_start, bss_size
            ));
        }
    }

    code
}

/// Compile the generated comparison program.
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
pub fn run_c_compare(
    output_dir: &Path,
    ref_lib: &Path,
    test_lib: &Path,
) -> std::io::Result<std::process::Output> {
    let compare_bin = output_dir.join("diff_compare");
    Command::new(&compare_bin).arg(ref_lib).arg(test_lib).output()
}
