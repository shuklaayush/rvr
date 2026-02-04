//! Syscall runtime code generation.
//!
//! Generates syscalls.c containing minimal Linux syscall helpers used by
//! the recompiled programs.

use rvr_ir::Xlen;

use super::signature::{MEMORY_FIXED_REF, reg_type};

const SYSCALLS_BODY: &str = r"
reg_t rv_sys_write(RvState* restrict state, reg_t fd, reg_t buf, reg_t count) {
    if (fd == 1 || fd == 2) {
        FILE* out = (fd == 1) ? stdout : stderr;
        uint8_t* ptr = guest_ptr(state, buf);
        size_t n = (size_t)count;
        size_t written = fwrite(ptr, 1, n, out);
        fflush(out);
        return (reg_t)written;
    }
    return (reg_t)-1;
}

reg_t rv_sys_read(RvState* restrict state, reg_t fd, reg_t buf, reg_t count) {
    if (fd == 0) {
        uint8_t* ptr = guest_ptr(state, buf);
        size_t n = (size_t)count;
        size_t read = fread(ptr, 1, n, stdin);
        return (reg_t)read;
    }
    return (reg_t)-1;
}

reg_t rv_sys_brk(RvState* restrict state, reg_t addr) {
    if (addr == 0) {
        return state->brk;
    }
    if (addr >= state->start_brk && (uint64_t)addr < RV_MEMORY_SIZE) {
        state->brk = addr;
        return addr;
    }
    return state->brk;
}

reg_t rv_sys_mmap(
    RvState* restrict state,
    reg_t addr,
    reg_t len,
    reg_t prot,
    reg_t flags,
    reg_t fd,
    reg_t off
) {
    (void)addr;
    (void)prot;
    (void)flags;
    (void)fd;
    (void)off;

    const reg_t page = 4096;
    reg_t current = state->brk;
    reg_t aligned_brk = align_up(current, page);
    reg_t aligned_len = align_up(len, page);
    reg_t new_brk = aligned_brk + aligned_len;

    if ((uint64_t)new_brk < RV_MEMORY_SIZE) {
        state->brk = new_brk;
        memset(guest_ptr(state, aligned_brk), 0, (size_t)aligned_len);
        return aligned_brk;
    }

    return (reg_t)-12; /* ENOMEM */
}

reg_t rv_sys_fstat(RvState* restrict state, reg_t fd, reg_t statbuf) {
    (void)state;
    (void)statbuf;
    if (fd == 1 || fd == 2) {
        return 0;
    }
    return (reg_t)-1;
}

reg_t rv_sys_getrandom(RvState* restrict state, reg_t buf, reg_t len, reg_t flags) {
    (void)flags;
    static uint64_t rng_state = 0x123456789abcdef0ULL;
    uint8_t* ptr = guest_ptr(state, buf);
    size_t n = (size_t)len;
    for (size_t i = 0; i < n; i++) {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        ptr[i] = (uint8_t)(rng_state & 0xFF);
    }
    return (reg_t)len;
}
";

fn push_syscalls_header(out: &mut String, base_name: &str, rtype: &str, guest_ptr_impl: &str) {
    use std::fmt::Write;

    out.push_str("#include \"");
    out.push_str(base_name);
    out.push_str(
        ".h\"\n#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <time.h>\n\nint clock_gettime(int clk_id, struct timespec* tp);\nstatic const int kClockRealtime = 0;\n\n/* Minimal Linux syscall helpers for recompiled guests */\n\ntypedef ",
    );
    out.push_str(rtype);
    out.push_str(
        " reg_t;\n\nstatic inline reg_t align_up(reg_t value, reg_t alignment) {\n    return (value + alignment - 1) & ~(alignment - 1);\n}\n\nstatic inline uint8_t* guest_ptr(RvState* restrict state, reg_t addr) {\n    (void)state;\n    ",
    );
    out.push_str(guest_ptr_impl);
    writeln!(out, "\n}}").expect("formatting guest_ptr");
}

fn push_syscalls_clock(out: &mut String, write_secs_stmt: &str, write_nsec_stmt: &str) {
    use std::fmt::Write;

    out.push_str(
        "\nreg_t rv_sys_clock_gettime(RvState* restrict state, reg_t clk_id, reg_t tp) {\n    (void)clk_id;\n    (void)state;\n    struct timespec ts;\n    clock_gettime(kClockRealtime, &ts);\n    uint64_t secs = (uint64_t)ts.tv_sec;\n    uint64_t nsecs = (uint64_t)ts.tv_nsec;\n    ",
    );
    writeln!(out, "{write_secs_stmt}").expect("formatting clock_gettime secs");
    writeln!(out, "{write_nsec_stmt}").expect("formatting clock_gettime nsecs");
    out.push_str("    return 0;\n}\n");
}

/// Syscall runtime generation configuration.
pub struct SyscallsConfig {
    /// Base name for output files.
    pub base_name: String,
    /// Whether fixed addresses are used.
    pub fixed_addresses: bool,
}

impl SyscallsConfig {
    /// Create a syscall config.
    pub fn new(base_name: impl Into<String>, fixed_addresses: bool) -> Self {
        Self {
            base_name: base_name.into(),
            fixed_addresses,
        }
    }
}

/// Generate syscalls.c source.
#[must_use]
pub fn gen_syscalls_source<X: Xlen>(cfg: &SyscallsConfig) -> String {
    let rtype = reg_type::<X>();

    // Memory access depends on fixed address mode
    let (mem_ref, mem_arg) = if cfg.fixed_addresses {
        (MEMORY_FIXED_REF, "")
    } else {
        ("state->memory", "state->memory, ")
    };
    let guest_ptr_impl = format!("return {mem_ref} + phys_addr(addr);");
    let write_mem_secs_stmt = format!("wr_mem_u64({mem_arg}tp, 0, secs);");
    let write_mem_nsec_stmt = format!("wr_mem_u64({mem_arg}tp, 8, nsecs);");

    let mut out = String::new();
    push_syscalls_header(&mut out, &cfg.base_name, rtype, &guest_ptr_impl);
    out.push_str(SYSCALLS_BODY);
    push_syscalls_clock(&mut out, &write_mem_secs_stmt, &write_mem_nsec_stmt);
    out
}
