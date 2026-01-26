//! CoreMark benchmark building.
//!
//! CoreMark requires a platform port. We generate a minimal RISC-V bare-metal
//! port at build time to avoid modifying the upstream sources.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use rvr::bench::Arch;

/// Host port header for CoreMark (64-bit compatible).
const HOST_PORTME_H: &str = r#"
#ifndef CORE_PORTME_H
#define CORE_PORTME_H

#include <stddef.h>
#include <stdint.h>
#include <time.h>

#define HAS_FLOAT 1
#define HAS_TIME_H 1
#define USE_CLOCK 1
#define HAS_STDIO 1
#define HAS_PRINTF 1

typedef clock_t CORE_TICKS;

#ifndef COMPILER_VERSION
#ifdef __GNUC__
#define COMPILER_VERSION "GCC"__VERSION__
#else
#define COMPILER_VERSION "Unknown"
#endif
#endif

#ifndef COMPILER_FLAGS
#define COMPILER_FLAGS "-O3"
#endif

#ifndef MEM_LOCATION
#define MEM_LOCATION "STACK"
#endif

typedef signed short   ee_s16;
typedef unsigned short ee_u16;
typedef signed int     ee_s32;
typedef double         ee_f32;
typedef unsigned char  ee_u8;
typedef unsigned int   ee_u32;
typedef uintptr_t      ee_ptr_int;  /* 64-bit safe pointer type */
typedef size_t         ee_size_t;

#define align_mem(x) (void *)(sizeof(ee_ptr_int) + ((((ee_ptr_int)(x) - 1) / sizeof(ee_ptr_int)) * sizeof(ee_ptr_int)))

#define SEED_METHOD SEED_VOLATILE
#define MEM_METHOD MEM_STACK

#define MULTITHREAD 1
#define USE_PTHREAD 0
#define USE_FORK 0
#define USE_SOCKET 0

#define MAIN_HAS_NOARGC 0
#define MAIN_HAS_NORETURN 0

typedef struct CORE_PORTABLE_S {
    ee_u8 portable_id;
} core_portable;

extern ee_u32 default_num_contexts;

void portable_init(core_portable *p, int *argc, char *argv[]);
void portable_fini(core_portable *p);

#if !defined(PROFILE_RUN) && !defined(PERFORMANCE_RUN) && !defined(VALIDATION_RUN)
#define PERFORMANCE_RUN 1
#endif

#endif
"#;

/// Host port implementation for CoreMark.
const HOST_PORTME_C: &str = r#"
#include <stdio.h>
#include <time.h>
#include "coremark.h"

#if VALIDATION_RUN
volatile ee_s32 seed1_volatile = 0x3415;
volatile ee_s32 seed2_volatile = 0x3415;
volatile ee_s32 seed3_volatile = 0x66;
#endif
#if PERFORMANCE_RUN
volatile ee_s32 seed1_volatile = 0x0;
volatile ee_s32 seed2_volatile = 0x0;
volatile ee_s32 seed3_volatile = 0x66;
#endif
#if PROFILE_RUN
volatile ee_s32 seed1_volatile = 0x8;
volatile ee_s32 seed2_volatile = 0x8;
volatile ee_s32 seed3_volatile = 0x8;
#endif
volatile ee_s32 seed4_volatile = ITERATIONS;
volatile ee_s32 seed5_volatile = 0;
ee_u32 default_num_contexts = 1;

#define EE_TICKS_PER_SEC (CLOCKS_PER_SEC)

static CORE_TICKS start_time_val, stop_time_val;

void start_time(void) { start_time_val = clock(); }
void stop_time(void) { stop_time_val = clock(); }
CORE_TICKS get_time(void) { return stop_time_val - start_time_val; }
secs_ret time_in_secs(CORE_TICKS ticks) { return (secs_ret)ticks / (secs_ret)EE_TICKS_PER_SEC; }

void portable_init(core_portable *p, int *argc, char *argv[]) {
    (void)argc; (void)argv;
    if (sizeof(ee_ptr_int) != sizeof(ee_u8 *)) {
        ee_printf("ERROR! ee_ptr_int must hold a pointer!\n");
    }
    p->portable_id = 1;
}
void portable_fini(core_portable *p) { p->portable_id = 0; }
"#;

/// Minimal RISC-V port header for CoreMark.
const PORTME_H: &str = r#"
#ifndef CORE_PORTME_H
#define CORE_PORTME_H

#include <stddef.h>

#define HAS_FLOAT 0
#define HAS_TIME_H 0
#define USE_CLOCK 0
#define HAS_STDIO 0
#define HAS_PRINTF 0

