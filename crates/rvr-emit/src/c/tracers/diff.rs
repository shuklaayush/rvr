//! Diff tracer header generation.
//!
//! Captures single-instruction state for differential testing.
//! Uses bounded memory (~64 bytes) - only stores the most recent instruction's effects.
//!
//! Captured state:
//! - PC and opcode
//! - Register write (rd, value)
//! - Memory access (addr, value, width, is_write)

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_diff<X: Xlen>() -> String {
    let rtype = reg_type::<X>();

    format!(
        r#"/* Diff tracer - captures single-instruction state for differential testing.
 *
 * Uses bounded memory (~64 bytes). Only stores the most recent instruction's effects.
 * State is cleared on trace_pc and accumulated during the instruction.
 */
#pragma once

#include <stdint.h>

typedef struct Tracer {{
    // Instruction identification
    {rtype} pc;              // Program counter
    uint32_t opcode;         // Raw instruction bits

    // Register write (at most one per instruction for RISC-V)
    uint8_t rd;              // Destination register (0 = none/x0)
    {rtype} rd_value;        // Value written

    // Memory access (at most one per instruction for RISC-V)
    {rtype} mem_addr;        // Address accessed
    {rtype} mem_value;       // Value read or written
    uint8_t mem_width;       // Access width: 1/2/4/8 bytes
    uint8_t is_write;        // 1 = store, 0 = load

    // Flags
    uint8_t has_rd;          // Non-zero if register was written
    uint8_t has_mem;         // Non-zero if memory was accessed
    uint8_t valid;           // Non-zero if instruction was traced
}} Tracer;

/* Initialize tracer (no-op for diff tracer) */
static inline void trace_init(Tracer* t) {{
    if (!t) return;
    t->valid = 0;
    t->has_rd = 0;
    t->has_mem = 0;
}}

/* Finalize tracer (no-op for diff tracer) */
static inline void trace_fini(Tracer* t) {{
    (void)t;
}}

/* Block entry (no-op) */
static inline void trace_block(Tracer* t, {rtype} pc) {{
    (void)t; (void)pc;
}}

/* Instruction dispatch - clear state, record PC */
static inline void trace_pc(Tracer* t, {rtype} pc, uint16_t op) {{
    (void)op;
    t->pc = pc;
    t->opcode = 0;
    t->rd = 0;
    t->rd_value = 0;
    t->mem_addr = 0;
    t->mem_value = 0;
    t->mem_width = 0;
    t->is_write = 0;
    t->has_rd = 0;
    t->has_mem = 0;
    t->valid = 1;
}}

/* Record opcode */
static inline void trace_opcode(Tracer* t, {rtype} pc, uint16_t op, uint32_t opcode) {{
    (void)pc; (void)op;
    t->opcode = opcode;
}}

/* Register reads (not tracked) */
static inline void trace_reg_read(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}}

/* Register write - record destination and value (ignore x0) */
static inline void trace_reg_write(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    (void)pc; (void)op;
    if (reg != 0) {{
        t->rd = reg;
        t->rd_value = value;
        t->has_rd = 1;
    }}
}}

/* Memory reads */
static inline void trace_mem_read_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 1;
    t->is_write = 0;
    t->has_mem = 1;
}}

static inline void trace_mem_read_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 2;
    t->is_write = 0;
    t->has_mem = 1;
}}

static inline void trace_mem_read_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 4;
    t->is_write = 0;
    t->has_mem = 1;
}}

static inline void trace_mem_read_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 8;
    t->is_write = 0;
    t->has_mem = 1;
}}

/* Memory writes */
static inline void trace_mem_write_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 1;
    t->is_write = 1;
    t->has_mem = 1;
}}

static inline void trace_mem_write_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 2;
    t->is_write = 1;
    t->has_mem = 1;
}}

static inline void trace_mem_write_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 4;
    t->is_write = 1;
    t->has_mem = 1;
}}

static inline void trace_mem_write_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    (void)pc; (void)op;
    t->mem_addr = addr;
    t->mem_value = value;
    t->mem_width = 8;
    t->is_write = 1;
    t->has_mem = 1;
}}

/* Control flow (not tracked) */
static inline void trace_branch_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

static inline void trace_branch_not_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

/* CSR access (not tracked for diff) */
static inline void trace_csr_read(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}

static inline void trace_csr_write(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}
"#,
        rtype = rtype,
    )
}
