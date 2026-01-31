//! Debug tracer header generation.

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_debug<X: Xlen>() -> String {
    let rtype = reg_type::<X>();

    format!(
        r#"/* Debug tracer - writes PCs to /tmp/rvr_trace.txt for comparison. */
#pragma once

#include <stdint.h>
#include <stdio.h>

typedef struct Tracer {{
    FILE* fp;
    uint64_t pcs;
}} Tracer;

static inline void trace_init(Tracer* t) {{
    if (!t) return;
    t->fp = fopen("/tmp/rvr_trace.txt", "w");
    t->pcs = 0;
}}

static inline void trace_fini(Tracer* t) {{
    if (!t) return;
    fprintf(stderr, "DEBUG: traced %llu PCs\n", (unsigned long long)t->pcs);
    if (t->fp) {{
        fclose(t->fp);
        t->fp = NULL;
    }}
}}

/* Block entry */
static inline void trace_block(Tracer* t, {rtype} pc) {{
    (void)t;
    (void)pc;
}}

/* Instruction dispatch */
static inline void trace_pc(Tracer* t, {rtype} pc, uint16_t op) {{
    (void)op;
    t->pcs++;
    if (t->fp && t->pcs <= 100000) {{
        fprintf(t->fp, "%x\n", (unsigned)pc);
    }}
}}

static inline void trace_opcode(Tracer* t, {rtype} pc, uint16_t op, uint32_t opcode) {{
    (void)t; (void)pc; (void)op; (void)opcode;
}}

/* Register access */
static inline void trace_reg_read(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}}
static inline void trace_reg_write(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}}

/* Memory reads */
static inline void trace_mem_read_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_read_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_read_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_read_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}

/* Memory writes */
static inline void trace_mem_write_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_write_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_write_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_write_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}

/* Control flow */
static inline void trace_branch_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}
static inline void trace_branch_not_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

/* CSR access */
static inline void trace_csr_read(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}
static inline void trace_csr_write(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}
"#,
        rtype = rtype
    )
}
