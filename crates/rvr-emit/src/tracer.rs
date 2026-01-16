//! Tracer configuration.

/// Tracer kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TracerKind {
    /// No tracing.
    None,
    /// Statistics tracer (counters).
    Stats,
    /// Preflight tracer (record/replay).
    Preflight,
    /// FFI tracer (external function).
    Ffi,
    /// Dynamic tracer (function pointer).
    Dynamic,
}

/// Variable passed to block functions.
#[derive(Clone, Debug)]
pub struct PassedVar {
    pub name: String,
    pub c_type: String,
    pub kind: PassedVarKind,
}

/// Kind of passed variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassedVarKind {
    /// Pointer passed directly.
    Ptr,
    /// Index into array.
    Index,
    /// Scalar value.
    Value,
}

/// Tracer configuration.
#[derive(Clone, Debug)]
pub struct TracerConfig {
    pub kind: TracerKind,
    pub passed_vars: Vec<PassedVar>,
}

impl TracerConfig {
    /// No tracing.
    pub fn none() -> Self {
        Self {
            kind: TracerKind::None,
            passed_vars: Vec::new(),
        }
    }

    /// Statistics tracer.
    pub fn stats() -> Self {
        Self {
            kind: TracerKind::Stats,
            passed_vars: Vec::new(),
        }
    }

    /// Check if tracing is enabled.
    pub fn is_none(&self) -> bool {
        self.kind == TracerKind::None
    }
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self::none()
    }
}
