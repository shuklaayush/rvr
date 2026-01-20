//! Function signature generation for block functions.
//!
//! Generates C function signatures that match Mojo ABI:
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

/// Function signature for block functions.
///
/// Captures parameter declarations, argument lists, and save/restore code
/// that matches the Mojo ABI for generated C code.
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
    /// Set of hot register indices for fast lookup.
    pub hot_reg_set: HashSet<u8>,
    /// Whether instret counting is enabled.
    pub counts_instret: bool,
    /// Whether tracing is enabled for reg access.
    pub trace_regs: bool,
}

impl FnSignature {
    /// Create function signature from emit config.
    pub fn new<X: Xlen>(config: &EmitConfig<X>) -> Self {
        let rtype = reg_type::<X>();
        let counts_instret = config.instret_mode.counts();
        let trace_regs = !config.tracer_config.is_none();

        // Base signature: state pointer and memory base
        let mut params = String::from("RvState* state, uint8_t* memory");
        let mut args = String::from("state, memory");
        let mut args_from_state = String::from("state, state->memory");
        let mut save_to_state = String::new();

        // Add instret if counting is enabled
        if counts_instret {
            params.push_str(", uint64_t instret");
            args.push_str(", instret");
            args_from_state.push_str(", state->instret");
            save_to_state.push_str("state->instret = instret;");
        }

        // Add tracer passed variables
        params.push_str(&config.tracer_config.passed_var_params::<X>());
        args.push_str(&config.tracer_config.passed_var_args());
        args_from_state.push_str(&config.tracer_config.passed_var_args_from_state());
        save_to_state.push_str(&config.tracer_config.passed_var_save_to_state());

        // Add hot registers
        let mut hot_reg_set = HashSet::new();
        for &reg in &config.hot_regs {
            hot_reg_set.insert(reg);
            let name = abi_name(reg);
            params.push_str(&format!(", {} {}", rtype, name));
            args.push_str(&format!(", {}", name));
            args_from_state.push_str(&format!(", state->regs[{}]", reg));
            save_to_state.push_str(&format!(" state->regs[{}] = {};", reg, name));
        }

        Self {
            params,
            args,
            args_from_state,
            save_to_state,
            hot_reg_set,
            counts_instret,
            trace_regs,
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
            if self.trace_regs {
                format!("trd_regval({}, {})", reg, abi_name(reg))
            } else {
                abi_name(reg).to_string()
            }
        } else if self.trace_regs {
            format!("trd_reg(state, {})", reg)
        } else {
            format!("state->regs[{}]", reg)
        }
    }

    /// Generate code to write to a register.
    pub fn reg_write(&self, reg: u8, value: &str) -> String {
        if reg == 0 {
            String::new()
        } else if self.is_hot_reg(reg) {
            if self.trace_regs {
                format!("{} = twr_regval({}, {});", abi_name(reg), reg, value)
            } else {
                format!("{} = {};", abi_name(reg), value)
            }
        } else if self.trace_regs {
            format!("twr_reg(state, {}, {});", reg, value)
        } else {
            format!("state->regs[{}] = {};", reg, value)
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

        assert!(sig.params.contains("RvState* state"));
        assert!(sig.params.contains("uint8_t* memory"));
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
        assert_eq!(sig.reg_read(2), "state->regs[2]");
    }

    #[test]
    fn test_reg_write() {
        let mut config = EmitConfig::<Rv64>::new(32);
        config.hot_regs = vec![1]; // ra is hot
        let sig = FnSignature::new(&config);

        assert_eq!(sig.reg_write(0, "42"), "");
        assert_eq!(sig.reg_write(1, "42"), "ra = 42;");
        assert_eq!(sig.reg_write(2, "42"), "state->regs[2] = 42;");
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
