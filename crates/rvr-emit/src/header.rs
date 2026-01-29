//! Header generation for recompiled C code.
//!
//! Generates the main header file containing:
//! - Constants (memory config, entry point, CSRs)
//! - RvState struct with layout assertions
//! - Memory and CSR access functions
//! - Helper functions for bitmanip operations

use std::fmt::Write;

use rvr_ir::Xlen;

use crate::config::{EmitConfig, FixedAddressConfig, InstretMode, SyscallMode};
use crate::inputs::EmitInputs;
use crate::signature::{FnSignature, MEMORY_FIXED_REF, STATE_FIXED_REF, reg_type};
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
    /// Enable HTIF (Host-Target Interface).
    pub htif_enabled: bool,
    /// Enable address checking.
    pub addr_check: bool,
    /// Entry point address (where execution starts).
    pub entry_point: u64,
    /// Text section start (lowest code address, used for dispatch table base).
    pub text_start: u64,
    /// Block start addresses.
    pub block_addresses: Vec<u64>,
    /// Function signature.
    pub sig: FnSignature,
    /// Tracer configuration.
    pub tracer_config: TracerConfig,
    /// Syscall mode.
    pub syscall_mode: SyscallMode,
    /// Fixed addresses for state and memory (optional).
    pub fixed_addresses: Option<FixedAddressConfig>,
    _marker: std::marker::PhantomData<X>,
}

