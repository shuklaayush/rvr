//! Header generation for recompiled C code.
//!
//! Generates the main header file containing:
//! - Constants (memory config, entry point, CSRs)
//! - RvState struct with layout assertions
//! - Memory and CSR access functions
//! - Helper functions for bitmanip operations

use std::fmt::Write;

use rvr_ir::Xlen;

use crate::config::{EmitConfig, InstretMode};
use crate::signature::{reg_type, FnSignature};
use crate::tracer::TracerConfig;

/// Number of CSRs.
pub const NUM_CSRS: usize = 4096;

/// CSR addresses.
pub const CSR_MISA: u32 = 0x301;
pub const CSR_CYCLE: u32 = 0xC00;
pub const CSR_CYCLEH: u32 = 0xC80;
pub const CSR_INSTRET: u32 = 0xC02;
pub const CSR_INSTRETH: u32 = 0xC82;
pub const CSR_MCYCLE: u32 = 0xB00;
pub const CSR_MCYCLEH: u32 = 0xB80;
pub const CSR_MINSTRET: u32 = 0xB02;
pub const CSR_MINSTRETH: u32 = 0xB82;

/// Header generation configuration.
pub struct HeaderConfig<X: Xlen> {
    /// Base name for output files.
    pub base_name: String,
    /// Memory address bits.
    pub memory_bits: u8,
    /// Number of registers.
    pub num_registers: usize,
    /// Instret counting mode.
    pub instret_mode: InstretMode,
    /// Enable tohost check.
    pub tohost_enabled: bool,
    /// Enable address checking.
    pub addr_check: bool,
    /// Entry point address.
    pub entry_point: u64,
    /// Block start addresses.
    pub block_addresses: Vec<u64>,
    /// Function signature.
    pub sig: FnSignature,
    /// Tracer configuration.
    pub tracer_config: TracerConfig,
    _marker: std::marker::PhantomData<X>,
}

impl<X: Xlen> HeaderConfig<X> {
    /// Create header config from emit config.
    pub fn new(
        base_name: impl Into<String>,
        config: &EmitConfig<X>,
        block_addresses: Vec<u64>,
    ) -> Self {
        Self {
            base_name: base_name.into(),
            memory_bits: config.memory_bits,
            num_registers: config.num_regs,
            instret_mode: config.instret_mode,
            tohost_enabled: config.tohost_enabled,
            addr_check: config.addr_check,
            entry_point: X::to_u64(config.entry_point),
            block_addresses,
            sig: FnSignature::new(config),
            tracer_config: config.tracer_config.clone(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Bytes per register.
    pub fn reg_bytes() -> usize {
        X::REG_BYTES
    }
}

/// Generate the main header file.
pub fn gen_header<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let mut s = String::new();

    // Pragma and includes
    s.push_str(&gen_pragma_and_includes(cfg));
    s.push_str(&gen_constants::<X>(cfg));
    s.push_str(&gen_state_struct::<X>(cfg));
    s.push_str(&gen_memory_functions::<X>(cfg));
    s.push_str(&gen_csr_functions::<X>(cfg));
    s.push_str(&gen_helpers());
    s.push_str(&gen_no_trace_helpers::<X>(cfg));
    s.push_str(&gen_fn_type(cfg));
    s.push_str(&gen_dispatch::<X>(cfg));

    s
}

/// Generate blocks header with forward declarations.
pub fn gen_blocks_header<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let decls = gen_block_declarations(cfg);
    format!(
        r#"#pragma once
#include "{}.h"

/* Trap handler for invalid addresses */
__attribute__((preserve_none)) void rv_trap({});

{}
"#,
        cfg.base_name, cfg.sig.params, decls
    )
}

fn gen_pragma_and_includes<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let htif_include = if cfg.tohost_enabled {
        format!("#include \"{}_htif.h\"\n", cfg.base_name)
    } else {
        String::new()
    };

