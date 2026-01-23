//! Runtime support for RISC-V programs targeting rvr.
//!
//! This crate provides common runtime components for bare-metal RISC-V programs:
//!
//! - **Entry point** (`entry` feature): `_start` that sets up stack, zeros BSS,
//!   calls `main`, and exits via ecall with the return value as exit code.
//!
//! - **Panic handlers** (mutually exclusive):
//!   - `panic-halt`: Infinite loop (safe, debugger-friendly)
//!   - `panic-trap`: Illegal instruction (exit_code=1 via trap)
//!   - `panic-abort`: Exit via ecall with code 1
//!
//! - **Allocator** (`alloc` feature): Bump allocator with const-generic heap size
//!
//! - **Critical section** (`critical-section` feature): Single-threaded critical
//!   section via mstatus CSR
//!
//! # Quick Start
//!
//! ```toml
//! [dependencies]
//! rvr-rt = { version = "0.1", features = ["entry", "panic-trap", "alloc"] }
//! ```
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! use rvr_rt::BumpAlloc;
//!
//! #[global_allocator]
//! static ALLOC: BumpAlloc<{ 16 * 1024 * 1024 }> = BumpAlloc::new();
//!
//! #[no_mangle]
//! pub extern "C" fn main() -> i32 {
//!     // Your code here
//!     0 // Exit code
//! }
//! ```
//!
//! # Features
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `entry` | Provides `_start` entry point with ecall-based exit |
//! | `panic-halt` | Panic handler that loops forever |
//! | `panic-trap` | Panic handler that executes `unimp` (exit_code=1) |
//! | `panic-abort` | Panic handler that calls exit syscall with code 1 |
//! | `alloc` | Bump allocator (`BumpAlloc<N>`) |
//! | `critical-section` | Critical section implementation for `critical-section` crate |

#![no_std]

// Entry point module
#[cfg(feature = "entry")]
mod entry;

// Panic handlers module
#[cfg(any(feature = "panic-halt", feature = "panic-trap", feature = "panic-abort", feature = "panic-htif"))]
mod panic;

// Allocator module
#[cfg(feature = "alloc")]
mod alloc;
#[cfg(feature = "alloc")]
pub use alloc::BumpAlloc;

// Critical section module
#[cfg(feature = "critical-section")]
mod critical;
