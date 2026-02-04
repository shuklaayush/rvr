//! Dynamic tracer header generation.

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_dynamic<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    format!(
        r"/* Dynamic tracer - runtime function pointers.
 *
 * Tracer struct contains function pointers that can be set at runtime.
 */
#pragma once

#include <stdint.h>

/* Function pointer types */
typedef void (*trace_init_fn)(void* tracer);
typedef void (*trace_fini_fn)(void* tracer);
typedef void (*trace_block_fn)(void* tracer, {rtype} pc);
typedef void (*trace_pc_fn)(void* tracer, {rtype} pc, uint16_t op);
typedef void (*trace_reg_fn)(void* tracer, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value);
typedef void (*trace_mem_byte_fn)(void* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value);
typedef void (*trace_mem_halfword_fn)(void* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value);
typedef void (*trace_mem_word_fn)(void* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value);
typedef void (*trace_mem_dword_fn)(void* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value);
typedef void (*trace_branch_fn)(void* tracer, {rtype} pc, uint16_t op, {rtype} target);
typedef void (*trace_csr_fn)(void* tracer, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value);

typedef struct Tracer {{
    void* inner;
    trace_init_fn fn_init;
    trace_fini_fn fn_fini;
    trace_block_fn fn_block;
    trace_pc_fn fn_pc;
    trace_reg_fn fn_reg_read;
    trace_reg_fn fn_reg_write;
    trace_mem_byte_fn fn_mem_read_byte;
    trace_mem_halfword_fn fn_mem_read_halfword;
    trace_mem_word_fn fn_mem_read_word;
    trace_mem_dword_fn fn_mem_read_dword;
    trace_mem_byte_fn fn_mem_write_byte;
    trace_mem_halfword_fn fn_mem_write_halfword;
    trace_mem_word_fn fn_mem_write_word;
    trace_mem_dword_fn fn_mem_write_dword;
    trace_branch_fn fn_branch_taken;
    trace_branch_fn fn_branch_not_taken;
    trace_csr_fn fn_csr_read;
    trace_csr_fn fn_csr_write;
}} Tracer;

static inline void trace_init(Tracer* t) {{ if (t->fn_init) t->fn_init(t->inner); }}
static inline void trace_fini(Tracer* t) {{ if (t->fn_fini) t->fn_fini(t->inner); }}

/* Block entry */
static inline void trace_block(Tracer* t, {rtype} pc) {{ if (t->fn_block) t->fn_block(t->inner, pc); }}

/* Instruction dispatch */
static inline void trace_pc(Tracer* t, {rtype} pc, uint16_t op) {{ if (t->fn_pc) t->fn_pc(t->inner, pc, op); }}

/* Register access */
static inline void trace_reg_read(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{ if (t->fn_reg_read) t->fn_reg_read(t->inner, pc, op, reg, value); }}
static inline void trace_reg_write(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{ if (t->fn_reg_write) t->fn_reg_write(t->inner, pc, op, reg, value); }}

/* Memory reads */
static inline void trace_mem_read_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{ if (t->fn_mem_read_byte) t->fn_mem_read_byte(t->inner, pc, op, addr, value); }}
static inline void trace_mem_read_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{ if (t->fn_mem_read_halfword) t->fn_mem_read_halfword(t->inner, pc, op, addr, value); }}
static inline void trace_mem_read_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{ if (t->fn_mem_read_word) t->fn_mem_read_word(t->inner, pc, op, addr, value); }}
static inline void trace_mem_read_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{ if (t->fn_mem_read_dword) t->fn_mem_read_dword(t->inner, pc, op, addr, value); }}

/* Memory writes */
static inline void trace_mem_write_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{ if (t->fn_mem_write_byte) t->fn_mem_write_byte(t->inner, pc, op, addr, value); }}
static inline void trace_mem_write_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{ if (t->fn_mem_write_halfword) t->fn_mem_write_halfword(t->inner, pc, op, addr, value); }}
static inline void trace_mem_write_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{ if (t->fn_mem_write_word) t->fn_mem_write_word(t->inner, pc, op, addr, value); }}
static inline void trace_mem_write_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{ if (t->fn_mem_write_dword) t->fn_mem_write_dword(t->inner, pc, op, addr, value); }}

/* Control flow */
static inline void trace_branch_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{ if (t->fn_branch_taken) t->fn_branch_taken(t->inner, pc, op, target); }}
static inline void trace_branch_not_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{ if (t->fn_branch_not_taken) t->fn_branch_not_taken(t->inner, pc, op, target); }}

/* CSR access */
static inline void trace_csr_read(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{ if (t->fn_csr_read) t->fn_csr_read(t->inner, pc, op, csr, value); }}
static inline void trace_csr_write(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{ if (t->fn_csr_write) t->fn_csr_write(t->inner, pc, op, csr, value); }}
"
    )
}
