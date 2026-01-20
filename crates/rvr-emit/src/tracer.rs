//! Tracer configuration and header generation for C-first tracer design.
//!
//! Tracers are implemented as C headers. This module provides configuration
//! for selecting which tracer to use and which variables to pass directly
//! to block functions for performance.

/// Tracer kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TracerKind {
    /// No tracing - all calls optimize away.
    None,
    /// Preflight tracer - records for replay/proofs.
    Preflight,
    /// Stats tracer - counts events/opcodes/registers.
    Stats,
    /// FFI tracer - external function calls.
    Ffi,
    /// Dynamic tracer - runtime function pointers.
    Dynamic,
}

impl TracerKind {
    /// Check if this kind is none.
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }

    /// Check if RvState needs a Tracer field.
    pub fn has_tracer_struct(&self) -> bool {
        *self != Self::None
    }
}

/// Variable passed directly to block functions.
///
/// Used to "hoist" tracer struct fields to function parameters for
/// performance (avoids pointer indirection on hot paths).
#[derive(Clone, Debug)]
pub struct PassedVar {
    /// Variable name.
    pub name: String,
    /// Kind of variable.
    pub kind: PassedVarKind,
}

impl PassedVar {
    /// Create a pointer variable.
    pub fn ptr(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: PassedVarKind::Ptr,
        }
    }

    /// Create an index variable.
    pub fn index(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: PassedVarKind::Index,
        }
    }

    /// Create a value variable.
    pub fn value(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: PassedVarKind::Value,
        }
    }
}

/// Kind of passed variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassedVarKind {
    /// Pointer: {rtype}* name
    Ptr,
    /// Index: uint32_t name
    Index,
    /// Value: {rtype} name
    Value,
}

/// Tracer configuration: which tracer + what to pass directly.
#[derive(Clone, Debug)]
pub struct TracerConfig {
    /// Tracer kind.
    pub kind: TracerKind,
    /// Variables passed directly to block functions.
    pub passed_vars: Vec<PassedVar>,
}

impl TracerConfig {
    /// Create config with given kind and no passed variables.
    pub fn new(kind: TracerKind) -> Self {
        Self {
            kind,
            passed_vars: Vec::new(),
        }
    }

    /// No tracing.
    pub fn none() -> Self {
        Self::new(TracerKind::None)
    }

    /// Preflight tracer with passed vars (data, data_idx, pc, pc_idx).
    pub fn preflight() -> Self {
        Self {
            kind: TracerKind::Preflight,
            passed_vars: vec![
                PassedVar::ptr("data"),
                PassedVar::index("data_idx"),
                PassedVar::ptr("pc"),
                PassedVar::index("pc_idx"),
            ],
        }
    }

    /// Stats tracer - counts events, opcodes, and register usage.
    pub fn stats() -> Self {
        Self::new(TracerKind::Stats)
    }

    /// FFI tracer - calls external functions.
    pub fn ffi() -> Self {
        Self::new(TracerKind::Ffi)
    }

    /// Dynamic tracer - runtime function pointers.
    pub fn dynamic() -> Self {
        Self::new(TracerKind::Dynamic)
    }

    /// Check if tracing is disabled.
    pub fn is_none(&self) -> bool {
        self.kind.is_none()
    }

    /// Check if RvState needs a Tracer field.
    pub fn has_tracer_struct(&self) -> bool {
        self.kind.has_tracer_struct()
    }

    /// Check if there are passed variables.
    pub fn has_passed_vars(&self) -> bool {
        !self.passed_vars.is_empty()
    }

    /// Parse tracer type from string.
    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::none()),
            "preflight" => Some(Self::preflight()),
            "stats" => Some(Self::stats()),
            "ffi" => Some(Self::ffi()),
            "dynamic" => Some(Self::dynamic()),
            _ => None,
        }
    }
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self::none()
    }
}

