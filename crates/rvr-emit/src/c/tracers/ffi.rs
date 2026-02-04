//! FFI tracer header generation.

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_ffi<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    format!(
        r"/* FFI tracer - pointer to external tracer, calls extern functions.
 *
 * The actual tracer lives externally. This struct holds a pointer to it.
 * Extern functions take Tracer* and access ->inner to get the actual tracer.
 */
#pragma once

#include <stdint.h>

/* Tracer holds a pointer to the external tracer */
typedef struct Tracer {{
    void* inner;
}} Tracer;

/* Extern declarations - implemented externally, take Tracer* */
extern void trace_init(Tracer* tracer);
extern void trace_fini(Tracer* tracer);

/* Block entry */
extern void trace_block(Tracer* tracer, {rtype} pc);

/* Instruction dispatch */
extern void trace_pc(Tracer* tracer, {rtype} pc, uint16_t op);
extern void trace_opcode(Tracer* tracer, {rtype} pc, uint16_t op, uint32_t opcode);

/* Register access */
extern void trace_reg_read(Tracer* tracer, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value);
extern void trace_reg_write(Tracer* tracer, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value);

/* Memory reads */
extern void trace_mem_read_byte(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value);
extern void trace_mem_read_halfword(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value);
extern void trace_mem_read_word(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value);
extern void trace_mem_read_dword(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value);

/* Memory writes */
extern void trace_mem_write_byte(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value);
extern void trace_mem_write_halfword(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value);
extern void trace_mem_write_word(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value);
extern void trace_mem_write_dword(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value);

/* Control flow */
extern void trace_branch_taken(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} target);
extern void trace_branch_not_taken(Tracer* tracer, {rtype} pc, uint16_t op, {rtype} target);

/* CSR access */
extern void trace_csr_read(Tracer* tracer, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value);
extern void trace_csr_write(Tracer* tracer, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value);
"
    )
}
