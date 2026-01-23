//! Dispatch table generation for recompiled C code.
//!
//! Generates dispatch.c containing:
//! - Trap handler for invalid addresses
//! - Dispatch table mapping PC -> block function
//! - Runtime execution function

use std::fmt::Write;

use rvr_ir::Xlen;

use crate::config::{EmitConfig, InstretMode};
use crate::inputs::EmitInputs;
use crate::signature::FnSignature;
use crate::tracer::TracerKind;

/// Instruction slot size (2 bytes for compressed instruction support).
pub const INSTRUCTION_SIZE: u64 = 2;

/// Dispatch generation configuration.
pub struct DispatchConfig<X: Xlen> {
    /// Base name for output files.
    pub base_name: String,
    /// Derived inputs (entry_point, pc_end, valid_addresses, initial_brk).
    pub inputs: EmitInputs,
    /// Instret counting mode.
    pub instret_mode: InstretMode,
    /// Function signature.
    pub sig: FnSignature,
    /// Memory address bits.
    pub memory_bits: u8,
    /// Whether tracing is enabled.
    pub has_tracing: bool,
    /// Built-in tracer kind when available.
    pub tracer_kind: Option<TracerKind>,
    /// Export functions mode: compiled for calling exported functions.
    pub export_functions: bool,
    _marker: std::marker::PhantomData<X>,
}

impl<X: Xlen> DispatchConfig<X> {
    /// Create dispatch config from emit config.
    pub fn new(config: &EmitConfig<X>, base_name: impl Into<String>, inputs: EmitInputs) -> Self {
        Self {
            base_name: base_name.into(),
            inputs,
            instret_mode: config.instret_mode,
            sig: FnSignature::new(config),
            memory_bits: config.memory_bits,
            has_tracing: !config.tracer_config.is_none(),
            tracer_kind: config.tracer_config.builtin_kind(),
            export_functions: config.export_functions,
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

    // C API helper functions
    s.push_str(&gen_api_helpers(cfg));
    s.push('\n');

    // Dispatch table
    s.push_str("/* Dispatch table: PC -> block function */\n");
    s.push_str("const rv_fn dispatch_table[] = {\n");

    let width = if X::VALUE == 64 { 16 } else { 8 };
    let mut addr = cfg.inputs.entry_point;
    while addr < cfg.inputs.pc_end {
        if cfg.inputs.valid_addresses.contains(&addr) {
            // Block start - point to its own function
            writeln!(s, "    B_{addr:0width$x},", addr = addr, width = width).unwrap();
        } else if let Some(&merged) = cfg.inputs.absorbed_to_merged.get(&addr) {
            // Absorbed block - point to merged block's function
            writeln!(s, "    B_{addr:0width$x},", addr = merged, width = width).unwrap();
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
void rv_trap({params}) {{
    state->has_exited = true;
    state->exit_code = 1;
    {save_to_state}
}}
"#,
        params = cfg.sig.params,
        save_to_state = cfg.sig.save_to_state,
    )
}

fn gen_api_helpers<X: Xlen>(cfg: &DispatchConfig<X>) -> String {
    let tracer_kind_val = match cfg.tracer_kind {
        Some(kind) => kind.as_c_kind(),
        None => {
            if cfg.has_tracing {
                255
            } else {
                0
            }
        }
    };

    let export_functions_val: u32 = if cfg.export_functions { 1 } else { 0 };
    let instret_mode_val: u32 = cfg.instret_mode.as_c_mode();

    format!(
        r#"/* Minimal C API - state management happens in Rust */

/* Exported metadata constants (read via dlsym) */
const uint32_t RV_TRACER_KIND = {tracer_kind};
const uint32_t RV_EXPORT_FUNCTIONS = {export_functions};
const uint32_t RV_INSTRET_MODE = {instret_mode};
"#,
        tracer_kind = tracer_kind_val,
        export_functions = export_functions_val,
        instret_mode = instret_mode_val,
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

    let reg_type = crate::signature::reg_type::<X>();

    format!(
        r#"/* Execute from given PC. Returns: 0=continue, 1=exited, 2=suspended */
__attribute__((hot, nonnull))
int rv_execute_from(RvState* restrict state, {reg_type} start_pc) {{
    {trace_init}
    state->pc = start_pc;
    dispatch_table[dispatch_index(start_pc)]({args_from_state});
    {trace_fini}
    if (state->has_exited) return 1;{suspend_check}
    return 0;
}}
"#,
        reg_type = reg_type,
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
        let mut inputs = EmitInputs::new(0x80000000, 0x80000010).with_initial_brk(0x80010000);
        inputs.valid_addresses.insert(0x80000000u64);
        inputs.valid_addresses.insert(0x80000004u64);

        let dispatch_cfg = DispatchConfig::new(&config, "test", inputs);

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
        let mut inputs = EmitInputs::new(0x80000000, 0x80000008).with_initial_brk(0x80010000);
        inputs.valid_addresses.insert(0x80000000u64);
        inputs
            .absorbed_to_merged
            .insert(0x80000002u64, 0x80000000u64);

        let dispatch_cfg = DispatchConfig::new(&config, "test", inputs);

        let dispatch = gen_dispatch_file::<Rv64>(&dispatch_cfg);

        // Address 0x80000002 should point to B_0000000080000000
        assert!(dispatch.contains("B_0000000080000000,\n    B_0000000080000000,"));
    }
}
