//! Function signature generation for block functions.
//!
//! Generates C function signatures with:
//! - state pointer + memory base
//! - optional instret counter
//! - tracer passed variables
//! - hot registers as direct parameters

use std::collections::HashSet;

use rvr_ir::Xlen;
use rvr_isa::reg_name;

use crate::config::EmitConfig;

// Re-export for backwards compatibility
pub use rvr_isa::REG_ABI_NAMES;

/// Get ABI name for register (alias for rvr_isa::reg_name).
#[inline]
pub fn abi_name(reg: u8) -> &'static str {
    reg_name(reg)
}

/// Get C type for register width.
pub fn reg_type<X: Xlen>() -> &'static str {
    if X::VALUE == 64 {
        "uint64_t"
    } else {
        "uint32_t"
    }
}

/// Fixed address constant for state.
pub const STATE_FIXED_REF: &str = "((RvState*)RV_STATE_ADDR)";

/// Fixed address constant for memory.
pub const MEMORY_FIXED_REF: &str = "((uint8_t*)RV_MEMORY_ADDR)";

/// Get state reference expression based on fixed address mode.
/// In fixed address mode: `((RvState*)RV_STATE_ADDR)`
/// In normal mode: `state`
pub fn state_ref(fixed_addresses: bool) -> &'static str {
    if fixed_addresses {
        STATE_FIXED_REF
    } else {
        "state"
    }
}

/// Function signature for block functions.
///
/// Captures parameter declarations, argument lists, and save/restore code
/// for generated C code.
#[derive(Clone, Debug)]
pub struct FnSignature {
    /// C function parameter declaration.
    /// Example: "RvState* state, uint8_t* memory, uint64_t instret, uint64_t ra, uint64_t sp"
    pub params: String,
    /// Argument list for calling block functions.
    /// Example: "state, memory, instret, ra, sp"
    pub args: String,
    /// Arguments extracted from state struct for initial calls.
    /// Example: "state, state->memory, state->instret, state->regs[1], state->regs[2]"
    pub args_from_state: String,
    /// Code to save hot registers back to state.
    /// Example: "state->instret = instret; state->regs[1] = ra; state->regs[2] = sp;"
    pub save_to_state: String,
    /// Code to save hot registers back to state, WITHOUT instret.
    /// Used in exit paths where instret is handled explicitly with increment.
    /// Example: "state->regs[1] = ra; state->regs[2] = sp;"
    pub save_to_state_no_instret: String,
    /// Set of hot register indices for fast lookup.
    pub hot_reg_set: HashSet<u8>,
    /// Whether instret counting is enabled.
    pub counts_instret: bool,
    /// Whether tracing is enabled for reg access.
    pub trace_regs: bool,
    /// Whether fixed addresses are used for state/memory.
    pub fixed_addresses: bool,
}

impl FnSignature {
    /// Create function signature from emit config.
    pub fn new<X: Xlen>(config: &EmitConfig<X>) -> Self {
        let rtype = reg_type::<X>();
        let counts_instret = config.instret_mode.counts();
        let trace_regs = !config.tracer_config.is_none();
        let fixed_addresses = config.fixed_addresses.is_some();
        let state = state_ref(fixed_addresses);

        // Base signature depends on whether fixed addresses are used
        let mut params = String::new();
        let mut args = String::new();
        let mut args_from_state = String::new();
        let mut save_to_state = String::new();
        let mut save_to_state_no_instret = String::new();

        if fixed_addresses {
            // With fixed addresses: state/memory are constants, not arguments
            // args_from_state is empty since we call with no state/memory args
        } else {
            // Without fixed addresses: state and memory are passed as arguments
            // restrict: state and memory never alias each other or hot registers
            params.push_str("RvState* restrict state, uint8_t* restrict memory");
            args.push_str("state, memory");
            args_from_state.push_str("state, state->memory");
        }

        // Add instret if counting is enabled
        if counts_instret {
            if !params.is_empty() {
                params.push_str(", ");
                args.push_str(", ");
                args_from_state.push_str(", ");
            }
            params.push_str("uint64_t instret");
            args.push_str("instret");
            args_from_state.push_str(&format!("{}->instret", state));
            save_to_state.push_str(&format!("{}->instret = instret;", state));
            // save_to_state_no_instret does NOT include instret
        }

        // Add tracer passed variables
        // Note: passed_var_* methods return strings with leading ", " when non-empty
        let tracer_params = config.tracer_config.passed_var_params::<X>();
        let tracer_args = config.tracer_config.passed_var_args();
        let tracer_args_from_state = config.tracer_config.passed_var_args_from_state();
        if !tracer_params.is_empty() {
            if params.is_empty() {
                // Strip leading ", " if params is empty
                params.push_str(tracer_params.trim_start_matches(", "));
                args.push_str(tracer_args.trim_start_matches(", "));
                args_from_state.push_str(tracer_args_from_state.trim_start_matches(", "));
            } else {
                params.push_str(&tracer_params);
                args.push_str(&tracer_args);
                args_from_state.push_str(&tracer_args_from_state);
            }
        }
        let tracer_save = config.tracer_config.passed_var_save_to_state();
        save_to_state.push_str(&tracer_save);
        save_to_state_no_instret.push_str(&tracer_save);

        // Add hot registers
        let mut hot_reg_set = HashSet::new();
        for &reg in &config.hot_regs {
            hot_reg_set.insert(reg);
            let name = abi_name(reg);
            if params.is_empty() {
                params.push_str(&format!("{} {}", rtype, name));
                args.push_str(name);
                args_from_state.push_str(&format!("{}->regs[{}]", state, reg));
            } else {
                params.push_str(&format!(", {} {}", rtype, name));
                args.push_str(&format!(", {}", name));
                args_from_state.push_str(&format!(", {}->regs[{}]", state, reg));
            }
            let reg_save = format!(" {}->regs[{}] = {};", state, reg, name);
            save_to_state.push_str(&reg_save);
            save_to_state_no_instret.push_str(&reg_save);
        }

        Self {
            params,
            args,
            args_from_state,
            save_to_state,
            save_to_state_no_instret,
            hot_reg_set,
            counts_instret,
            trace_regs,
            fixed_addresses,
        }
    }