    format!(
        r#"#pragma once
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <sys/mman.h>

{}/* Branch prediction hints */
static inline int likely(int x) {{ return __builtin_expect(!!(x), 1); }}
static inline int unlikely(int x) {{ return __builtin_expect(!!(x), 0); }}

"#,
        htif_include
    )
}

fn gen_constants<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    format!(
        r#"/* Architecture constants */
#define XLEN {xlen}

/* Memory configuration */
#define MEMORY_BITS {memory_bits}
#define RV_MEMORY_SIZE (1ull << {memory_bits})
#define RV_MEMORY_MASK ((1ull << {memory_bits}) - 1)

/* Entry point */
#define RV_ENTRY_POINT {entry_point:#x}

/* CSR addresses */
#define CSR_MISA      {csr_misa:#x}
#define CSR_CYCLE     {csr_cycle:#x}
#define CSR_CYCLEH    {csr_cycleh:#x}
#define CSR_INSTRET   {csr_instret:#x}
#define CSR_INSTRETH  {csr_instreth:#x}
#define CSR_MCYCLE    {csr_mcycle:#x}
#define CSR_MCYCLEH   {csr_mcycleh:#x}
#define CSR_MINSTRET  {csr_minstret:#x}
#define CSR_MINSTRETH {csr_minstreth:#x}

/* RISC-V division special values */
#define RV_DIV_BY_ZERO UINT32_MAX
#define RV_INT32_MIN   INT32_MIN

"#,
        xlen = X::VALUE,
        memory_bits = cfg.memory_bits,
        entry_point = cfg.entry_point,
        csr_misa = CSR_MISA,
        csr_cycle = CSR_CYCLE,
        csr_cycleh = CSR_CYCLEH,
        csr_instret = CSR_INSTRET,
        csr_instreth = CSR_INSTRETH,
        csr_mcycle = CSR_MCYCLE,
        csr_mcycleh = CSR_MCYCLEH,
        csr_minstret = CSR_MINSTRET,
        csr_minstreth = CSR_MINSTRETH,
    )
}

