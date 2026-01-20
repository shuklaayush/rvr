//! Syscall runtime code generation.
//!
//! Generates syscalls.c containing minimal Linux syscall helpers used by
//! the recompiled programs.

use std::fmt::Write;

use rvr_ir::Xlen;

use crate::signature::reg_type;

/// Syscall runtime generation configuration.
pub struct SyscallsConfig {
    /// Base name for output files.
    pub base_name: String,
}

impl SyscallsConfig {
    /// Create a syscall config.
    pub fn new(base_name: impl Into<String>) -> Self {
        Self {
            base_name: base_name.into(),
        }
    }
}

/// Generate syscalls.c source.
pub fn gen_syscalls_source<X: Xlen>(cfg: &SyscallsConfig) -> String {
    let mut s = String::new();
    let rtype = reg_type::<X>();

    writeln!(
        s,
        r#"#include \"{}.h\"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* Minimal Linux syscall helpers for recompiled guests */

typedef {rtype} reg_t;

static inline reg_t align_up(reg_t value, reg_t alignment) {{
    return (value + alignment - 1) & ~(alignment - 1);
}}

static inline uint8_t* guest_ptr(RvState* state, reg_t addr) {{
    return state->memory + phys_addr(addr);
}}

reg_t rv_sys_write(RvState* state, reg_t fd, reg_t buf, reg_t count) {{
    if (fd == 1 || fd == 2) {{
        FILE* out = (fd == 1) ? stdout : stderr;
        uint8_t* ptr = guest_ptr(state, buf);
        size_t n = (size_t)count;
        size_t written = fwrite(ptr, 1, n, out);
        fflush(out);
        return (reg_t)written;
    }}
    return (reg_t)-1;
}}

reg_t rv_sys_read(RvState* state, reg_t fd, reg_t buf, reg_t count) {{
    if (fd == 0) {{
        uint8_t* ptr = guest_ptr(state, buf);
        size_t n = (size_t)count;
        size_t read = fread(ptr, 1, n, stdin);
        return (reg_t)read;
    }}
    return (reg_t)-1;
}}

reg_t rv_sys_brk(RvState* state, reg_t addr) {{
    if (addr == 0) {{
        return state->brk;
    }}
    if (addr >= state->start_brk && (uint64_t)addr < RV_MEMORY_SIZE) {{
        state->brk = addr;
        return addr;
    }}
    return state->brk;
}}

reg_t rv_sys_mmap(
    RvState* state,
    reg_t addr,
    reg_t len,
    reg_t prot,
    reg_t flags,
    reg_t fd,
    reg_t off
) {{
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

    if ((uint64_t)new_brk < RV_MEMORY_SIZE) {{
        state->brk = new_brk;
        memset(guest_ptr(state, aligned_brk), 0, (size_t)aligned_len);
        return aligned_brk;
    }}

    return (reg_t)-12; /* ENOMEM */
}}

reg_t rv_sys_fstat(RvState* state, reg_t fd, reg_t statbuf) {{
    (void)state;
    (void)statbuf;
    if (fd == 1 || fd == 2) {{
        return 0;
    }}
    return (reg_t)-1;
}}

reg_t rv_sys_getrandom(RvState* state, reg_t buf, reg_t len, reg_t flags) {{
    (void)flags;
    static uint64_t rng_state = 0x123456789abcdef0ULL;
    uint8_t* ptr = guest_ptr(state, buf);
    size_t n = (size_t)len;
    for (size_t i = 0; i < n; i++) {{
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        ptr[i] = (uint8_t)(rng_state & 0xFF);
    }}
    return (reg_t)len;
}}

reg_t rv_sys_clock_gettime(RvState* state, reg_t clk_id, reg_t tp) {{
    (void)clk_id;
    struct timespec ts;
#if defined(CLOCK_REALTIME)
    clock_gettime(CLOCK_REALTIME, &ts);
#else
    timespec_get(&ts, TIME_UTC);
#endif
    uint64_t secs = (uint64_t)ts.tv_sec;
    uint64_t nsecs = (uint64_t)ts.tv_nsec;
    wr_mem_u64(state->memory, tp, 0, secs);
    wr_mem_u64(state->memory, tp, 8, nsecs);
    return 0;
}}
"#,
        cfg.base_name,
        rtype = rtype
    )
    .unwrap();

    s
}
