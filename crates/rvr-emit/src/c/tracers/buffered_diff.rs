//! Buffered diff tracer header generation.
//!
//! Captures N instruction states in a ring buffer for block-level differential testing.
//! Avoids per-instruction FFI callbacks by buffering in C and reading from Rust after execution.
//!
//! Captured state per entry:
//! - PC and opcode
//! - Register write (rd, value)
//! - Memory access (addr, value, width, `is_write`)

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_buffered_diff<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    format!(include_str!("templates/buffered_diff.h.in"), rtype = rtype)
}