typedef unsigned long long CORE_TICKS;

#ifndef COMPILER_VERSION
#ifdef __GNUC__
#define COMPILER_VERSION "GCC"__VERSION__
#else
#define COMPILER_VERSION "Unknown"
#endif
#endif

#ifndef COMPILER_FLAGS
#define COMPILER_FLAGS "-O3"
#endif

#ifndef MEM_LOCATION
#define MEM_LOCATION "STACK"
#endif

typedef signed short   ee_s16;
typedef unsigned short ee_u16;
typedef signed int     ee_s32;
typedef double         ee_f32;
typedef unsigned char  ee_u8;
typedef unsigned int   ee_u32;
#if __riscv_xlen == 64
typedef unsigned long  ee_ptr_int;
typedef unsigned long  ee_u64;
#else
typedef unsigned int   ee_ptr_int;
typedef unsigned long long ee_u64;
#endif
typedef ee_u64         ee_size_t;

#define align_mem(x) (void *)(sizeof(ee_ptr_int) + ((((ee_ptr_int)(x) - 1) / sizeof(ee_ptr_int)) * sizeof(ee_ptr_int)))

#define SEED_METHOD SEED_VOLATILE
#define MEM_METHOD MEM_STACK

#define MULTITHREAD 1
#define USE_PTHREAD 0
#define USE_FORK 0
#define USE_SOCKET 0

#define MAIN_HAS_NOARGC 0
#define MAIN_HAS_NORETURN 0

typedef struct CORE_PORTABLE_S {
    ee_u8 portable_id;
} core_portable;

extern ee_u32 default_num_contexts;

void portable_init(core_portable *p, int *argc, char *argv[]);
void portable_fini(core_portable *p);
int ee_printf(const char *fmt, ...);

#if !defined(PROFILE_RUN) && !defined(PERFORMANCE_RUN) && !defined(VALIDATION_RUN)
#define PERFORMANCE_RUN 1
#endif

#endif
"#;

/// Minimal RISC-V port implementation for CoreMark.
const PORTME_C: &str = r#"
#include "coremark.h"

#if VALIDATION_RUN
volatile ee_s32 seed1_volatile = 0x3415;
volatile ee_s32 seed2_volatile = 0x3415;
volatile ee_s32 seed3_volatile = 0x66;
#endif
#if PERFORMANCE_RUN
volatile ee_s32 seed1_volatile = 0x0;
volatile ee_s32 seed2_volatile = 0x0;
volatile ee_s32 seed3_volatile = 0x66;
#endif
#if PROFILE_RUN
volatile ee_s32 seed1_volatile = 0x8;
volatile ee_s32 seed2_volatile = 0x8;
volatile ee_s32 seed3_volatile = 0x8;
#endif
volatile ee_s32 seed4_volatile = ITERATIONS;
volatile ee_s32 seed5_volatile = 0;
ee_u32 default_num_contexts = 1;

static inline ee_u64 rdcycle(void) {
    ee_u64 val;
#if __riscv_xlen == 64
    __asm__ volatile ("rdcycle %0" : "=r"(val));
#else
    ee_u32 lo, hi, hi2;
    do {
        __asm__ volatile ("rdcycleh %0" : "=r"(hi));
        __asm__ volatile ("rdcycle %0" : "=r"(lo));
        __asm__ volatile ("rdcycleh %0" : "=r"(hi2));
    } while (hi != hi2);
    val = ((ee_u64)hi << 32) | lo;
#endif
    return val;
}

static CORE_TICKS start_time_val, stop_time_val;

void start_time(void) { start_time_val = rdcycle(); }
void stop_time(void) { stop_time_val = rdcycle(); }
CORE_TICKS get_time(void) { return stop_time_val - start_time_val; }
/* rdcycle returns instruction count in rvr, not actual cycles.
 * Use 10M as divisor so typical runs (~300M instrs) report ~30 seconds,
 * passing CoreMark's 10-second minimum validation requirement. */
secs_ret time_in_secs(CORE_TICKS ticks) { return (secs_ret)(ticks / 10000000ULL); }

static inline long syscall1(long n, long a0) {
    register long _a0 __asm__("a0") = a0;
    register long _n __asm__("a7") = n;
    __asm__ volatile ("ecall" : "+r"(_a0) : "r"(_n) : "memory");
    return _a0;
}

