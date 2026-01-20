//! Stats tracer header generation.

use rvr_ir::Xlen;

use crate::signature::reg_type;

const REG_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2",
    "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5",
    "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7",
    "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

pub fn gen_tracer_stats<X: Xlen>() -> String {
    let reg_names = REG_NAMES
        .iter()
        .map(|n| format!("\"{}\"", n))
        .collect::<Vec<_>>()
        .join(", ");
    let rtype = reg_type::<X>();

    format!(
        r#"/* Stats tracer - counts events and tracks per-opcode/register stats.
 * Uses op_name() and OP_TABLE_SIZE from generated header.
 */
#pragma once

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

/* ABI register names */
static const char* const REG_NAMES[32] = {{
    {reg_names}
}};

/* Page bitmap: 4GB / 4KB pages / 64 bits = 16384 words = 128KB */
#define PAGE_BITMAP_WORDS 16384
#define PAGE_SHIFT 12

/* Address bitmap: 4GB addresses / 8 bits = 512MB (allocated externally) */
#define ADDR_BITMAP_BYTES (1ULL << 29)
#define ADDR_BITMAP_WORDS (ADDR_BITMAP_BYTES / 8)

typedef struct Tracer {{
    uint64_t blocks;
    uint64_t pcs;
    uint64_t reg_reads;
    uint64_t reg_writes;
    uint64_t mem_reads;
    uint64_t mem_writes;
    uint64_t branches_taken;
    uint64_t branches_not_taken;
    uint64_t csr_reads;
    uint64_t csr_writes;
    uint64_t last_pc;  /* Always 64-bit for consistent layout */
    uint64_t opcode_counts[OP_TABLE_SIZE];
    uint64_t reg_read_counts[32];
    uint64_t reg_write_counts[32];
    uint64_t mem_pages[16384];  /* Combined read/write page bitmap */
    uint64_t* addr_bitmap;  /* 512MB sparse bitmap for exact unique addresses */
}} Tracer;

/* Set bit in page bitmap */
static inline void set_page_bit(uint64_t* bitmap, uint64_t addr) {{
    uint64_t page = addr >> PAGE_SHIFT;
    uint64_t word = page >> 6;
    uint64_t bit = page & 63;
    if (word < PAGE_BITMAP_WORDS) {{
        bitmap[word] |= (1ULL << bit);
    }}
}}

/* Count set bits in bitmap using popcount */
static inline uint64_t count_pages(uint64_t* bitmap) {{
    uint64_t count = 0;
    for (int i = 0; i < PAGE_BITMAP_WORDS; i++) {{
        count += __builtin_popcountll(bitmap[i]);
    }}
    return count;
}}

/* Set bit in address bitmap */
static inline void set_addr_bit(uint64_t* bitmap, uint64_t addr) {{
    uint64_t word = addr >> 6;
    uint64_t bit = addr & 63;
    bitmap[word] |= (1ULL << bit);
}}

/* Count set bits in address bitmap */
static inline uint64_t count_addrs(uint64_t* bitmap) {{
    uint64_t count = 0;
    for (size_t i = 0; i < ADDR_BITMAP_WORDS; i++) {{
        count += __builtin_popcountll(bitmap[i]);
    }}
    return count;
}}

typedef struct {{ uint16_t op; uint64_t count; }} OpCount;
typedef struct {{ uint8_t reg; uint64_t reads; uint64_t writes; uint64_t total; }} RegStats;

static int op_count_cmp(const void* a, const void* b) {{
    uint64_t ca = ((OpCount*)a)->count, cb = ((OpCount*)b)->count;
    return (ca < cb) - (ca > cb);  /* descending */
}}

static int reg_stats_cmp(const void* a, const void* b) {{
    uint64_t ca = ((RegStats*)a)->total, cb = ((RegStats*)b)->total;
    return (ca < cb) - (ca > cb);  /* descending */
}}

static inline void trace_init(Tracer* t) {{
    /* addr_bitmap should be allocated externally */
}}

static inline void trace_fini(Tracer* t) {{
    /* Execution Summary */
    printf("\n## Execution\n\n");
    printf("| %-16s | %15s |\n", "Metric", "Count");
    printf("|------------------|----------------:|\n");
    if (t->blocks) printf("| %-16s | %15lu |\n", "blocks", t->blocks);
    if (t->pcs) printf("| %-16s | %15lu |\n", "instructions", t->pcs);
    if (t->branches_taken) printf("| %-16s | %15lu |\n", "branches taken", t->branches_taken);
    if (t->branches_not_taken) printf("| %-16s | %15lu |\n", "branches skipped", t->branches_not_taken);
    if (t->csr_reads) printf("| %-16s | %15lu |\n", "csr reads", t->csr_reads);
    if (t->csr_writes) printf("| %-16s | %15lu |\n", "csr writes", t->csr_writes);

    /* Opcode Counts */
    OpCount ops[OP_TABLE_SIZE];
    int n_ops = 0;
    for (int i = 0; i < OP_TABLE_SIZE; i++) {{
        if (t->opcode_counts[i] > 0) {{
            ops[n_ops].op = (uint16_t)i;
            ops[n_ops].count = t->opcode_counts[i];
            n_ops++;
        }}
    }}
    qsort(ops, n_ops, sizeof(OpCount), op_count_cmp);

    printf("\n## Opcodes\n\n");
    printf("| %-12s | %15s |\n", "Opcode", "Count");
    printf("|--------------|----------------:|\n");
    for (int i = 0; i < n_ops; i++) {{
        printf("| %-12s | %15lu |\n", op_name(ops[i].op), ops[i].count);
    }}

    /* Register Access */
    RegStats regs[32];
    int n_regs = 0;
    for (int i = 0; i < 32; i++) {{
        uint64_t reads = t->reg_read_counts[i];
        uint64_t writes = t->reg_write_counts[i];
        if (reads > 0 || writes > 0) {{
            regs[n_regs].reg = (uint8_t)i;
            regs[n_regs].reads = reads;
            regs[n_regs].writes = writes;
            regs[n_regs].total = reads + writes;
            n_regs++;
        }}
    }}
    qsort(regs, n_regs, sizeof(RegStats), reg_stats_cmp);

    printf("\n## Registers\n\n");
    printf("| %-16s | %15s |\n", "Metric", "Count");
    printf("|------------------|----------------:|\n");
    uint64_t reg_total = t->reg_reads + t->reg_writes;
    if (reg_total) printf("| %-16s | %15lu |\n", "total", reg_total);
    if (t->reg_reads) printf("| %-16s | %15lu |\n", "reads", t->reg_reads);
    if (t->reg_writes) printf("| %-16s | %15lu |\n", "writes", t->reg_writes);
    printf("\n");
    printf("| %-12s | %15s | %15s | %15s |\n", "Reg", "Reads", "Writes", "Total");
    printf("|--------------|----------------:|----------------:|----------------:|\n");
    for (int i = 0; i < n_regs; i++) {{
        char reg_name[16];
        snprintf(reg_name, sizeof(reg_name), "%s (x%d)", REG_NAMES[regs[i].reg], regs[i].reg);
        printf("| %-12s | %15lu | %15lu | %15lu |\n",
            reg_name, regs[i].reads, regs[i].writes, regs[i].total);
    }}

    /* Memory Access */
    printf("\n## Memory\n\n");
    printf("| %-20s | %15s |\n", "Metric", "Count");
    printf("|----------------------|----------------:|\n");
    uint64_t mem_total = t->mem_reads + t->mem_writes;
    if (mem_total) printf("| %-20s | %15lu |\n", "total", mem_total);
    if (t->mem_reads) printf("| %-20s | %15lu |\n", "reads", t->mem_reads);
    if (t->mem_writes) printf("| %-20s | %15lu |\n", "writes", t->mem_writes);
    if (t->addr_bitmap) {{
        uint64_t unique_addrs = count_addrs(t->addr_bitmap);
        if (unique_addrs) printf("| %-20s | %15lu |\n", "unique addrs", unique_addrs);
    }}
    uint64_t unique_pages = count_pages(t->mem_pages);
    if (unique_pages) printf("| %-20s | %15lu |\n", "unique pages (4KB)", unique_pages);
    printf("\n");
}}

/* Block entry */
static inline void trace_block(Tracer* t, {rtype} pc) {{
    t->blocks++;
    t->last_pc = pc;
}}

/* Instruction dispatch */
static inline void trace_pc(Tracer* t, {rtype} pc, uint16_t op) {{
    t->pcs++;
    t->opcode_counts[op]++;
}}

/* Register access */
static inline void trace_reg_read(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    t->reg_reads++;
    t->reg_read_counts[reg]++;
}}

static inline void trace_reg_write(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    t->reg_writes++;
    t->reg_write_counts[reg]++;
}}

