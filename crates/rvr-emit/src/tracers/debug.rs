//! Debug tracer header generation.

use rvr_ir::Xlen;

use crate::signature::reg_type;

pub fn gen_tracer_debug<X: Xlen>() -> String {
    let rtype = reg_type::<X>();

    format!(
        r#"/* Debug tracer - detects repeated PCs and logs when stuck. */
#pragma once

#include <stdint.h>
#include <stdio.h>

#ifndef TRACER_DEBUG_REPEAT_LIMIT
#define TRACER_DEBUG_REPEAT_LIMIT 10000ULL
#endif

#ifndef TRACER_DEBUG_SAMPLE
#define TRACER_DEBUG_SAMPLE 1ULL
#endif

#ifndef TRACER_DEBUG_MAX_PRINT
#define TRACER_DEBUG_MAX_PRINT 50000ULL
#endif

typedef struct Tracer {{
    uint64_t pcs;
    uint64_t last_pc;
    uint64_t repeat_count;
    uint16_t last_op;
    uint8_t tripped;
}} Tracer;

static inline void trace_init(Tracer* t) {{
    (void)t;
}}

static inline void trace_fini(Tracer* t) {{
    if (!t) return;
    fprintf(stderr,
        "DEBUG: last pc=0x%llx op=%u (pcs=%llu)\n",
        (unsigned long long)t->last_pc,
        (unsigned)t->last_op,
        (unsigned long long)t->pcs);
}}

/* Block entry */
static inline void trace_block(Tracer* t, {rtype} pc) {{
    (void)t;
    (void)pc;
}}

/* Instruction dispatch */
static inline void trace_pc(Tracer* t, {rtype} pc, uint16_t op) {{
    t->pcs++;
    uint64_t prev_pc = t->last_pc;
    t->last_pc = (uint64_t)pc;
    t->last_op = op;
    if ((t->pcs % TRACER_DEBUG_SAMPLE) == 0 && t->pcs <= TRACER_DEBUG_MAX_PRINT) {{
        fprintf(stderr,
            "DEBUG: pc=0x%llx op=%u (pcs=%llu)\n",
            (unsigned long long)pc,
            (unsigned)op,
            (unsigned long long)t->pcs);
    }}
    if ((uint64_t)pc == prev_pc) {{
        t->repeat_count++;
        if (!t->tripped && t->repeat_count >= TRACER_DEBUG_REPEAT_LIMIT) {{
            t->tripped = 1;
            fprintf(stderr,
                "DEBUG: repeated pc=0x%llx op=%u (count=%llu)\n",
                (unsigned long long)t->last_pc,
                (unsigned)op,
                (unsigned long long)t->repeat_count);
        }}
    }} else {{
        t->repeat_count = 0;
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
