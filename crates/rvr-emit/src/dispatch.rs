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
void rv_trap({}) {{
    state->has_exited = true;
    state->exit_code = 1;
}}
"#,
        cfg.sig.params
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
    let tracer_helpers = match cfg.tracer_kind {
        Some(TracerKind::Preflight) => format!(
            r#"
/* Tracer setup (preflight) */
void rv_tracer_preflight_setup(RvState* state, uint8_t* data, uint32_t data_capacity, void* pc, uint32_t pc_capacity) {{
    (void)data_capacity;
    (void)pc_capacity;
    if (!state) return;
    memset(&state->tracer, 0, sizeof(Tracer));
    state->tracer.data = data;
    state->tracer.pc = ({rtype}*)pc;
}}
"#,
            rtype = crate::signature::reg_type::<X>()
        ),
        Some(TracerKind::Stats) => r#"
/* Tracer setup (stats) */
void rv_tracer_stats_setup(RvState* state, uint64_t* addr_bitmap) {
    if (!state) return;
    memset(&state->tracer, 0, sizeof(Tracer));
    state->tracer.addr_bitmap = addr_bitmap;
}
"#
        .to_string(),
        _ => String::new(),
    };

    format!(
        r#"/* C API helpers for external runners */

/* Return size of RvState struct */
size_t rv_state_size(void) {{
    return sizeof(RvState);
}}

/* Return alignment of RvState struct */
size_t rv_state_align(void) {{
    return _Alignof(RvState);
}}

/* Exported metadata constants (read via dlsym) */
constexpr uint32_t RV_REG_BYTES = XLEN / 8;
constexpr uint32_t RV_TRACER_KIND = {tracer_kind};

/* Reset RvState to initial values (zero regs/csrs, set pc, clear exit) */
void rv_state_reset(RvState* state) {{
    if (!state) return;
    memset(state->regs, 0, sizeof(state->regs));
    memset(state->csrs, 0, sizeof(state->csrs));
    state->pc = RV_ENTRY_POINT;
    state->instret = 0;
    state->reservation_valid = 0;
    state->has_exited = 0;
    state->exit_code = 0;
    state->brk = state->start_brk;
}}

/* Get instruction count */
uint64_t rv_get_instret(const RvState* state) {{
    return state ? state->instret : 0;
}}

/* Get exit code */
uint8_t rv_get_exit_code(const RvState* state) {{
    return state ? state->exit_code : 0;
}}

/* Check if execution has exited */
bool rv_has_exited(const RvState* state) {{
    return state ? state->has_exited : true;
}}

/* Get current PC */
uint64_t rv_get_pc(const RvState* state) {{
    return state ? (uint64_t)state->pc : 0;
}}

/* Set PC */
void rv_set_pc(RvState* state, uint64_t pc) {{
    if (state) state->pc = ({rtype})pc;
}}

/* Get memory pointer */
uint8_t* rv_get_memory(const RvState* state) {{
    return state ? state->memory : nullptr;
}}

/* Get memory size */
uint64_t rv_get_memory_size(void) {{
    return RV_MEMORY_SIZE;
}}

/* Get entry point */
uint32_t rv_get_entry_point(void) {{
    return RV_ENTRY_POINT;
}}

{tracer_helpers}
"#,
        tracer_kind = tracer_kind_val,
        tracer_helpers = tracer_helpers,
        rtype = crate::signature::reg_type::<X>(),
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
