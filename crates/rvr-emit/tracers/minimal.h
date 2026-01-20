/* Minimal tracer header example. */
#pragma once

#include <stdint.h>

typedef struct Tracer { char _unused; } Tracer;

static inline void trace_init(Tracer* t) { (void)t; }
static inline void trace_fini(Tracer* t) { (void)t; }

static inline void trace_block(Tracer* t, uint64_t pc) { (void)t; (void)pc; }
static inline void trace_pc(Tracer* t, uint64_t pc, uint16_t op) { (void)t; (void)pc; (void)op; }
static inline void trace_opcode(Tracer* t, uint64_t pc, uint16_t op, uint32_t opcode) { (void)t; (void)pc; (void)op; (void)opcode; }

static inline void trace_reg_read(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value) {
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}
static inline void trace_reg_write(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value) {
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}

static inline void trace_mem_read_byte(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint8_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}
static inline void trace_mem_read_halfword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint16_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}
static inline void trace_mem_read_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}
static inline void trace_mem_read_dword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint64_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}

static inline void trace_mem_write_byte(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint8_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}
static inline void trace_mem_write_halfword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint16_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}
static inline void trace_mem_write_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}
static inline void trace_mem_write_dword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint64_t value) {
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}

static inline void trace_branch_taken(Tracer* t, uint64_t pc, uint16_t op, uint64_t target) {
    (void)t; (void)pc; (void)op; (void)target;
}
static inline void trace_branch_not_taken(Tracer* t, uint64_t pc, uint16_t op, uint64_t target) {
    (void)t; (void)pc; (void)op; (void)target;
}

static inline void trace_csr_read(Tracer* t, uint64_t pc, uint16_t op, uint16_t csr, uint64_t value) {
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}
static inline void trace_csr_write(Tracer* t, uint64_t pc, uint16_t op, uint16_t csr, uint64_t value) {
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}
