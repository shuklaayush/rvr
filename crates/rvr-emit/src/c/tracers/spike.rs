//! Spike-compatible tracer header generation.
//!
//! Generates traces in Spike's `--log-commits` format for differential testing.
//! Format: `core   0: 3 0x<PC> (0x<OPCODE>) [x<RD> 0x<VALUE>] [mem 0x<ADDR>]`

use rvr_ir::Xlen;

use super::super::signature::reg_type;

const SPIKE_TEMPLATE: &str = r#"/* Spike-compatible tracer - outputs in Spike's --log-commits format.
 *
 * Format: core   0: 3 0x<PC> (0x<OPCODE>) [x<RD> 0x<VALUE>] [mem 0x<ADDR>]
 *
 * Set RVR_TRACE_FILE environment variable to specify output file.
 * Default: /tmp/rvr_trace.log
 */
#pragma once

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Tracer {{
    FILE* fp;
    @RTYPE@ pending_pc;
    uint32_t pending_opcode;
    uint8_t pending_rd;
    @RTYPE@ pending_rd_value;
    @RTYPE@ pending_mem_addr;
    uint8_t has_pending;
    uint8_t has_rd;
    uint8_t has_mem;
    uint64_t count;
}} Tracer;

static inline void trace_flush(Tracer* t) {{
    if (!t->has_pending || !t->fp) return;

    fprintf(t->fp, "core   0: 3 0x@PC_FMT@ (0x%08x)",
            @PC_CAST@t->pending_pc, (unsigned)t->pending_opcode);

    if (t->has_rd) {{
        fprintf(t->fp, " x%u 0x@VAL_FMT@",
                (unsigned)t->pending_rd, @VAL_CAST@t->pending_rd_value);
    }}

    if (t->has_mem) {{
        fprintf(t->fp, " mem 0x@VAL_FMT@", @VAL_CAST@t->pending_mem_addr);
    }}

    fprintf(t->fp, "\n");
    t->has_pending = 0;
    t->has_rd = 0;
    t->has_mem = 0;
}}

static inline void trace_init(Tracer* t) {{
    if (!t) return;
    const char* path = getenv("RVR_TRACE_FILE");
    if (!path) path = "/tmp/rvr_trace.log";
    t->fp = fopen(path, "w");
    t->has_pending = 0;
    t->has_rd = 0;
    t->has_mem = 0;
    t->count = 0;
}}

static inline void trace_fini(Tracer* t) {{
    if (!t) return;
    trace_flush(t);
    if (t->fp) {{
        fclose(t->fp);
        t->fp = NULL;
    }}
    fprintf(stderr, "spike-trace: %llu instructions traced\n", (unsigned long long)t->count);
}}

/* Block entry */
static inline void trace_block(Tracer* t, @RTYPE@ pc) {{
    (void)t; (void)pc;
}}

/* Instruction dispatch - flush previous, start new */
static inline void trace_pc(Tracer* t, @RTYPE@ pc, uint16_t op) {{
    (void)op;
    trace_flush(t);
    t->pending_pc = pc;
    t->pending_opcode = 0;
    t->has_pending = 1;
    t->has_rd = 0;
    t->has_mem = 0;
    t->count++;
}}

/* Opcode details */
static inline void trace_opcode(Tracer* t, @RTYPE@ pc, uint16_t op, uint32_t opcode) {{
    (void)pc; (void)op;
    t->pending_opcode = opcode;
}}

/* Register access */
static inline void trace_reg_read(Tracer* t, @RTYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ value) {{
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}}

static inline void trace_reg_write(Tracer* t, @RTYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ value) {{
    (void)pc; (void)op;
    if (reg != 0) {{
        t->pending_rd = reg;
        t->pending_rd_value = value;
        t->has_rd = 1;
    }}
}}

/* Memory reads */
static inline void trace_mem_read_byte(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint8_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

static inline void trace_mem_read_halfword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint16_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

static inline void trace_mem_read_word(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint32_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

static inline void trace_mem_read_dword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint64_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

/* Memory writes */
static inline void trace_mem_write_byte(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint8_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

static inline void trace_mem_write_halfword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint16_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

static inline void trace_mem_write_word(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint32_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

static inline void trace_mem_write_dword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint64_t value) {{
    (void)pc; (void)op; (void)value;
    t->pending_mem_addr = addr;
    t->has_mem = 1;
}}

/* Control flow */
static inline void trace_branch_taken(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

static inline void trace_branch_not_taken(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

/* CSR access */
static inline void trace_csr_read(Tracer* t, @RTYPE@ pc, uint16_t op, uint16_t csr, @RTYPE@ value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}

static inline void trace_csr_write(Tracer* t, @RTYPE@ pc, uint16_t op, uint16_t csr, @RTYPE@ value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}
"#;

pub fn gen_tracer_spike<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    let is_rv64 = X::VALUE == 64;
    let pc_fmt = if is_rv64 { "%016lx" } else { "%08x" };
    let val_fmt = if is_rv64 { "%016lx" } else { "%08x" };
    let pc_cast = if is_rv64 {
        "(unsigned long)"
    } else {
        "(unsigned)"
    };
    let val_cast = if is_rv64 {
        "(unsigned long)"
    } else {
        "(unsigned)"
    };

    super::expand_template(
        SPIKE_TEMPLATE,
        &[
            ("@RTYPE@", rtype),
            ("@PC_FMT@", pc_fmt),
            ("@VAL_FMT@", val_fmt),
            ("@PC_CAST@", pc_cast),
            ("@VAL_CAST@", val_cast),
        ],
    )
}
