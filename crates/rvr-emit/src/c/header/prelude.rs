use super::{
    CSR_CYCLE, CSR_CYCLEH, CSR_INSTRET, CSR_INSTRETH, CSR_MCYCLE, CSR_MCYCLEH, CSR_MINSTRET,
    CSR_MINSTRETH, CSR_MISA, HeaderConfig, Write, Xlen,
};

pub(super) fn gen_pragma_and_includes<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let htif_include = if cfg.htif_enabled {
        format!("#include \"{}_htif.h\"\n", cfg.base_name)
    } else {
        String::new()
    };

    // Include tracer header when tracing is enabled
    let tracer_include = if cfg.tracer_config.is_none() {
        String::new()
    } else {
        "#include \"rv_tracer.h\"\n".to_string()
    };

    format!(
        r"#pragma once
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <sys/mman.h>

{htif_include}{tracer_include}/* Branch prediction hints */
static inline int likely(int x) {{ return __builtin_expect(!!(x), 1); }}
static inline int unlikely(int x) {{ return __builtin_expect(!!(x), 0); }}

",
    )
}

pub(super) fn gen_constants<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let mut s = format!(
        r"/* Architecture constants (C23 constexpr) */
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

",
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
            r"/* Fixed addresses for state and memory (requires runtime mapping) */
constexpr uint64_t RV_STATE_ADDR  = {:#x}ull;
constexpr uint64_t RV_MEMORY_ADDR = {:#x}ull;

",
            fixed.state_addr, fixed.memory_addr
        )
        .unwrap();
    }

    s
}
