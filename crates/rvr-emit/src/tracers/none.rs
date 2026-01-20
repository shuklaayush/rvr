//! No-op tracer header generation.

use rvr_ir::Xlen;

use crate::signature::reg_type;

pub fn gen_tracer_none<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    format!(
        r#"/* No-op tracer - all functions compile away to nothing. */
#pragma once

#include <stdint.h>

typedef struct Tracer {{ char _unused; }} Tracer;

static inline void trace_init(Tracer* t) {{}}
static inline void trace_fini(Tracer* t) {{}}

/* Block entry */
static inline void trace_block(Tracer* t, {0} pc) {{}}

/* Instruction dispatch */
static inline void trace_pc(Tracer* t, {0} pc, uint16_t op) {{}}
static inline void trace_opcode(Tracer* t, {0} pc, uint16_t op, uint32_t opcode) {{}}

/* Register access */
static inline void trace_reg_read(Tracer* t, {0} pc, uint16_t op, uint8_t reg, {0} value) {{}}
static inline void trace_reg_write(Tracer* t, {0} pc, uint16_t op, uint8_t reg, {0} value) {{}}

/* Memory reads */
static inline void trace_mem_read_byte(Tracer* t, {0} pc, uint16_t op, {0} addr, uint8_t value) {{}}
static inline void trace_mem_read_halfword(Tracer* t, {0} pc, uint16_t op, {0} addr, uint16_t value) {{}}
static inline void trace_mem_read_word(Tracer* t, {0} pc, uint16_t op, {0} addr, uint32_t value) {{}}
static inline void trace_mem_read_dword(Tracer* t, {0} pc, uint16_t op, {0} addr, uint64_t value) {{}}

/* Memory writes */
static inline void trace_mem_write_byte(Tracer* t, {0} pc, uint16_t op, {0} addr, uint8_t value) {{}}
static inline void trace_mem_write_halfword(Tracer* t, {0} pc, uint16_t op, {0} addr, uint16_t value) {{}}
static inline void trace_mem_write_word(Tracer* t, {0} pc, uint16_t op, {0} addr, uint32_t value) {{}}
static inline void trace_mem_write_dword(Tracer* t, {0} pc, uint16_t op, {0} addr, uint64_t value) {{}}

/* Control flow */
static inline void trace_branch_taken(Tracer* t, {0} pc, uint16_t op, {0} target) {{}}
static inline void trace_branch_not_taken(Tracer* t, {0} pc, uint16_t op, {0} target) {{}}

/* CSR access */
static inline void trace_csr_read(Tracer* t, {0} pc, uint16_t op, uint16_t csr, {0} value) {{}}
static inline void trace_csr_write(Tracer* t, {0} pc, uint16_t op, uint16_t csr, {0} value) {{}}
"#,
        rtype
    )
}
