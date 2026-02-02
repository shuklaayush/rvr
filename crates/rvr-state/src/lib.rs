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
//! # Suspension
//!
//! `RvState` is also generic over a `SuspenderState` type. By default, `()` is used
//! which means no suspension support. Use `InstretSuspender` for instret-based suspension.
//!
//! ```ignore
//! use rvr_state::{RvState, Rv64State, Rv64StateWith, PreflightTracer, InstretSuspender};
//! use rvr_ir::Rv64;
//!
//! // No tracing, no suspension (default) - minimal layout
//! let state = Rv64State::new();
//!
//! // With preflight tracer - adds Tracer struct at end
//! let state = Rv64StateWith::<PreflightTracer<Rv64>>::new();
//! ```

mod memory;
mod state;
mod suspender;
mod tracer;

pub use memory::{DEFAULT_MEMORY_SIZE, FixedMemory, GUARD_SIZE, GuardedMemory, MemoryError};
pub use state::{
    NUM_CSRS, NUM_REGS_E, NUM_REGS_I, Rv32EState, Rv32State, Rv32StateWith, Rv64EState, Rv64State,
    Rv64StateWith, RvState,
};
pub use suspender::{InstretSuspender, SuspenderState};
pub use tracer::{
    BufferedDiffIterator,
    BufferedDiffTracer,
    CountingTracer,
    DebugTracer,
    DiffEntry,
    DiffTracer,
    DynamicTracer,
    FfiTracer,
    FfiTracerPtr,
    NoopTracer,
    PreflightTracer,
    StatsTracer,
    // Behavior trait and implementations
    Tracer,
    // State types (FFI struct layouts)
    TracerState,
};