fn gen_state_struct<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let rtype = reg_type::<X>();
    let reg_bytes = HeaderConfig::<X>::reg_bytes();

    // Compute offsets
    let offset_memory = 0;
    let offset_regs = 8;
    let size_regs = cfg.num_registers * reg_bytes;
    let offset_csrs = offset_regs + size_regs;
    let size_csrs = NUM_CSRS * reg_bytes;
    let offset_pc = offset_csrs + size_csrs;
    let offset_pad0 = offset_pc + reg_bytes;
    // Align instret to 8 bytes
    let instret_align_offset = offset_pad0 + 4;
    let instret_padding = (8 - (instret_align_offset % 8)) % 8;
    let offset_instret = instret_align_offset + instret_padding;
    let offset_reservation_addr = offset_instret + 8;
    let offset_reservation_valid = offset_reservation_addr + reg_bytes;
    let offset_has_exited = offset_reservation_valid + 1;
    let offset_exit_code = offset_has_exited + 1;
    let offset_pad1 = offset_exit_code + 1;
    // Align to 8 bytes
    let pad2_align_offset = offset_pad1 + 1;
    let pad2_padding = (8 - (pad2_align_offset % 8)) % 8;
    let offset_pad2 = pad2_align_offset + pad2_padding;
    let offset_brk = offset_pad2 + 8;
    let offset_start_brk = offset_brk + reg_bytes;
    let base_machine_size = offset_start_brk + reg_bytes;

    // Optional fields
    let mut extra_fields = String::new();
    let has_tracer = !cfg.tracer_config.is_none();
    if has_tracer {
        writeln!(
            extra_fields,
            "    /* Tracer - embedded struct */\n    void* tracer;                       /* offset {} */",
            base_machine_size
        ).unwrap();
    }

    let _expected_size = if has_tracer {
        base_machine_size + 8 // Tracer pointer
    } else {
        base_machine_size
    };

    let mut s = format!(
        r#"/* VM State - layout must match Mojo RvState */
typedef struct RvState {{
    uint8_t* memory;                    /* offset {offset_memory} */
    {rtype} regs[{num_regs}];           /* offset {offset_regs} */
    {rtype} csrs[{num_csrs}];           /* offset {offset_csrs} */
    {rtype} pc;                         /* offset {offset_pc} */
    uint32_t _pad0;                     /* offset {offset_pad0} */
    uint64_t instret;                   /* offset {offset_instret} */

    /* Reservation for LR/SC */
    {rtype} reservation_addr;           /* offset {offset_reservation_addr} */
    uint8_t reservation_valid;          /* offset {offset_reservation_valid} */

    /* Execution control */
    uint8_t has_exited;                 /* offset {offset_has_exited} */
    uint8_t exit_code;                  /* offset {offset_exit_code} */
    uint8_t _pad1;                      /* offset {offset_pad1} */
    int64_t _pad2;                      /* offset {offset_pad2} */

    /* Heap management */
    {rtype} brk;                        /* offset {offset_brk} */
    {rtype} start_brk;                  /* offset {offset_start_brk} */
{extra_fields}}} RvState;

"#,
        rtype = rtype,
        num_regs = cfg.num_registers,
        num_csrs = NUM_CSRS,
        offset_memory = offset_memory,
        offset_regs = offset_regs,
        offset_csrs = offset_csrs,
        offset_pc = offset_pc,
        offset_pad0 = offset_pad0,
        offset_instret = offset_instret,
        offset_reservation_addr = offset_reservation_addr,
        offset_reservation_valid = offset_reservation_valid,
        offset_has_exited = offset_has_exited,
        offset_exit_code = offset_exit_code,
        offset_pad1 = offset_pad1,
        offset_pad2 = offset_pad2,
        offset_brk = offset_brk,
        offset_start_brk = offset_start_brk,
        extra_fields = extra_fields,
    );

    // Layout verification
    s.push_str(&format!(
        r#"/* Layout verification */
_Static_assert(offsetof(RvState, memory) == {offset_memory}, "memory offset");
_Static_assert(offsetof(RvState, regs) == {offset_regs}, "regs offset");
_Static_assert(offsetof(RvState, csrs) == {offset_csrs}, "csrs offset");
_Static_assert(offsetof(RvState, pc) == {offset_pc}, "pc offset");
_Static_assert(offsetof(RvState, instret) == {offset_instret}, "instret offset");
_Static_assert(offsetof(RvState, reservation_addr) == {offset_reservation_addr}, "reservation_addr offset");
_Static_assert(offsetof(RvState, has_exited) == {offset_has_exited}, "has_exited offset");
_Static_assert(offsetof(RvState, brk) == {offset_brk}, "brk offset");

"#,
        offset_memory = offset_memory,
        offset_regs = offset_regs,
        offset_csrs = offset_csrs,
        offset_pc = offset_pc,
        offset_instret = offset_instret,
        offset_reservation_addr = offset_reservation_addr,
        offset_has_exited = offset_has_exited,
        offset_brk = offset_brk,
    ));

    s
}

fn gen_memory_functions<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let addr_type = reg_type::<X>();

    let addr_check = if cfg.addr_check {
        format!(
            "    if (unlikely((int{0}_t)(addr << ({0} - MEMORY_BITS)) >> ({0} - MEMORY_BITS) != (int{0}_t)addr)) __builtin_trap();\n",
            X::VALUE
        )
    } else {
        String::new()
    };

    format!(
        r#"static inline {addr_type} phys_addr({addr_type} addr) {{
{addr_check}    return addr & RV_MEMORY_MASK;
}}

