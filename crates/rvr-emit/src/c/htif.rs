//! HTIF (Host-Target Interface) code generation for riscv-tests.
//!
//! Generates C code to handle HTIF protocol used by riscv-tests for:
//! - Exit signaling (exit code via tohost)
//! - Syscall handling (write to stdout)

use rvr_ir::Xlen;

// HTIF constants matching riscv-tests expectations
// tohost at 0x80001000, fromhost at 0x80001008 (sequential in .tohost section)
const TOHOST_ADDR: u64 = 0x80001000;
const FROMHOST_ADDR: u64 = 0x80001008;
const SYS_WRITE: u64 = 64;
const STDOUT_FD: u64 = 1;

/// Configuration for HTIF code generation.
pub struct HtifConfig {
    pub base_name: String,
    pub enabled: bool,
    pub verbose: bool,
}

impl HtifConfig {
    pub fn new(base_name: &str, enabled: bool) -> Self {
        Self {
            base_name: base_name.to_string(),
            enabled,
            verbose: false,
        }
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

fn addr_type<X: Xlen>() -> &'static str {
    if X::VALUE == 64 {
        "uint64_t"
    } else {
        "uint32_t"
    }
}

/// Generate HTIF header file content.
pub fn gen_htif_header<X: Xlen>(cfg: &HtifConfig) -> String {
    if !cfg.enabled {
        return r#"#pragma once
/* HTIF disabled */
"#
        .to_string();
    }

    let addr_type = addr_type::<X>();
    format!(
        r#"#pragma once

#include <stdint.h>

/* Forward declaration to avoid circular includes */
typedef struct RvState RvState;

/* HTIF constants (C23 constexpr) */
constexpr uint64_t HTIF_TOHOST_ADDR   = {tohost:#x};
constexpr uint64_t HTIF_FROMHOST_ADDR = {fromhost:#x};
constexpr uint32_t HTIF_FIELD_SIZE    = 8;  /* 64-bit fields */
constexpr uint64_t HTIF_SYS_WRITE     = {sys_write};
constexpr uint64_t HTIF_STDOUT_FD     = {stdout_fd};

/* HTIF handler - called when writing to TOHOST address */
__attribute__((preserve_most)) void handle_tohost_write(RvState* restrict state, {addr_type} value);
"#,
        tohost = TOHOST_ADDR,
        fromhost = FROMHOST_ADDR,
        sys_write = SYS_WRITE,
        stdout_fd = STDOUT_FD,
        addr_type = addr_type,
    )
}

/// Generate HTIF source file content.
pub fn gen_htif_source<X: Xlen>(cfg: &HtifConfig) -> String {
    if !cfg.enabled {
        return format!(
            r#"#include "{}_htif.h"
/* HTIF disabled */
"#,
            cfg.base_name
        );
    }

    let addr_type = addr_type::<X>();

    let print_code = if cfg.verbose {
        format!(
            r#"for ({addr_type} i = 0; i < length; ++i) {{
            fputc((int)(state->memory[buffer_addr + i] & 0xFFu), stdout);
        }}
        fflush(stdout);"#,
            addr_type = addr_type
        )
    } else {
        String::new()
    };

    format!(
        r#"#include "{base_name}.h"
#include "{base_name}_htif.h"

__attribute__((hot, pure, nonnull))
static inline uint64_t read_memory_dword(RvState* restrict state, {addr_type} addr) {{
    assert(addr <= RV_MEMORY_SIZE - 8 && "Memory dword read out of bounds");
    uint64_t val;
    memcpy(&val, &state->memory[addr], sizeof(val));
    return val;
}}

__attribute__((cold, nonnull, preserve_most))
void handle_tohost_write(RvState* restrict state, {addr_type} value) {{
    if (unlikely(value == 0)) return;

    /* HTIF exit encoding: LSB=1 means exit, exit_code = value >> 1 */
    if ((value & 1u) == 1u) {{
        state->exit_code = (uint8_t)((value >> 1) & 0xFFu);
        state->has_exited = true;
        return;
    }}

    /* HTIF syscall: 64-bit fields at offsets 0, 8, 16, 24 */
    uint64_t syscall_num = read_memory_dword(state, value);
    uint64_t arg0 = read_memory_dword(state, value + HTIF_FIELD_SIZE);
    uint64_t arg1 = read_memory_dword(state, value + HTIF_FIELD_SIZE * 2);
    uint64_t arg2 = read_memory_dword(state, value + HTIF_FIELD_SIZE * 3);

    if (likely(syscall_num == HTIF_SYS_WRITE && arg0 == HTIF_STDOUT_FD)) {{
        {addr_type} buffer_addr = ({addr_type})arg1;
        {addr_type} length = ({addr_type})arg2;

        {print_code}

        /* Write return value and signal completion */
        memcpy(&state->memory[value], &length, sizeof({addr_type}));
        {addr_type} one = 1;
        memcpy(&state->memory[HTIF_FROMHOST_ADDR], &one, sizeof(one));
    }} else {{
        fprintf(stderr, "Unsupported syscall: %llu\\n", (unsigned long long)syscall_num);
        state->exit_code = 1;
        state->has_exited = true;
    }}
}}
"#,
        base_name = cfg.base_name,
        addr_type = addr_type,
        print_code = print_code,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_gen_htif_header_enabled() {
        let cfg = HtifConfig::new("test", true);
        let header = gen_htif_header::<Rv64>(&cfg);
        assert!(header.contains("HTIF_TOHOST_ADDR"));
        assert!(header.contains("handle_tohost_write"));
        assert!(header.contains("uint64_t value"));
    }

    #[test]
    fn test_gen_htif_header_disabled() {
        let cfg = HtifConfig::new("test", false);
        let header = gen_htif_header::<Rv64>(&cfg);
        assert!(header.contains("HTIF disabled"));
        assert!(!header.contains("HTIF_TOHOST_ADDR"));
    }

    #[test]
    fn test_gen_htif_source_enabled() {
        let cfg = HtifConfig::new("test", true);
        let source = gen_htif_source::<Rv64>(&cfg);
        assert!(source.contains("handle_tohost_write"));
        assert!(source.contains("read_memory_dword"));
        assert!(source.contains("HTIF_SYS_WRITE"));
    }

    #[test]
    fn test_gen_htif_source_disabled() {
        let cfg = HtifConfig::new("test", false);
        let source = gen_htif_source::<Rv64>(&cfg);
        assert!(source.contains("HTIF disabled"));
    }
}