    /// Check if register is hot.
    pub fn is_hot_reg(&self, reg: u8) -> bool {
        self.hot_reg_set.contains(&reg)
    }

    /// Generate code to read a register value.
    pub fn reg_read(&self, reg: u8) -> String {
        if reg == 0 {
            "0".to_string()
        } else if self.is_hot_reg(reg) {
            abi_name(reg).to_string()
        } else {
            format!("{}->regs[{}]", state_ref(self.fixed_addresses), reg)
        }
    }

    /// Generate code to write to a register.
    pub fn reg_write(&self, reg: u8, value: &str) -> String {
        if reg == 0 {
            String::new()
        } else if self.is_hot_reg(reg) {
            format!("{} = {};", abi_name(reg), value)
        } else {
            format!(
                "{}->regs[{}] = {};",
                state_ref(self.fixed_addresses),
                reg,
                value
            )
        }
    }

    /// Generate instret increment if counting is enabled.
    pub fn instret_increment(&self, count: u32) -> String {
        if self.counts_instret {
            format!("instret += {};", count)
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EmitConfig, InstretMode};
    use rvr_ir::Rv64;

    #[test]
    fn test_signature_basic() {
        let config = EmitConfig::<Rv64>::new(32);
        let sig = FnSignature::new(&config);

        assert!(sig.params.contains("RvState* restrict state"));
        assert!(sig.params.contains("uint8_t* restrict memory"));
        assert!(sig.args.contains("state"));
        assert!(sig.args.contains("memory"));
    }

    #[test]
    fn test_signature_with_instret() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.instret_mode = InstretMode::Count;
        let sig = FnSignature::new(&config);

        assert!(sig.params.contains("uint64_t instret"));
        assert!(sig.args.contains("instret"));
        assert!(sig.save_to_state.contains("state->instret = instret"));
    }

    #[test]
    fn test_signature_no_instret() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.instret_mode = InstretMode::Off;
        let sig = FnSignature::new(&config);

        assert!(!sig.params.contains("instret"));
        assert!(!sig.counts_instret);
    }

    #[test]
    fn test_signature_with_hot_regs() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.hot_regs = vec![1, 2]; // ra, sp
        let sig = FnSignature::new(&config);

        assert!(sig.params.contains("uint64_t ra"));
        assert!(sig.params.contains("uint64_t sp"));
        assert!(sig.is_hot_reg(1));
        assert!(sig.is_hot_reg(2));
        assert!(!sig.is_hot_reg(3));
    }

    #[test]
    fn test_reg_read() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.hot_regs = vec![1]; // ra is hot
        let sig = FnSignature::new(&config);

        assert_eq!(sig.reg_read(0), "0");
        assert_eq!(sig.reg_read(1), "ra");
        // Non-hot regs use state reference
        assert!(sig.reg_read(2).contains("->regs[2]"));
    }

    #[test]
    fn test_reg_write() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.hot_regs = vec![1]; // ra is hot
        let sig = FnSignature::new(&config);

        assert_eq!(sig.reg_write(0, "42"), "");
        assert_eq!(sig.reg_write(1, "42"), "ra = 42;");
        // Non-hot regs use state reference
        assert!(sig.reg_write(2, "42").contains("->regs[2] = 42;"));
    }

    #[test]
    fn test_abi_names() {
        assert_eq!(abi_name(0), "zero");
        assert_eq!(abi_name(1), "ra");
        assert_eq!(abi_name(2), "sp");
        assert_eq!(abi_name(10), "a0");
        assert_eq!(abi_name(31), "t6");
    }
}
