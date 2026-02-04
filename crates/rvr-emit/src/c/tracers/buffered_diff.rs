//! Buffered diff tracer header generation.
//!
//! Captures N instruction states in a ring buffer for block-level differential testing.
//! Avoids per-instruction FFI callbacks by buffering in C and reading from Rust after execution.
//!
//! Captured state per entry:
//! - PC and opcode
//! - Register write (rd, value)
//! - Memory access (addr, value, width, `is_write`)

use rvr_ir::Xlen;

use super::super::signature::reg_type;

const BUFFERED_DIFF_TEMPLATE: &str = r"/* Buffered diff tracer - captures N instruction states for block-level comparison.
 *
 * Uses a ring buffer of DiffEntry structs. On overflow, oldest entries are overwritten
 * and `dropped` is incremented. After block execution, Rust reads the buffer.
 *
 * Layout must match Rust's BufferedDiffTracer and DiffEntry structs exactly.
 */
#pragma once

#include <stdint.h>

/* Single instruction's captured state - must match Rust DiffEntry layout */
typedef struct DiffEntry {{
    @RTYPE@ pc;              // Program counter
    uint32_t opcode;         // Raw instruction bits
    uint8_t rd;              // Destination register (0 = none/x0)
    uint8_t has_rd;          // Non-zero if register was written
    uint8_t has_mem;         // Non-zero if memory was accessed
    uint8_t is_write;        // 1 = store, 0 = load
    @RTYPE@ rd_value;        // Value written to rd
    @RTYPE@ mem_addr;        // Address accessed
    @RTYPE@ mem_value;       // Value read or written
    uint8_t mem_width;       // Access width: 1/2/4/8 bytes
    uint8_t _pad[7];         // Padding to align struct
}} DiffEntry;

/* Tracer state - ring buffer of entries */
typedef struct Tracer {{
    DiffEntry* buffer;       // Ring buffer (allocated by Rust)
    uint32_t capacity;       // Buffer size
    uint32_t head;           // Next write position
    uint32_t count;          // Number of valid entries (saturates at capacity)
    uint32_t dropped;        // Number of entries dropped due to overflow
    // Current instruction being built (finalized on next trace_pc)
    DiffEntry current;
    uint8_t current_valid;   // Non-zero if current has data
    uint8_t _pad[7];
}} Tracer;

/* Initialize tracer - called before execution */
static inline void trace_init(Tracer* t) {{
    if (!t) return;
    t->head = 0;
    t->count = 0;
    t->dropped = 0;
    t->current_valid = 0;
}}

/* Finalize tracer - flush any pending entry */
static inline void trace_fini(Tracer* t) {{
    if (!t || !t->current_valid || !t->buffer) return;
    // Flush current entry to buffer
    uint32_t idx = t->head;
    t->buffer[idx] = t->current;
    t->head = (t->head + 1) % t->capacity;
    if (t->count < t->capacity) {{
        t->count++;
    }} else {{
        t->dropped++;
    }}
    t->current_valid = 0;
}}

/* Block entry (no-op) */
static inline void trace_block(Tracer* t, @RTYPE@ pc) {{
    (void)t; (void)pc;
}}

/* Instruction dispatch - finalize previous entry, start new one */
static inline void trace_pc(Tracer* t, @RTYPE@ pc, uint16_t op) {{
    (void)op;
    if (!t || !t->buffer) return;

    // Finalize previous entry if any
    if (t->current_valid) {{
        uint32_t idx = t->head;
        t->buffer[idx] = t->current;
        t->head = (t->head + 1) % t->capacity;
        if (t->count < t->capacity) {{
            t->count++;
        }} else {{
            t->dropped++;
        }}
    }}

    // Start new entry
    t->current.pc = pc;
    t->current.opcode = 0;
    t->current.rd = 0;
    t->current.has_rd = 0;
    t->current.has_mem = 0;
    t->current.is_write = 0;
    t->current.rd_value = 0;
    t->current.mem_addr = 0;
    t->current.mem_value = 0;
    t->current.mem_width = 0;
    t->current_valid = 1;
}}

/* Record opcode */
static inline void trace_opcode(Tracer* t, @RTYPE@ pc, uint16_t op, uint32_t opcode) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.opcode = opcode;
}}

/* Register reads (not tracked) */
static inline void trace_reg_read(Tracer* t, @RTYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ value) {{
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}}

/* Register write - record destination and value (ignore x0) */
static inline void trace_reg_write(Tracer* t, @RTYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ value) {{
    (void)pc; (void)op;
    if (!t) return;
    if (reg != 0) {{
        t->current.rd = reg;
        t->current.rd_value = value;
        t->current.has_rd = 1;
    }}
}}

/* Memory reads */
static inline void trace_mem_read_byte(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint8_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 1;
    t->current.is_write = 0;
    t->current.has_mem = 1;
}}

static inline void trace_mem_read_halfword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint16_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 2;
    t->current.is_write = 0;
    t->current.has_mem = 1;
}}

static inline void trace_mem_read_word(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint32_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 4;
    t->current.is_write = 0;
    t->current.has_mem = 1;
}}

static inline void trace_mem_read_dword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint64_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 8;
    t->current.is_write = 0;
    t->current.has_mem = 1;
}}

/* Memory writes */
static inline void trace_mem_write_byte(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint8_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 1;
    t->current.is_write = 1;
    t->current.has_mem = 1;
}}

static inline void trace_mem_write_halfword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint16_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 2;
    t->current.is_write = 1;
    t->current.has_mem = 1;
}}

static inline void trace_mem_write_word(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint32_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 4;
    t->current.is_write = 1;
    t->current.has_mem = 1;
}}

static inline void trace_mem_write_dword(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ addr, uint64_t value) {{
    (void)pc; (void)op;
    if (!t) return;
    t->current.mem_addr = addr;
    t->current.mem_value = value;
    t->current.mem_width = 8;
    t->current.is_write = 1;
    t->current.has_mem = 1;
}}

/* Control flow (not tracked) */
static inline void trace_branch_taken(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

static inline void trace_branch_not_taken(Tracer* t, @RTYPE@ pc, uint16_t op, @RTYPE@ target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

/* CSR access (not tracked for diff) */
static inline void trace_csr_read(Tracer* t, @RTYPE@ pc, uint16_t op, uint16_t csr, @RTYPE@ value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}

static inline void trace_csr_write(Tracer* t, @RTYPE@ pc, uint16_t op, uint16_t csr, @RTYPE@ value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}
";

pub fn gen_tracer_buffered_diff<X: Xlen>() -> String {
    let rtype = reg_type::<X>();

    super::expand_template(BUFFERED_DIFF_TEMPLATE, &[("@RTYPE@", rtype)])
}