/* Memory access: compute phys base first, then add offset */
__attribute__((hot, pure, nonnull, always_inline))
static inline uint32_t rd_mem_u8(uint8_t* restrict memory, {addr_type} base, int16_t off) {{
    uint8_t* phys = &memory[phys_addr(base)];
    return phys[off];
}}

__attribute__((hot, pure, nonnull, always_inline))
static inline int32_t rd_mem_i8(uint8_t* restrict memory, {addr_type} base, int16_t off) {{
    uint8_t* phys = &memory[phys_addr(base)];
    return (int8_t)phys[off];
}}

__attribute__((hot, pure, nonnull, always_inline))
static inline uint32_t rd_mem_u16(uint8_t* restrict memory, {addr_type} base, int16_t off) {{
    uint8_t* phys = &memory[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 2);
    uint16_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, pure, nonnull, always_inline))
static inline int32_t rd_mem_i16(uint8_t* restrict memory, {addr_type} base, int16_t off) {{
    uint8_t* phys = &memory[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 2);
    uint16_t val;
    memcpy(&val, ptr, sizeof(val));
    return (int16_t)val;
}}

__attribute__((hot, pure, nonnull, always_inline))
static inline uint32_t rd_mem_u32(uint8_t* restrict memory, {addr_type} base, int16_t off) {{
    uint8_t* phys = &memory[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 4);
    uint32_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, pure, nonnull, always_inline))
static inline int64_t rd_mem_i32(uint8_t* restrict memory, {addr_type} base, int16_t off) {{
    uint8_t* phys = &memory[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 4);
    uint32_t val;
    memcpy(&val, ptr, sizeof(val));
    return (int32_t)val;
}}

__attribute__((hot, pure, nonnull, always_inline))
static inline uint64_t rd_mem_u64(uint8_t* restrict memory, {addr_type} base, int16_t off) {{
    uint8_t* phys = &memory[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 8);
    uint64_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline void wr_mem_u8(uint8_t* restrict memory, {addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &memory[phys_addr(base)];
    phys[off] = (uint8_t)val;
}}

__attribute__((hot, nonnull, always_inline))
static inline void wr_mem_u16(uint8_t* restrict memory, {addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &memory[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 2);
    memcpy(ptr, &val, 2);
}}

__attribute__((hot, nonnull, always_inline))
static inline void wr_mem_u32(uint8_t* restrict memory, {addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &memory[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 4);
    memcpy(ptr, &val, sizeof(val));
}}

__attribute__((hot, nonnull, always_inline))
static inline void wr_mem_u64(uint8_t* restrict memory, {addr_type} base, int16_t off, uint64_t val) {{
    uint8_t* phys = &memory[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 8);
    memcpy(ptr, &val, sizeof(val));
}}

"#,
        addr_type = addr_type,
        addr_check = addr_check,
    )
}

