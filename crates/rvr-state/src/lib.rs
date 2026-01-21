//! RISC-V machine state for runtime execution.
//!
//! This crate provides the `RvState` struct with a layout matching the generated C code,
//! allowing Rust to own and manage the state directly rather than using opaque pointers.
//!
//! # Tracing
//!
//! `RvState` is generic over a `TracerState` type. By default, `()` is used which is
//! a zero-sized type (ZST) that adds no overhead.
//!
//! ```ignore
//! use rvr_state::{RvState, Rv64State, Rv64StateWith, PreflightTracer};
//! use rvr_ir::Rv64;
//!
//! // No tracing (default) - matches C layout without tracer
//! let state = Rv64State::new();
//!
//! // With preflight tracer - matches C layout with Tracer struct at end
//! let state = Rv64StateWith::<PreflightTracer<Rv64>>::new();
//! ```

mod memory;
mod state;
mod tracer;

pub use memory::{GuardedMemory, MemoryError, GUARD_SIZE, DEFAULT_MEMORY_SIZE};
pub use state::{
    RvState, Rv32State, Rv64State, Rv32EState, Rv64EState,
    Rv32StateWith, Rv64StateWith,
    NUM_CSRS, NUM_REGS_E, NUM_REGS_I,
};
pub use tracer::{
    // State types (FFI struct layouts)
    TracerState, PreflightTracer, StatsTracer, FfiTracer, DynamicTracer, DebugTracer,
    // Behavior trait and implementations
    Tracer, FfiTracerPtr, NoopTracer, CountingTracer,
};