/// Generate a minimal tracer header.
///
/// This provides a `Tracer` struct plus `trace_*` hooks used by the emitter.
/// The default implementation is no-op (keeps overhead near zero), but it
/// allows tracing-enabled builds to compile cleanly.
pub fn gen_tracer_header<X: rvr_ir::Xlen>(cfg: &TracerConfig) -> String {
    if cfg.is_none() {
        return "#pragma once\n/* Tracing disabled */\n".to_string();
    }

    let rtype = crate::signature::reg_type::<X>();
    let mut fields = String::new();
    for var in &cfg.passed_vars {
        let ty = match var.kind {
            PassedVarKind::Ptr => format!("{}*", rtype),
            PassedVarKind::Index => "uint32_t".to_string(),
            PassedVarKind::Value => rtype.to_string(),
        };
        fields.push_str(&format!("    {} {};\n", ty, var.name));
    }

    format!(
        r#"#pragma once
#include <stdint.h>

typedef struct Tracer {{
{fields}}} Tracer;

static inline void trace_init(Tracer* t) {{ (void)t; }}
static inline void trace_fini(Tracer* t) {{ (void)t; }}

static inline void trace_block(Tracer* t, uint64_t pc) {{ (void)t; (void)pc; }}
static inline void trace_pc(Tracer* t, uint64_t pc) {{ (void)t; (void)pc; }}
static inline void trace_branch_taken(Tracer* t, uint64_t pc, uint64_t target) {{ (void)t; (void)pc; (void)target; }}
static inline void trace_branch_not_taken(Tracer* t, uint64_t pc, uint64_t target) {{ (void)t; (void)pc; (void)target; }}

static inline void trace_mem_read_byte(uint64_t addr, uint8_t v) {{ (void)addr; (void)v; }}
static inline void trace_mem_read_halfword(uint64_t addr, uint16_t v) {{ (void)addr; (void)v; }}
static inline void trace_mem_read_word(uint64_t addr, uint32_t v) {{ (void)addr; (void)v; }}
static inline void trace_mem_read_dword(uint64_t addr, uint64_t v) {{ (void)addr; (void)v; }}

static inline void trace_mem_write_byte(uint64_t addr, uint8_t v) {{ (void)addr; (void)v; }}
static inline void trace_mem_write_halfword(uint64_t addr, uint16_t v) {{ (void)addr; (void)v; }}
static inline void trace_mem_write_word(uint64_t addr, uint32_t v) {{ (void)addr; (void)v; }}
static inline void trace_mem_write_dword(uint64_t addr, uint64_t v) {{ (void)addr; (void)v; }}

static inline void trace_reg_read(uint32_t r, uint64_t v) {{ (void)r; (void)v; }}
static inline void trace_reg_write(uint32_t r, uint64_t v) {{ (void)r; (void)v; }}
static inline void trace_csr_read(uint32_t c, uint32_t v) {{ (void)c; (void)v; }}
static inline void trace_csr_write(uint32_t c, uint32_t v) {{ (void)c; (void)v; }}
"#,
        fields = fields
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracer_kind() {
        assert!(TracerKind::None.is_none());
        assert!(!TracerKind::Preflight.is_none());
        assert!(!TracerKind::None.has_tracer_struct());
        assert!(TracerKind::Stats.has_tracer_struct());
    }

    #[test]
    fn test_tracer_config_none() {
        let config = TracerConfig::none();
        assert!(config.is_none());
        assert!(!config.has_tracer_struct());
        assert!(!config.has_passed_vars());
    }

    #[test]
    fn test_tracer_config_preflight() {
        let config = TracerConfig::preflight();
        assert!(!config.is_none());
        assert!(config.has_tracer_struct());
        assert!(config.has_passed_vars());
        assert_eq!(config.passed_vars.len(), 4);
        assert_eq!(config.passed_vars[0].name, "data");
        assert_eq!(config.passed_vars[0].kind, PassedVarKind::Ptr);
        assert_eq!(config.passed_vars[1].name, "data_idx");
        assert_eq!(config.passed_vars[1].kind, PassedVarKind::Index);
    }

    #[test]
    fn test_tracer_from_string() {
        assert!(TracerConfig::from_string("none").unwrap().is_none());
        assert_eq!(TracerConfig::from_string("preflight").unwrap().kind, TracerKind::Preflight);
        assert_eq!(TracerConfig::from_string("stats").unwrap().kind, TracerKind::Stats);
        assert!(TracerConfig::from_string("invalid").is_none());
    }
}