fn gen_csr_functions<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let instret_param = if cfg.instret_mode.counts() {
        ", uint64_t instret"
    } else {
        ""
    };
    let instret_val = if cfg.instret_mode.counts() {
        "instret"
    } else {
        "s->instret"
    };

    format!(
        r#"/* CSR access */
__attribute__((hot, pure, nonnull))
static inline uint32_t rd_csr(const RvState* restrict s{instret_param}, uint32_t csr) {{
    switch (csr) {{
        case CSR_MCYCLE:
        case CSR_CYCLE:
        case CSR_MINSTRET:
        case CSR_INSTRET:
            return (uint32_t)({instret_val} & 0xFFFFFFFFu);
        case CSR_MCYCLEH:
        case CSR_CYCLEH:
        case CSR_MINSTRETH:
        case CSR_INSTRETH:
            return (uint32_t)({instret_val} >> 32);
        default:
            return s->csrs[csr];
    }}
}}

__attribute__((hot, nonnull))
static inline void wr_csr(RvState* restrict s, uint32_t csr, uint32_t val) {{
    switch (csr) {{
        case CSR_MCYCLE:
        case CSR_MCYCLEH:
        case CSR_MINSTRET:
        case CSR_MINSTRETH:
        case CSR_CYCLE:
        case CSR_CYCLEH:
        case CSR_INSTRET:
        case CSR_INSTRETH:
            return;
        default:
            s->csrs[csr] = val;
    }}
}}

/* Division helpers with RISC-V semantics */
static inline uint32_t rv_div(int32_t a, int32_t b) {{
    if (b == 0) return RV_DIV_BY_ZERO;
    if (a == RV_INT32_MIN && b == -1) return (uint32_t)RV_INT32_MIN;
    return (uint32_t)(a / b);
}}

static inline uint32_t rv_divu(uint32_t a, uint32_t b) {{
    if (b == 0) return RV_DIV_BY_ZERO;
    return a / b;
}}

static inline uint32_t rv_rem(int32_t a, int32_t b) {{
    if (b == 0) return (uint32_t)a;
    if (a == RV_INT32_MIN && b == -1) return 0;
    return (uint32_t)(a % b);
}}

static inline uint32_t rv_remu(uint32_t a, uint32_t b) {{
    if (b == 0) return a;
    return a % b;
}}

/* 64-bit division helpers for RV64 */
static inline uint64_t rv_div64(int64_t a, int64_t b) {{
    if (b == 0) return UINT64_MAX;
    if (a == INT64_MIN && b == -1) return (uint64_t)INT64_MIN;
    return (uint64_t)(a / b);
}}

static inline uint64_t rv_divu64(uint64_t a, uint64_t b) {{
    if (b == 0) return UINT64_MAX;
    return a / b;
}}

static inline uint64_t rv_rem64(int64_t a, int64_t b) {{
    if (b == 0) return (uint64_t)a;
    if (a == INT64_MIN && b == -1) return 0;
    return (uint64_t)(a % b);
}}

static inline uint64_t rv_remu64(uint64_t a, uint64_t b) {{
    if (b == 0) return a;
    return a % b;
}}

/* RV64 word-width division helpers */
static inline uint64_t rv_divw(int32_t a, int32_t b) {{
    if (b == 0) return UINT64_MAX;
    if (a == INT32_MIN && b == -1) return (uint64_t)(int64_t)INT32_MIN;
    return (uint64_t)(int64_t)(a / b);
}}

static inline uint64_t rv_divuw(uint32_t a, uint32_t b) {{
    if (b == 0) return UINT64_MAX;
    return (uint64_t)(int64_t)(int32_t)(a / b);
}}

static inline uint64_t rv_remw(int32_t a, int32_t b) {{
    if (b == 0) return (uint64_t)(int64_t)a;
    if (a == INT32_MIN && b == -1) return 0;
    return (uint64_t)(int64_t)(a % b);
}}

static inline uint64_t rv_remuw(uint32_t a, uint32_t b) {{
    if (b == 0) return (uint64_t)(int64_t)(int32_t)a;
    return (uint64_t)(int64_t)(int32_t)(a % b);
}}

"#,
        instret_param = instret_param,
        instret_val = instret_val,
    )
}