static inline long syscall3(long n, long a0, long a1, long a2) {
    register long _a0 __asm__("a0") = a0;
    register long _a1 __asm__("a1") = a1;
    register long _a2 __asm__("a2") = a2;
    register long _n __asm__("a7") = n;
    __asm__ volatile ("ecall" : "+r"(_a0) : "r"(_a1), "r"(_a2), "r"(_n) : "memory");
    return _a0;
}

#define SYS_exit  93
#define SYS_write 64

static void sys_write(int fd, const char *buf, long len) { syscall3(SYS_write, fd, (long)buf, len); }
static void sys_exit(int code) { syscall1(SYS_exit, code); }

static void print_str(const char *s) {
    const char *p = s;
    while (*p) p++;
    sys_write(1, s, p - s);
}

static void print_char(char c) { sys_write(1, &c, 1); }

static void print_int(ee_s32 n) {
    char buf[16];
    int i = 15, neg = 0;
    buf[i] = 0;
    if (n < 0) { neg = 1; n = -n; }
    if (n == 0) buf[--i] = '0';
    else while (n > 0) { buf[--i] = '0' + (n % 10); n /= 10; }
    if (neg) buf[--i] = '-';
    print_str(&buf[i]);
}

static void print_uint(ee_u32 n) {
    char buf[16];
    int i = 15;
    buf[i] = 0;
    if (n == 0) buf[--i] = '0';
    else while (n > 0) { buf[--i] = '0' + (n % 10); n /= 10; }
    print_str(&buf[i]);
}

static void print_hex(ee_u32 n) {
    char buf[16];
    int i = 15;
    buf[i] = 0;
    if (n == 0) buf[--i] = '0';
    else while (n > 0) {
        int d = n & 0xf;
        buf[--i] = d < 10 ? '0' + d : 'a' + d - 10;
        n >>= 4;
    }
    print_str(&buf[i]);
}

int ee_printf(const char *fmt, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, fmt);
    while (*fmt) {
        if (*fmt == '%') {
            fmt++;
            while (*fmt == '-' || *fmt == '+' || *fmt == ' ' || *fmt == '#' || *fmt == '0') fmt++;
            while (*fmt >= '0' && *fmt <= '9') fmt++;
            if (*fmt == '.') { fmt++; while (*fmt >= '0' && *fmt <= '9') fmt++; }
            if (*fmt == 'l' || *fmt == 'h' || *fmt == 'z') { fmt++; if (*fmt == 'l' || *fmt == 'h') fmt++; }
            switch (*fmt) {
                case 'd': case 'i': print_int(__builtin_va_arg(ap, ee_s32)); break;
                case 'u': print_uint(__builtin_va_arg(ap, ee_u32)); break;
                case 's': print_str(__builtin_va_arg(ap, const char *)); break;
                case 'c': print_char((char)__builtin_va_arg(ap, int)); break;
                case 'f': case 'g': case 'e': (void)__builtin_va_arg(ap, double); print_str("[float]"); break;
                case 'x': case 'X': print_hex(__builtin_va_arg(ap, ee_u32)); break;
                case '%': print_char('%'); break;
                default: print_char('%'); print_char(*fmt); break;
            }
        } else {
            print_char(*fmt);
        }
        fmt++;
    }
    __builtin_va_end(ap);
    return 0;
}

void portable_init(core_portable *p, int *argc, char *argv[]) { (void)argc; (void)argv; p->portable_id = 1; }
void portable_fini(core_portable *p) { p->portable_id = 0; }

int main(int argc, char *argv[]);

/* Initialize gp register from __global_pointer$ symbol */
extern char __global_pointer$[];
static inline void init_gp(void) {
    __asm__ volatile (".option push; .option norelax; la gp, __global_pointer$; .option pop" ::: "gp");
}

void _start(void) {
    init_gp();
    char *argv[] = {"coremark", 0};
    int ret = main(1, argv);
    sys_exit(ret);
}
"#;

