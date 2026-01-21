//! Tracer types and FFI for RISC-V execution tracing.
//!
//! This module provides:
//! - `TracerState` trait and types for FFI-compatible tracer structs
//! - `Tracer` trait for runtime tracer behavior
//! - FFI callback functions for C → Rust tracer calls
//!
//! # Tracer State vs Tracer
//!
//! - `TracerState`: FFI struct layout embedded in `RvState` (data)
//! - `Tracer`: Behavior trait with trace methods (code)
//!
//! For C tracers (preflight, stats), all data and code is in C.
//! For FFI tracers, data is `FfiTracer` (just a pointer) and code is in Rust.

mod ffi;
mod state;

// Re-export state types
pub use state::{DebugTracer, DynamicTracer, FfiTracer, PreflightTracer, StatsTracer, TracerState};

// Re-export FFI types
pub use ffi::{CountingTracer, FfiTracerPtr, NoopTracer, Tracer};