fn gen_helpers() -> String {
    r#"/* Zbb/Zbkb helpers: loop-free, constant-time */

/* ORC.B: set each byte to 0xFF if non-zero, else 0x00 */
static inline uint32_t rv_orc_b32(uint32_t x) {
    x |= x >> 4; x |= x >> 2; x |= x >> 1;
    x &= 0x01010101u;
    x |= x << 1; x |= x << 2; x |= x << 4;
    return x;
}

static inline uint64_t rv_orc_b64(uint64_t x) {
    x |= x >> 4; x |= x >> 2; x |= x >> 1;
    x &= 0x0101010101010101ull;
    x |= x << 1; x |= x << 2; x |= x << 4;
    return x;
}

/* BREV8: reverse bits within each byte */
static inline uint32_t rv_brev8_32(uint32_t x) {
    x = ((x >> 1) & 0x55555555u) | ((x & 0x55555555u) << 1);
    x = ((x >> 2) & 0x33333333u) | ((x & 0x33333333u) << 2);
    x = ((x >> 4) & 0x0F0F0F0Fu) | ((x & 0x0F0F0F0Fu) << 4);
    return x;
}

static inline uint64_t rv_brev8_64(uint64_t x) {
    x = ((x >> 1) & 0x5555555555555555ull) | ((x & 0x5555555555555555ull) << 1);
    x = ((x >> 2) & 0x3333333333333333ull) | ((x & 0x3333333333333333ull) << 2);
    x = ((x >> 4) & 0x0F0F0F0F0F0F0F0Full) | ((x & 0x0F0F0F0F0F0F0F0Full) << 4);
    return x;
}

/* ZIP: interleave bits [15:0] into even positions, [31:16] into odd (RV32) */
static inline uint32_t rv_zip32(uint32_t x) {
    uint32_t lo = x & 0xFFFFu, hi = x >> 16;
    lo = (lo | (lo << 8)) & 0x00FF00FFu; hi = (hi | (hi << 8)) & 0x00FF00FFu;
    lo = (lo | (lo << 4)) & 0x0F0F0F0Fu; hi = (hi | (hi << 4)) & 0x0F0F0F0Fu;
    lo = (lo | (lo << 2)) & 0x33333333u; hi = (hi | (hi << 2)) & 0x33333333u;
    lo = (lo | (lo << 1)) & 0x55555555u; hi = (hi | (hi << 1)) & 0x55555555u;
    return lo | (hi << 1);
}

/* UNZIP: gather even bits to [15:0], odd bits to [31:16] (RV32) */
static inline uint32_t rv_unzip32(uint32_t x) {
    uint32_t lo = x & 0x55555555u, hi = (x >> 1) & 0x55555555u;
    lo = (lo | (lo >> 1)) & 0x33333333u; hi = (hi | (hi >> 1)) & 0x33333333u;
    lo = (lo | (lo >> 2)) & 0x0F0F0F0Fu; hi = (hi | (hi >> 2)) & 0x0F0F0F0Fu;
    lo = (lo | (lo >> 4)) & 0x00FF00FFu; hi = (hi | (hi >> 4)) & 0x00FF00FFu;
    lo = (lo | (lo >> 8)) & 0x0000FFFFu; hi = (hi | (hi >> 8)) & 0x0000FFFFu;
    return lo | (hi << 16);
}

"#.to_string()
}