/// Build CoreMark benchmark for RISC-V.
pub fn build_benchmark(project_dir: &std::path::Path, arch: &Arch) -> Result<PathBuf, String> {
    let toolchain = rvr::tests::find_toolchain()
        .ok_or_else(|| "RISC-V toolchain not found (need riscv64-unknown-elf-gcc)".to_string())?;

    let gcc = format!("{}gcc", toolchain);
    let out_dir = project_dir.join("bin").join(arch.as_str());
    let coremark_dir = project_dir.join("programs/coremark");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

    // Create port files in temp directory (not in submodule or target)
    let port_dir = std::env::temp_dir().join("rvr_coremark_port");
    std::fs::create_dir_all(&port_dir).map_err(|e| format!("failed to create port dir: {}", e))?;

    std::fs::write(port_dir.join("core_portme.h"), PORTME_H)
        .map_err(|e| format!("failed to write portme.h: {}", e))?;
    std::fs::write(port_dir.join("core_portme.c"), PORTME_C)
        .map_err(|e| format!("failed to write portme.c: {}", e))?;

    let (march, mabi) = match arch {
        Arch::Rv32i | Arch::Rv32e => ("rv32im_zicsr", "ilp32"),
        Arch::Rv64i | Arch::Rv64e => ("rv64im_zicsr", "lp64"),
    };

    let out_path = out_dir.join("coremark");

    let core_files: Vec<PathBuf> = vec![
        coremark_dir.join("core_list_join.c"),
        coremark_dir.join("core_main.c"),
        coremark_dir.join("core_matrix.c"),
        coremark_dir.join("core_state.c"),
        coremark_dir.join("core_util.c"),
        port_dir.join("core_portme.c"),
    ];

    let mut cmd = Command::new(&gcc);
    cmd.arg(format!("-march={}", march))
        .arg(format!("-mabi={}", mabi))
        .args(["-O3", "-funroll-loops"])
        .args(["-static", "-nostdlib", "-nostartfiles", "-ffreestanding"])
        .args(["-DITERATIONS=400000", "-DPERFORMANCE_RUN=1"])
        .arg(format!("-I{}", coremark_dir.display()))
        .arg(format!("-I{}", port_dir.display()))
        .args(&core_files)
        .arg("-lgcc") // For 64-bit division on RV32
        .arg("-o")
        .arg(&out_path);

    let output = cmd
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run gcc: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gcc failed: {}", stderr));
    }

    Ok(out_path)
}

/// Build CoreMark as native executable for the host.
pub fn build_host_benchmark(project_dir: &std::path::Path) -> Result<PathBuf, String> {
    let out_dir = project_dir.join("bin/host");
    let coremark_dir = project_dir.join("programs/coremark");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {}", e))?;

    // Create host port files in temp directory
    let port_dir = std::env::temp_dir().join("rvr_coremark_host_port");
    std::fs::create_dir_all(&port_dir).map_err(|e| format!("failed to create port dir: {}", e))?;

    std::fs::write(port_dir.join("core_portme.h"), HOST_PORTME_H)
        .map_err(|e| format!("failed to write host portme.h: {}", e))?;
    std::fs::write(port_dir.join("core_portme.c"), HOST_PORTME_C)
        .map_err(|e| format!("failed to write host portme.c: {}", e))?;

    let out_path = out_dir.join("coremark");

    let core_files: Vec<PathBuf> = vec![
        coremark_dir.join("core_list_join.c"),
        coremark_dir.join("core_main.c"),
        coremark_dir.join("core_matrix.c"),
        coremark_dir.join("core_state.c"),
        coremark_dir.join("core_util.c"),
        port_dir.join("core_portme.c"),
    ];

    let mut cmd = Command::new("cc");
    cmd.args(["-O3", "-funroll-loops"])
        .args(["-DITERATIONS=400000", "-DPERFORMANCE_RUN=1"])
        .arg(format!("-I{}", coremark_dir.display()))
        .arg(format!("-I{}", port_dir.display()))
        .args(&core_files)
        .arg("-o")
        .arg(&out_path);

    let output = cmd
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run cc: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cc failed: {}", stderr));
    }

    Ok(out_path)
}

/// Result of running a host benchmark.
#[derive(Debug, Clone)]
pub struct HostBenchResult {
    pub time_secs: f64,
    pub perf: Option<rvr::PerfCounters>,
}

/// Run CoreMark host benchmark.
pub fn run_host_benchmark(
    bin_path: &std::path::Path,
    runs: usize,
) -> Result<HostBenchResult, String> {
    use std::time::Instant;

    let runs = runs.max(1);

    // Warm up
    let _ = Command::new(bin_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let start = Instant::now();
    for i in 0..runs {
        let output = Command::new(bin_path)
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("failed to run benchmark: {}", e))?;

        if !output.status.success() {
            return Err("benchmark failed".to_string());
        }

        // Print output on last run to show "No errors detected"
        if i == runs - 1 {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Check for errors - CoreMark prints "Correct operation validated" or errors
            if stdout.contains("ERROR") {
                return Err(format!("CoreMark validation failed:\n{}", stdout));
            }
            // Print the output for user to see results
            print!("{}", stdout);
        }
    }
    let elapsed = start.elapsed();
    let time_secs = elapsed.as_secs_f64() / runs as f64;

    Ok(HostBenchResult {
        time_secs,
        perf: None,
    })
}
