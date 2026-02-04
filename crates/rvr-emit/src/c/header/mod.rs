//! Header generation for recompiled C code.
//!
//! Generates the main header file containing:
//! - Constants (memory config, entry point, CSRs)
//! - `RvState` struct with layout assertions
//! - Memory and CSR access functions
//! - Helper functions for bitmanip operations

use std::fmt::Write;

use rvr_ir::Xlen;

use super::signature::{FnSignature, MEMORY_FIXED_REF, STATE_FIXED_REF, reg_type};
use super::tracer::TracerConfig;
use crate::config::{AddressMode, EmitConfig, FixedAddressConfig, InstretMode, SyscallMode};
use crate::inputs::EmitInputs;
use crate::layout::RvStateLayout;

use csr::gen_csr_functions;
use dispatch::{gen_block_declarations, gen_dispatch, gen_fn_type, gen_syscall_declarations};
use helpers::gen_helpers;
use memory::gen_memory_functions;
use prelude::{gen_constants, gen_pragma_and_includes};
use state::gen_state_struct;
use trace::gen_trace_helpers;

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

pub(super) fn expand_template(template: &str, replacements: &[(&str, &str)]) -> String {
    let mut result = template.replace("{{", "{").replace("}}", "}");
    for (from, to) in replacements {
        result = result.replace(from, to);
    }
    result
}

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
    /// Address translation mode.
    pub address_mode: AddressMode,
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
            htif_enabled: config.htif_enabled(),
            address_mode: config.address_mode,
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
    #[must_use]
    pub const fn reg_bytes() -> usize {
        X::REG_BYTES
    }
}

/// Generate the main header file.
#[must_use]
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
#[must_use]
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

mod csr;
mod dispatch;
mod helpers;
mod memory;
mod prelude;
mod state;
mod trace;
