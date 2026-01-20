//! Tracer configuration and header generation.
//!
//! Tracers are implemented as C headers. This module selects a built-in
//! tracer or loads a custom header and describes which tracer fields are
//! passed directly to block functions.

use std::fs;
use std::path::PathBuf;

use rvr_ir::Xlen;

use crate::tracers;

/// Built-in tracer kind.
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
}

/// Tracer source.
#[derive(Clone, Debug)]
pub enum TracerSource {
    /// Built-in tracer header.
    Builtin(TracerKind),
    /// Inline custom header content.
    Inline { name: String, header: String },
    /// Custom header loaded from a file.
    File { name: String, path: PathBuf },
}

impl TracerSource {
    /// Name used for display/logging.
    pub fn name(&self) -> &str {
        match self {
            TracerSource::Builtin(kind) => match kind {
                TracerKind::None => "none",
                TracerKind::Preflight => "preflight",
                TracerKind::Stats => "stats",
                TracerKind::Ffi => "ffi",
                TracerKind::Dynamic => "dynamic",
            },
            TracerSource::Inline { name, .. } => name,
            TracerSource::File { name, .. } => name,
        }
    }

    /// Return true when this represents a disabled tracer.
    pub fn is_none(&self) -> bool {
        matches!(self, TracerSource::Builtin(TracerKind::None))
    }
}

/// Variable passed directly to block functions.
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

/// Tracer configuration: source + passed variables.
#[derive(Clone, Debug)]
pub struct TracerConfig {
    /// Tracer header source.
    pub source: TracerSource,
    /// Variables passed directly to block functions.
    pub passed_vars: Vec<PassedVar>,
}

impl TracerConfig {
    /// Create config for a built-in tracer kind.
    pub fn builtin(kind: TracerKind) -> Self {
        let passed_vars = match kind {
            TracerKind::Preflight => vec![
                PassedVar::ptr("data"),
                PassedVar::index("data_idx"),
                PassedVar::ptr("pc"),
                PassedVar::index("pc_idx"),
            ],
            _ => Vec::new(),
        };
        Self {
            source: TracerSource::Builtin(kind),
            passed_vars,
        }
    }

    /// No tracing.
    pub fn none() -> Self {
        Self::builtin(TracerKind::None)
    }

    /// Preflight tracer with default passed vars (data, data_idx, pc, pc_idx).
    pub fn preflight() -> Self {
        Self::builtin(TracerKind::Preflight)
    }

    /// Stats tracer.
    pub fn stats() -> Self {
        Self::builtin(TracerKind::Stats)
    }

    /// FFI tracer.
    pub fn ffi() -> Self {
        Self::builtin(TracerKind::Ffi)
    }

    /// Dynamic tracer.
    pub fn dynamic() -> Self {
        Self::builtin(TracerKind::Dynamic)
    }

    /// Custom tracer with inline header content.
    pub fn custom_inline(
        name: impl Into<String>,
        header: impl Into<String>,
        passed_vars: Vec<PassedVar>,
    ) -> Self {
        Self {
            source: TracerSource::Inline {
                name: name.into(),
                header: header.into(),
            },
            passed_vars,
        }
    }

    /// Custom tracer with header loaded from a file.
    pub fn custom_file(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        passed_vars: Vec<PassedVar>,
    ) -> Self {
        Self {
            source: TracerSource::File {
                name: name.into(),
                path: path.into(),
            },
            passed_vars,
        }
    }

    /// Replace passed vars.
    pub fn with_passed_vars(mut self, vars: Vec<PassedVar>) -> Self {
        self.passed_vars = vars;
        self
    }

    /// Add a passed variable.
    pub fn push_passed_var(&mut self, var: PassedVar) {
        self.passed_vars.push(var);
    }

    /// Check if tracing is disabled.
    pub fn is_none(&self) -> bool {
        self.source.is_none()
    }

    /// Check if RvState needs a Tracer field.
    pub fn has_tracer_struct(&self) -> bool {
        !self.is_none()
    }

    /// Check if there are passed variables.
    pub fn has_passed_vars(&self) -> bool {
        !self.passed_vars.is_empty()
    }

    /// Parse tracer type from string for built-ins.
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

    /// Generate passed-var params (", uint64_t* data, uint32_t data_idx, ...").
    pub fn passed_var_params<X: Xlen>(&self) -> String {
        if self.passed_vars.is_empty() {
            return String::new();
        }

        let rtype = crate::signature::reg_type::<X>();
        let mut result = String::new();
        for var in &self.passed_vars {
            let param_type = match var.kind {
                PassedVarKind::Ptr => format!("{}*", rtype),
                PassedVarKind::Index => "uint32_t".to_string(),
                PassedVarKind::Value => rtype.to_string(),
            };
            result.push_str(&format!(", {} {}", param_type, var.name));
        }
        result
    }

    /// Generate passed-var args (", data, data_idx, ...").
    pub fn passed_var_args(&self) -> String {
        if self.passed_vars.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        for var in &self.passed_vars {
            result.push_str(&format!(", {}", var.name));
        }
        result
    }

    /// Generate passed-var args from state->tracer (", state->tracer.data, ...").
    pub fn passed_var_args_from_state(&self) -> String {
        if self.passed_vars.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        for var in &self.passed_vars {
            result.push_str(&format!(", state->tracer.{}", var.name));
        }
        result
    }

    /// Generate save-to-state code for passed vars.
    pub fn passed_var_save_to_state(&self) -> String {
        if self.passed_vars.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        for var in &self.passed_vars {
            if var.kind == PassedVarKind::Index {
                result.push_str(&format!(" state->tracer.{0} = {0};", var.name));
            }
        }
        result
    }
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self::none()
    }
}

/// Generate tracer header based on config.
pub fn gen_tracer_header<X: Xlen>(cfg: &TracerConfig) -> std::io::Result<String> {
    match &cfg.source {
        TracerSource::Builtin(kind) => Ok(tracers::gen_tracer_header::<X>(*kind)),
        TracerSource::Inline { header, .. } => Ok(header.clone()),
        TracerSource::File { path, .. } => fs::read_to_string(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracer_kind() {
        assert!(TracerKind::None.is_none());
        assert!(!TracerKind::Preflight.is_none());
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
        assert!(matches!(
            TracerConfig::from_string("preflight").unwrap().source,
            TracerSource::Builtin(TracerKind::Preflight)
        ));
        assert!(matches!(
            TracerConfig::from_string("stats").unwrap().source,
            TracerSource::Builtin(TracerKind::Stats)
        ));
        assert!(TracerConfig::from_string("invalid").is_none());
    }
}
