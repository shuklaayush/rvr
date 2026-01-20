//! Dispatch table generation for recompiled C code.
//!
//! Generates dispatch.c containing:
//! - Trap handler for invalid addresses
//! - Dispatch table mapping PC -> block function
//! - Runtime execution function

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use rvr_ir::Xlen;

use crate::config::{EmitConfig, InstretMode};
use crate::signature::FnSignature;

/// Instruction slot size (2 bytes for compressed instruction support).
pub const INSTRUCTION_SIZE: u64 = 2;

/// Dispatch generation configuration.
pub struct DispatchConfig<X: Xlen> {
    /// Base name for output files.
    pub base_name: String,
    /// Entry point address.
    pub entry_point: u64,
    /// End address (exclusive).
    pub pc_end: u64,
    /// Initial brk value.
    pub initial_brk: u64,
    /// Instret counting mode.
    pub instret_mode: InstretMode,
    /// Valid block start addresses.
    pub valid_addresses: HashSet<u64>,
    /// Absorbed block mapping: absorbed_pc -> merged_block_start.
    pub absorbed_to_merged: HashMap<u64, u64>,
    /// Function signature.
    pub sig: FnSignature,
    /// Memory address bits.
    pub memory_bits: u8,
    /// Whether tracing is enabled.
    pub has_tracing: bool,
    _marker: std::marker::PhantomData<X>,
}

impl<X: Xlen> DispatchConfig<X> {
    /// Create dispatch config from emit config.
    pub fn new(
        config: &EmitConfig<X>,
        base_name: impl Into<String>,
        entry_point: u64,
        pc_end: u64,
        initial_brk: u64,
        valid_addresses: HashSet<u64>,
        absorbed_to_merged: HashMap<u64, u64>,
    ) -> Self {
        Self {
            base_name: base_name.into(),
            entry_point,
            pc_end,
            initial_brk,
            instret_mode: config.instret_mode,
            valid_addresses,
            absorbed_to_merged,
            sig: FnSignature::new(config),
            memory_bits: config.memory_bits,
            has_tracing: !config.tracer_config.is_none(),
            _marker: std::marker::PhantomData,
        }
    }
}

/// Generate the dispatch.c file.
pub fn gen_dispatch_file<X: Xlen>(cfg: &DispatchConfig<X>) -> String {
    let mut s = String::new();

    // Include blocks header
    writeln!(s, "#include \"{}_blocks.h\"\n", cfg.base_name).unwrap();

    // Trap handler
    s.push_str(&gen_trap_handler(cfg));
    s.push('\n');

    // Dispatch table
    s.push_str("/* Dispatch table: PC -> block function */\n");
    s.push_str("const rv_fn dispatch_table[] = {\n");

    let mut addr = cfg.entry_point;
    while addr < cfg.pc_end {
        if cfg.valid_addresses.contains(&addr) {
            // Block start - point to its own function
            writeln!(s, "    B_{:016x},", addr).unwrap();
        } else if let Some(&merged) = cfg.absorbed_to_merged.get(&addr) {
            // Absorbed block - point to merged block's function
            writeln!(s, "    B_{:016x},", merged).unwrap();
        } else {
            s.push_str("    rv_trap,\n");
        }
        addr += INSTRUCTION_SIZE;
    }

    s.push_str("};\n\n");

    // Runtime functions
    s.push_str(&gen_runtime_functions(cfg));

    s
}

fn gen_trap_handler<X: Xlen>(cfg: &DispatchConfig<X>) -> String {
    format!(
        r#"/* Trap handler for invalid addresses - replaces NULL checks */
__attribute__((preserve_none, cold))
void rv_trap({}) {{
    state->has_exited = true;
    state->exit_code = 1;
}}
"#,
        cfg.sig.params
    )
}

fn gen_runtime_functions<X: Xlen>(cfg: &DispatchConfig<X>) -> String {
    let suspend_check = if cfg.instret_mode.suspends() {
        "\n    if (state->target_instret <= state->instret) return 2;"
    } else {
        ""
    };

    let trace_init = if cfg.has_tracing {
        "trace_init(&state->tracer);"
    } else {
        ""
    };

    let trace_fini = if cfg.has_tracing {
        "trace_fini(&state->tracer);"
    } else {
        ""
    };

    format!(
        r#"/* Execute from given PC. Returns: 0=continue, 1=exited, 2=suspended */
__attribute__((hot, nonnull))
int rv_execute_from(RvState* restrict state, uint32_t start_pc) {{
    {trace_init}
    state->pc = start_pc;
    dispatch_table[dispatch_index(start_pc)]({args_from_state});
    {trace_fini}
    if (state->has_exited) return 1;{suspend_check}
    return 0;
}}
"#,
        args_from_state = cfg.sig.args_from_state,
        suspend_check = suspend_check,
        trace_init = trace_init,
        trace_fini = trace_fini,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_gen_dispatch() {
        let config = EmitConfig::<Rv64>::standard();
        let mut valid = HashSet::new();
        valid.insert(0x80000000u64);
        valid.insert(0x80000004u64);

        let dispatch_cfg = DispatchConfig::new(
            &config,
            "test",
            0x80000000,
            0x80000010,
            0x80010000,
            valid,
            HashMap::new(),
        );

        let dispatch = gen_dispatch_file::<Rv64>(&dispatch_cfg);

        assert!(dispatch.contains("dispatch_table"));
        assert!(dispatch.contains("B_0000000080000000"));
        assert!(dispatch.contains("B_0000000080000004"));
        assert!(dispatch.contains("rv_trap"));
        assert!(dispatch.contains("rv_execute_from"));
    }

    #[test]
    fn test_absorbed_mapping() {
        let config = EmitConfig::<Rv64>::standard();
        let mut valid = HashSet::new();
        valid.insert(0x80000000u64);

        let mut absorbed = HashMap::new();
        absorbed.insert(0x80000002u64, 0x80000000u64);

        let dispatch_cfg = DispatchConfig::new(
            &config,
            "test",
            0x80000000,
            0x80000008,
            0x80010000,
            valid,
            absorbed,
        );

        let dispatch = gen_dispatch_file::<Rv64>(&dispatch_cfg);

        // Address 0x80000002 should point to B_0000000080000000
        assert!(dispatch.contains("B_0000000080000000,\n    B_0000000080000000,"));
    }
}
