//! Preflight tracer header generation.

use rvr_ir::Xlen;

use super::super::signature::reg_type;

const PREFLIGHT_TEMPLATE: &str = r#"/* Preflight tracer - records execution for replay and proof generation.
 *
 * Storage layout:
 * - pc buffer: @RTYPE@* - stores each executed PC
 * - data buffer: uint8_t* - stores register/memory/csr accesses as raw bytes
 *   - reg read/write: @RBYTES@ bytes (register value)
 *   - mem read/write: @RBYTES@ bytes (addr) + N bytes (value, where N = access size)
 *   - csr read/write: @RBYTES@ bytes (csr value)
 */
#pragma once

#include "rv_constants.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

typedef struct Tracer {{
    uint8_t* data;
    uint32_t data_idx;
    uint64_t data_count;
    @RTYPE@* pc;
    uint32_t pc_idx;
    uint64_t pc_count;
    uint64_t start_ns;
}} Tracer;

static inline uint64_t get_time_ns(void) {{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}}

static inline void format_size(char* buf, size_t buflen, uint64_t bytes) {{
    if (bytes >= (1ULL << 30)) {{
        snprintf(buf, buflen, "%.2fGB", (double)bytes / (double)(1ULL << 30));
    }} else if (bytes >= (1ULL << 20)) {{
        snprintf(buf, buflen, "%.2fMB", (double)bytes / (double)(1ULL << 20));
    }} else if (bytes >= (1ULL << 10)) {{
        snprintf(buf, buflen, "%.2fKB", (double)bytes / (double)(1ULL << 10));
    }} else {{
        snprintf(buf, buflen, "%lluB", (unsigned long long)bytes);
    }}
}}

static inline void format_rate(char* buf, size_t buflen, double rate) {{
    if (rate >= 1e9) {{
        snprintf(buf, buflen, "%.2fGB/s", rate / 1e9);
    }} else if (rate >= 1e6) {{
        snprintf(buf, buflen, "%.2fMB/s", rate / 1e6);
    }} else if (rate >= 1e3) {{
        snprintf(buf, buflen, "%.2fKB/s", rate / 1e3);
    }} else {{
        snprintf(buf, buflen, "%.2fB/s", rate);
    }}
}}

static inline void print_trace_stats(uint64_t data_bytes, uint64_t pc_count, uint64_t elapsed_ns) {{
    uint64_t pc_bytes = pc_count * @RBYTES@;
    uint64_t total_bytes = data_bytes + pc_bytes;

    char pc_str[32], data_str[32], total_str[32];
    format_size(pc_str, sizeof(pc_str), pc_bytes);
    format_size(data_str, sizeof(data_str), data_bytes);
    format_size(total_str, sizeof(total_str), total_bytes);

    printf("\033[32mINFO \033[0m trace:      %s PCs + %s data = %s\n", pc_str, data_str, total_str);

    if (elapsed_ns > 0) {{
        double throughput = (double)total_bytes / ((double)elapsed_ns / 1e9);
        char rate_str[32];
        format_rate(rate_str, sizeof(rate_str), throughput);
        printf("\033[32mINFO \033[0m throughput: %s\n", rate_str);
    }}
}}

static inline void trace_init(Tracer* t) {{
    t->data_idx = 0;
    t->pc_idx = 0;
    t->start_ns = get_time_ns();
}}

static inline void trace_fini(Tracer* t) {{
    uint64_t elapsed_ns = get_time_ns() - t->start_ns;
    t->data_count += t->data_idx;
    t->pc_count += t->pc_idx;
    print_trace_stats(t->data_count, t->pc_count, elapsed_ns);
}}

/* Block entry */
static inline void trace_block(Tracer* t, @RTYPE@ pc) {{
}}

/* Instruction dispatch */
static inline void trace_pc(Tracer* t, @RTYPE@ pc, uint16_t op) {{
    t->pc[t->pc_idx++] = pc;
}}

/* Register access */
static inline void trace_reg_read(Tracer* t, @RTYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ value) {{
    memcpy(&t->data[t->data_idx], &value, @RBYTES@);
    t->data_idx += @RBYTES@;
}}

static inline void trace_reg_write(Tracer* t, @RTYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ value) {{
    memcpy(&t->data[t->data_idx], &value, @RBYTES@);
    t->data_idx += @RBYTES@;
}}

/* Memory reads - record addr + value bytes */
static inline void trace_mem_read_byte(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint8_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    t->data[t->data_idx++] = value;
}}

static inline void trace_mem_read_halfword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint16_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    memcpy(&t->data[t->data_idx], &value, 2);
    t->data_idx += 2;
}}

static inline void trace_mem_read_word(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint32_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    memcpy(&t->data[t->data_idx], &value, 4);
    t->data_idx += 4;
}}

static inline void trace_mem_read_dword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint64_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    memcpy(&t->data[t->data_idx], &value, 8);
    t->data_idx += 8;
}}

/* Memory writes - record addr + value bytes */
static inline void trace_mem_write_byte(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint8_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    t->data[t->data_idx++] = value;
}}

static inline void trace_mem_write_halfword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint16_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    memcpy(&t->data[t->data_idx], &value, 2);
    t->data_idx += 2;
}}

static inline void trace_mem_write_word(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint32_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    memcpy(&t->data[t->data_idx], &value, 4);
    t->data_idx += 4;
}}

static inline void trace_mem_write_dword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint64_t value) {{
    memcpy(&t->data[t->data_idx], &addr, @RBYTES@);
    t->data_idx += @RBYTES@;
    memcpy(&t->data[t->data_idx], &value, 8);
    t->data_idx += 8;
}}

/* Control flow */
static inline void trace_branch_taken(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ target) {{
}}

static inline void trace_branch_not_taken(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ target) {{
}}

/* CSR access */
static inline void trace_csr_read(Tracer* t, @RTYPE@ pc, uint16_t op, uint16_t csr, @RTYPE@ value) {{
    memcpy(&t->data[t->data_idx], &value, @RBYTES@);
    t->data_idx += @RBYTES@;
}}

static inline void trace_csr_write(Tracer* t, @RTYPE@ pc, uint16_t op, uint16_t csr, @RTYPE@ value) {{
    memcpy(&t->data[t->data_idx], &value, @RBYTES@);
    t->data_idx += @RBYTES@;
}}
"#;

pub fn gen_tracer_preflight<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    let rbytes = X::REG_BYTES;
    let rbytes_str = rbytes.to_string();

    super::expand_template(
        PREFLIGHT_TEMPLATE,
        &[("@RTYPE@", rtype), ("@RBYTES@", rbytes_str.as_str())],
    )
}