impl<X: Xlen> HeaderConfig<X> {
    /// Create header config from emit config.
    pub fn new(
        base_name: impl Into<String>,
        config: &EmitConfig<X>,
        inputs: &EmitInputs,
        block_addresses: Vec<u64>,
    ) -> Self {
        Self {
            base_name: base_name.into(),
            memory_bits: config.memory_bits,
            num_registers: config.num_regs,
            instret_mode: config.instret_mode,
            htif_enabled: config.htif_enabled,
            addr_check: config.addr_check,
            entry_point: inputs.entry_point,
            text_start: inputs.text_start,
            block_addresses,
            sig: FnSignature::new(config),
            tracer_config: config.tracer_config.clone(),
            syscall_mode: config.syscall_mode,
            fixed_addresses: config.fixed_addresses,
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

    // Generate traced helpers only when tracing is enabled
    if !cfg.tracer_config.is_none() {
        s.push_str(&gen_trace_helpers::<X>(cfg));
    }

    if cfg.syscall_mode == SyscallMode::Linux {
        s.push_str(&gen_syscall_declarations::<X>());
    }
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
    let htif_include = if cfg.htif_enabled {
        format!("#include \"{}_htif.h\"\n", cfg.base_name)
    } else {
        String::new()
    };

    // Include tracer header when tracing is enabled
    let tracer_include = if !cfg.tracer_config.is_none() {
        "#include \"rv_tracer.h\"\n".to_string()
    } else {
        String::new()
    };

    format!(
        r#"#pragma once
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <sys/mman.h>

{htif}{tracer}/* Branch prediction hints */
static inline int likely(int x) {{ return __builtin_expect(!!(x), 1); }}
static inline int unlikely(int x) {{ return __builtin_expect(!!(x), 0); }}

"#,
        htif = htif_include,
        tracer = tracer_include,
    )
}

fn gen_constants<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let mut s = format!(
        r#"/* Architecture constants (C23 constexpr) */
constexpr int XLEN = {xlen};

/* Memory configuration */
constexpr int MEMORY_BITS = {memory_bits};
constexpr uint64_t RV_MEMORY_SIZE = 1ull << {memory_bits};
constexpr uint64_t RV_MEMORY_MASK = (1ull << {memory_bits}) - 1;

/* Entry point */
constexpr uint32_t RV_ENTRY_POINT = {entry_point:#x};

/* CSR addresses */
constexpr uint32_t CSR_MISA      = {csr_misa:#x};
constexpr uint32_t CSR_CYCLE     = {csr_cycle:#x};
constexpr uint32_t CSR_CYCLEH    = {csr_cycleh:#x};
constexpr uint32_t CSR_INSTRET   = {csr_instret:#x};
constexpr uint32_t CSR_INSTRETH  = {csr_instreth:#x};
constexpr uint32_t CSR_MCYCLE    = {csr_mcycle:#x};
constexpr uint32_t CSR_MCYCLEH   = {csr_mcycleh:#x};
constexpr uint32_t CSR_MINSTRET  = {csr_minstret:#x};
constexpr uint32_t CSR_MINSTRETH = {csr_minstreth:#x};

/* RISC-V division special values */
constexpr uint32_t RV_DIV_BY_ZERO = UINT32_MAX;
constexpr int32_t  RV_INT32_MIN   = INT32_MIN;

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
    );

    // Add fixed address constants if enabled
    if let Some(fixed) = cfg.fixed_addresses {
        write!(
            s,
            r#"/* Fixed addresses for state and memory (requires runtime mapping) */
constexpr uint64_t RV_STATE_ADDR  = {:#x}ull;
constexpr uint64_t RV_MEMORY_ADDR = {:#x}ull;

"#,
            fixed.state_addr, fixed.memory_addr
        )
        .unwrap();
    }

    s
}

fn gen_state_struct<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let rtype = reg_type::<X>();
    let reg_bytes = HeaderConfig::<X>::reg_bytes();
    let has_suspend = cfg.instret_mode.suspends();
    let has_tracer = !cfg.tracer_config.is_none();

    // Compute offsets - hot fields first for better cache locality and smaller offsets
    // Layout: regs, pc, instret, [target_instret], reservation, exit control, brk, memory, [tracer], csrs

    // Hot fields first
    let offset_regs = 0;
    let size_regs = cfg.num_registers * reg_bytes;
    let offset_pc = offset_regs + size_regs;
    // instret is uint64_t, needs 8-byte alignment (implicit padding after pc on RV32)
    let instret_unaligned = offset_pc + reg_bytes;
    let offset_instret = (instret_unaligned + 7) & !7; // Align to 8 bytes

    // Optional target_instret for suspend mode
    let offset_target_instret = offset_instret + 8;
    let suspender_size = if has_suspend { 8 } else { 0 };

    // Reservation for LR/SC
    let offset_reservation_addr = offset_instret + 8 + suspender_size;
    let offset_reservation_valid = offset_reservation_addr + reg_bytes;

    // Execution control (pack booleans together)
    let offset_has_exited = offset_reservation_valid + 1;
    let offset_exit_code = offset_has_exited + 1;
    let offset_pad0 = offset_exit_code + 1;

    // Align to 8 bytes for brk
    let brk_align_offset = offset_pad0 + 1;
    let brk_padding = (8 - (brk_align_offset % 8)) % 8;
    let offset_brk = brk_align_offset + brk_padding;
    let offset_start_brk = offset_brk + reg_bytes;

    // Memory pointer (cold - only used for init, not in hot paths with fixed addresses)
    let offset_memory = offset_start_brk + reg_bytes;

    // Tracer if enabled (before CSRs)
    let offset_tracer = offset_memory + 8;

    // CSRs at end (huge array, rarely accessed in hot paths)
    let offset_csrs = if has_tracer {
        offset_tracer // tracer size added by C compiler
    } else {
        offset_memory + 8
    };

    // Optional suspender field
    let suspender_field = if has_suspend {
        format!(
            "    uint64_t target_instret;            /* offset {} */\n",
            offset_target_instret
        )
    } else {
        String::new()
    };

    // Optional tracer field (before CSRs)
    let tracer_field = if has_tracer {
        format!(
            "\n    /* Tracer - embedded struct */\n    Tracer tracer;                      /* offset {} */\n",
            offset_tracer
        )
    } else {
        String::new()
    };

    // CSR offset comment - if tracer is present, offset depends on Tracer size
    let csr_offset_comment = if has_tracer {
        "after Tracer".to_string()
    } else {
        offset_csrs.to_string()
    };

    let mut s = format!(
        r#"/* VM State - hot fields first for cache locality */
typedef struct RvState {{
    /* Hot path fields (small offsets for efficient addressing) */
    {rtype} regs[{num_regs}];           /* offset {offset_regs} */
    {rtype} pc;                         /* offset {offset_pc} */
    uint64_t instret;                   /* offset {offset_instret} */
{suspender_field}
    /* Reservation for LR/SC */
    {rtype} reservation_addr;           /* offset {offset_reservation_addr} */
    uint8_t reservation_valid;          /* offset {offset_reservation_valid} */

    /* Execution control */
    uint8_t has_exited;                 /* offset {offset_has_exited} */
    uint8_t exit_code;                  /* offset {offset_exit_code} */
    uint8_t _pad0;                      /* offset {offset_pad0} */

    /* Heap management */
    {rtype} brk;                        /* offset {offset_brk} */
    {rtype} start_brk;                  /* offset {offset_start_brk} */

    /* Cold fields (rarely accessed in hot paths) */
    uint8_t* memory;                    /* offset {offset_memory} */
{tracer_field}
    /* CSRs at end (large array, rarely used) */
    {rtype} csrs[{num_csrs}];           /* offset {csr_offset_comment} */
}} RvState;

"#,
        rtype = rtype,
        num_regs = cfg.num_registers,
        num_csrs = NUM_CSRS,
        offset_regs = offset_regs,
        offset_pc = offset_pc,
        offset_instret = offset_instret,
        suspender_field = suspender_field,
        offset_reservation_addr = offset_reservation_addr,
        offset_reservation_valid = offset_reservation_valid,
        offset_has_exited = offset_has_exited,
        offset_exit_code = offset_exit_code,
        offset_pad0 = offset_pad0,
        offset_brk = offset_brk,
        offset_start_brk = offset_start_brk,
        offset_memory = offset_memory,
        tracer_field = tracer_field,
        csr_offset_comment = csr_offset_comment,
    );

    // Layout verification (C23 static_assert without message)
    // Only verify offsets that are statically known (not tracer-dependent)
    let mut asserts = format!(
        r#"/* Layout verification (C23 static_assert) */
static_assert(offsetof(RvState, regs) == {offset_regs});
static_assert(offsetof(RvState, pc) == {offset_pc});
static_assert(offsetof(RvState, instret) == {offset_instret});
static_assert(offsetof(RvState, reservation_addr) == {offset_reservation_addr});
static_assert(offsetof(RvState, has_exited) == {offset_has_exited});
static_assert(offsetof(RvState, brk) == {offset_brk});
static_assert(offsetof(RvState, memory) == {offset_memory});

"#,
        offset_regs = offset_regs,
        offset_pc = offset_pc,
        offset_instret = offset_instret,
        offset_reservation_addr = offset_reservation_addr,
        offset_has_exited = offset_has_exited,
        offset_brk = offset_brk,
        offset_memory = offset_memory,
    );

    // Add CSR offset verification only if no tracer (otherwise it's dynamic)
    if !has_tracer {
        writeln!(
            asserts,
            "static_assert(offsetof(RvState, csrs) == {});",
            offset_csrs
        )
        .unwrap();
        asserts.push('\n');
    }

    s.push_str(&asserts);
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

    // Conditional parts based on fixed address mode
    let (mem_param, mem_ref, nonnull) = if cfg.fixed_addresses.is_some() {
        ("", MEMORY_FIXED_REF, "")
    } else {
        ("uint8_t* restrict memory, ", "memory", "nonnull, ")
    };

    format!(
        r#"static inline {addr_type} phys_addr({addr_type} addr) {{
{addr_check}    return addr & RV_MEMORY_MASK;
}}

/* Memory access: compute phys base first, then add offset. */
__attribute__((hot, pure, {nonnull}always_inline))
static inline uint32_t rd_mem_u8({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    return phys[off];
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline int32_t rd_mem_i8({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    return (int8_t)phys[off];
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline uint32_t rd_mem_u16({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 2);
    uint16_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline int32_t rd_mem_i16({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 2);
    uint16_t val;
    memcpy(&val, ptr, sizeof(val));
    return (int16_t)val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline uint32_t rd_mem_u32({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 4);
    uint32_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline int64_t rd_mem_i32({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 4);
    uint32_t val;
    memcpy(&val, ptr, sizeof(val));
    return (int32_t)val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline uint64_t rd_mem_u64({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 8);
    uint64_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u8({mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    phys[off] = (uint8_t)val;
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u16({mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 2);
    memcpy(ptr, &val, 2);
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u32({mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 4);
    memcpy(ptr, &val, sizeof(val));
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u64({mem_param}{addr_type} base, int16_t off, uint64_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 8);
    memcpy(ptr, &val, sizeof(val));
}}

"#,
        addr_type = addr_type,
        addr_check = addr_check,
        mem_param = mem_param,
        mem_ref = mem_ref,
        nonnull = nonnull,
    )
}

fn gen_csr_functions<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let instret_param = if cfg.instret_mode.counts() {
        ", uint64_t instret"
    } else {
        ""
    };

    // Conditional parts based on fixed address mode
    let (state_param_rd, state_param_wr, state_ref, nonnull, instret_val) =
        if cfg.fixed_addresses.is_some() {
            let instret = if cfg.instret_mode.counts() {
                "instret".to_string()
            } else {
                format!("{}->instret", STATE_FIXED_REF)
            };
            ("", "", STATE_FIXED_REF, "", instret)
        } else {
            let instret = if cfg.instret_mode.counts() {
                "instret"
            } else {
                "s->instret"
            };
            (
                "const RvState* restrict s, ",
                "RvState* restrict s, ",
                "s",
                "nonnull, ",
                instret.to_string(),
            )
        };

    format!(
        r#"/* CSR access */
__attribute__((hot, pure, {nonnull}always_inline))
static inline uint32_t rd_csr({state_param_rd}uint32_t csr{instret_param}) {{
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
            return {state_ref}->csrs[csr];
    }}
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_csr({state_param_wr}uint32_t csr, uint32_t val) {{
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
            {state_ref}->csrs[csr] = val;
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

/* Multiply-high helpers */
static inline uint32_t rv_mulh(int32_t a, int32_t b) {{
    return (uint32_t)(((int64_t)a * (int64_t)b) >> 32);
}}

static inline uint32_t rv_mulhsu(int32_t a, uint32_t b) {{
    return (uint32_t)(((int64_t)a * (int64_t)(uint64_t)b) >> 32);
}}

static inline uint32_t rv_mulhu(uint32_t a, uint32_t b) {{
    return (uint32_t)(((uint64_t)a * (uint64_t)b) >> 32);
}}

static inline uint64_t rv_mulh64(int64_t a, int64_t b) {{
    __int128 prod = (__int128)a * (__int128)b;
    return (uint64_t)(prod >> 64);
}}

static inline uint64_t rv_mulhsu64(int64_t a, uint64_t b) {{
    __int128 prod = (__int128)a * (__int128)b;
    return (uint64_t)(prod >> 64);
}}

static inline uint64_t rv_mulhu64(uint64_t a, uint64_t b) {{
    unsigned __int128 prod = (unsigned __int128)a * (unsigned __int128)b;
    return (uint64_t)(prod >> 64);
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

"#
    .to_string()
}

/// Generate traced helpers that call trace functions when tracing is enabled.
fn gen_trace_helpers<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
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

    // Conditional parts based on fixed address mode
    let (mem_param, mem_arg, state_param, state_arg, state_ref) = if cfg.fixed_addresses.is_some() {
        ("", "", "", "", STATE_FIXED_REF)
    } else {
        (
            "uint8_t* restrict memory, ",
            "memory, ",
            "RvState* restrict s, ",
            "s, ",
            "s",
        )
    };

    format!(
        r#"/* Traced memory read helpers - call optimized base functions. */
__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint32_t val = rd_mem_u8({mem_arg}base, off);
    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline int32_t trd_mem_i8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    int32_t val = rd_mem_i8({mem_arg}base, off);
    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint32_t val = rd_mem_u16({mem_arg}base, off);
    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline int32_t trd_mem_i16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    int32_t val = rd_mem_i16({mem_arg}base, off);
    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint32_t val = rd_mem_u32({mem_arg}base, off);
    trace_mem_read_word(t, pc, op, phys_addr(base) + off, val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline int64_t trd_mem_i32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    int64_t val = rd_mem_i32({mem_arg}base, off);
    trace_mem_read_word(t, pc, op, phys_addr(base) + off, (uint32_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline uint64_t trd_mem_u64(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint64_t val = rd_mem_u64({mem_arg}base, off);
    trace_mem_read_dword(t, pc, op, phys_addr(base) + off, val);
    return val;
}}

/* Traced memory write helpers - call optimized base functions. */
__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    trace_mem_write_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    wr_mem_u8({mem_arg}base, off, val);
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    trace_mem_write_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    wr_mem_u16({mem_arg}base, off, val);
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    trace_mem_write_word(t, pc, op, phys_addr(base) + off, val);
    wr_mem_u32({mem_arg}base, off, val);
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u64(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint64_t val) {{
    trace_mem_write_dword(t, pc, op, phys_addr(base) + off, val);
    wr_mem_u64({mem_arg}base, off, val);
}}

/* Traced register helpers - call trace functions */
__attribute__((hot, nonnull, always_inline))
static inline {rtype} trd_reg(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint8_t reg) {{
    {rtype} val = {state_ref}->regs[reg];
    trace_reg_read(t, pc, op, reg, val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_reg(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint8_t reg, {rtype} val) {{
    trace_reg_write(t, pc, op, reg, val);
    {state_ref}->regs[reg] = val;
}}

/* Traced hot register helpers - for registers in local vars/args */
__attribute__((hot, always_inline))
static inline {rtype} trd_regval(Tracer* t, {addr_type} pc, uint16_t op, uint8_t reg, {rtype} val) {{
    trace_reg_read(t, pc, op, reg, val);
    return val;
}}

__attribute__((hot, always_inline))
static inline {rtype} twr_regval(Tracer* t, {addr_type} pc, uint16_t op, uint8_t reg, {rtype} val) {{
    trace_reg_write(t, pc, op, reg, val);
    return val;
}}

/* Traced CSR access - call trace functions */
__attribute__((hot, nonnull))
static inline {rtype} trd_csr(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint16_t csr{instret_param}) {{
    {rtype} val = rd_csr({state_arg}csr{instret_arg});
    trace_csr_read(t, pc, op, csr, val);
    return val;
}}

__attribute__((hot, nonnull))
static inline void twr_csr(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint16_t csr, {rtype} val) {{
    trace_csr_write(t, pc, op, csr, val);
    wr_csr({state_arg}csr, val);
}}

"#,
        addr_type = addr_type,
        rtype = rtype,
        mem_param = mem_param,
        mem_arg = mem_arg,
        state_param = state_param,
        state_arg = state_arg,
        state_ref = state_ref,
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
    let width = if X::VALUE == 64 { 16 } else { 8 };
    for &addr in &cfg.block_addresses {
        writeln!(
            decls,
            "__attribute__((preserve_none)) void B_{addr:0width$x}({});",
            cfg.sig.params,
            addr = addr,
            width = width
        )
        .unwrap();
    }
    decls
}

/// Generate syscall runtime function declarations.
fn gen_syscall_declarations<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    format!(
        r#"/* Syscall runtime helpers (provided by runtime) */
{rtype} rv_sys_write(RvState* restrict state, {rtype} fd, {rtype} buf, {rtype} count);
{rtype} rv_sys_read(RvState* restrict state, {rtype} fd, {rtype} buf, {rtype} count);
{rtype} rv_sys_brk(RvState* restrict state, {rtype} addr);
{rtype} rv_sys_mmap(RvState* restrict state, {rtype} addr, {rtype} len, {rtype} prot, {rtype} flags, {rtype} fd, {rtype} off);
{rtype} rv_sys_fstat(RvState* restrict state, {rtype} fd, {rtype} statbuf);
{rtype} rv_sys_getrandom(RvState* restrict state, {rtype} buf, {rtype} len, {rtype} flags);
{rtype} rv_sys_clock_gettime(RvState* restrict state, {rtype} clk_id, {rtype} tp);

"#,
        rtype = rtype,
    )
}

fn gen_dispatch<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let text_start = cfg.text_start;
    let rtype = crate::signature::reg_type::<X>();

    // Fast path: power-of-2 text_start allows single AND instruction
    // Slow path: subtraction needed for arbitrary text_start
    let (dispatch_body, comment) = if text_start.is_power_of_two() {
        let mask = text_start - 1;
        (
            format!("return (pc & {mask:#x}) >> 1;"),
            "/* Dispatch: (pc & mask) >> 1 */",
        )
    } else {
        tracing::debug!(
            text_start = format_args!("{:#x}", text_start),
            "text_start is not power of 2, using slower dispatch"
        );
        (
            format!("return (pc - {text_start:#x}) >> 1;"),
            "/* Dispatch: (pc - text_start) >> 1 (slower, non-power-of-2 start) */",
        )
    };

    format!(
        r#"{comment}
static inline uint64_t dispatch_index({rtype} pc) {{
    {dispatch_body}
}}

extern const rv_fn dispatch_table[];

/* Runtime function - only this is needed from C */
int rv_execute_from(RvState* restrict state, {rtype} start_pc);

/* Metadata constant (read via dlsym) */
extern const uint32_t RV_TRACER_KIND;

"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_gen_header() {
        let config = EmitConfig::<Rv64>::standard();
        let inputs = EmitInputs::new(0x80000000, 0x80000008);
        let header_cfg = HeaderConfig::new("test", &config, &inputs, vec![0x80000000]);
        let header = gen_header::<Rv64>(&header_cfg);

        assert!(header.contains("#pragma once"));
        assert!(header.contains("MEMORY_BITS"));
        assert!(header.contains("RvState"));
        assert!(header.contains("phys_addr"));
    }

    #[test]
    fn test_gen_blocks_header() {
        let config = EmitConfig::<Rv64>::standard();
        let inputs = EmitInputs::new(0x80000000, 0x80000008);
        let header_cfg = HeaderConfig::new("test", &config, &inputs, vec![0x80000000, 0x80000004]);
        let blocks = gen_blocks_header::<Rv64>(&header_cfg);

        assert!(blocks.contains("B_0000000080000000"));
        assert!(blocks.contains("B_0000000080000004"));
        assert!(blocks.contains("rv_trap"));
    }
}