fn gen_no_trace_helpers<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let rtype = reg_type::<X>();
    let addr_type = reg_type::<X>();
    let instret_param = if cfg.instret_mode.counts() {
        ", uint64_t instret"
    } else {
        ""
    };
    let instret_arg = if cfg.instret_mode.counts() {
        ", instret"
    } else {
        ""
    };

    format!(
        r#"/* Traced helpers (passthrough when tracing disabled) */
__attribute__((always_inline)) static inline uint32_t trd_mem_u8(uint8_t* m, {addr_type} b, int16_t o) {{ return rd_mem_u8(m, b, o); }}
__attribute__((always_inline)) static inline int32_t trd_mem_i8(uint8_t* m, {addr_type} b, int16_t o) {{ return rd_mem_i8(m, b, o); }}
__attribute__((always_inline)) static inline uint32_t trd_mem_u16(uint8_t* m, {addr_type} b, int16_t o) {{ return rd_mem_u16(m, b, o); }}
__attribute__((always_inline)) static inline int32_t trd_mem_i16(uint8_t* m, {addr_type} b, int16_t o) {{ return rd_mem_i16(m, b, o); }}
__attribute__((always_inline)) static inline uint32_t trd_mem_u32(uint8_t* m, {addr_type} b, int16_t o) {{ return rd_mem_u32(m, b, o); }}
__attribute__((always_inline)) static inline uint64_t trd_mem_u64(uint8_t* m, {addr_type} b, int16_t o) {{ return rd_mem_u64(m, b, o); }}
__attribute__((always_inline)) static inline void twr_mem_u8(uint8_t* m, {addr_type} b, int16_t o, uint32_t v) {{ wr_mem_u8(m, b, o, v); }}
__attribute__((always_inline)) static inline void twr_mem_u16(uint8_t* m, {addr_type} b, int16_t o, uint32_t v) {{ wr_mem_u16(m, b, o, v); }}
__attribute__((always_inline)) static inline void twr_mem_u32(uint8_t* m, {addr_type} b, int16_t o, uint32_t v) {{ wr_mem_u32(m, b, o, v); }}
__attribute__((always_inline)) static inline void twr_mem_u64(uint8_t* m, {addr_type} b, int16_t o, uint64_t v) {{ wr_mem_u64(m, b, o, v); }}
__attribute__((always_inline)) static inline {rtype} trd_regval({rtype} v) {{ return v; }}
__attribute__((always_inline)) static inline {rtype} twr_regval({rtype} v) {{ return v; }}
__attribute__((always_inline)) static inline {rtype} trd_reg(RvState* s, uint32_t r) {{ return s->regs[r]; }}
__attribute__((always_inline)) static inline void twr_reg(RvState* s, uint32_t r, {rtype} v) {{ s->regs[r] = v; }}
__attribute__((always_inline)) static inline uint32_t trd_csr(RvState* s{instret_param}, uint32_t c) {{ return rd_csr(s{instret_arg}, c); }}
__attribute__((always_inline)) static inline void twr_csr(RvState* s, uint32_t c, uint32_t v) {{ wr_csr(s, c, v); }}

"#,
        addr_type = addr_type,
        rtype = rtype,
        instret_param = instret_param,
        instret_arg = instret_arg,
    )
}

fn gen_fn_type<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    format!(
        r#"/* Block function type */
typedef __attribute__((preserve_none)) void (*rv_fn)({});

"#,
        cfg.sig.params
    )
}

fn gen_block_declarations<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let mut decls = String::from("/* Block forward declarations */\n");
    for &addr in &cfg.block_addresses {
        writeln!(
            decls,
            "__attribute__((preserve_none)) void B_{:08x}({});",
            addr, cfg.sig.params
        ).unwrap();
    }
    decls
}

fn gen_dispatch<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    // Compute mask for dispatch_index
    let entry = cfg.entry_point;
    let mask = if entry.is_power_of_two() {
        entry - 1
    } else {
        // Round up to next power of 2 minus 1
        (1u64 << (64 - entry.leading_zeros())) - 1
    };

    format!(
        r#"/* Dispatch: (pc & MASK) >> 1 */
static inline size_t dispatch_index(uint32_t pc) {{
    return (pc & {mask:#x}) >> 1;
}}

extern const rv_fn dispatch_table[];
int rv_execute_from(RvState* state, uint32_t start_pc);

"#,
        mask = mask
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_gen_header() {
        let config = EmitConfig::<Rv64>::standard();
        let header_cfg = HeaderConfig::new("test", &config, vec![0x80000000]);
        let header = gen_header::<Rv64>(&header_cfg);

        assert!(header.contains("#pragma once"));
        assert!(header.contains("MEMORY_BITS"));
        assert!(header.contains("RvState"));
        assert!(header.contains("phys_addr"));
    }

    #[test]
    fn test_gen_blocks_header() {
        let config = EmitConfig::<Rv64>::standard();
        let header_cfg = HeaderConfig::new("test", &config, vec![0x80000000, 0x80000004]);
        let blocks = gen_blocks_header::<Rv64>(&header_cfg);

        assert!(blocks.contains("B_80000000"));
        assert!(blocks.contains("B_80000004"));
        assert!(blocks.contains("rv_trap"));
    }
}