/* Memory reads */
static inline void trace_mem_read_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    t->mem_reads++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) set_addr_bit(t->addr_bitmap, addr);
}}

static inline void trace_mem_read_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    t->mem_reads++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) for (int i = 0; i < 2; i++) set_addr_bit(t->addr_bitmap, addr + i);
}}

static inline void trace_mem_read_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    t->mem_reads++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) for (int i = 0; i < 4; i++) set_addr_bit(t->addr_bitmap, addr + i);
}}

static inline void trace_mem_read_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    t->mem_reads++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) for (int i = 0; i < 8; i++) set_addr_bit(t->addr_bitmap, addr + i);
}}

/* Memory writes */
static inline void trace_mem_write_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    t->mem_writes++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) set_addr_bit(t->addr_bitmap, addr);
}}

static inline void trace_mem_write_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    t->mem_writes++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) for (int i = 0; i < 2; i++) set_addr_bit(t->addr_bitmap, addr + i);
}}

static inline void trace_mem_write_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    t->mem_writes++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) for (int i = 0; i < 4; i++) set_addr_bit(t->addr_bitmap, addr + i);
}}

static inline void trace_mem_write_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    t->mem_writes++;
    set_page_bit(t->mem_pages, addr);
    if (t->addr_bitmap) for (int i = 0; i < 8; i++) set_addr_bit(t->addr_bitmap, addr + i);
}}

/* Control flow */
static inline void trace_branch_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    t->branches_taken++;
}}

static inline void trace_branch_not_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    t->branches_not_taken++;
}}

/* CSR access */
static inline void trace_csr_read(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    t->csr_reads++;
}}

static inline void trace_csr_write(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    t->csr_writes++;
}}
"#,
        reg_names = reg_names,
        rtype = rtype
    )
}
